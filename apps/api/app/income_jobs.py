"""Income jobs; the caller supplies the sole state owner.

Functions transport/format engine outputs. Financial calculations stay in Rust.
"""
from __future__ import annotations

import polars as pl

def balance_sheet(owner, state):
    bs = {k: v for k, v in state['books'].items() if len(v)}
    bs.update(mbs_hists=state['mbs_hists'], asof=state['asof'],
              equity=state['equity'], hedges=state['hedges'])
    return bs


def base_calibration(owner, state):
    from portfolio_risk.analytics.calibration import calibrate_books
    if "_base_oas" not in state:
        state["_base_oas"] = calibrate_books(owner.balance_sheet(state), state['market']["swap_rates"],
                           state['market']["vol_pts"], state['dep_hist'], state['settings'].seed)
    return state["_base_oas"]


def run_nii(owner, sr, vp, *, node_parent: str | None = None, node_prefix: str = "nii"):
    state = owner.current_state()
    """Forward balance-sheet NII. When `node_parent` is given, attaches its
    paths/cashflow/reduce nodes under that branch (so nested callers -- kpis,
    scenario_grid, optimize -- can place it in their own tree without id
    clashes) and stays quiet on stage/pct so the caller owns the bar."""
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    npaths = state['settings'].n_paths
    nested = node_parent is not None
    pre = node_prefix
    if not nested:                       # standalone run: own the root stages
        owner.node("build", parent=None, kind="build", status="running",
             label="Position build")
        owner.node(pre, parent="build", kind="branch", label="Base market",
             status="running")
        owner.node("build", status="done")
        parent = pre
    else:
        parent = node_parent

    owner.node(f"{pre}:paths", parent=parent, kind="paths", label="LMM paths",
         status="running", detail=f"{npaths} paths \u00d7 {state['settings'].horizon_months}m")
    if not nested:
        owner.report(stage="simulating LMM paths", pct=8.0,
               log=f"LMM: {npaths} paths \u00d7 {state['settings'].horizon_months}m")
    owner.report(scenario_paths=npaths, paths_generated=npaths)
    owner.node(f"{pre}:paths", status="done", paths=npaths)

    bs = owner.balance_sheet(state)
    owner.node(f"{pre}:cf", parent=parent, kind="cashflow",
         label="forward balance & NII", status="running")
    if not nested:
        owner.report(stage="forward balance & NII", pct=35.0)
    out = run_balance_sheet_nii(bs, sr, vp, state['dep_hist'],
                                horizon=state['settings'].horizon_months,
                                seed=state['settings'].seed, asof=state['asof'])
    n = sum(int(len(state['books'][k])) for k in bs if isinstance(state['books'].get(k), pl.DataFrame))
    cf = n * npaths
    owner.node(f"{pre}:cf", status="done", calcs=cf)
    owner.node(f"{pre}:reduce", parent=parent, kind="reduce",
         label="reduce to monthly NII", status="running")
    if not nested:
        owner.report(stage="reducing to monthly NII", pct=55.0)
    owner.report(records=n, path_evaluations=cf, reductions=n, cashflow_calcs=cf,
           log="reduced path NII to monthly means")
    owner.node(f"{pre}:reduce", status="done")
    if not nested:
        owner.node(pre, status="done")
    return out


def run_kpis_scenario(owner, sr, vp, scenario=False, spread_shift=0.0):
    from portfolio_risk.analytics.calibration import spread_oas
    state = owner.current_state()
    fixed = spread_oas(owner.base_calibration(state), spread_shift)
    return owner.run_kpis(sr, vp, fixed)


