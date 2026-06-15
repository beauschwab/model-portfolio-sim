"""In-memory application state + engine adapters. Single-process demo
store: books/market/scenarios/settings live in module state; runs execute
in a thread pool and land in JOBS. Swap for Postgres/Redis in production
(the surface is deliberately repository-shaped)."""
from __future__ import annotations

import io
import json
import struct
import threading
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import numpy as np
import polars as pl

from .schemas import MarketScenario, RiskSettings

_LOCK = threading.Lock()
_POOL = ThreadPoolExecutor(max_workers=1)   # numba kernels saturate cores
_CUR_JID: str | None = None                 # job on the single worker thread
KRD_PILLARS = 10                             # curve pillars bumped for key-rate durations

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
    from portfolio_risk.demo import (demo_deposit_history, demo_market,
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
    from portfolio_risk.demo import demo_hedge_book
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


# ---- Arrow envelope serialization -------------------------------------------
# Computed polars frames cross the wire as Apache Arrow IPC, not JSON. A result
# tree is split into a JSON "skeleton" (every pl.DataFrame leaf replaced by a
# {"__arrow__": i} marker) plus N concatenated Arrow IPC blobs. Scalars, lists,
# and numpy-derived arrays (e.g. runoff_vectors) stay inline in the skeleton.
ARROW_ENVELOPE_MIME = "application/vnd.arrow-envelope"
_IPC_COMPRESSION = None      # Arrow-JS IPC compression is version-fragile; these
                             # frames are small -- uncompressed is the safe wire.
# polars defaults string columns to the Arrow Utf8View layout (type 24), which
# apache-arrow JS cannot read. oldest() emits plain Utf8 the JS reader accepts.
_IPC_COMPAT = pl.CompatLevel.oldest()


def _arrow_safe(df: pl.DataFrame) -> pl.DataFrame:
    """Cast columns Arrow IPC cannot encode (pl.Object, e.g. the demo
    `call_schedule` list column) to JSON-encoded Utf8. Anything else
    unencodable surfaces loudly from write_ipc -- intentionally."""
    obj_cols = [c for c, dt in zip(df.columns, df.dtypes) if dt == pl.Object]
    if not obj_cols:
        return df
    return df.with_columns([
        pl.col(c).map_elements(
            lambda v: None if v is None else json.dumps(v, default=str),
            return_dtype=pl.Utf8).alias(c)
        for c in obj_cols
    ])


def _frames_to_arrow(obj):
    """Walk a result tree, returning (skeleton, blobs). Each pl.DataFrame leaf
    becomes a {"__arrow__": idx} marker and an Arrow IPC blob at that index."""
    blobs: list[bytes] = []

    def walk(o):
        if isinstance(o, pl.DataFrame):
            buf = io.BytesIO()
            _arrow_safe(o).write_ipc(buf, compression=_IPC_COMPRESSION,
                                     compat_level=_IPC_COMPAT)
            blobs.append(buf.getvalue())
            return {"__arrow__": len(blobs) - 1}
        if isinstance(o, dict):
            return {k: walk(v) for k, v in o.items()}
        if isinstance(o, (list, tuple)):
            return [walk(v) for v in o]
        if isinstance(o, np.ndarray):
            return walk(o.tolist())
        if isinstance(o, np.generic):
            return o.item()
        return o

    return walk(obj), blobs


def _pack_envelope(skeleton, blobs: list[bytes]) -> bytes:
    """Frame skeleton + blobs into the ARW1 binary envelope (little-endian)."""
    sj = json.dumps(skeleton).encode("utf-8")
    parts = [b"ARW1", struct.pack("<I", len(sj)), sj,
             struct.pack("<I", len(blobs))]
    parts += [struct.pack("<I", len(b)) for b in blobs]
    parts += blobs
    return b"".join(parts)


def to_arrow_envelope(obj) -> bytes:
    """Serialize a result object (frames + scalars) to an ARW1 envelope."""
    return _pack_envelope(*_frames_to_arrow(obj))


def report(stage: str | None = None, pct: float | None = None,
           log: str | None = None, **inc: float) -> None:
    """Update the running job's progress telemetry. Stage/pct are set;
    keyword counters in **inc are accumulated (records, revaluations,
    reductions, path_evaluations, scenario_paths, books_done, ...).
    Safe no-op when no job is bound to the worker thread."""
    jid = _CUR_JID
    if jid is None or jid not in JOBS:
        return
    with _LOCK:
        p = JOBS[jid].setdefault("progress", {})
        if stage is not None:
            p["stage"] = stage
        if pct is not None:
            # monotonic: nested adapters (run_nii inside kpis/optimize) report
            # their own band; never let the bar run backward within a job
            nxt = round(max(0.0, min(100.0, float(pct))), 1)
            p["pct"] = max(p.get("pct", 0.0), nxt)
        t0 = JOBS[jid].get("_t0")
        if t0 is not None:
            p["elapsed_s"] = round(time.perf_counter() - t0, 3)
        stats = p.setdefault("stats", {})
        for k, v in inc.items():
            stats[k] = stats.get(k, 0) + v
        if log:
            p.setdefault("log", []).append(
                {"t": p.get("elapsed_s", 0.0), "msg": log})
            p["log"] = p["log"][-60:]      # keep the tail bounded


def compute_run_plan(kind: str, books: list[str]) -> dict:
    """Model the workload a run will dispatch, from current settings and
    book sizes. These are the quantities a quant developer reaches for:
    records in scope, Monte-Carlo paths, scenario path-steps simulated,
    full revaluations, path evaluations, and mean reductions. Modeled
    (settings x book sizes) -- actual kernel work may differ."""
    s = SETTINGS
    per_book = {b: int(len(BOOKS[b])) for b in books if b in BOOKS}
    positions = sum(per_book.values())
    paths = int(s.n_paths)
    horizon = int(s.horizon_months)
    shocks = list(map(float, s.shocks_bp))
    n_shocks = len(shocks)
    if kind == "risk":
        scope = positions
        revals_per = 1 + 2 + 2 * KRD_PILLARS          # base + parallel dv01 + KRD pillars
        path_steps = paths
    elif kind in ("stress", "deposit_stress"):
        scope = sum(per_book.get(b, 0) for b in ("mbs", "deposits"))
        revals_per = 1 + n_shocks
        path_steps = paths * horizon
    elif kind in ("nii", "kpis"):
        scope = positions
        revals_per = 1
        path_steps = paths * horizon
    elif kind == "scenario_nii":
        scope = positions
        revals_per = 9
        path_steps = paths * horizon * 9
    else:                                              # unitlib, strategy, optimize
        scope = positions
        revals_per = 1
        path_steps = paths * horizon
    revaluations = scope * revals_per
    return {
        "kind": kind,
        "records": positions,
        "records_by_book": per_book,
        "in_scope": scope,
        "monte_carlo_paths": paths,
        "horizon_months": horizon,
        "rate_shocks_bp": shocks,
        "scenario_path_steps": path_steps,
        "revaluations": revaluations,
        "path_evaluations": revaluations * paths,
        "reductions": revaluations,
        "crn_seed": int(s.seed),
        "note": "workload modeled from settings x book sizes",
    }


def submit(kind: str, fn, *args, plan: dict | None = None) -> str:
    jid = uuid.uuid4().hex[:12]
    JOBS[jid] = {"id": jid, "kind": kind, "status": "queued",
                 "detail": None, "result": None,
                 "progress": {"stage": "queued", "pct": 0.0,
                              "plan": plan or {}, "stats": {},
                              "elapsed_s": 0.0, "log": []}}

    def run():
        global _CUR_JID
        _CUR_JID = jid
        JOBS[jid]["status"] = "running"
        JOBS[jid]["_t0"] = time.perf_counter()
        report(stage="starting", pct=1.0, log=f"{kind} run started")
        try:
            if SETTINGS.n_threads > 0:
                import numba
                numba.set_num_threads(SETTINGS.n_threads)
            JOBS[jid]["result"] = fn(*args)   # raw tree; encoded at /result
            JOBS[jid]["status"] = "done"
            report(stage="done", pct=100.0, log="run complete")
        except Exception as e:                      # surface to client
            JOBS[jid]["status"] = "error"
            JOBS[jid]["detail"] = f"{type(e).__name__}: {e}"
            report(stage="error", log=f"{type(e).__name__}: {e}")
        finally:
            JOBS[jid].pop("_t0", None)
            _CUR_JID = None

    _POOL.submit(run)
    return jid


# ---- engine adapters (each returns JSON-able frames) -------------------------
def run_risk_all(books: list[str], sr, vp, spread_shift: float = 0.0):
    from portfolio_risk import run_cd_risk, run_corp_risk, run_deposit_risk
    from portfolio_risk.risk import run_risk
    out = {}
    order = [b for b in ("mbs", "loans", "debt", "deposits", "cds") if b in books]
    total = max(len(order), 1)
    done = 0
    factor = 1 + 2 + 2 * KRD_PILLARS              # base + parallel dv01 + KRD pillars
    report(stage="seeding CRN", pct=3.0,
           log=f"CRN draws: {SETTINGS.n_paths} paths, seed {SETTINGS.seed}")

    def _did(book: str):
        nonlocal done
        done += 1
        n = int(len(BOOKS[book]))
        rv = n * factor
        report(stage=f"revalued {book}", pct=3 + 94 * done / total,
               records=n, revaluations=rv,
               path_evaluations=rv * SETTINGS.n_paths, reductions=rv,
               books_done=1, log=f"{book}: {n} positions \u2192 {rv} revaluations")

    if "mbs" in books:
        report(stage="revaluing mbs", log="mbs: OAS-held dv01 + KRD by pillar")
        out["mbs"] = run_risk(BOOKS["mbs"], sr, vp, *MBS_HISTS,
                              seed=SETTINGS.seed)
        _did("mbs")
    if "loans" in books:
        report(stage="revaluing loans")
        out["loans"] = run_corp_risk(BOOKS["loans"], ASOF, sr, vp,
                                     *MBS_HISTS, seed=SETTINGS.seed)
        _did("loans")
    if "debt" in books:
        report(stage="revaluing debt")
        out["debt"] = run_corp_risk(BOOKS["debt"], ASOF, sr, vp,
                                    *MBS_HISTS, seed=SETTINGS.seed)
        _did("debt")
    if "deposits" in books:
        report(stage="revaluing deposits")
        out["deposits"] = run_deposit_risk(BOOKS["deposits"], sr, vp,
                                           DEP_HIST, seed=SETTINGS.seed)
        _did("deposits")
    if "cds" in books:
        report(stage="revaluing cds")
        out["cds"] = run_cd_risk(BOOKS["cds"], ASOF, sr, vp,
                                 seed=SETTINGS.seed)
        _did("cds")
    # spread leg: first-order P&L = -dv01 * spread_bp on spread products
    if spread_shift:
        report(stage="applying spread leg", log="first-order spread P&L")
        bp = spread_shift * 1e4
        for k, f in out.items():
            if "dv01" in f.columns:
                out[k] = f.with_columns(
                    (pl.col("dv01") * -bp).alias("spread_pnl"))
    return out


def run_stress_all(books, sr, vp):
    from portfolio_risk import run_deposit_stress
    from portfolio_risk.stress import run_stress
    out = {}
    shocks = tuple(SETTINGS.shocks_bp)
    factor = 1 + len(shocks)
    report(stage="seeding CRN", pct=4.0,
           log=f"{len(shocks)} forward shocks: {list(shocks)} bp")
    if "mbs" in books:
        report(stage="stressing mbs", pct=20.0)
        n = int(len(BOOKS["mbs"]))
        pos, agg, prof = run_stress(BOOKS["mbs"], sr, vp, *MBS_HISTS,
                                    shocks_bp=shocks,
                                    seed=SETTINGS.seed)
        out["mbs"] = {"agg": agg, "profile": prof}
        report(stage="stressed mbs", pct=60.0, records=n,
               revaluations=n * factor, path_evaluations=n * factor * SETTINGS.n_paths,
               reductions=n * factor, books_done=1,
               log=f"mbs: {n} positions \u00d7 {factor} states")
    if "deposits" in books:
        report(stage="stressing deposits", pct=70.0)
        n = int(len(BOOKS["deposits"]))
        pos, agg, prof = run_deposit_stress(
            BOOKS["deposits"], sr, vp, DEP_HIST,
            shocks_bp=shocks, seed=SETTINGS.seed)
        out["deposits"] = {"agg": agg, "profile": prof}
        report(stage="stressed deposits", pct=96.0, records=n,
               revaluations=n * factor, path_evaluations=n * factor * SETTINGS.n_paths,
               reductions=n * factor, books_done=1,
               log=f"deposits: {n} positions \u00d7 {factor} states")
    return out


def run_nii(sr, vp):
    from portfolio_risk.accounting import run_balance_sheet_nii
    report(stage="simulating LMM paths", pct=8.0,
           scenario_paths=SETTINGS.n_paths,
           log=f"LMM: {SETTINGS.n_paths} paths \u00d7 {SETTINGS.horizon_months}m")
    bs = {k: BOOKS[k] for k in ("mbs", "loans", "debt", "deposits",
                                "cds", "mm") if k in BOOKS}
    bs["hedges"] = HEDGES
    bs["mbs_hists"] = MBS_HISTS
    report(stage="forward balance & NII", pct=35.0)
    out = run_balance_sheet_nii(bs, sr, vp, DEP_HIST,
                                horizon=SETTINGS.horizon_months,
                                seed=SETTINGS.seed, asof=ASOF)
    n = sum(int(len(BOOKS[k])) for k in bs if isinstance(BOOKS.get(k), pl.DataFrame))
    report(stage="reducing to monthly NII", pct=55.0,
           records=n, path_evaluations=n * SETTINGS.n_paths, reductions=n,
           log="reduced path NII to monthly means")
    return out


def run_kpis(sr, vp):
    from portfolio_risk.kpis import compute_kpis
    bs = {k: BOOKS.get(k) for k in ("mbs", "loans", "debt", "deposits",
                                    "cds", "mm")}
    bs["mbs_hists"] = MBS_HISTS
    bs["asof"] = ASOF
    bs["equity"] = EQUITY
    bs["hedges"] = HEDGES
    nii = run_nii(sr, vp)
    report(stage="EVE & parallel dv01", pct=65.0, log="full-reval EVE + dv01")
    out = compute_kpis(bs, sr, vp, DEP_HIST,
                       nii_monthly=nii["monthly"], seed=SETTINGS.seed)
    report(stage="LCR / NSFR / CET1", pct=90.0, log="liquidity + capital ratios")
    out["nii_summary"] = nii["summary"]
    return out


def build_unitlib_job(sr, vp):
    from portfolio_risk.unitlib import build_unit_library
    global UNITLIB, BASE_KPIS
    UNITLIB = build_unit_library(sr, vp, MBS_HISTS, DEP_HIST,
                                 seed=SETTINGS.seed)
    BASE_KPIS = run_kpis(sr, vp)
    return {"units": len(UNITLIB["units"]),
            "templates": list(UNITLIB["templates"]),
            "grid_m": UNITLIB["grid_m"]}


def eval_strategy_sync(allocations: list[dict]):
    from portfolio_risk.unitlib import evaluate_strategy
    if UNITLIB is None:
        raise RuntimeError("unit library not built -- POST /run "
                           "kind='unitlib' first")
    return evaluate_strategy(UNITLIB, allocations, base_kpis=BASE_KPIS)


def run_strategy_job(sr, vp):
    from portfolio_risk.strategies import run_strategies
    nii = run_nii(sr, vp)
    return run_strategies(list(PROGRAMS.values()), sr, vp,
                          runoff_by_book=nii["runoff_vectors"],
                          horizon=SETTINGS.horizon_months,
                          seed=SETTINGS.seed)


def run_optimize_job(sr, vp, opt: dict):
    from portfolio_risk.optimizer import optimize_balance_sheet
    from portfolio_risk.unitlib import build_unit_library
    from portfolio_risk.kpis import compute_kpis
    report(stage="base NII path", pct=4.0,
           scenario_paths=SETTINGS.n_paths,
           log="base market NII path for the objective")
    nii = run_nii(sr, vp)
    scen_libs = []
    markets = [("base", sr, vp)]
    for name in opt.get("scenarios", []):
        sc = SCENARIOS.get(name)
        if sc:
            s2, v2, _ = apply_scenario(sc, 0)
            markets.append((name, s2, v2))
    bs = {k: BOOKS.get(k) for k in ("mbs", "loans", "debt", "deposits",
                                    "cds", "mm")}
    bs["mbs_hists"] = MBS_HISTS; bs["asof"] = ASOF
    bs["equity"] = EQUITY; bs["hedges"] = HEDGES
    total = max(len(markets), 1)
    span = 84.0 / total                               # 8% .. 92% across markets
    for i, (name, s2, v2) in enumerate(markets):
        base = 8.0 + span * i
        report(stage=f"unit library: {name}", pct=base,
               scenario_paths=SETTINGS.n_paths,
               log=f"building unit tensor under {name} market ({i + 1}/{total})")
        lib = build_unit_library(s2, v2, MBS_HISTS, DEP_HIST,
                                 seed=SETTINGS.seed)
        n_units = len(lib.get("units", [])) if isinstance(lib, dict) else 0
        report(stage=f"KPIs: {name}", pct=base + span * 0.6,
               revaluations=n_units, reductions=n_units,
               unit_columns=n_units,
               log=f"{name}: {n_units} unit columns priced")
        scen_libs.append((lib, compute_kpis(bs, s2, v2, DEP_HIST,
                                            nii_monthly=nii["monthly"],
                                            seed=SETTINGS.seed)))
    report(stage="solving LP", pct=94.0,
           log=f"maximin LP across {total} markets")
    out = optimize_balance_sheet(
        scen_libs, lcr_min=opt.get("lcr_min", 1.10),
        nsfr_min=opt.get("nsfr_min", 1.05),
        cet1_min=opt.get("cet1_min", 0.10),
        eve_limit=opt.get("eve_limit", 0.15),
        commercial=opt.get("commercial"),
        max_total_assets=opt.get("max_total_assets"))
    nb = len(out.get("binding_constraints", []) or []) if isinstance(out, dict) else 0
    report(stage="LP solved", pct=99.0, binding_constraints=nb,
           log=f"{'feasible' if isinstance(out, dict) and out.get('feasible') else 'infeasible'} \u00b7 {nb} binding")
    return out


def run_scenario_grid(sc: MarketScenario):
    """9Q scenario: revalue the books at each quarter's market and report
    value/NII drift along the named path (base OAS held fixed -- the
    engine's global invariant)."""
    rows = []
    for q in range(9):
        report(stage=f"quarter {q + 1}/9", pct=4 + 92 * q / 9,
               scenario_paths=SETTINGS.n_paths,
               log=f"Q{q + 1}: revalue at scenario market")
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
