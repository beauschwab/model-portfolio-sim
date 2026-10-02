"""Graph invalidation, full-rebuild equivalence and the native batch boundary."""
import copy
from dataclasses import replace

import numba
import numpy as np
import polars as pl
import pytest

from portfolio_risk.analytics.incremental import price_books, SUPPORTED_BOOKS
from portfolio_risk.core.batch import CashflowBatch, NumpyDiscountBackend
from portfolio_risk.core.dependency import DependencyCache, Evaluation, fingerprint
from portfolio_risk.core.runtime import RunConfig, run_context


@pytest.fixture(scope="module", params=['python', 'rust'])
def inputs(request):
    from portfolio_risk.core.native import library_path
    if request.param == "rust" and not library_path().is_file():
        pytest.skip("build native product backend")
    from portfolio_risk.demo import model_balance_sheet, demo_market, demo_deposit_history
    previous = numba.get_num_threads()
    numba.set_num_threads(min(2, numba.config.NUMBA_NUM_THREADS))
    bs = model_balance_sheet(scale=.001)
    sr, vp = demo_market()
    yield dict(books={b: bs[b].head(3) for b in SUPPORTED_BOOKS},
               asof=bs['asof'], swap_rates=sr, vol_pts=vp,
               mbs_hists=bs['mbs_hists'], dep_hist=demo_deposit_history(),
               config=RunConfig(33, 35, compute_backend=request.param), seed=7)
    numba.set_num_threads(previous)


def computed(result, stage):
    return sum(v['computed'] for k, v in result['graph'].items() if k.startswith(stage))


def equivalent(actual, expected):
    for book in actual['positions']:
        assert actual['positions'][book]['id'].to_list() == expected['positions'][book]['id'].to_list()
        np.testing.assert_allclose(actual['positions'][book].drop('id').to_numpy(),
                                   expected['positions'][book].drop('id').to_numpy(), rtol=1e-8, atol=2e-5)
    assert actual['scope_net_value'] == pytest.approx(expected['scope_net_value'], rel=1e-8, abs=1e-4)


def test_batch_hand_price_validation_and_immutable_storage():
    values = np.array([200., 100., 80.])
    batch = CashflowBatch(np.array([0, 2, 3]), np.array([1., 2., 3.]), values, 2)
    values[:] = 0
    price = NumpyDiscountBackend().price(batch, np.array([.01, -.02]))
    np.testing.assert_allclose(price, [(200*np.exp(-.01)+100*np.exp(-.02))/2, 40*np.exp(.06)])
    with pytest.raises(ValueError):
        batch.values.setflags(write=True)
    with pytest.raises(ValueError):
        CashflowBatch(np.array([0, 0]), np.empty(0), np.empty(0), 2)
    with pytest.raises(ValueError):
        CashflowBatch(np.array([0, 1]), np.array([np.nan]), np.array([1.]), 2)
    with pytest.raises(ValueError):
        NumpyDiscountBackend().price(batch, np.array([np.nan, 0]))
    assert not len(NumpyDiscountBackend().price(CashflowBatch.from_rows([], 1), np.empty(0)))


def test_cache_bounds_freezing_and_failed_batch():
    cache = DependencyCache(max_bytes=4096, max_entries=2)
    ev = Evaluation(cache)
    original = np.arange(5.)
    result = ev.one('arrays', 'a', lambda: {'array': original})
    original[:] = -1
    assert result['array'][0] == 0
    with pytest.raises(ValueError):
        result['array'].setflags(write=True)
    with pytest.raises(TypeError):
        result['other'] = 0
    for i in range(5):
        ev.one('small', str(i), lambda: 1.)
        assert cache.info()['entries'] <= 2 and cache.info()['bytes'] <= 4096
    before = cache.info().copy()
    with pytest.raises(ValueError):
        ev.batch('bad', ['1', '2'], lambda _: [1])
    assert cache.info() == before
    tiny = DependencyCache(max_bytes=1)
    assert Evaluation(tiny).one('large', 'k', lambda: original).shape == (5,)
    assert tiny.info()['entries'] == 0
    with pytest.raises(ValueError):
        fingerprint(float('nan'))


