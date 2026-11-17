"""Standard EVE/NII rate shocks (`rate-shock-1`).

Gates: native scenario construction and limit status against the independent
reference; full-revaluation EVE, anchored NII and ramp NII against the Python
reference engine on at least one scenario per metric; fixed base OAS; exact
zero-ramp identity; and that the production path calls no Python financial stage.
"""
import math
from dataclasses import replace

import numpy as np
import pytest

from portfolio_risk import demo
from portfolio_risk.analytics import accounting, calibration, rate_shocks, rate_shocks_reference
from portfolio_risk.core.config import SWAP_TENORS
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import RunConfig, run_context

pytestmark = pytest.mark.skipif(not library_path().is_file(), reason='build native products')
CONFIG = RunConfig(7, 9, 12, compute_backend='python')
RUST = replace(CONFIG, compute_backend='rust')


def forbidden(*args, **kwargs):
    raise AssertionError('Python financial stage called')


def policy(**changes):
    out = rate_shocks.default_policy() | dict(parallel_bp=[100., 200.], floor=0.0)
    return out | changes


@pytest.fixture(scope='module')
def inputs():
    bs = demo.model_balance_sheet(scale=.001, basis='amortized_cost', include_markets_bs=True)
    for key in ('mbs', 'loans', 'debt', 'deposits', 'cds'):
        bs[key] = bs[key].head(3)
    bs['hedges'] = demo.demo_hedge_book(scale=.0001, asof=bs['asof'])     # swaps and swaptions
    return bs, *demo.demo_market(), demo.demo_deposit_history()


def test_scenario_set_matches_the_independent_reference_and_hand_values():
    curve = [0.012, 0.02, 0.025, 0.03, 0.032, 0.035, 0.038, 0.04, 0.041, 0.042]
    p = policy(parallel_bp=[100., 200., 300., 400.])
    expected = rate_shocks_reference.rate_shock_set(p, SWAP_TENORS.tolist(), curve)
    with run_context(RUST):
        actual = rate_shocks.shock_set(p, curve)
    assert [s['name'] for s in actual['scenarios']] == [s['name'] for s in expected['scenarios']]
    assert len(actual['scenarios']) == 14
    for a, e in zip(actual['scenarios'], expected['scenarios']):
        assert (a['family'], a['timing'], a['floor_applied']) == (e['family'], e['timing'], e['floor_applied'])
        for key in ('swap_rates', 'requested_shift_bp', 'applied_shift_bp'):
            if key in e:
                np.testing.assert_allclose(a[key], e[key], rtol=1e-13, atol=1e-13)
        if e['timing'] == 'ramp':
            assert a['ramp']['months'] == 12
            assert a['ramp']['shift'] == pytest.approx(e['ramp']['shift'], abs=1e-15)
    by_name = {s['name']: s for s in actual['scenarios']}
    # Hand values: -200 bp from 1.2% stops at the 0% floor; the 30y pillar moves in full.
    assert by_name['parallel_down_200']['swap_rates'][0] == 0.0
    assert by_name['parallel_down_200']['swap_rates'][-1] == pytest.approx(0.022, abs=1e-15)
    # 10y steepener: -0.65*300*e^-2.5 + 0.9*150*(1 - e^-2.5).
    hand = -0.65 * 300 * math.exp(-2.5) + 0.9 * 150 * (1 - math.exp(-2.5))
    assert by_name['steepener']['requested_shift_bp'][6] == pytest.approx(hand, abs=1e-12)
    assert by_name['short_up']['requested_shift_bp'][0] == pytest.approx(300 * math.exp(-0.25), abs=1e-12)
    assert by_name['ramp_down_200']['ramp']['shift'] == pytest.approx(-0.012, abs=1e-15)
    assert by_name['ramp_down_200']['floor_applied'] is True
    # An unbound ramp carries the requested shift exactly and is not reported as floored.
    assert by_name['ramp_up_200']['ramp']['shift'] == 0.02 and by_name['ramp_up_200']['floor_applied'] is False


