"""Independent reference calculations and end-to-end temporary assumption gates."""
from dataclasses import replace
import numba
import numpy as np
import polars as pl
import pytest
from portfolio_risk.analytics.whatif import apply_overrides, compare_books
from portfolio_risk.analytics.incremental import price_books, SUPPORTED_BOOKS
from portfolio_risk.core.dependency import DependencyCache
from portfolio_risk.core.runtime import RunConfig, run_context


@pytest.fixture(scope='module', params=['python', 'rust'])
def inputs(request):
    from portfolio_risk.core.native import library_path
    if request.param == "rust" and not library_path().is_file():
        pytest.skip("build native product backend")
    from portfolio_risk.demo import model_balance_sheet, demo_market, demo_deposit_history
    old = numba.get_num_threads()
    numba.set_num_threads(min(2, numba.config.NUMBA_NUM_THREADS))
    bs = model_balance_sheet(scale=.001, basis='amortized_cost', include_markets_bs=True)
    sr, vp = demo_market()
    yield dict(books={k: bs[k].head(2) for k in SUPPORTED_BOOKS}, asof=bs['asof'],
               swap_rates=sr, vol_pts=vp, mbs_hists=bs['mbs_hists'], dep_hist=demo_deposit_history(),
               seed=7, config=RunConfig(32, 32, 6, compute_backend=request.param))
    numba.set_num_threads(old)


def test_assumptions_hold_oas_and_explicit_recalibration(inputs):
    ident = inputs['books']['loans']['id'][0]
    coupon = float(inputs['books']['loans']['coupon_or_spread'][0])
    original = inputs['books']['loans'].clone()
    patch = {'loans': {ident: {'coupon_or_spread': coupon + .01}}}
    cache = DependencyCache()
    out = compare_books(**inputs, cache=cache, assumption_overrides=patch)
    a, b = out['baseline']['positions']['loans'], out['revised']['positions']['loans']
    np.testing.assert_array_equal(a['base_oas_bp'], b['base_oas_bp'])
    assert b['model_price'][0] > a['model_price'][0]
    assert inputs['books']['loans'].equals(original)
    assert out['revised']['graph']['cashflows:loans']['computed'] == 1
    rebased = compare_books(**inputs, cache=cache, assumption_overrides=patch, calibration_mode='recalibrate')
    np.testing.assert_allclose(rebased['revised']['positions']['loans']['model_price'], original['price'], atol=1e-6)
    assert rebased['revised']['positions']['loans']['base_oas_bp'][0] != b['base_oas_bp'][0]


def test_invalid_or_frozen_assumptions_fail_closed(inputs):
    ident = inputs['books']['mbs']['cusip'][0]
    for patch in [{'burn_k': 1}, {'wac': float('nan')}, {'wam': 12.3}, {'price': 90}, {'id': 123}]:
        with pytest.raises(ValueError):
            apply_overrides(inputs['books'], {'mbs': {ident: patch}})


def test_analytics_matches_independent_full_drivers(inputs):
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.analytics.kpis import compute_kpis
    from portfolio_risk.products.corp import run_corp_risk
    cache = DependencyCache(max_bytes=256*1024*1024)
    result = price_books(**inputs, cache=cache, include_analytics=True)
    bs = inputs['books'] | {'asof': inputs['asof'], 'mbs_hists': inputs['mbs_hists']}
    with run_context(inputs['config']):
        nii = run_balance_sheet_nii(bs, inputs['swap_rates'], inputs['vol_pts'], inputs['dep_hist'],
                                    seed=7, horizon=6, asof=inputs['asof'])
        oas = {k: v['base_oas_bp'].to_numpy() / 1e4 for k, v in result['positions'].items()}
        kpis = compute_kpis(bs, inputs['swap_rates'], inputs['vol_pts'], inputs['dep_hist'],
                            seed=7, nii_monthly=nii['monthly'], oas_by_book=oas)
        reference_risk = run_corp_risk(bs['loans'], inputs['asof'], inputs['swap_rates'], inputs['vol_pts'],
                                       *inputs['mbs_hists'], seed=7, oas=oas['loans'])
    for column in reference_risk.columns:
        if column.startswith('krd01_'):
            np.testing.assert_allclose(result['positions']['loans'][column], reference_risk[column], rtol=1e-9, atol=1e-7)
    np.testing.assert_allclose(result['nii']['monthly']['nii'], nii['monthly']['nii'], rtol=1e-11, atol=1e-7)
    for k, v in kpis['dv01s'].items():
        assert result['kpis']['dv01s'].get(k, 0) == pytest.approx(v, rel=1e-9, abs=1e-7)
    for group, metric in [('eve', 'eve_$'), ('eve', 'dv01_net_$'), ('lcr', 'lcr_pct'), ('nsfr', 'nsfr_pct')]:
        assert result['kpis'][group][metric] == pytest.approx(kpis[group][metric], rel=1e-10)
    assert result['kpis']['capital']['cet1_path'][-1]['cet1_$'] == pytest.approx(kpis['capital']['cet1_path'][-1]['cet1_$'])
    warm = price_books(**inputs, cache=cache, include_analytics=True)
    assert sum(s['computed'] for s in warm['graph'].values()) == 0