def test_shared_dependencies_survive_instrument_churn_within_total_budget():
    cache = DependencyCache(max_bytes=32_768, max_entries=16)
    ev = Evaluation(cache)
    shared = ev.one('rate_paths', 'base', lambda: np.arange(64.))
    for i in range(40):
        ev.one('cashflows:mbs', str(i), lambda: np.arange(512.))
        info = cache.info()
        assert info['bytes'] <= info['max_bytes']
        assert info['entries'] <= info['max_entries']
        assert info['shared_bytes'] <= info['shared_max_bytes']
        assert info['shared_entries'] <= info['shared_max_entries']
    assert ev.one('rate_paths', 'base', lambda: pytest.fail('shared path was evicted')) is shared
    with pytest.raises(ValueError):
        shared.setflags(write=True)
    # An ordinary node too large to coexist with protected paths is returned
    # without emptying the useful instrument tier or exceeding the total cap.
    before = cache.info().copy()
    ev.one('cashflows:mbs', 'oversized', lambda: np.arange(4000.))
    assert cache.info() == before
    # Protection is bounded too: it cannot grow with every market request.
    for i in range(10):
        ev.one('rate_paths', str(i), lambda: np.arange(64.))
    assert cache.info()['shared_entries'] == cache.shared_max_entries
    warm = Evaluation(cache)
    warm.one('rate_paths', 'base', lambda: np.arange(64.))
    assert warm.stats['rate_paths']['computed'] == 1
    before = cache.info().copy()
    with pytest.raises(TypeError):
        ev.batch('market_fit', ['good', 'bad'], lambda _: [np.arange(4.), object()])
    assert cache.info() == before  # A failed freeze publishes no partial batch.
    cache.clear()
    assert cache.info()['entries'] == cache.info()['bytes'] == cache.info()['shared_bytes'] == 0


@pytest.mark.parametrize('base_paths', [9, 11])
@pytest.mark.parametrize('scenario_shift', [0., .0025])
def test_income_reuses_live_cashflows_under_eviction(inputs, base_paths, scenario_shift):
    args = inputs | {'config': RunConfig(9, base_paths, 7, compute_backend='python'), 'include_analytics': True,
                     'include_key_rates': False,
                     'scenario_market': (inputs['swap_rates'] + scenario_shift, inputs['vol_pts'])}
    cold = price_books(**args, cache=DependencyCache(max_bytes=0))
    retained = price_books(**args, cache=DependencyCache(max_bytes=128*1024*1024))
    equivalent(cold, retained)
    np.testing.assert_allclose(cold['nii']['monthly'].to_numpy(), retained['nii']['monthly'].to_numpy(), rtol=1e-12)
    np.testing.assert_allclose(cold['nii']['runoff'].to_numpy(), retained['nii']['runoff'].to_numpy(), rtol=1e-12)
    assert cold['kpis'] == retained['kpis']
    for book, frame in inputs['books'].items():
        # Spot plus two risk legs; a shifted market needs original base too.
        legs = 3 if scenario_shift == 0 else 4
        if book == 'mbs' and base_paths != 9:
            legs += 1 if scenario_shift == 0 else 2
        assert cold['graph'][f'cashflows:{book}']['computed'] == legs * len(frame)


@pytest.mark.parametrize('base_paths', [9, 11])
def test_edited_contract_income_retains_separate_base_anchor_under_eviction(inputs, base_paths):
    from portfolio_risk.analytics.whatif import apply_overrides
    args = inputs | {'config': RunConfig(9, base_paths, 7, compute_backend='python'), 'include_analytics': True, 'include_key_rates': False}
    edited = apply_overrides(inputs['books'], {
        'mbs': {inputs['books']['mbs']['cusip'][0]: {'wac': .07}},
        'loans': {inputs['books']['loans']['id'][0]: {'coupon_or_spread': .07}}})
    cold = price_books(**args, valuation_books=edited, cache=DependencyCache(max_bytes=0))
    retained = price_books(**args, valuation_books=edited, cache=DependencyCache())
    equivalent(cold, retained)
    np.testing.assert_allclose(cold['nii']['monthly'].to_numpy(), retained['nii']['monthly'].to_numpy(), rtol=1e-12)


def test_full_key_rate_sweep_never_rebuilds_live_base_cashflows(inputs):
    args = inputs | {'books': {'mbs': inputs['books']['mbs']}, 'config': RunConfig(9, 9, 7, compute_backend='python'),
                     'include_analytics': True}
    cold = price_books(**args, cache=DependencyCache(max_bytes=0))
    retained = price_books(**args, cache=DependencyCache())
    equivalent(cold, retained)
    assert cold['graph']['cashflows:mbs']['computed'] == 23 * len(inputs['books']['mbs'])
    np.testing.assert_allclose(cold['nii']['monthly'].to_numpy(), retained['nii']['monthly'].to_numpy(), rtol=1e-12)


