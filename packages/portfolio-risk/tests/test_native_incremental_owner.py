"""Native graph ownership, independent parity and opaque cache lifecycle."""
from dataclasses import replace

import numpy as np
import polars as pl
import pytest

from portfolio_risk import demo
from portfolio_risk.analytics import incremental
from portfolio_risk.analytics.whatif import apply_overrides
from portfolio_risk.core.dependency import DependencyCache
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import RunConfig

pytestmark = pytest.mark.skipif(not library_path().is_file(), reason='build native graph')


def test_native_handle_is_bound_to_original_library(monkeypatch,tmp_path):
    from portfolio_risk.core.graph_native import NativeGraph
    graph=NativeGraph(1024,10)
    other=NativeGraph(1024,10)
    monkeypatch.setenv('PORTFOLIO_RISK_RUST_LIB',str(tmp_path/'different.dll'))
    try:
        with pytest.raises(RuntimeError,match='MODEL_VERSION_CHANGED'):
            graph.call('info')
        # Closing never dispatches this handle into the alternate registry.
        graph.close()
    finally:
        monkeypatch.delenv('PORTFOLIO_RISK_RUST_LIB')
    with pytest.raises(RuntimeError,match='closed'):graph.call('info')
    assert other.call('info')['entries']==0
    other.close()


@pytest.mark.parametrize('base_paths,shift', [(3, 0.), (5, .002)])
def test_full_native_graph_matches_reference_without_python_callbacks(monkeypatch, base_paths, shift):
    bs = demo.model_balance_sheet(scale=.001)
    sr, vp = demo.demo_market()
    books = {key: bs[key].head(2) for key in incremental.SUPPORTED_BOOKS}
    edits = {'mbs': {books['mbs']['cusip'][0]: {'wac': .071}},
             'deposits': {books['deposits']['id'][0]: {'attrition_base': .04}},
             'loans': {books['loans']['id'][0]: {'coupon_or_spread': .06}}}
    valued = apply_overrides(books, edits)
    args = dict(books=books, valuation_books=valued, asof=bs['asof'], swap_rates=sr,
                vol_pts=vp, mbs_hists=bs['mbs_hists'], dep_hist=demo.demo_deposit_history(),
                config=RunConfig(3, base_paths, 7, compute_backend='python'), seed=29, scenario_market=(sr+shift, vp),
                include_analytics=True, include_key_rates=True)
    expected = incremental.price_books(**args, cache=DependencyCache())
    def forbidden(*args, **kwargs):
        raise AssertionError('Python dependency graph or financial callback executed')
    for name in ['Evaluation', 'fingerprint', 'paths', 'solve_oas_from_A', 'bootstrap_curve',
                 'calibrate_abcd', 'factor_loadings', 'build_paths', 'build_rate_paths', 'run_engine']:
        if hasattr(incremental, name):
            monkeypatch.setattr(incremental, name, forbidden)
    cache = DependencyCache()
    actual = incremental.price_books(**(args | {'config': replace(args['config'], compute_backend='rust')}), cache=cache)
    for key in books:
        a, b = actual['positions'][key], expected['positions'][key]
        assert a['id'].to_list() == b['id'].to_list()
        np.testing.assert_allclose(a.drop('id').to_numpy(), b.drop('id').to_numpy(), rtol=1e-7, atol=2e-5)
    for key in ['monthly', 'runoff']:
        np.testing.assert_allclose(actual['nii'][key].to_numpy(), expected['nii'][key].to_numpy(), rtol=1e-7, atol=2e-5)
    for key in ['eve_$', 'dv01_net_$']:
        assert actual['kpis']['eve'][key] == pytest.approx(expected['kpis']['eve'][key], rel=1e-7, abs=2e-5)
    assert not cache._entries and not cache._shared
    assert cache.info()['entries'] > 0
    cache.clear()
    assert cache.info()['entries'] == cache.info()['bytes'] == 0


def test_native_dependency_budgets_and_context_failure_leave_valid_snapshot():
    bs = demo.model_balance_sheet(scale=.001)
    sr, vp = demo.demo_market()
    args = dict(books={'loans': bs['loans'].head(5)}, asof=bs['asof'], swap_rates=sr, vol_pts=vp,
                config=RunConfig(2, 2, 3, compute_backend='rust'), seed=29)
    for budget in [0, 1, 32_768]:
        cache = DependencyCache(max_bytes=budget, max_entries=7)
        before = incremental.price_books(**args, cache=cache)
        assert cache.info()['bytes'] <= budget and cache.info()['entries'] <= 7
        bad = args['books']['loans'].with_columns(pl.lit(-1.).alias('price'))
        with pytest.raises(ValueError):
            incremental.price_books(**(args | {'books': {'loans': bad}}), cache=cache)
        bad = args['books']['loans'].with_columns(pl.lit(-1.).alias('face'))
        with pytest.raises(ValueError):
            incremental.price_books(**(args | {'books': {'loans': bad}}), cache=cache)
        after = incremental.price_books(**args, cache=cache)
        assert before['positions']['loans'].equals(after['positions']['loans'])


def test_tiny_cache_admission_does_not_poison_public_pricing_handle():
    bs=demo.model_balance_sheet(scale=.001);sr,vp=demo.demo_market()
    args=dict(books={'loans':bs['loans'].head(1)},asof=bs['asof'],swap_rates=sr,vol_pts=vp,
        config=RunConfig(1,1,1,compute_backend='rust'),include_analytics=True,include_key_rates=False)
    expected=incremental.price_books(**args,cache=DependencyCache(max_bytes=0,max_entries=0))
    for budget in (3000,3250,3500,4000):
        cache=DependencyCache(max_bytes=budget,max_entries=8)
        for _ in range(3):
            actual=incremental.price_books(**args,cache=cache)
            assert actual['positions']['loans'].equals(expected['positions']['loans'])
            assert cache.info()['bytes']<=budget


def test_native_override_domains_and_age_dependent_defaults():
    from portfolio_risk.core.runtime import run_context
    bs=demo.model_balance_sheet(scale=.001)
    books={'mbs':bs['mbs'].head(3),'loans':bs['loans'].head(2)}
    edits={'mbs':{books['mbs']['cusip'][0]:{'age':120.}, books['mbs']['cusip'][1]:{'hpi_orig_ratio':1.2}},
           'loans':{books['loans']['id'][0]:{'cap':.08,'floor':.02}}}
    with run_context(RunConfig(compute_backend='python')):
        expected=apply_overrides(books,edits)
    with run_context(RunConfig(compute_backend='rust')):
        actual=apply_overrides(books,edits)
        for key in books:assert actual[key].equals(expected[key])
        for patch in [{'wam':1.5},{'wac':True},{'wac':None},{'burn_k':1.}]:
            with pytest.raises(ValueError):apply_overrides(books,{'mbs':{books['mbs']['cusip'][0]:patch}})
        with pytest.raises(ValueError):apply_overrides(books,{'loans':{books['loans']['id'][0]:{'cap':.01,'floor':.02}}})
