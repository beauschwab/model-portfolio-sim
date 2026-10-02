"""Pricing jobs; the caller supplies the sole state owner.

Functions transport/format engine outputs. Financial calculations stay in Rust.
"""
from __future__ import annotations

import numpy as np
import polars as pl

def apply_scenario(owner, sc: MarketScenario, quarter: int, state=None
                   ) -> tuple[np.ndarray, np.ndarray, float]:
    """Map trader-space legs onto engine inputs at a given quarter:
    10y leg shifts all pillars in parallel; 2s10s leg twists linearly
    around the 5y pivot (2y -x/2, 10y +x/2); vol leg shifts the surface;
    spread leg is returned for OAS-level application by the caller."""
    def leg(xs: list[float]) -> float:
        if not xs:
            return 0.0
        return xs[min(quarter, len(xs) - 1)] * 1e-4

    state = state or owner.current_state()
    sr = state['market']["swap_rates"].copy()
    tens = np.array([1, 2, 3, 4, 5, 7, 10, 15, 20, 30], dtype=float)
    sr = sr + leg(sc.ust10y_bp)
    tw = leg(sc.twos_tens_bp)
    if tw:
        sr = sr + tw * (np.clip(tens, 2.0, 10.0) - 5.0) / 8.0  # 2s10s pivot 5y
    vp = state['market']["vol_pts"].copy()
    vleg = leg(sc.vol_bp)
    if vleg:
        vp = vp.copy()
        vp[:, 2] = np.maximum(vp[:, 2] + vleg, 1e-4)
    return sr, vp, leg(sc.spread_bp)


def pricing_scope(owner, state, books, overrides):
    """Validate scope/IDs before queueing; numerical validation is engine-owned."""
    from portfolio_risk.analytics.incremental import SUPPORTED_BOOKS
    selected = list(SUPPORTED_BOOKS) if books is None else books
    if set(selected) - set(SUPPORTED_BOOKS):
        raise ValueError("pricing supports mbs, loans, debt, deposits and cds; money markets and hedges are not included")
    if set(overrides) - set(selected):
        raise ValueError("spread overrides must refer to selected books")
    for name, values in overrides.items():
        frame = state['books'][name]
        ids = frame['cusip' if name == 'mbs' else 'id'].to_list()
        if set(values) - set(ids):
            raise ValueError(f"{name}: spread override refers to an unknown position")
    return selected


def run_pricing(owner, books, sr, vp, spread_shift=0.0, overrides=None, options=None):
    from portfolio_risk.analytics.incremental import price_books
    from portfolio_risk.analytics.whatif import compare_books
    from portfolio_risk.core.dependency import DependencyCache
    from portfolio_risk.core.runtime import RunConfig
    state = owner.current_state()
    options = options or {}
    state['settings'].require_production_backend()
    if options.get('backend', 'rust') != 'rust':
        raise ValueError('PYTHON_BACKEND_DEPRECATED: production pricing requires the native graph')
    with owner._LOCK:
        if "cache" not in owner.PRICING:
            # Mixed-book curve risk retains ~179 MiB at 128 paths. Keep the
            # interactive working set, with explicit byte AND node ceilings.
            owner.PRICING["cache"] = DependencyCache(max_bytes=512 * 1024 * 1024, max_entries=500_000)
        cache = owner.PRICING["cache"]
    settings = state['settings']
    config = RunConfig(settings.n_paths, settings.n_paths_base, settings.horizon_months,
                       state['assumptions'].get('deposit_segments'),
                       tuple(state['assumptions'].get('cd_ew_params', [])) or None, compute_backend=settings.compute_backend)
    owner.report(stage="resolving pricing dependencies", pct=5.0)
    compare = options.get('compare', False)
    runner = compare_books if compare else price_books
    extra = ({'assumption_overrides': options.get('assumption_overrides', {}),
              'calibration_mode': options.get('calibration_mode', 'hold')} if compare else {})
    result = runner({b: state['books'][b] for b in books}, asof=state['asof'],
        swap_rates=state['market']['swap_rates'], vol_pts=state['market']['vol_pts'],
        cache=cache, config=config, seed=settings.seed, mbs_hists=state['mbs_hists'],
        dep_hist=state['dep_hist'], scenario_market=(sr, vp), spread_shift=spread_shift,
        spread_overrides_bp=overrides, backend=None,
        include_analytics=options.get('include_analytics', False),
        balance_sheet_extras=({'mm': state['books'].get('mm'), 'hedges': state['hedges'], 'equity': state['equity']}
                              if set(books) == {'mbs', 'loans', 'debt', 'deposits', 'cds'} else None), **extra)
    result['revision'] = state['revision']
    graphs = [result['baseline']['graph'], result['revised']['graph']] if compare else [result['graph']]
    owner.report(stage="priced selected books", pct=99.0,
           pricing_nodes_computed=sum(s['computed'] for g in graphs for s in g.values()),
           pricing_nodes_reused=sum(s['reused'] for g in graphs for s in g.values()))
    return result