def test_solver_convergence_is_independent_of_batch_neighbors():
    from types import SimpleNamespace
    from portfolio_risk.core.config import TGRID
    from portfolio_risk.core.pricing import solve_oas_from_A, pv_from_A
    from portfolio_risk.products.corp import corp_solve_oas, corp_pv
    A = np.stack([np.exp(-.05 * TGRID), np.exp(-.01 * TGRID)])
    target = pv_from_A(A, np.array([.001, .20]), 1)
    together = solve_oas_from_A(A, 1, target)[0]
    separate = [solve_oas_from_A(A[i:i+1], 1, target[i:i+1])[0][0] for i in range(2)]
    np.testing.assert_array_equal(together, separate)
    deck = SimpleNamespace(n=2, per_off=np.array([0, 3, 6]),
                           t_pay=np.array([1., 2., 3., 1., 2., 3.]))
    cf = np.array([.04, .04, 1.04, .04, .04, 1.04])
    deck.tgt = corp_pv(deck, cf, np.array([.001, .20]), 1)
    together = corp_solve_oas(deck, cf, 1)[0]
    separate = []
    for i in range(2):
        single = SimpleNamespace(n=1, per_off=np.array([0, 3]), t_pay=deck.t_pay[i*3:(i+1)*3],
                                 tgt=deck.tgt[i:i+1])
        separate.append(corp_solve_oas(single, cf[i*3:(i+1)*3], 1)[0][0])
    np.testing.assert_array_equal(together, separate)


def test_all_products_match_existing_calibration_and_warm_reuse(inputs):
    from portfolio_risk.analytics.calibration import calibrate_books
    cache = DependencyCache()
    first = price_books(**inputs, cache=cache)
    with run_context(inputs['config']):
        legacy = calibrate_books(inputs['books'] | {'asof': inputs['asof'], 'mbs_hists': inputs['mbs_hists']},
                                 inputs['swap_rates'], inputs['vol_pts'], inputs['dep_hist'], inputs['seed'])
    for book, oas in legacy.items():
        np.testing.assert_allclose(first['positions'][book]['base_oas_bp'], oas * 1e4, atol=1e-5)
        np.testing.assert_allclose(first['positions'][book]['model_price'], inputs['books'][book]['price'], atol=1e-6)
    warm = price_books(**inputs, cache=cache)
    equivalent(first, warm)
    assert computed(warm, '') == 0


def test_spread_and_notional_only_touch_downstream_nodes(inputs):
    cache = DependencyCache()
    base = price_books(**inputs, cache=cache)
    book = 'loans'
    instrument = inputs['books'][book]['id'][1]
    shocked = price_books(**inputs, cache=cache, spread_overrides_bp={book: {instrument: 25}})
    assert computed(shocked, 'marks:') == 1
    assert computed(shocked, '') == 1
    np.testing.assert_array_equal(base['positions'][book]['base_oas_bp'], shocked['positions'][book]['base_oas_bp'])
    assert shocked['positions'][book]['model_price'][1] < base['positions'][book]['model_price'][1]
    equivalent(shocked, price_books(**inputs, cache=DependencyCache(max_bytes=0),
                                   spread_overrides_bp={book: {instrument: 25}}))
    edited = inputs['books'] | {book: inputs['books'][book].with_columns(pl.col('face') * 2)}
    scaled = price_books(**(inputs | {'books': edited}), cache=cache)
    assert computed(scaled, '') == 0
    np.testing.assert_allclose(scaled['positions'][book]['market_value'], base['positions'][book]['market_value'] * 2)


def test_one_contract_or_target_edit_preserves_other_instruments(inputs):
    cache = DependencyCache()
    price_books(**inputs, cache=cache)
    original = inputs['books']['loans']
    ident = original['id'][0]
    frame = original.with_columns(pl.when(pl.col('id') == ident).then(pl.col('coupon_or_spread') + .002)
                                  .otherwise(pl.col('coupon_or_spread')).alias('coupon_or_spread'))
    args = inputs | {'books': inputs['books'] | {'loans': frame}}
    result = price_books(**args, cache=cache)
    assert computed(result, 'cashflows:') == computed(result, 'calibration:') == computed(result, 'marks:') == 1
    assert computed(result, 'rate_paths') == 0
    equivalent(result, price_books(**args, cache=DependencyCache(max_bytes=0)))
    target = frame.with_columns(pl.when(pl.col('id') == ident).then(pl.col('price') - .25)
                               .otherwise(pl.col('price')).alias('price'))
    result = price_books(**(args | {'books': args['books'] | {'loans': target}}), cache=cache)
    assert computed(result, 'cashflows:') == 0
    assert computed(result, 'calibration:') == computed(result, 'marks:') == 1


