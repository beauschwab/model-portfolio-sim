"""Per-pool MBS prepay speed multiplier (`prepay_mult`).

The multiplier scales turnover plus refi before the CPR cap, so 1 is the model's
own speed, 0 stops prepayment and the cap still binds. Rust is the production
runtime; the Python kernels are the independent reference it is compared with.
"""
import datetime as dt

import numpy as np
import polars as pl
import pytest

from portfolio_risk import demo
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.core.scenarios import CRN, build_paths, extract_sec, run_engine, setup

native = pytest.mark.skipif(not library_path().is_file(), reason='build native product backend')
SPEEDS = [0., .5, 1., 2.5, 1., 4., 1.5, 1.]


def backends(fn):
    values = []
    for backend in ('python', 'rust'):
        with run_context(RunConfig(n_paths=7, n_paths_base=9, horizon=3, compute_backend=backend)):
            values.append(fn())
    return values


def close(a, b, rtol=1e-7, atol=1e-5):
    if isinstance(a, pl.DataFrame):
        assert a.columns == b.columns
        for column, dtype in a.schema.items():
            if dtype.is_numeric():
                np.testing.assert_allclose(b[column].to_numpy(), a[column].to_numpy(), rtol=rtol, atol=atol)
    elif isinstance(a, dict):
        assert a.keys() == b.keys()
        for key in a:
            close(a[key], b[key], rtol, atol)
    elif isinstance(a, (tuple, list)):
        assert len(a) == len(b)
        for x, y in zip(a, b):
            close(x, y, rtol, atol)
    elif a is None or isinstance(a, str):
        assert a == b
    else:
        np.testing.assert_allclose(b, a, rtol=rtol, atol=atol)


@pytest.fixture(scope='module')
def market():
    port = demo.demo_portfolio(8)
    sr, vp = demo.demo_market()
    cc, ps = demo.demo_histories()
    models, loadings, abcd, _, _, _ = setup(port, sr, vp, cc, ps)
    paths = build_paths(sr, vp, abcd, loadings, models, CRN(32, 19))
    return port, sr, vp, cc, ps, paths


def test_unit_multiplier_is_the_model_speed_and_zero_stops_prepayment(market):
    port, *_, paths = market
    base = run_engine(paths, extract_sec(port))
    unit = run_engine(paths, extract_sec(port.with_columns(prepay_mult=pl.lit(1.0))))
    for x, y in zip(base, unit):
        np.testing.assert_array_equal(x, y)

    stopped = run_engine(paths, extract_sec(port.with_columns(prepay_mult=pl.lit(0.0))))
    faster = run_engine(paths, extract_sec(port.with_columns(prepay_mult=pl.lit(2.0))))
    # every pool returns its whole balance eventually; speed decides how early.
    # With no prepayment only scheduled principal comes back in the first year.
    early = lambda out: out[6][:, :12].sum(axis=1)   # first-year expected principal by pool
    assert np.all(early(stopped) < early(base))
    assert np.all(early(faster) > early(base))


def test_multiplier_is_applied_before_the_cpr_cap(market):
    from portfolio_risk.core.config import PREPAY_PARAMS
    port, *_, paths = market
    huge = run_engine(paths, extract_sec(port.with_columns(prepay_mult=pl.lit(10.0))))
    capped = run_engine(paths, extract_sec(port.with_columns(prepay_mult=pl.lit(1e6))))
    # beyond the cap a larger multiplier changes nothing
    if PREPAY_PARAMS[5] < 1:
        np.testing.assert_allclose(capped[6], run_engine(paths, extract_sec(port.with_columns(prepay_mult=pl.lit(1e7))))[6])
    assert np.all(huge[6][:, :12].sum(axis=1) <= capped[6][:, :12].sum(axis=1) + 1e-12)


def test_multiplier_is_validated(market):
    port = market[0]
    for bad in (-0.1, float('inf')):
        with pytest.raises(ValueError, match='prepay_mult'):
            extract_sec(port.with_columns(prepay_mult=pl.lit(bad)))
    # a missing value means the model's own speed
    with_null = port.with_columns(prepay_mult=pl.Series([None, *[2.0] * (len(port) - 1)], dtype=pl.Float64))
    assert extract_sec(with_null)[8][0] == 1.0


@native
@pytest.mark.parametrize('forward', [False, True])
def test_native_kernel_matches_reference_with_multipliers(market, forward):
    port, *_, paths = market
    sec = extract_sec(port.with_columns(prepay_mult=pl.Series(SPEEDS)))
    close(*backends(lambda: run_engine(paths, sec, oas=np.linspace(-.01, .03, 8),
                                        horizons=np.array([1, 13, 27]), want_fwd=forward)),
          rtol=1e-10, atol=1e-10)


