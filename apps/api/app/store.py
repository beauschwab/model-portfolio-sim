"""Repository-shaped state facade and engine adapters.

Module dictionaries are a worker-local materialization of durable revisions.
The explicit memory execution mode is retained for isolated engine tests.
"""
from __future__ import annotations

import copy
import sys
from contextvars import ContextVar
import threading
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import numpy as np
import polars as pl

from .schemas import MarketScenario, RiskSettings
from .contracts import WorkspaceSnapshot, PreparedLibraryBundle, QueueFull
from . import state_views
from . import telemetry, job_queue, pricing_jobs, income_jobs, strategy_jobs, ledger_jobs, demo_inputs

_owner = sys.modules[__name__]

_LOCK = threading.RLock()
_POOL = ThreadPoolExecutor(max_workers=1)   # isolated memory execution only
_CUR_JID: str | None = None                 # job on the single worker thread
KRD_PILLARS = 10                             # curve pillars bumped for key-rate durations

BOOKS: dict[str, pl.DataFrame] = {}
TAPES: dict[str, dict] = {}
COHORT_PUBLICATIONS: dict[str, dict] = {}
MARKET: dict[str, Any] = {}
SCENARIOS: dict[str, MarketScenario] = {}
SETTINGS = RiskSettings()
JOBS: dict[str, dict] = {}
DEP_HIST: pl.DataFrame | None = None
MBS_HISTS = None
ASOF = None
EQUITY = 0.0
HEDGES = None
PROGRAMS: dict[str, dict] = {}
UNITLIB = None
BASE_KPIS = None  # legacy names retained for callers; evaluations use CACHE atomically
CACHE: PreparedLibraryBundle = {}
PRICING: dict = {}  # bounded content-keyed engine cache, independent of library publication
DECISIONS: dict = {}  # opaque session id -> engine session and input revision
STATE_META = {"revision": 0}
ASSUMPTIONS: dict = {}
_RUN_STATE = ContextVar("application_run_state", default=None)
MAX_JOBS = 50
MAX_QUEUE = 32
MAX_RESULT_BYTES = 256 * 1024 * 1024
JOB_TTL_SECONDS = 3600


def snapshot_local() -> WorkspaceSnapshot:
    with _LOCK:
        return {"books": {k: v.clone() for k, v in BOOKS.items()},
                "tapes": copy.deepcopy(TAPES), "cohort_publications": copy.deepcopy(COHORT_PUBLICATIONS),
                "market": copy.deepcopy(MARKET), "scenarios": copy.deepcopy(SCENARIOS),
                "settings": SETTINGS.model_copy(deep=True), "dep_hist": DEP_HIST,
                "mbs_hists": MBS_HISTS, "asof": ASOF, "equity": EQUITY,
                "hedges": HEDGES, "programs": copy.deepcopy(PROGRAMS),
                "assumptions": copy.deepcopy(ASSUMPTIONS), "revision": STATE_META["revision"]}


def snapshot() -> WorkspaceSnapshot:
    from . import persistence
    with _LOCK:
        persistence.refresh()
        return snapshot_local()


def current_state() -> WorkspaceSnapshot:
    return _RUN_STATE.get() or snapshot()


def restore_snapshot(state: WorkspaceSnapshot) -> None:
    state_views.restore(_owner, state)


def changed():
    """Caller holds the repository lock; invalidate the published cache bundle."""
    global UNITLIB, BASE_KPIS
    from . import persistence
    persistence.changed()
    STATE_META["revision"] += 1
    CACHE.clear()
    UNITLIB = BASE_KPIS = None


def replace_book(name, rows):
    from .books import normalize_book
    with _LOCK:
        frame = normalize_book(name, rows, BOOKS[name], ASOF)
        BOOKS[name] = frame
        COHORT_PUBLICATIONS.pop(name, None)
        changed()
    return len(frame)


def market_view():
    with _LOCK:
        return {"swap_tenors": [1, 2, 3, 4, 5, 7, 10, 15, 20, 30],
                "swap_rates": MARKET["swap_rates"].tolist(), "vol_pts": MARKET["vol_pts"].tolist(),
                "source": MARKET.get("source", ""), "revision": STATE_META["revision"],
                "provenance": copy.deepcopy(MARKET.get("provenance", {}))}


def replace_market(m):
    with _LOCK:
        MARKET.update(swap_rates=np.array(m.swap_rates), vol_pts=np.array(m.vol_pts),
                      source="Manually supplied research market", provenance={
                          "curve": "assumed", "volatility": "assumed",
                          "warnings": ["Manually supplied inputs; source snapshot association cleared."]})
        changed()


def fetch_research_data(req):
    from .market_data import fetch_snapshot, summary
    report("Downloading research data", 0.1)
    result = fetch_snapshot(req)
    report("Research snapshot saved", 1.0)
    return summary(result)