def run_risk_all(owner, books: list[str], sr, vp, spread_shift: float = 0.0, scenario=False):
    state = owner.current_state()
    from portfolio_risk import run_cd_risk, run_corp_risk, run_deposit_risk
    from portfolio_risk.analytics.risk import run_risk
    from portfolio_risk.analytics.calibration import spread_oas
    fixed = spread_oas(owner.base_calibration(state), spread_shift) if scenario else {}
    books = [b for b in books if len(state['books'].get(b, []))]
    out = {}
    order = [b for b in ("mbs", "loans", "debt", "deposits", "cds") if b in books]
    total = max(len(order), 1)
    done = 0
    factor = 1 + 2 + 2 * owner.KRD_PILLARS              # base + parallel dv01 + KRD pillars
    _oas_books = {"mbs", "loans", "debt", "cds"}  # books priced via an OAS solve
    npaths = state['settings'].n_paths

    # declare the pipeline skeleton: build -> branch per book -> paths/cashflow/oas
    owner.node("build", parent=None, kind="build", status="running",
         label="Position build", detail=f"{total} books in scope")
    for b in order:
        owner.node(f"book:{b}", parent="build", kind="branch", label=b.upper(),
             detail=f"{int(len(state['books'][b]))} positions \u00b7 {factor} bump scenarios")
        owner.node(f"{b}:paths", parent=f"book:{b}", kind="paths", label="rate paths")
        owner.node(f"{b}:cf", parent=f"book:{b}", kind="cashflow", label="cashflow engine")
        if b in _oas_books:
            owner.node(f"{b}:oas", parent=f"book:{b}", kind="oas", label="OAS solve")

    owner.report(stage="seeding CRN", pct=3.0,
           log=f"CRN draws: {npaths} paths, seed {state['settings'].seed}")
    owner.node("build", status="done", records=sum(int(len(state['books'][b])) for b in order))

    def _start(book: str):
        n = int(len(state['books'][book]))
        owner.node(f"book:{book}", status="running")
        owner.node(f"{book}:paths", status="running")
        owner.node(f"{book}:paths", status="done", paths=npaths)
        owner.node(f"{book}:cf", status="running")
        owner.report(paths_generated=npaths)

    def _did(book: str):
        nonlocal done
        done += 1
        n = int(len(state['books'][book]))
        rv = n * factor
        cf = n * factor * npaths
        owner.node(f"{book}:cf", status="done", calcs=cf)
        owner.report(cashflow_calcs=cf)
        if book in _oas_books:
            owner.node(f"{book}:oas", status="running")
            owner.node(f"{book}:oas", status="done", calcs=n * factor)
            owner.report(oas_calcs=n * factor)
        owner.node(f"book:{book}", status="done")
        owner.report(stage=f"revalued {book}", pct=3 + 94 * done / total,
               records=n, revaluations=rv,
               path_evaluations=rv * npaths, reductions=rv,
               books_done=1, log=f"{book}: {n} positions \u2192 {rv} revaluations")

    if "mbs" in books:
        owner.report(stage="revaluing mbs", log="mbs: OAS-held dv01 + KRD by pillar")
        _start("mbs")
        out["mbs"] = run_risk(state['books']["mbs"], sr, vp, *state['mbs_hists'],
                              seed=state['settings'].seed, oas=fixed.get("mbs"))
        _did("mbs")
    if "loans" in books:
        owner.report(stage="revaluing loans")
        _start("loans")
        out["loans"] = run_corp_risk(state['books']["loans"], state['asof'], sr, vp,
                                     *state['mbs_hists'], seed=state['settings'].seed, oas=fixed.get("loans"))
        _did("loans")
    if "debt" in books:
        owner.report(stage="revaluing debt")
        _start("debt")
        out["debt"] = run_corp_risk(state['books']["debt"], state['asof'], sr, vp,
                                    *state['mbs_hists'], seed=state['settings'].seed, oas=fixed.get("debt"))
        _did("debt")
    if "deposits" in books:
        owner.report(stage="revaluing deposits")
        _start("deposits")
        out["deposits"] = run_deposit_risk(state['books']["deposits"], sr, vp,
                                           state['dep_hist'], seed=state['settings'].seed, oas=fixed.get("deposits"))
        _did("deposits")
    if "cds" in books:
        owner.report(stage="revaluing cds")
        _start("cds")
        out["cds"] = run_cd_risk(state['books']["cds"], state['asof'], sr, vp,
                                 seed=state['settings'].seed, oas=fixed.get("cds"))
        _did("cds")
    if state['hedges'] is not None and {"mbs", "loans", "debt", "deposits", "cds"}.issubset(books):
        from portfolio_risk.products.hedges import run_hedge_risk
        swaps, swaptions = state['hedges']
        hedge = run_hedge_risk(swaps, swaptions, state['asof'], sr, vp,
                               seed=state['settings'].seed, horizon=state['settings'].horizon_months)
        out["hedges"] = pl.DataFrame([{ "dv01": hedge["book_dv01_$"], **hedge["book_krd"] }])
    return out