def test_scenario_holds_base_oas_and_matches_independent_product_price(inputs):
    from portfolio_risk.core.scenarios import CRN, build_rate_paths
    from portfolio_risk.core.curve import bootstrap_curve, forwards_from_dfs
    from portfolio_risk.core.vol import factor_loadings, calibrate_abcd
    from portfolio_risk.core.config import SWAP_TENORS
    from portfolio_risk.products.corp import CorpDeck, _corp_A, corp_pv
    cache = DependencyCache()
    base = price_books(**inputs, cache=cache)
    rates, vols = inputs['swap_rates'] + .005, inputs['vol_pts']
    kwargs = dict(scenario_market=(rates, vols), spread_shift=.001)
    scenario = price_books(**inputs, cache=cache, **kwargs)
    assert computed(scenario, 'calibration:') == 0
    assert computed(scenario, 'cashflows:') == sum(map(len, inputs['books'].values()))
    for book in inputs['books']:
        np.testing.assert_array_equal(base['positions'][book]['base_oas_bp'], scenario['positions'][book]['base_oas_bp'])
    equivalent(scenario, price_books(**inputs, cache=DependencyCache(max_bytes=0), **kwargs))
    B = factor_loadings()
    dfs = bootstrap_curve(SWAP_TENORS, rates)
    abcd = calibrate_abcd(vols, forwards_from_dfs(dfs), dfs, B)
    paths = build_rate_paths(rates, vols, abcd, B, CRN(inputs['config'].n_paths, inputs['seed']))
    deck = CorpDeck(inputs['books']['loans'], inputs['asof'])
    oas = base['positions']['loans']['base_oas_bp'].to_numpy() / 1e4 + .001
    expected = corp_pv(deck, _corp_A(deck, paths), oas, inputs['config'].n_paths)
    np.testing.assert_allclose(scenario['positions']['loans']['model_price'], expected * 100, atol=1e-9)


def test_assumption_seed_and_path_dependencies(inputs):
    from portfolio_risk.products.deposits import SEGMENTS
    from portfolio_risk.products.cds import CD_EW_PARAMS
    cache = DependencyCache()
    price_books(**inputs, cache=cache)
    segments = copy.deepcopy(SEGMENTS)
    segment = inputs['books']['deposits']['segment'][0]
    segments[segment]['base'] += .001
    config = replace(inputs['config'], deposit_segments=segments)
    updated = price_books(**(inputs | {'config': config}), cache=cache)
    affected = sum(s == segment for s in inputs['books']['deposits']['segment'])
    assert computed(updated, 'cashflows:') == affected
    assert computed(updated, 'rate_paths') == 0
    equivalent(updated, price_books(**(inputs | {'config': config}), cache=DependencyCache(max_bytes=0)))
    ewp = CD_EW_PARAMS.copy()
    ewp[0] += .001
    config = replace(config, cd_ew_params=tuple(ewp))
    updated = price_books(**(inputs | {'config': config}), cache=cache)
    assert computed(updated, 'cashflows:') == len(inputs['books']['cds'])
    changed_seed = price_books(**(inputs | {'seed': 8}), cache=cache)
    assert computed(changed_seed, 'cashflows:') == sum(map(len, inputs['books'].values()))
    equivalent(changed_seed, price_books(**(inputs | {'seed': 8}), cache=DependencyCache(max_bytes=0)))
    config = replace(inputs['config'], n_paths_base=37)
    updated = price_books(**(inputs | {'config': config}), cache=cache)
    assert computed(updated, 'cashflows:') == len(inputs['books']['mbs'])


def test_row_reorder_removal_and_old_snapshot_cannot_poison_cache(inputs):
    cache = DependencyCache()
    base = price_books(**inputs, cache=cache)
    books = {k: v.reverse().head(2) for k, v in inputs['books'].items()}
    newer = price_books(**(inputs | {'books': books}), cache=cache)
    assert computed(newer, '') == 0
    equivalent(newer, price_books(**(inputs | {'books': books}), cache=DependencyCache(max_bytes=0)))
    equivalent(base, price_books(**inputs, cache=cache))
    empty = price_books(**(inputs | {'books': {k: v.clear() for k, v in books.items()}}), cache=cache)
    assert empty['scope_net_value'] == 0 and computed(empty, '') == 0
    with pytest.raises(ValueError, match='unknown position'):
        price_books(**inputs, cache=cache, spread_overrides_bp={'loans': {'missing': 25}})


def test_bad_backend_output_never_enters_mark_cache(inputs):
    class BadBackend:
        identity = 'bad-v1'
        def price(self, batch, oas):
            return np.full(len(oas), np.nan)
    cache = DependencyCache()
    price_books(**inputs, cache=cache)
    with pytest.raises(ValueError, match='finite PV'):
        price_books(**inputs, cache=cache, backend=BadBackend())
    assert computed(price_books(**inputs, cache=cache), '') == 0
