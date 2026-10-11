"""Deposit terminal treatment (deposit-terminal-1) and horizon sensitivity.

Gates: payout default identity, closed-form hand fixtures for every mode in
both kernels (engine and stress restart), independent Python-vs-Rust parity,
mode validation, cache/graph identity, and the Rust horizon report.
"""
from dataclasses import replace

import numpy as np
import polars as pl
import pytest

from portfolio_risk import demo
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.products.deposits import (AGE_KNOTS, TAIL_MIN_DECAY,
                                              TERMINAL_MODES, DepositDeck,
                                              LogisticBetaECM, _deposit_A,
                                              deposit_engine,
                                              deposit_stress_engine,
                                              terminal_code)

native = pytest.mark.skipif(not library_path().is_file(), reason='build native product library')
MODES = list(TERMINAL_MODES)
SVC = 0.0024


def both(fn, mode=None, **config):
    """(python reference, rust) results under the same explicit run config."""
    out = []
    for backend in ('python', 'rust'):
        with run_context(RunConfig(**({'n_paths': 7, 'n_paths_base': 9, 'horizon': 3} | config),
                                   compute_backend=backend, deposit_terminal=mode)):
            out.append(fn())
    return out


def equal(a, b, rtol=1e-10, atol=1e-10):
    if isinstance(a, pl.DataFrame):
        assert a.columns == b.columns
        for col, dtype in a.schema.items():
            if dtype.is_numeric():
                equal(a[col].to_numpy(), b[col].to_numpy(), rtol, atol)
            else:
                assert a[col].to_list() == b[col].to_list()
    elif isinstance(a, dict):
        assert a.keys() == b.keys()
        for k in a:
            equal(a[k], b[k], rtol, atol)
    elif isinstance(a, (tuple, list)):
        assert len(a) == len(b)
        for x, y in zip(a, b):
            equal(x, y, rtol, atol)
    elif a is None or isinstance(a, str):
        assert a == b
    else:
        np.testing.assert_allclose(a, b, rtol=rtol, atol=atol)


# --- controlled one-path fixture ------------------------------------------------
def fixture_args(T, r, s, a, oas=0.0):
    """One path, constant paid rate r (offset, model path 0), constant short s,
    no velocity, flat age curve, no flight (amp 0), size multiplier 1:
    monthly attrition is exactly a; deflator df_m = (1+s/12)^-(m+1)."""
    m = np.arange(T)
    short = np.full((1, T), s)
    df = ((1.0 + s / 12.0) ** -(m + 1.0))[None, :]
    knots = np.array([0.0, 1e4])
    coefs = np.array([0.0, 0.0, 0.0, 1.0])
    one = lambda v: np.array([float(v)])
    return (np.zeros((1, T)), short, np.zeros((1, T)), df, knots, coefs,
            one(r), one(a), one(1.0), one(0.0), one(0.0), one(0.0), one(0.0),
            one(SVC), 0.0, 1.0, one(oas))


def hand_flows(mode, T, r, s, a):
    """Independent closed form of every monthly outflow (undiscounted)."""
    c = r / 12.0 + SVC / 12.0
    bal = (1.0 - a) ** np.arange(T)
    cf = bal * (c + a)
    resid = (1.0 - a) ** T
    if mode == 'payout':
        cf[-1] += resid
    elif mode == 'runoff_tail':
        cf[-1] += resid * (c + a) / max(a + s / 12.0, TAIL_MIN_DECAY)
    return cf, resid


@pytest.mark.parametrize('backend', ['python', pytest.param('rust', marks=native)])
@pytest.mark.parametrize('mode', MODES)
def test_hand_fixture_each_mode(backend, mode):
    T, r, s, a = 36, 0.015, 0.04, 0.02
    with run_context(RunConfig(compute_backend=backend)):
        A, Pout, _, _, _, Iout, Rout = deposit_engine(
            *fixture_args(T, r, s, a), np.zeros(1, dtype=np.int64), False, terminal_code(mode))
    cf, resid = hand_flows(mode, T, r, s, a)
    df = (1.0 + s / 12.0) ** -(np.arange(T) + 1.0)
    np.testing.assert_allclose(A[0], cf * df, rtol=1e-13, atol=1e-16)
    bal = (1.0 - a) ** np.arange(T)
    np.testing.assert_allclose(Iout[0], bal * r / 12.0, rtol=1e-13)
    runoff = bal * a
    if mode == 'payout':
        runoff[-1] = bal[-1]
    # runoff/interest stay in-horizon for every mode; the tail is valuation only
    np.testing.assert_allclose(Pout[0], runoff, rtol=1e-13)
    assert Rout[0] == pytest.approx(resid, rel=1e-13)