def test_limit_status_matches_the_reference_and_never_passes_without_a_limit():
    p = policy(parallel_bp=[200.], limits=[
        dict(metric='eve', scenario='parallel_up_200', applicability='internal_limit', max_decline_pct=10.),
        dict(metric='nii_12m', scenario='parallel_down_200', applicability='internal_limit', max_decline_pct=5.),
        dict(metric='nii_24m', scenario='short_up', applicability='internal_limit', max_decline_pct=5.),
        dict(metric='eve', scenario='ramp_up_200', applicability='internal_limit', max_decline_pct=5.),
        dict(metric='nii_12m', scenario='steepener', applicability='not_applicable', max_decline_pct=None)])
    names = rate_shocks_reference._names(p)
    base = dict(name='base', eve=1000., nii=[10.] * 18)
    outcomes = [dict(name=n, eve=None if n.startswith('ramp_') else {'parallel_up_200': 880.}.get(n, 1005.),
                     nii=[9.6 if n == 'parallel_down_200' else 10.1] * 18) for n in names]
    expected = rate_shocks_reference.rate_shock_report(p, base, outcomes)
    with run_context(RUST):
        actual = rate_shocks.shock_report(p, base, outcomes)
    assert actual['status'] == expected['status'] == 'breach'
    assert actual['counts'] == expected['counts']
    rows = {(r['scenario'], r['metric']): r for r in actual['rows']}
    for e in expected['rows']:
        a = rows[e['scenario'], e['metric']]
        assert (a['status'], a['applicability']) == (e['status'], e['applicability'])
        for key in ('base', 'value', 'delta', 'delta_pct', 'headroom_pct', 'max_decline_pct'):
            assert a[key] == pytest.approx(e[key], rel=1e-13, abs=1e-12) if e[key] is not None else a[key] is None
    assert rows['parallel_up_200', 'eve']['status'] == 'breach'                  # -12% against 10%
    assert rows['parallel_up_200', 'eve']['delta_pct'] == pytest.approx(-12.)
    assert rows['parallel_down_200', 'nii_12m']['status'] == 'within_limit'      # -4% against 5%
    assert rows['short_up', 'nii_24m']['status'] == 'unavailable'                # 18-month projection
    assert rows['ramp_up_200', 'eve']['status'] == 'unavailable'                 # ramps have no EVE
    assert rows['steepener', 'nii_12m']['status'] == 'not_applicable'
    assert rows['flattener', 'eve']['status'] == 'unconfigured'                  # +0.5% but no limit: not a pass


@pytest.mark.parametrize('change', [
    dict(schema='rate-shock-2'), dict(parallel_bp=[200., 100.]), dict(parallel_bp=[0.]), dict(ramp_months=0),
    dict(decay_years=0.), dict(floor=0.5), dict(unknown=1),
    dict(limits=[dict(metric='eve', scenario='parallel_up_300', applicability='internal_limit', max_decline_pct=5.)]),
    dict(limits=[dict(metric='eve', scenario='short_up', applicability='internal_limit', max_decline_pct=None)]),
    dict(limits=[dict(metric='eve', scenario='short_up', applicability='not_applicable', max_decline_pct=5.)]),
    dict(limits=[dict(metric='pv', scenario='short_up', applicability='internal_limit', max_decline_pct=5.)]),
    dict(limits=[dict(metric='eve', scenario='short_up', applicability='internal_limit', max_decline_pct=5.)] * 2)])
def test_malformed_policies_and_orphan_limits_are_rejected(change):
    with run_context(RUST), pytest.raises(ValueError):
        rate_shocks.shock_set(policy(**change), [0.03] * 10)


