"""Bounded native calibration ownership and independent fitted-function parity."""
import numpy as np
import polars as pl
import pytest

from portfolio_risk import demo
from portfolio_risk.core import curve, vol, quant_native
from portfolio_risk.core.config import SWAP_TENORS, TENOR, SHIFT
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.products import deposits

pytestmark = pytest.mark.skipif(not library_path().is_file(), reason='build native product backend')


@pytest.fixture(scope='module')
def market():
    sr, quotes = demo.demo_market()
    dfs = curve.bootstrap_curve(SWAP_TENORS, sr)
    return quotes, curve.forwards_from_dfs(dfs), dfs, vol.factor_loadings()


@pytest.mark.parametrize('kind', ['volatility', 'deposit'])
def test_native_fit_has_no_python_optimizer_or_model_callback(monkeypatch, market, kind):
    hist = demo.demo_deposit_history()
    fn = (lambda: vol.calibrate_abcd(*market, quiet=True)) if kind == 'volatility' else lambda: deposits.LogisticBetaECM().fit(hist)
    expected = fn()
    def forbidden(*args, **kwargs):
        raise AssertionError('Python calibration executed inside native fit')
    monkeypatch.setattr(vol, 'least_squares', forbidden)
    monkeypatch.setattr(deposits, 'least_squares', forbidden)
    monkeypatch.setattr(vol, 'model_swaption_vol', forbidden)
    monkeypatch.setattr(vol, 'model_swaption_value_jac', forbidden)
    monkeypatch.setattr(deposits.LogisticBetaECM, 'equilibrium', forbidden)
    calls = []
    original = quant_native.call
    def capture(op, *args, **kwargs):
        calls.append(op)
        return original(op, *args, **kwargs)
    monkeypatch.setattr(quant_native, 'call', capture)
    with run_context(RunConfig(compute_backend='rust')):
        actual = fn()
    assert calls == ([25] if kind == 'volatility' else [26])
    if kind == 'volatility':
        np.testing.assert_allclose(actual, expected, rtol=1e-8, atol=1e-9)
    else:
        for key in ('lam_up', 'lam_dn'):
            assert actual[key] == pytest.approx(expected[key], abs=2e-7)


@pytest.mark.parametrize('seed', range(5))
def test_volatility_quotes_and_warm_start_fit_match(market, seed):
    quotes, f, dfs, b = market
    quotes = quotes.copy()
    quotes[:, 2] += np.random.default_rng(seed).normal(0, .003, len(quotes))
    start = vol.calibrate_abcd(*market, quiet=True) if seed % 2 else None
    expected = vol.calibrate_abcd(quotes, f, dfs, b, x0=start, quiet=True)
    with run_context(RunConfig(compute_backend='rust')):
        actual = vol.calibrate_abcd(quotes, f, dfs, b, x0=start, quiet=True)
    np.testing.assert_allclose(actual, expected, rtol=1e-8, atol=1e-9)
    fitted = lambda p: np.array([vol.model_swaption_vol(0, e, n, p, f, dfs, b) for e, n, _ in quotes])
    np.testing.assert_allclose(fitted(actual), fitted(expected), rtol=1e-9, atol=1e-10)


@pytest.mark.parametrize('seed', range(5))
def test_deposit_equilibrium_and_dynamic_paths_match(seed):
    hist = demo.demo_deposit_history(seed=seed)
    model = deposits.LogisticBetaECM()
    expected = model.fit(hist)
    with run_context(RunConfig(compute_backend='rust')):
        actual = model.fit(hist)
    # Parameter identification is weak; gate the fitted function in rate units.
    # 1e-8 is 0.0001 bp, much smaller than observed history noise (~1 bp).
    grid = np.linspace(0., .09, 181)
    np.testing.assert_allclose(model.equilibrium(actual, grid), model.equilibrium(expected, grid), rtol=0., atol=1e-8)
    paths = np.stack([hist['ff'].to_numpy(), hist['ff'].to_numpy()[::-1]])
    np.testing.assert_allclose(model.paths(paths, actual, .01), model.paths(paths, expected, .01), rtol=0., atol=1e-8)
    for key in ('lam_up', 'lam_dn'):
        assert actual[key] == pytest.approx(expected[key], abs=1e-6)


def test_constant_deposit_history_stays_finite_and_bounded():
    # Underdetermined parameters must not produce NaNs or diverging rate paths.
    hist = pl.DataFrame({'ff': np.full(24, .02), 'dep_rate': np.full(24, .006)})
    with run_context(RunConfig(compute_backend='rust')):
        fit = deposits.LogisticBetaECM().fit(hist)
    assert np.isfinite(fit['p']).all()
    assert .01 <= fit['lam_up'] <= 1 and .01 <= fit['lam_dn'] <= 1
    eq = deposits.LogisticBetaECM().equilibrium(fit, np.array([.02]))
    np.testing.assert_allclose(eq, [.006], rtol=0, atol=1e-7)


@pytest.mark.parametrize('bad', ['nan_history', 'short_history', 'wrong_columns', 'bad_expiry', 'bad_start', 'zero_curve'])
def test_invalid_fit_inputs_fail_closed(market, bad):
    quotes, f, dfs, b = (x.copy() for x in market)
    if bad in ('nan_history', 'short_history', 'wrong_columns'):
        history = {'nan_history': np.full((10, 2), np.nan), 'short_history': np.zeros((1, 2)), 'wrong_columns': np.zeros((10, 3))}[bad]
        op, inputs, shapes = 26, [history], [(7,), (3,)]
    else:
        start = [.05, .1, .5, .12]
        if bad == 'bad_expiry': quotes[0, 0] = 1000.
        if bad == 'bad_start': start[0] = 2.
        if bad == 'zero_curve': dfs[:] = 1.
        op, inputs, shapes = 25, [quotes, f, dfs, b, start, [TENOR, SHIFT]], [(4,), (3,)]
    with pytest.raises(ValueError, match='no fallback|no NaNs'):
        quant_native.call(op, inputs, shapes)


def test_native_fit_diagnostics_and_thread_determinism(market):
    from numba import get_num_threads, set_num_threads
    quotes, f, dfs, b = market
    initial = [.05, .1, .5, .12]
    before = get_num_threads()
    try:
        outputs = []
        for count in (1, 2):
            set_num_threads(count)
            outputs.append(quant_native.call(25, [quotes, f, dfs, b, initial, [TENOR, SHIFT]], [(4,), (3,)]))
        for a, b in zip(*outputs): np.testing.assert_array_equal(a, b)
        stats = outputs[0][1]
        assert stats[0] > 0 and 1 <= stats[1] <= 400 and stats[2] in (1, 2, 3, 4)
    finally:
        set_num_threads(before)