@pytest.mark.parametrize('backend', ['python', pytest.param('rust', marks=native)])
def test_stationary_runoff_tail_is_exact_continuation(backend):
    """With a flat market and constant attrition the closed-form tail equals
    simulating the remaining run-off month by month."""
    r, s, a = 0.015, 0.04, 0.02
    with run_context(RunConfig(compute_backend=backend)):
        tail = deposit_engine(*fixture_args(24, r, s, a), np.zeros(1, dtype=np.int64), False, 2)[0]
        long = deposit_engine(*fixture_args(3000, r, s, a), np.zeros(1, dtype=np.int64), False, 1)[0]
    assert tail.sum() == pytest.approx(long.sum(), rel=1e-12)


@pytest.mark.parametrize('backend', ['python', pytest.param('rust', marks=native)])
def test_tail_decay_floor(backend):
    T, r, s, a = 12, 0.0, -0.01, 0.0
    with run_context(RunConfig(compute_backend=backend)):
        A = deposit_engine(*fixture_args(T, r, s, a), np.zeros(1, dtype=np.int64), False, 2)[0]
    cf, _ = hand_flows('runoff_tail', T, r, s, a)
    np.testing.assert_allclose(A[0], cf * (1 + s / 12) ** -(np.arange(T) + 1.0), rtol=1e-13)
    assert np.isfinite(A).all()


@pytest.mark.parametrize('backend', ['python', pytest.param('rust', marks=native)])
@pytest.mark.parametrize('mode', MODES)
def test_stress_restart_matches_hand_forward_value(backend, mode):
    """Stress-kernel restart from a supplied balance at month h reproduces the
    closed-form forward value of the remaining flows, including the terminal."""
    T, r, s, a, h, oas = 30, 0.02, 0.035, 0.03, 10, 0.004
    args = fixture_args(T, r, s, a, oas)
    with run_context(RunConfig(compute_backend=backend)):
        ck = np.zeros((1, 1, 1), dtype=np.float32)
        ck[0, 0, 0] = 0.75
        fv = deposit_stress_engine(*args, h, 0, ck, terminal_code(mode))[0]
    b0 = float(np.float32(0.75))
    cf, _ = hand_flows(mode, T, r, s, a)
    cf = cf * b0 / (1.0 - a) ** h            # balances restart at b0 at month h
    m = np.arange(h, T)
    disc = (1.0 + s / 12.0) ** -(m - h + 1.0) * np.exp(-oas * (m - h + 1.0) / 12.0)
    assert fv == pytest.approx((cf[h:] * disc).sum(), rel=1e-12)


@pytest.mark.parametrize('mode', MODES)
def test_zero_shock_invariant_each_mode(mode):
    """Engine forward values and the zero-shock stress restart agree per mode."""
    from portfolio_risk.core.config import SEED, STRESS_HORIZONS_M, SWAP_TENORS
    from portfolio_risk.core.curve import bootstrap_curve, forwards_from_dfs
    from portfolio_risk.core.scenarios import CRN, build_rate_paths
    from portfolio_risk.core.vol import calibrate_abcd, factor_loadings
    from portfolio_risk.products.deposits import _dep_args, deposit_shocked_paths
    sr, vp = demo.demo_market()
    B = factor_loadings(); dfs0 = bootstrap_curve(SWAP_TENORS, sr)
    paths = build_rate_paths(sr, vp, calibrate_abcd(vp, forwards_from_dfs(dfs0), dfs0, B), B, CRN(32, SEED))
    deck = DepositDeck(demo.demo_deposit_book(12))
    m = LogisticBetaECM(); params = m.fit(demo.demo_deposit_history())
    r0 = float(m.equilibrium(params, paths['short'][:, 0].mean()))
    dep = m.paths(paths['short'].astype(np.float64), params, r0)
    oas = np.full(deck.n, -0.01)
    _, _, FVb, _, ck, *_ = _deposit_A(deck, paths, dep, r0, oas, STRESS_HORIZONS_M, True, terminal=mode)
    for hi in (0, 13, 26):
        h = int(STRESS_HORIZONS_M[hi])
        sp, dep2 = deposit_shocked_paths(paths, h, 0.0, m, params, r0)
        fv = deposit_stress_engine(*_dep_args(deck, sp, dep2, r0), oas, h, hi, ck, terminal_code(mode))
        np.testing.assert_allclose(fv, FVb[:, hi], rtol=1e-6)