def test_report_matches_the_python_reference_engine_and_holds_base_oas(inputs, monkeypatch):
    bs, sr, vp, history = inputs
    p = policy(parallel_bp=[200.], floor=None, limits=[
        dict(metric='eve', scenario='parallel_up_200', applicability='internal_limit', max_decline_pct=0.01),
        dict(metric='nii_12m', scenario='ramp_down_200', applicability='internal_limit', max_decline_pct=99.)])
    with run_context(CONFIG):
        oas = calibration.calibrate_books(bs, sr, vp, history, seed=19)
        expected = rate_shocks.run_rate_shocks(bs, sr, vp, history, p, horizon=12, seed=19, oas_by_book=oas)
    seen = []
    # The production path must not refit OAS or call the Python reference stages.
    monkeypatch.setattr(calibration, 'calibrate_books', forbidden)
    for name in ('rate_shock_set', 'rate_shock_report'):
        monkeypatch.setattr(rate_shocks_reference, name, forbidden)
    from portfolio_risk.analytics import forecast
    monkeypatch.setattr(forecast, 'condition_paths', forbidden)
    monkeypatch.setattr(forecast, 'ramp_paths', forbidden)
    with run_context(RUST):
        actual = rate_shocks.run_rate_shocks(bs, sr, vp, history, p, horizon=12, seed=19, oas_by_book=oas,
                                             progress=lambda name, index, total: seen.append((name, index, total)))
    names = [s for s in actual['scenarios']['scenario']]
    assert names == ['parallel_up_200', 'parallel_down_200', 'short_up', 'short_down', 'steepener', 'flattener',
                     'ramp_up_200', 'ramp_down_200']
    assert seen == [(n, i, 9) for i, n in enumerate(['base'] + names)]
    a = {(r['scenario'], r['metric']): r for r in actual['rows'].iter_rows(named=True)}
    e = {(r['scenario'], r['metric']): r for r in expected['rows'].iter_rows(named=True)}
    assert a.keys() == e.keys() and len(a) == 24
    scale = abs(expected['base_eve'])
    for key, row in e.items():
        for field in ('base', 'value', 'delta'):
            if row[field] is None:
                assert a[key][field] is None
            else:
                # Same bound as the native/reference accounting and KPI parity gates.
                bound = 1e-7 * (scale if key[1] == 'eve' else abs(e[key]['base'])) + 1e-5
                assert abs(a[key][field] - row[field]) <= bound, (key, field)
        assert a[key]['status'] == row['status'], key
    # One scenario per metric moves materially, so the comparison is not vacuous.
    assert abs(e['parallel_up_200', 'eve']['delta']) > 1e-4 * scale
    assert abs(e['parallel_up_200', 'nii_12m']['delta']) > 1e-4 * abs(e['parallel_up_200', 'nii_12m']['base'])
    assert abs(e['ramp_down_200', 'nii_12m']['delta']) > 1e-4 * abs(e['ramp_down_200', 'nii_12m']['base'])
    assert a['parallel_up_200', 'nii_24m']['status'] == 'unconfigured' and a['parallel_up_200', 'nii_24m']['value'] is None
    assert a['ramp_up_200', 'eve']['value'] is None
    assert actual['status'] == expected['status']
    assert actual['nii_monthly'].columns == ['month', 'base'] + names
    # EVE includes the hedge book: the base equals the KPI board level plus the hedge mark-to-market.
    from portfolio_risk.analytics.kpis import compute_kpis
    with run_context(RUST):
        board = compute_kpis(bs, sr, vp, history, seed=19, oas_by_book=oas)['eve']['eve_$']
    assert bs['hedges'] is not None and abs(actual['base_hedge_value']) > 1e-6 * scale
    assert actual['base_eve'] == pytest.approx(board + actual['base_hedge_value'], rel=1e-14)
    assert actual['base_hedge_value'] == pytest.approx(expected['base_hedge_value'], rel=1e-7, abs=1e-5)