def apply_research_curve(sid, expected_revision):
    from .market_data import DataError, get_snapshot
    from .schemas import Market
    snap = get_snapshot(sid)
    curve = snap.get("curve")
    if snap["dataset"] != "eris_sofr" or not curve:
        raise DataError("only an Eris discount-curve snapshot can replace the active curve")
    with _LOCK:
        if STATE_META["revision"] != expected_revision:
            raise RuntimeError("inputs changed while reviewing this snapshot; reload before applying")
        checked = Market(swap_rates=curve["swap_rates"], vol_pts=MARKET["vol_pts"].tolist())
        prior_vol = MARKET.get("provenance", {}).get("volatility", "assumed")
        warnings = list(snap["warnings"])
        warnings.append(f"Book valuation date remains {ASOF}; market date is {curve['as_of']}. This is a research repricing of the existing book, not an aged portfolio.")
        MARKET.update(swap_rates=np.array(checked.swap_rates), source=f"Eris SOFR {curve['as_of']} Â· derived research curve",
                      provenance={"snapshot_id": sid, "curve": "derived", "curve_as_of": curve["as_of"],
                                  "volatility": prior_vol, "warnings": warnings,
                                  "projection": copy.deepcopy(curve)})
        changed()
        return market_view()


def prune_jobs():
    return job_queue.prune_jobs(_owner)


def job_status(jid):
    from . import persistence
    if persistence.REPO is not None:
        return persistence.job_status(jid)
    with _LOCK:
        prune_jobs()
        j = JOBS[jid]
        return copy.deepcopy({k: v for k, v in j.items() if k != "result"})


def job_result(jid):
    from . import persistence
    if persistence.REPO is not None:
        return persistence.job_result(jid)
    with _LOCK:
        prune_jobs()
        j = JOBS[jid]
        if j["status"] != "done":
            raise RuntimeError(f"job {j['status']}")
        return j["result"]


def balance_sheet(state):
    return income_jobs.balance_sheet(_owner, state)


def base_calibration(state):
    return income_jobs.base_calibration(_owner, state)



def seed_demo():
    return demo_inputs.seed_demo(_owner)


def apply_scenario(sc: MarketScenario, quarter: int, state=None) -> tuple[np.ndarray, np.ndarray, float]:
    return pricing_jobs.apply_scenario(_owner, sc, quarter, state)


from .result_transport import (ARROW_ENVELOPE_MIME, _arrow_safe, _frames_to_arrow,
                               _pack_envelope, to_arrow_envelope)


def report(stage: str | None=None, pct: float | None=None, log: str | None=None, **inc: float) -> None:
    return telemetry.report(_owner, stage, pct, log, **inc)


def node(nid: str, *, parent: str | None=None, label: str | None=None, kind: str | None=None, status: str | None=None, detail: str | None=None, **stat: float) -> None:
    return telemetry.node(_owner, nid, parent=parent, label=label, kind=kind, status=status, detail=detail, **stat)


def compute_run_plan(kind: str, books: list[str], state=None) -> dict:
    return telemetry.compute_run_plan(_owner, kind, books, state)


def submit(kind: str, fn, *args, plan: dict | None=None, state=None) -> str:
    return job_queue.submit(_owner, kind, fn, *args, plan=plan, state=state)


# ---- engine adapters (each returns JSON-able frames) -------------------------

def pricing_scope(state, books, overrides):
    return pricing_jobs.pricing_scope(_owner, state, books, overrides)


def run_pricing(books, sr, vp, spread_shift=0.0, overrides=None, options=None):
    return pricing_jobs.run_pricing(_owner, books, sr, vp, spread_shift, overrides, options)


def run_risk_all(books: list[str], sr, vp, spread_shift: float=0.0, scenario=False):
    return pricing_jobs.run_risk_all(_owner, books, sr, vp, spread_shift, scenario)


def run_stress_all(books, sr, vp, scenario=False, spread_shift=0.0):
    return pricing_jobs.run_stress_all(_owner, books, sr, vp, scenario, spread_shift)


def run_nii(sr, vp, *, node_parent: str | None=None, node_prefix: str='nii'):
    return income_jobs.run_nii(_owner, sr, vp, node_parent=node_parent, node_prefix=node_prefix)


def run_kpis_scenario(sr, vp, scenario=False, spread_shift=0.0):
    return income_jobs.run_kpis_scenario(_owner, sr, vp, scenario, spread_shift)


def run_kpis(sr, vp, oas_by_book=None):
    return income_jobs.run_kpis(_owner, sr, vp, oas_by_book)


def build_unitlib_job(sr, vp):
    return strategy_jobs.build_unitlib_job(_owner, sr, vp)


def eval_strategy_sync(allocations: list[dict]):
    return strategy_jobs.eval_strategy_sync(_owner, allocations)


def run_strategy_job(sr, vp):
    return strategy_jobs.run_strategy_job(_owner, sr, vp)


def run_optimize_job(sr, vp, opt: dict):
    return strategy_jobs.run_optimize_job(_owner, sr, vp, opt)


def run_scenario_grid(sc: MarketScenario):
    return income_jobs.run_scenario_grid(_owner, sc)


def prepare_forecast(req):
    return income_jobs.prepare_forecast(_owner, req)


def run_forecast(plan, provenance):
    return income_jobs.run_forecast(_owner, plan, provenance)


def run_balance_stress(specification):
    return ledger_jobs.run_balance_stress(_owner, specification)


def run_treasury(specification):
    return ledger_jobs.run_treasury(_owner, specification)


def run_saved_balance_stress(request):
    return ledger_jobs.run_saved_balance_stress(_owner, request)


def run_streamed_balance_stress(request):
    return ledger_jobs.run_streamed_balance_stress(_owner, request)






# Storage sees an explicit state port, never imports concrete feature handlers.
from . import persistence
persistence.bind_workspace(_owner)
