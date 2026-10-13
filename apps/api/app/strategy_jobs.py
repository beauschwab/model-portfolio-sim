"""Strategy jobs; the caller supplies the sole state owner.

Functions transport/format engine outputs. Financial calculations stay in Rust.
"""
from __future__ import annotations



def build_unitlib_job(owner, sr, vp):
    state = owner.current_state()
    from portfolio_risk.strategy.unitlib import build_unit_library
    npaths = state['settings'].n_paths
    owner.node("build", parent=None, kind="build", status="running",
         label="Position build")
    owner.node("unit:lib", parent="build", kind="cashflow", label="unit library",
         detail="template unit tensor", status="running")
    owner.report(stage="building unit library", pct=4.0, scenario_paths=npaths,
           paths_generated=npaths, log="template unit tensor")
    lib = build_unit_library(sr, vp, state['mbs_hists'], state['dep_hist'],
                             seed=state['settings'].seed, horizon=state['settings'].horizon_months,
                             asof=state['asof'])
    n_units = len(lib["units"])
    owner.node("unit:lib", status="done", calcs=n_units * npaths, units=n_units)
    owner.report(unit_columns=n_units, cashflow_calcs=n_units * npaths)
    base = owner.run_kpis(sr, vp)
    with owner._LOCK:
        from . import persistence
        persistence.assert_current(state['revision'])
        if owner.STATE_META["revision"] != state['revision']:
            raise RuntimeError("inputs changed while building; rebuild the strategy library")
        owner.CACHE.update(revision=state['revision'], library=lib, base=base, compute_backend=state['settings'].compute_backend)
        owner.UNITLIB, owner.BASE_KPIS = lib, base
    return {"units": n_units, "templates": list(lib["templates"]), "grid_m": lib["grid_m"],
            "horizon_months": lib["horizon"], "revision": state['revision']}


def eval_strategy_sync(owner, allocations: list[dict]):
    from portfolio_risk.strategy.unitlib import evaluate_strategy
    from portfolio_risk.core.runtime import RunConfig, run_context
    with owner._LOCK:
        from portfolio_risk.strategy.unitlib import TEMPLATES
        known=(owner.CACHE.get('library',{}).get('templates',{}) if owner.CACHE else TEMPLATES)
        if any(row['template'] not in known for row in allocations):
            raise ValueError('unknown template in the selected allocation library')
        if not owner.CACHE or owner.CACHE["revision"] != owner.STATE_META["revision"]:
            raise RuntimeError("REBUILD_REQUIRED: build the unit library for the current inputs")
        lib, base, revision = owner.CACHE["library"], owner.CACHE["base"], owner.CACHE["revision"]
        backend = owner.CACHE.get('compute_backend', owner.SETTINGS.compute_backend)
        if backend != 'rust':
            raise RuntimeError('PYTHON_BACKEND_DEPRECATED: rebuild the unit library with Rust')
    with run_context(RunConfig(compute_backend=backend)):
        out = evaluate_strategy(lib, allocations, base_kpis=base)
    out["revision"] = revision
    return out


def run_strategy_job(owner, sr, vp):
    state = owner.current_state()
    from portfolio_risk.strategy.strategies import run_strategies
    nii = owner.run_nii(sr, vp)
    return run_strategies(list(state['programs'].values()), sr, vp,
                          runoff_by_book=nii["runoff_vectors"],
                          horizon=state['settings'].horizon_months,
                          seed=state['settings'].seed)