def run_kpis(owner, sr, vp, oas_by_book=None):
    state = owner.current_state()
    from portfolio_risk.analytics.kpis import compute_kpis
    bs = owner.balance_sheet(state)
    nii = owner.run_nii(sr, vp)
    oas_by_book = owner.base_calibration(state) if oas_by_book is None else oas_by_book
    owner.node("kpi:eve", parent="build", kind="solve", label="EVE & parallel dv01",
         status="running")
    owner.report(stage="EVE & parallel dv01", pct=65.0, log="full-reval EVE + dv01")
    out = compute_kpis(bs, sr, vp, state['dep_hist'],
                       nii_monthly=nii["monthly"], seed=state['settings'].seed, oas_by_book=oas_by_book)
    owner.node("kpi:eve", status="done")
    owner.node("kpi:ratios", parent="build", kind="solve", label="LCR / NSFR / CET1",
         status="running")
    owner.report(stage="LCR / NSFR / CET1", pct=90.0, log="liquidity + capital ratios")
    owner.node("kpi:ratios", status="done")
    out["nii_summary"] = nii["summary"]
    return out


def run_scenario_grid(owner, sc: MarketScenario):
    state = owner.current_state()
    """9Q scenario: revalue the books at each quarter's market and report
    value/NII drift along the named path (base OAS held fixed -- the
    engine's global invariant)."""
    rows = []
    owner.node("build", parent=None, kind="build", status="running",
         label="Position build", detail=f"{sc.name}: 9 quarters")
    for q in range(9):
        owner.node(f"q{q}", parent="build", kind="branch", label=f"Q{q + 1}",
             detail="revalue at scenario market")
    owner.node("build", status="done")
    for q in range(9):
        owner.node(f"q{q}", status="running")
        owner.report(stage=f"quarter {q + 1}/9", pct=4 + 92 * q / 9,
               scenario_paths=state['settings'].n_paths,
               log=f"Q{q + 1}: revalue at scenario market")
        sr, vp, spr = owner.apply_scenario(sc, q)
        nii = owner.run_nii(sr, vp, node_parent=f"q{q}", node_prefix=f"q{q}")
        owner.node(f"q{q}", status="done")
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
    return {"scenario": sc.name, "path": rows, "method": "independent instantaneous forecasts on the unchanged book; spread does not alter contractual NII"}


def prepare_forecast(owner, req):
    from fastapi import HTTPException
    from .market_data import get_snapshot
    from .forecast_data import DATASETS, normalized_rows
    from portfolio_risk.analytics.forecast import compile_forecast
    state = owner.snapshot()
    if state["revision"] != req.expected_revision:
        raise HTTPException(409, "Inputs changed. Reload before previewing or running a forecast.")
    try:
        source = get_snapshot(req.snapshot_id)
    except KeyError:
        raise HTTPException(404, "unknown research snapshot")
    if source["dataset"] not in DATASETS:
        raise HTTPException(422, "snapshot is not a supported forecast source")
    try:
        plan = compile_forecast(normalized_rows(source), req.scenario, str(req.start_period), req.horizon_months)
    except ValueError as exc:
        raise HTTPException(422, str(exc))
    preview = {"snapshot_id": source["id"], "dataset": source["dataset"],
               "source_as_of": source["as_of"], "book_as_of": str(state["asof"]),
               "revision": state["revision"], "scenario": req.scenario, "start_period": str(req.start_period),
               "alignment": req.alignment, "horizon_months": req.horizon_months,
               "warnings": source["warnings"] + plan["warnings"], "coverage": plan["coverage"],
               "unused_variables": plan["unused_variables"], "sources": source["sources"],
               "drivers": [{"month": m + 1, **{k: float(v[m]) for k, v in plan["targets"].items()}}
                           for m in range(req.horizon_months)]}
    return state, plan, preview


def run_forecast(owner, plan, provenance):
    from portfolio_risk.analytics.forecast import run_forecast_nii
    state = owner.current_state()
    owner.report(stage="base and conditional monthly cashflows", pct=10)
    result = run_forecast_nii(owner.balance_sheet(state), state["market"]["swap_rates"],
        state["market"]["vol_pts"], state["dep_hist"], plan, seed=state["settings"].seed)
    result["provenance"] = provenance
    return result