# --- default identity and validation --------------------------------------------
def test_payout_default_is_bit_identical_and_ordering():
    """Omitted mode == explicit payout, exactly; truncate <= payout, tail >= truncate."""
    from portfolio_risk.core.config import SEED, SWAP_TENORS
    from portfolio_risk.core.curve import bootstrap_curve, forwards_from_dfs
    from portfolio_risk.core.scenarios import CRN, build_rate_paths
    from portfolio_risk.core.vol import calibrate_abcd, factor_loadings
    sr, vp = demo.demo_market()
    B = factor_loadings(); dfs0 = bootstrap_curve(SWAP_TENORS, sr)
    paths = build_rate_paths(sr, vp, calibrate_abcd(vp, forwards_from_dfs(dfs0), dfs0, B), B, CRN(16, SEED))
    deck = DepositDeck(demo.demo_deposit_book(10))
    m = LogisticBetaECM(); params = m.fit(demo.demo_deposit_history())
    dep = m.paths(paths['short'].astype(np.float64), params, .01)
    default = _deposit_A(deck, paths, dep, .01)
    explicit = _deposit_A(deck, paths, dep, .01, terminal='payout')
    with run_context(RunConfig(compute_backend='python', deposit_terminal='payout')):
        ambient = _deposit_A(deck, paths, dep, .01)
    for x, y, z in zip(default, explicit, ambient):
        assert np.array_equal(x, y) and np.array_equal(x, z)
    trunc = _deposit_A(deck, paths, dep, .01, terminal='truncate')
    tail = _deposit_A(deck, paths, dep, .01, terminal='runoff_tail')
    assert (trunc[0].sum(1) < default[0].sum(1)).all()
    assert (tail[0].sum(1) > trunc[0].sum(1)).all()
    # in-horizon months are untouched; only the final column differs
    assert np.array_equal(trunc[0][:, :-1], default[0][:, :-1])
    assert np.array_equal(tail[0][:, :-1], default[0][:, :-1])
    assert np.array_equal(trunc[6], default[6]) and np.array_equal(tail[5], default[5])
    np.testing.assert_allclose(default[1].sum(1) / 16, 1.0, rtol=1e-9)    # payout conserves
    np.testing.assert_allclose(trunc[1].sum(1) / 16 + trunc[6] / 16, 1.0, rtol=1e-9)


def test_mode_is_validated():
    with pytest.raises(ValueError, match='deposit_terminal'):
        RunConfig(deposit_terminal='par')
    with pytest.raises(ValueError, match='terminal'):
        terminal_code('par')
    assert [terminal_code(m) for m in MODES] == [0, 1, 2]


@native
def test_native_rejects_unknown_mode_and_bad_kernel_code():
    from portfolio_risk.core.lifecycle_native import deposit_request
    from portfolio_risk.core.quant_native import term_call
    sr, vp = demo.demo_market()
    with run_context(RunConfig(n_paths=3, compute_backend='rust')):
        request = deposit_request(demo.demo_deposit_book(2), sr, vp, demo.demo_deposit_history(), 7, None, None)
        assert 'terminal' not in request['assumptions']      # payout is omitted
        request['assumptions']['terminal'] = 'par'
        with pytest.raises(Exception, match='unknown variant'):
            term_call('deposit-risk-1', request)
        with pytest.raises(Exception):
            deposit_engine(*fixture_args(6, .01, .03, .02), np.zeros(1, dtype=np.int64), False, 3)