def test_a_scenario_outside_the_model_domain_is_unavailable_and_never_a_pass(inputs, monkeypatch):
    bs, sr, vp, history = inputs
    assert 0.03 < float(np.min(sr)) and float(np.max(sr)) < 0.06
    # -600 bp floored at 0% puts every pillar on exactly zero, which the volatility calibration refuses.
    p = policy(parallel_bp=[600.], ramp_bp=[], floor=0.0, limits=[
        dict(metric='eve', scenario='parallel_down_600', applicability='internal_limit', max_decline_pct=50.),
        dict(metric='nii_12m', scenario='parallel_up_600', applicability='internal_limit', max_decline_pct=99.)])
    with run_context(RUST):
        oas = calibration.calibrate_books(bs, sr, vp, history, seed=19)
        out = rate_shocks.run_rate_shocks(bs, sr, vp, history, p, horizon=12, seed=19, oas_by_book=oas)
    rows = {(r['scenario'], r['metric']): r for r in out['rows'].iter_rows(named=True)}
    for metric in rate_shocks.METRICS:
        row = rows['parallel_down_600', metric]
        assert row['value'] is None and row['delta'] is None
        assert row['unavailable_reason'].startswith('the model could not value this scenario: ')
    assert rows['parallel_down_600', 'eve']['status'] == 'unavailable'
    assert rows['parallel_down_600', 'nii_12m']['status'] == 'unconfigured'
    assert rows['parallel_up_600', 'nii_12m']['status'] == 'within_limit'     # other scenarios still report
    assert out['status'] == 'unavailable'
    evaluated = dict(zip(out['scenarios']['scenario'], out['scenarios']['evaluated']))
    assert evaluated['parallel_down_600'] is False and evaluated['parallel_up_600'] is True
    assert out['nii_monthly']['parallel_down_600'].null_count() == 12
    # Cancellation and deadlines are not scenario results: they stop the report.
    calls = []

    def cancelled(*args, **kwargs):
        calls.append(1)
        if len(calls) > 1:
            raise ValueError('Rust term computation failed: COMPUTE_CANCELLED; no fallback was used')
        return original(*args, **kwargs)
    original = accounting.run_balance_sheet_nii
    monkeypatch.setattr(accounting, 'run_balance_sheet_nii', cancelled)
    with run_context(RUST), pytest.raises(ValueError, match='COMPUTE_CANCELLED'):
        rate_shocks.run_rate_shocks(bs, sr, vp, history, p, horizon=12, seed=19, oas_by_book=oas)


def test_zero_ramp_and_zero_shock_reproduce_the_anchored_base_exactly(inputs):
    bs, sr, vp, history = inputs
    with run_context(RUST):
        base = accounting.run_balance_sheet_nii(bs, sr, vp, history, horizon=12, seed=19, asof=bs['asof'],
                                                capture_anchor=True)
        anchor = base.pop('accounting_anchor')
        anchored = accounting.run_balance_sheet_nii(bs, sr, vp, history, horizon=12, seed=19, asof=bs['asof'],
                                                    accounting_anchor=anchor)
        zero = accounting.run_balance_sheet_nii(bs, sr, vp, history, horizon=12, seed=19, asof=bs['asof'],
                                                forecast_plan=dict(ramp=dict(shift=0., months=12)),
                                                accounting_anchor=anchor)
        moved = accounting.run_balance_sheet_nii(bs, sr, vp, history, horizon=12, seed=19, asof=bs['asof'],
                                                 forecast_plan=dict(ramp=dict(shift=.02, months=12)),
                                                 accounting_anchor=anchor)
        with pytest.raises(ValueError):
            accounting.run_balance_sheet_nii(bs, sr, vp, history, horizon=12, seed=19, asof=bs['asof'],
                                             forecast_plan=dict(ramp=dict(shift=.02, months=0)),
                                             accounting_anchor=anchor)
        with pytest.raises(ValueError):       # paths may not cross the model rate domain
            accounting.run_balance_sheet_nii(bs, sr, vp, history, horizon=12, seed=19, asof=bs['asof'],
                                             forecast_plan=dict(ramp=dict(shift=-.15, months=12)),
                                             accounting_anchor=anchor)
    assert zero['monthly'].equals(anchored['monthly'])
    np.testing.assert_allclose(anchored['monthly']['nii'], base['monthly']['nii'], rtol=1e-9, atol=1e-6)
    # A ramp leaves month one nearly unchanged and moves later months more.
    delta = (moved['monthly']['nii'] - anchored['monthly']['nii']).abs().to_numpy()
    assert delta[-1] > 5 * delta[0] and delta[-1] > 0


def test_a_forecast_with_both_a_ramp_and_targets_is_rejected_not_truncated():
    from portfolio_risk.analytics.forecast import condition_paths
    from portfolio_risk.core.lifecycle_native import native_forecast
    plan = dict(ramp=dict(shift=.02, months=12), targets=dict(short_rate=[.03]*12))
    with pytest.raises(ValueError, match='mutually exclusive'):
        condition_paths({}, plan)
    assert set(native_forecast(plan)) == {'ramp', 'targets'}                 # the engine sees, and rejects, both
    assert native_forecast(dict(ramp=plan['ramp'], targets={})) == dict(ramp=plan['ramp'])
    assert set(native_forecast(dict(targets=plan['targets']))) == {'targets'}