def run_optimize_job(owner, sr, vp, opt: dict):
    state = owner.current_state()
    from portfolio_risk.strategy.optimizer import optimize_balance_sheet
    from portfolio_risk.strategy.unitlib import build_unit_library
    from portfolio_risk.analytics.kpis import compute_kpis
    npaths = state['settings'].n_paths
    owner.node("build", parent=None, kind="build", status="running",
         label="Position build")
    owner.node("opt:base", parent="build", kind="branch", label="Base NII objective",
         status="running")
    owner.report(stage="base NII path", pct=4.0,
           scenario_paths=npaths,
           log="base market NII path for the objective")
    nii = owner.run_nii(sr, vp, node_parent="opt:base", node_prefix="opt:base")
    owner.node("opt:base", status="done")
    owner.node("build", status="done")
    scen_libs = []
    markets = [("base", sr, vp, 0.0)]
    for name in opt.get("scenarios", []):
        sc = state['scenarios'].get(name)
        if sc:
            s2, v2, spread = owner.apply_scenario(sc, 0)
            markets.append((name, s2, v2, spread))
    bs = owner.balance_sheet(state)
    fixed = owner.base_calibration(state)
    total = max(len(markets), 1)
    span = 84.0 / total                               # 8% .. 92% across markets
    for i, (name, s2, v2, spread) in enumerate(markets):
        base = 8.0 + span * i
        owner.node(f"mkt:{name}", parent="build", kind="branch",
             label=f"market: {name}", detail=f"unit tensor ({i + 1}/{total})",
             status="running")
        owner.node(f"mkt:{name}:paths", parent=f"mkt:{name}", kind="paths",
             label="LMM paths", status="running")
        owner.node(f"mkt:{name}:paths", status="done", paths=npaths)
        owner.node(f"mkt:{name}:cf", parent=f"mkt:{name}", kind="cashflow",
             label="unit library", status="running")
        owner.report(stage=f"unit library: {name}", pct=base,
               scenario_paths=npaths, paths_generated=npaths,
               log=f"building unit tensor under {name} market ({i + 1}/{total})")
        lib = build_unit_library(s2, v2, state['mbs_hists'], state['dep_hist'],
                                 seed=state['settings'].seed, horizon=state['settings'].horizon_months, asof=state['asof'])
        n_units = len(lib.get("units", [])) if isinstance(lib, dict) else 0
        owner.node(f"mkt:{name}:cf", status="done", calcs=n_units * npaths)
        owner.node(f"mkt:{name}:kpi", parent=f"mkt:{name}", kind="solve",
             label="KPIs", status="running")
        owner.report(stage=f"KPIs: {name}", pct=base + span * 0.6,
               revaluations=n_units, reductions=n_units,
               unit_columns=n_units, cashflow_calcs=n_units * npaths,
               oas_calcs=n_units,
               log=f"{name}: {n_units} unit columns priced")
        scenario_nii = nii if name == "base" else owner.run_nii(s2, v2, node_parent=f"mkt:{name}", node_prefix=f"opt:{name}:nii")
        from portfolio_risk.analytics.calibration import spread_oas
        base = compute_kpis(bs, s2, v2, state['dep_hist'], nii_monthly=scenario_nii["monthly"],
                            seed=state['settings'].seed, oas_by_book=spread_oas(fixed, spread))
        scen_libs.append((lib, base))
        owner.node(f"mkt:{name}:kpi", status="done", calcs=n_units)
        owner.node(f"mkt:{name}", status="done")
    owner.node("opt:lp", parent="build", kind="solve", label="LP maximin solve",
         detail=f"{total} markets", status="running")
    owner.report(stage="solving LP", pct=94.0,
           log=f"maximin LP across {total} markets")
    out = optimize_balance_sheet(
        scen_libs, lcr_min=opt.get("lcr_min", 1.10),
        nsfr_min=opt.get("nsfr_min", 1.05),
        cet1_min=opt.get("cet1_min", 0.10),
        eve_limit=opt.get("eve_limit", 0.15),
        commercial=opt.get("commercial"),
        max_total_assets=opt.get("max_total_assets"), cash_budget=opt.get("cash_budget", 0.0),
        capital_limits=opt.get("capital_limits", []))
    nb = len(out.get("binding_constraints", []) or []) if isinstance(out, dict) else 0
    owner.node("opt:lp", status="done", detail=f"{nb} binding constraints")
    owner.report(stage="LP solved", pct=99.0, binding_constraints=nb,
           log=f"{'feasible' if isinstance(out, dict) and out.get('feasible') else 'infeasible'} \u00b7 {nb} binding")
    return out