# --- Rust vs independent Python reference ---------------------------------------
@native
@pytest.mark.parametrize('mode', MODES)
def test_native_kernel_parity_each_mode(mode):
    from portfolio_risk.core.config import SWAP_TENORS
    from portfolio_risk.core.curve import bootstrap_curve, forwards_from_dfs
    from portfolio_risk.core.scenarios import CRN, build_rate_paths
    from portfolio_risk.core.vol import calibrate_abcd, factor_loadings
    from portfolio_risk.products.deposits import _dep_args
    sr, vp = demo.demo_market()
    B = factor_loadings(); dfs0 = bootstrap_curve(SWAP_TENORS, sr)
    paths = build_rate_paths(sr, vp, calibrate_abcd(vp, forwards_from_dfs(dfs0), dfs0, B), B, CRN(7, 19))
    deck = DepositDeck(demo.demo_deposit_book(8))
    m = LogisticBetaECM(); params = m.fit(demo.demo_deposit_history())
    dep = m.paths(paths['short'].astype(float), params, .01)
    oas = np.full(8, .012); hz = np.array([1, 13, 27])
    engine = both(lambda: _deposit_A(deck, paths, dep, .01, oas=oas, horizons=hz, want_fwd=True), mode)
    equal(*engine)
    ck = engine[0][4]
    equal(*both(lambda: deposit_stress_engine(*_dep_args(deck, paths, dep, .01), oas, 13, 1, ck,
                                              terminal_code()), mode))


@native
@pytest.mark.parametrize('mode', MODES)
@pytest.mark.parametrize('driver', ['risk', 'stress', 'nii'])
def test_native_driver_parity_each_mode(mode, driver):
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.products.deposits import run_deposit_risk, run_deposit_stress
    sr, vp = demo.demo_market(); dh = demo.demo_deposit_history()
    bs = demo.model_balance_sheet(scale=.001)
    book = bs['deposits'].head(3)
    calls = {'risk': lambda: run_deposit_risk(book, sr, vp, dh),
             'stress': lambda: run_deposit_stress(book, sr, vp, dh, shocks_bp=[0, 200]),
             'nii': lambda: run_balance_sheet_nii({'deposits': book}, sr, vp, dh, horizon=3)}
    equal(*both(calls[driver], mode), rtol=1e-7, atol=1e-5)


@native
def test_modes_change_risk_and_residual_column():
    from portfolio_risk.products.deposits import run_deposit_risk
    sr, vp = demo.demo_market(); dh = demo.demo_deposit_history(); book = demo.demo_deposit_book(6)
    out = {}
    for mode in MODES:
        with run_context(RunConfig(n_paths=8, compute_backend='rust', deposit_terminal=mode)):
            out[mode] = run_deposit_risk(book, sr, vp, dh, oas=np.full(6, .01))
    # same fixed OAS: payout and tail carry terminal value, truncate does not
    p = {k: v['model_price'].to_numpy() for k, v in out.items()}
    assert (p['truncate'] < p['payout']).all() and (p['truncate'] < p['runoff_tail']).all()
    # the residual balance is reported and mode independent
    r = out['payout']['residual_balance'].to_numpy()
    assert (r > 0).all() and (r < book['balance'].to_numpy()).all()
    for mode in MODES:
        assert np.array_equal(out[mode]['residual_balance'].to_numpy(), r)


# --- dependency identity ----------------------------------------------------------
@pytest.mark.parametrize('backend', ['python', pytest.param('rust', marks=native)])
def test_mode_change_invalidates_graph_identity(backend):
    from portfolio_risk.analytics import incremental
    from portfolio_risk.core.dependency import DependencyCache
    bs = demo.model_balance_sheet(scale=.001)
    sr, vp = demo.demo_market()
    books = {'deposits': bs['deposits'].head(3)}
    base = RunConfig(3, 3, 7, compute_backend=backend)
    args = dict(books=books, asof=bs['asof'], swap_rates=sr, vol_pts=vp,
                dep_hist=demo.demo_deposit_history(), seed=29, scenario_market=(sr + .001, vp),
                include_analytics=False, include_key_rates=False)
    shared = DependencyCache()
    value = lambda result: result['positions']['deposits']['market_value'].to_numpy()
    payout = incremental.price_books(**args, config=base, cache=shared)
    trunc = incremental.price_books(**args, config=replace(base, deposit_terminal='truncate'), cache=shared)
    fresh = incremental.price_books(**args, config=replace(base, deposit_terminal='truncate'),
                                    cache=DependencyCache())
    assert not np.allclose(value(payout), value(trunc), rtol=1e-12, atol=0)
    np.testing.assert_array_equal(value(trunc), value(fresh))
    again = incremental.price_books(**args, config=replace(base, deposit_terminal='payout'), cache=shared)
    np.testing.assert_array_equal(value(again), value(payout))


