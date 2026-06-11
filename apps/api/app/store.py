"""In-memory application state + engine adapters. Single-process demo
store: books/market/scenarios/settings live in module state; runs execute
in a thread pool and land in JOBS. Swap for Postgres/Redis in production
(the surface is deliberately repository-shaped)."""
from __future__ import annotations

import threading
import uuid
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import numpy as np
import polars as pl

from .schemas import MarketScenario, RiskSettings

_LOCK = threading.Lock()
_POOL = ThreadPoolExecutor(max_workers=1)   # numba kernels saturate cores

BOOKS: dict[str, pl.DataFrame] = {}
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
BASE_KPIS = None


def seed_demo():
    """Load the WFC-proportional model balance sheet + demo market."""
    from mbs_risk.demo import (demo_deposit_history, demo_market,
                               model_balance_sheet)
    global DEP_HIST, MBS_HISTS, ASOF
    bs = model_balance_sheet(scale=0.01, basis="amortized_cost",
                             include_markets_bs=True)
    for k in ("mbs", "loans", "debt", "deposits", "cds", "mm"):
        if bs.get(k) is not None:
            BOOKS[k] = bs[k]
    MBS_HISTS = bs["mbs_hists"]
    ASOF = bs["asof"]
    global EQUITY, HEDGES
    EQUITY = bs.get("equity", 0.0)
    from mbs_risk.demo import demo_hedge_book
    HEDGES = demo_hedge_book(scale=0.01)
    DEP_HIST = demo_deposit_history()
    sr, vp = demo_market()
    MARKET["swap_rates"] = sr
    MARKET["vol_pts"] = vp
    MARKET["source"] = bs["source"]


def apply_scenario(sc: MarketScenario, quarter: int
                   ) -> tuple[np.ndarray, np.ndarray, float]:
    """Map trader-space legs onto engine inputs at a given quarter:
    10y leg shifts all pillars in parallel; 2s10s leg twists linearly
    around the 5y pivot (2y -x/2, 10y +x/2); vol leg shifts the surface;
    spread leg is returned for OAS-level application by the caller."""
    def leg(xs: list[float]) -> float:
        if not xs:
            return 0.0
        return xs[min(quarter, len(xs) - 1)] * 1e-4

    sr = MARKET["swap_rates"].copy()
    tens = np.array([1, 2, 3, 4, 5, 7, 10, 15, 20, 30], dtype=float)
    sr = sr + leg(sc.ust10y_bp)
    tw = leg(sc.twos_tens_bp)
    if tw:
        sr = sr + tw * (np.clip(tens, 2.0, 10.0) - 5.0) / 8.0  # 2s10s pivot 5y
    vp = MARKET["vol_pts"].copy()
    vleg = leg(sc.vol_bp)
    if vleg:
        vp = vp.copy()
        vp[:, 2] = np.maximum(vp[:, 2] + vleg, 1e-4)
    return sr, vp, leg(sc.spread_bp)