def run_stress_all(owner, books, sr, vp, scenario=False, spread_shift=0.0):
    state = owner.current_state()
    from portfolio_risk import run_deposit_stress
    from portfolio_risk.analytics.stress import run_stress
    out = {}
    from portfolio_risk.analytics.calibration import spread_oas
    fixed = spread_oas(owner.base_calibration(state), spread_shift) if scenario else {}
    books = [b for b in books if len(state['books'].get(b, []))]
    shocks = tuple(state['settings'].shocks_bp)
    factor = 1 + len(shocks)
    npaths = state['settings'].n_paths
    owner.node("build", parent=None, kind="build", status="running",
         label="Position build", detail=f"{len(shocks)} forward shocks")
    owner.report(stage="seeding CRN", pct=4.0,
           log=f"{len(shocks)} forward shocks: {list(shocks)} bp")
    owner.node("build", status="done")

    def _stress_book(book: str, runner, p_run: float, p_done: float):
        n = int(len(state['books'][book]))
        owner.node(f"book:{book}", parent="build", kind="branch", label=book.upper(),
             detail=f"{n} positions \u00d7 {factor} states", status="running")
        owner.node(f"{book}:paths", parent=f"book:{book}", kind="paths",
             label="forward shock paths", status="running")
        owner.node(f"{book}:paths", status="done", paths=npaths)
        owner.report(paths_generated=npaths)
        owner.node(f"{book}:cf", parent=f"book:{book}", kind="cashflow",
             label="stress cashflow engine", status="running")
        owner.report(stage=f"stressing {book}", pct=p_run)
        res = runner(n)
        cf = n * factor * npaths
        owner.node(f"{book}:cf", status="done", calcs=cf)
        owner.node(f"book:{book}", status="done")
        owner.report(stage=f"stressed {book}", pct=p_done, records=n,
               revaluations=n * factor, path_evaluations=cf,
               reductions=n * factor, books_done=1, cashflow_calcs=cf,
               log=f"{book}: {n} positions \u00d7 {factor} states")
        return res

    if "mbs" in books:
        def _run_mbs(_n):
            pos, agg, prof = run_stress(state['books']["mbs"], sr, vp, *state['mbs_hists'],
                                        shocks_bp=shocks, seed=state['settings'].seed, oas=fixed.get("mbs"))
            out["mbs"] = {"agg": agg, "profile": prof}
        _stress_book("mbs", _run_mbs, 20.0, 60.0)
    if "deposits" in books:
        def _run_dep(_n):
            pos, agg, prof = run_deposit_stress(
                state['books']["deposits"], sr, vp, state['dep_hist'],
                shocks_bp=shocks, seed=state['settings'].seed, oas=fixed.get("deposits"))
            out["deposits"] = {"agg": agg, "profile": prof}
        _stress_book("deposits", _run_dep, 70.0, 96.0)
    return out