@native
def test_native_stress_and_batched_kernels_match_reference_with_multipliers(market):
    from portfolio_risk.core.config import MOY, PREPAY_PARAMS, RATIONAL_SIGMOID, SEASONALITY
    from portfolio_risk.core.kernels import batched_pv_engine, stress_engine
    from portfolio_risk.models.prepay import BURN_LUT, BURN_SCALE, LTV_COEFS, LTV_KNOTS, SMM_LUT, SMM_SCALE
    port, *_, paths = market
    sec = extract_sec(port.with_columns(prepay_mult=pl.Series(SPEEDS)))
    oas, hz = np.full(8, .012), np.array([1, 13, 27])
    base = run_engine(paths, sec, oas=oas, horizons=hz, want_fwd=True)
    for i, h in enumerate(hz):
        close(*backends(lambda: stress_engine(
            paths['mtg'], paths['hpi'], paths['yoy'], paths['df'], MOY, SEASONALITY, PREPAY_PARAMS,
            LTV_KNOTS, LTV_COEFS, SMM_LUT, SMM_SCALE, BURN_LUT, BURN_SCALE,
            *sec, oas, h, i, base[3], base[4], True)), rtol=1e-10, atol=1e-10)
    scen = np.zeros(paths['mtg'].shape[0], dtype=np.int64)
    close(*backends(lambda: batched_pv_engine(
        np.ascontiguousarray(paths['mtg']), np.ascontiguousarray(paths['hpi']),
        np.ascontiguousarray(paths['yoy']), np.ascontiguousarray(paths['df']),
        scen, 1, MOY, SEASONALITY, PREPAY_PARAMS, LTV_KNOTS, LTV_COEFS, SMM_LUT, SMM_SCALE,
        BURN_LUT, BURN_SCALE, *sec, oas, np.zeros(8), RATIONAL_SIGMOID)), rtol=1e-10, atol=1e-10)


@pytest.fixture(scope='module')
def book():
    bs = demo.model_balance_sheet(scale=.001)
    for key in ('mbs', 'loans', 'debt', 'cds', 'deposits'):
        bs[key] = bs[key].head(3)
    bs['mbs'] = bs['mbs'].with_columns(prepay_mult=pl.Series([0.5, 1.0, 2.0]))
    return bs


@native
@pytest.mark.parametrize('product', ['risk', 'stress', 'nii'])
def test_native_drivers_match_reference_with_multipliers(market, book, product):
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.analytics.risk import run_risk
    from portfolio_risk.analytics.stress import run_stress
    _, sr, vp, cc, ps, _ = market
    calls = {
        'risk': lambda: run_risk(book['mbs'], sr, vp, cc, ps),
        'stress': lambda: run_stress(book['mbs'], sr, vp, cc, ps, shocks_bp=[0, 200]),
        'nii': lambda: run_balance_sheet_nii(book, sr, vp, demo.demo_deposit_history(), horizon=3),
    }
    close(*backends(calls[product]))


@native
def test_native_risk_moves_with_the_multiplier_and_rejects_bad_values(market, book):
    from portfolio_risk.analytics.risk import run_risk
    _, sr, vp, cc, ps, _ = market
    mbs = book['mbs']
    with run_context(RunConfig(n_paths=7, n_paths_base=9, horizon=3, compute_backend='rust')):
        base = run_risk(mbs.drop('prepay_mult'), sr, vp, cc, ps)
        unit = run_risk(mbs.with_columns(prepay_mult=pl.lit(1.0)), sr, vp, cc, ps)
        fast = run_risk(mbs.with_columns(prepay_mult=pl.lit(3.0)), sr, vp, cc, ps)
        np.testing.assert_array_equal(unit['dv01'].to_numpy(), base['dv01'].to_numpy())
        # OAS is solved to the book price either way; faster prepayment shortens duration
        assert np.all(fast['dv01'].to_numpy() < base['dv01'].to_numpy())
        with pytest.raises(ValueError, match='prepay_mult'):
            run_risk(mbs.with_columns(prepay_mult=pl.lit(-1.0)), sr, vp, cc, ps)


def test_whatif_accepts_a_temporary_multiplier(book):
    from portfolio_risk.analytics.whatif import apply_overrides
    mbs = book['mbs'].drop('prepay_mult')
    cusip = mbs['cusip'][0]
    for backend in ('python', 'rust') if library_path().is_file() else ('python',):
        with run_context(RunConfig(compute_backend=backend)):
            out = apply_overrides({'mbs': mbs}, {'mbs': {cusip: {'prepay_mult': 2.0}}})['mbs']
            assert out['prepay_mult'].to_list() == [2.0, 1.0, 1.0]
            with pytest.raises(ValueError):
                apply_overrides({'mbs': mbs}, {'mbs': {cusip: {'prepay_mult': 11.0}}})