def _frames_to_json(obj):
    if isinstance(obj, pl.DataFrame):
        return obj.to_dicts()
    if isinstance(obj, dict):
        return {k: _frames_to_json(v) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return [_frames_to_json(v) for v in obj]
    if isinstance(obj, np.generic):
        return obj.item()
    return obj


def submit(kind: str, fn, *args) -> str:
    jid = uuid.uuid4().hex[:12]
    JOBS[jid] = {"id": jid, "kind": kind, "status": "queued",
                 "detail": None, "result": None}

    def run():
        JOBS[jid]["status"] = "running"
        try:
            if SETTINGS.n_threads > 0:
                import numba
                numba.set_num_threads(SETTINGS.n_threads)
            JOBS[jid]["result"] = _frames_to_json(fn(*args))
            JOBS[jid]["status"] = "done"
        except Exception as e:                      # surface to client
            JOBS[jid]["status"] = "error"
            JOBS[jid]["detail"] = f"{type(e).__name__}: {e}"

    _POOL.submit(run)
    return jid


# ---- engine adapters (each returns JSON-able frames) -------------------------
def run_risk_all(books: list[str], sr, vp, spread_shift: float = 0.0):
    from mbs_risk import run_cd_risk, run_corp_risk, run_deposit_risk
    from mbs_risk.risk import run_risk
    out = {}
    if "mbs" in books:
        r = run_risk(BOOKS["mbs"], sr, vp, *MBS_HISTS,
                     seed=SETTINGS.seed)
        out["mbs"] = r
    if "loans" in books:
        out["loans"] = run_corp_risk(BOOKS["loans"], ASOF, sr, vp,
                                     seed=SETTINGS.seed)
    if "debt" in books:
        out["debt"] = run_corp_risk(BOOKS["debt"], ASOF, sr, vp,
                                    seed=SETTINGS.seed)
    if "deposits" in books:
        out["deposits"] = run_deposit_risk(BOOKS["deposits"], sr, vp,
                                           DEP_HIST, seed=SETTINGS.seed)
    if "cds" in books:
        out["cds"] = run_cd_risk(BOOKS["cds"], ASOF, sr, vp,
                                 seed=SETTINGS.seed)
    # spread leg: first-order P&L = -dv01 * spread_bp on spread products
    if spread_shift:
        bp = spread_shift * 1e4
        for k, f in out.items():
            if "dv01" in f.columns:
                out[k] = f.with_columns(
                    (pl.col("dv01") * -bp).alias("spread_pnl"))
    return out


def run_stress_all(books, sr, vp):
    from mbs_risk import run_deposit_stress
    from mbs_risk.stress import run_stress
    out = {}
    if "mbs" in books:
        pos, agg, prof = run_stress(BOOKS["mbs"], sr, vp, *MBS_HISTS,
                                    shocks_bp=tuple(SETTINGS.shocks_bp),
                                    seed=SETTINGS.seed)
        out["mbs"] = {"agg": agg, "profile": prof}
    if "deposits" in books:
        pos, agg, prof = run_deposit_stress(
            BOOKS["deposits"], sr, vp, DEP_HIST,
            shocks_bp=tuple(SETTINGS.shocks_bp), seed=SETTINGS.seed)
        out["deposits"] = {"agg": agg, "profile": prof}
    return out


def run_nii(sr, vp):
    from mbs_risk.accounting import run_balance_sheet_nii
    bs = {k: BOOKS[k] for k in ("mbs", "loans", "debt", "deposits",
                                "cds", "mm") if k in BOOKS}
    bs["hedges"] = HEDGES
    bs["mbs_hists"] = MBS_HISTS
    return run_balance_sheet_nii(bs, sr, vp, DEP_HIST,
                                 horizon=SETTINGS.horizon_months,
                                 seed=SETTINGS.seed, asof=ASOF)


def run_kpis(sr, vp):
    from mbs_risk.kpis import compute_kpis
    bs = {k: BOOKS.get(k) for k in ("mbs", "loans", "debt", "deposits",
                                    "cds", "mm")}
    bs["mbs_hists"] = MBS_HISTS
    bs["asof"] = ASOF
    bs["equity"] = EQUITY
    bs["hedges"] = HEDGES
    nii = run_nii(sr, vp)
    out = compute_kpis(bs, sr, vp, DEP_HIST,
                       nii_monthly=nii["monthly"], seed=SETTINGS.seed)
    out["nii_summary"] = nii["summary"]
    return out


def build_unitlib_job(sr, vp):
    from mbs_risk.unitlib import build_unit_library
    global UNITLIB, BASE_KPIS
    UNITLIB = build_unit_library(sr, vp, MBS_HISTS, DEP_HIST,
                                 seed=SETTINGS.seed)
    BASE_KPIS = run_kpis(sr, vp)
    return {"units": len(UNITLIB["units"]),
            "templates": list(UNITLIB["templates"]),
            "grid_m": UNITLIB["grid_m"]}


def eval_strategy_sync(allocations: list[dict]):
    from mbs_risk.unitlib import evaluate_strategy
    if UNITLIB is None:
        raise RuntimeError("unit library not built -- POST /run "
                           "kind='unitlib' first")
    return _frames_to_json(evaluate_strategy(UNITLIB, allocations,
                                             base_kpis=BASE_KPIS))


def run_strategy_job(sr, vp):
    from mbs_risk.strategies import run_strategies
    nii = run_nii(sr, vp)
    return run_strategies(list(PROGRAMS.values()), sr, vp,
                          runoff_by_book=nii["runoff_vectors"],
                          horizon=SETTINGS.horizon_months,
                          seed=SETTINGS.seed)


def run_scenario_grid(sc: MarketScenario):
    """9Q scenario: revalue the books at each quarter's market and report
    value/NII drift along the named path (base OAS held fixed -- the
    engine's global invariant)."""
    rows = []
    for q in range(9):
        sr, vp, spr = apply_scenario(sc, q)
        nii = run_nii(sr, vp)
        s = nii["summary"]
        rows.append({"quarter": q + 1,
                     "ust10y_pct": float(sr[6] * 100),
                     "twos_tens_bp": float((sr[6] - sr[1]) * 1e4),
                     "nii_annualized": float(
                         s.filter(pl.col("metric") == "nii_annualized_$")
                          ["value"][0]),
                     "nim_pct": float(
                         s.filter(pl.col("metric") == "nim_model_%")
                          ["value"][0])})
    return {"scenario": sc.name, "path": rows}
