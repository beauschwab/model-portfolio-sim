"""Standard EVE/NII rate-shock report (`rate-shock-1`).

Rust owns the scenario set, deltas and limit status; this module sequences the
existing native revaluation and income runs. Base OAS is solved once on the base
market and held, every scenario reuses the base draws, and accounting yields are
anchored to the base run so a shock never rewrites the purchase yield of the
existing book. Ramps are income-only path shifts and never produce an EVE.
"""
import copy

import numpy as np
import polars as pl

from ..core.config import SWAP_TENORS

SCHEMA = 'rate-shock-1'
METRICS = ('eve', 'nii_12m', 'nii_24m')

# Conventional supervisory shapes; every number is a reviewable policy input.
DEFAULT_POLICY = dict(schema=SCHEMA, parallel_bp=[100., 200., 300., 400.], ramp_bp=[200.], ramp_months=12,
                      short_bp=300., long_bp=150., decay_years=4., steepener=[-0.65, 0.9],
                      flattener=[0.8, -0.6], floor=None, limits=[])

ROW_SCHEMA = dict(scenario=pl.String, metric=pl.String, base=pl.Float64, value=pl.Float64, delta=pl.Float64,
                  delta_pct=pl.Float64, applicability=pl.String, max_decline_pct=pl.Float64,
                  headroom_pct=pl.Float64, status=pl.String, unavailable_reason=pl.String)


def default_policy():
    return copy.deepcopy(DEFAULT_POLICY)


def _controller(op, **request):
    from ..core.quant_native import enabled, term_call
    if enabled():
        return term_call('financial-controller-1', dict(op=op, **request))
    from . import rate_shocks_reference as reference
    return getattr(reference, op)(**request)


def _reason(exc):
    text = str(exc).removeprefix('Rust term computation failed: ').removesuffix('; no fallback was used')
    return f'the model could not value this scenario: {text}'[:240]


def shock_set(policy, swap_rates):
    """Scenario curves and ramps for a par swap curve; validates the policy and its limits."""
    return _controller('rate_shock_set', policy=policy, tenors=SWAP_TENORS.tolist(),
                       swap_rates=np.asarray(swap_rates, dtype=float).tolist())


def shock_report(policy, base, outcomes):
    return _controller('rate_shock_report', policy=policy, base=base, outcomes=outcomes)


def run_rate_shocks(bs, swap_rates, vol_pts, dep_hist, policy, *, horizon, seed, oas_by_book, progress=None):
    """Full-revaluation EVE and anchored NII for the policy's scenario set.

    `oas_by_book` is the base calibration; it is never refitted here. `progress(name, index, total)`
    is called before each scenario."""
    from .accounting import run_balance_sheet_nii
    from .kpis import compute_kpis
    shocks = shock_set(policy, swap_rates)
    scenarios = shocks['scenarios']
    asof = bs.get('asof')

    def hedge_value(curve):
        if bs.get('hedges') is None:
            return 0.
        from ..products.hedges import run_hedge_risk
        swaps, swaptions = bs['hedges']
        return float(run_hedge_risk(swaps, swaptions, asof, curve, vol_pts, seed=seed, horizon=horizon)['mtm_total_$'])

    def eve(curve):
        # The KPI board's EVE level leaves the hedge book out; a shock must include its value change.
        books = float(compute_kpis(bs, curve, vol_pts, dep_hist, seed=seed, oas_by_book=oas_by_book)['eve']['eve_$'])
        return books + hedge_value(curve)

    if progress:
        progress('base', 0, len(scenarios) + 1)
    base_nii = run_balance_sheet_nii(bs, swap_rates, vol_pts, dep_hist, horizon=horizon, seed=seed, asof=asof,
                                     capture_anchor=True)
    anchor = base_nii.pop('accounting_anchor')
    base = dict(name='base', eve=eve(swap_rates), nii=base_nii['monthly']['nii'].to_list())
    outcomes, monthly, reasons = [], {'month': base_nii['monthly']['month'], 'base': base_nii['monthly']['nii']}, {}
    for index, scenario in enumerate(scenarios):
        name = scenario['name']
        if progress:
            progress(name, index + 1, len(scenarios) + 1)
        try:
            if scenario['timing'] == 'ramp':
                nii = run_balance_sheet_nii(bs, swap_rates, vol_pts, dep_hist, horizon=horizon, seed=seed, asof=asof,
                                            forecast_plan=dict(ramp=scenario['ramp']), accounting_anchor=anchor)
                value = None
            else:
                curve = np.asarray(scenario['swap_rates'], dtype=float)
                nii = run_balance_sheet_nii(bs, curve, vol_pts, dep_hist, horizon=horizon, seed=seed, asof=asof,
                                            accounting_anchor=anchor)
                value = eve(curve)
        except ValueError as exc:
            # A shocked market outside the model's domain is reported, never clipped or replaced.
            # Cancellation and deadlines are not scenario results and always propagate.
            if 'COMPUTE_CANCELLED' in str(exc) or 'COMPUTE_DEADLINE_EXCEEDED' in str(exc):
                raise
            reasons[name] = _reason(exc)
            monthly[name] = pl.Series(name, [None] * horizon, dtype=pl.Float64)
            outcomes.append(dict(name=name, eve=None, nii=[], unavailable=reasons[name]))
            continue
        series = nii['monthly']['nii']
        monthly[name] = series
        outcomes.append(dict(name=name, eve=value, nii=series.to_list()))
    report = shock_report(policy, base, outcomes)
    definitions = pl.DataFrame([dict(scenario=s['name'], family=s['family'], timing=s['timing'],
                                     floor_applied=s['floor_applied'], evaluated=s['name'] not in reasons,
                                     **{f'shift_{int(t)}y_bp': float(v) for t, v in zip(
                                         SWAP_TENORS, s['applied_shift_bp'] if s['timing'] != 'ramp'
                                         else [s['applied_shift_bp']] * len(SWAP_TENORS))})
                                for s in scenarios])
    return dict(schema=SCHEMA, status=report['status'], counts=report['counts'],
                rows=pl.DataFrame(report['rows'], schema=ROW_SCHEMA), scenarios=definitions,
                nii_monthly=pl.DataFrame(monthly), conventions=shocks['conventions'], policy=policy,
                horizon_months=horizon, base_eve=base['eve'], base_hedge_value=hedge_value(swap_rates))