def test_analytics_selective_edit_matches_full_rebuild(inputs):
    ident = inputs['books']['deposits']['id'][0]
    patch = {'deposits': {ident: {'rate_paid': .03, 'attrition_base': .03}}}
    cache = DependencyCache(max_bytes=256*1024*1024)
    price_books(**inputs, cache=cache, include_analytics=True)
    incremental = compare_books(**inputs, cache=cache, include_analytics=True, assumption_overrides=patch)
    rebuilt = compare_books(**inputs, cache=DependencyCache(), include_analytics=True, assumption_overrides=patch)
    for book in inputs['books']:
        a, b = incremental['revised']['positions'][book], rebuilt['revised']['positions'][book]
        np.testing.assert_allclose(a.drop('id').to_numpy(), b.drop('id').to_numpy(), rtol=1e-10, atol=1e-6)
    np.testing.assert_allclose(incremental['revised']['nii']['monthly']['nii'], rebuilt['revised']['nii']['monthly']['nii'], atol=1e-7)
    stats = incremental['revised']['graph']
    assert stats['calibration:deposits']['computed'] == 0
    assert stats['cashflows:deposits']['computed'] == 23  # spot + 22 central-difference legs
    assert stats['income:deposits']['computed'] == 1
    assert all(s['computed'] == 0 for k, s in stats.items() if k.endswith((':mbs', ':loans', ':debt', ':cds')))
    assert incremental['revised']['nii']['total'] < incremental['baseline']['nii']['total']


def test_full_book_kpis_with_auxiliary_and_empty_scope(inputs):
    from portfolio_risk.demo import model_balance_sheet, demo_hedge_book
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    bs = model_balance_sheet(scale=.001, include_markets_bs=True)
    extras = {'mm': bs['mm'], 'hedges': demo_hedge_book(scale=.001), 'equity': bs['equity']}
    out = price_books(**inputs, cache=DependencyCache(), include_analytics=True, balance_sheet_extras=extras)
    with run_context(inputs['config']):
        ref = run_balance_sheet_nii(inputs['books'] | extras | {'mbs_hists': inputs['mbs_hists']},
                                    inputs['swap_rates'], inputs['vol_pts'], inputs['dep_hist'],
                                    horizon=6, seed=7, asof=inputs['asof'])
    np.testing.assert_allclose(out['nii']['monthly']['nii'], ref['monthly']['nii'], atol=1e-6, rtol=1e-10)
    assert out['kpis']['includes_auxiliary']
    empty = price_books(**(inputs | {'books': {}}), cache=DependencyCache(), include_analytics=True)
    assert empty['nii']['total'] == 0 and empty['scope_net_value'] == 0


def test_optional_hpi_column_only_invalidates_edited_instrument(inputs):
    books = inputs['books']
    ident = books['mbs']['cusip'][0]
    out = compare_books(**inputs, cache=DependencyCache(), assumption_overrides={'mbs': {ident: {'hpi_orig_ratio': 2.}}})
    assert out['revised']['graph']['cashflows:mbs']['computed'] == 1
    assert out['revised']['graph']['calibration:mbs']['computed'] == 0
    from portfolio_risk.core.config import HPI_MU
    other = books['mbs']['cusip'][1]
    changed = apply_overrides(books, {'mbs': {ident: {'hpi_orig_ratio': 2.}, other: {'age': 120.}}})
    assert changed['mbs']['hpi_orig_ratio'][1] == pytest.approx((1 + HPI_MU) ** 10)


def test_effective_yields_are_independent_of_batch_composition():
    from portfolio_risk.analytics.accounting import book_yield
    months = np.arange(1, 121)
    cash = np.vstack([np.r_[np.full(119, rate/12), 1+rate/12] for rate in (.005, .05, .3)])
    expected = np.array([-.02, .05, .19])
    prices = (cash * (1 + expected[:, None]/12) ** -months).sum(axis=1)
    batched = book_yield(cash, prices)
    alone = np.array([book_yield(cash[i:i+1], prices[i:i+1])[0] for i in range(3)])
    np.testing.assert_array_equal(batched, alone)
    np.testing.assert_allclose(batched, expected, atol=1e-12, rtol=0)