# --- horizon-sensitivity report ---------------------------------------------------
@native
def test_horizon_report_single_horizon_matches_risk():
    from portfolio_risk.products.deposits import deposit_horizon_sensitivity, run_deposit_risk
    sr, vp = demo.demo_market(); dh = demo.demo_deposit_history(); book = demo.demo_deposit_book(12)
    for mode in ('payout', 'runoff_tail'):
        with run_context(RunConfig(n_paths=16, compute_backend='rust', deposit_terminal=mode)):
            risk = run_deposit_risk(book, sr, vp, dh)
            rep = deposit_horizon_sensitivity(book, sr, vp, dh, horizons=(360,))
        assert rep['policy'] == 'deposit-terminal-1' and rep['base_mode'] == mode
        pos = rep['positions'].filter(pl.col('mode') == mode)
        np.testing.assert_allclose(pos['oas_bps'].to_numpy(), risk['oas_bps'].to_numpy(), rtol=1e-12)
        np.testing.assert_allclose(pos['pv'].to_numpy(),
                                   risk['model_price'].to_numpy() / 100 * book['balance'].to_numpy(), rtol=1e-12)
        np.testing.assert_allclose(pos['residual_balance'].to_numpy(),
                                   risk['residual_balance'].to_numpy(), rtol=1e-12)
        np.testing.assert_allclose(pos['implied_oas_bps'].to_numpy(), risk['oas_bps'].to_numpy(), atol=1e-6)


@native
def test_horizon_report_quantifies_terminal_sensitivity():
    from portfolio_risk.products.deposits import deposit_horizon_sensitivity
    sr, vp = demo.demo_market(); dh = demo.demo_deposit_history(); book = demo.demo_deposit_book(40)
    with run_context(RunConfig(n_paths=32, compute_backend='rust')):
        rep = deposit_horizon_sensitivity(book, sr, vp, dh, horizons=(360, 480))
    s = {(row['horizon_m'], row['mode']): row for row in rep['summary'].to_dicts()}
    pos = rep['positions']
    # truncate carries no terminal value; residual is mode independent and decays
    assert (pos.filter(pl.col('mode') == 'truncate')['terminal_pv'] == 0).all()
    for h in (360, 480):
        assert len({s[h, m]['residual_balance_$'] for m in MODES}) == 1
        assert s[h, 'payout']['terminal_pv_$'] > 0 and s[h, 'runoff_tail']['terminal_pv_$'] > 0
        for m in MODES:
            assert s[h, m]['pv_$'] == pytest.approx(s[h, 'truncate']['pv_$'] + s[h, m]['terminal_pv_$'], rel=1e-12)
    assert s[480, 'payout']['residual_balance_$'] < 0.5 * s[360, 'payout']['residual_balance_$']
    # extending the horizon shrinks the terminal sensitivity across modes
    spread = lambda h: max(s[h, m]['pv_$'] for m in MODES) - min(s[h, m]['pv_$'] for m in MODES)
    assert spread(480) < 0.5 * spread(360)
    # the run-off tail at the base horizon extrapolates the extended-horizon value
    # better than the par payout, which is better than truncation
    target = s[480, 'runoff_tail']['pv_$']
    err = {m: abs(s[360, m]['pv_$'] - target) for m in MODES}
    assert err['runoff_tail'] < err['payout'] < err['truncate']


@native
def test_horizon_report_validation():
    from portfolio_risk.products.deposits import deposit_horizon_sensitivity
    sr, vp = demo.demo_market(); dh = demo.demo_deposit_history(); book = demo.demo_deposit_book(2)
    with run_context(RunConfig(n_paths=3, compute_backend='rust')):
        for bad in [(), (480, 360), (1,), (360, 4096)]:
            with pytest.raises(Exception):
                deposit_horizon_sensitivity(book, sr, vp, dh, horizons=bad)
