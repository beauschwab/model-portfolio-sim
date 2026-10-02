"""Raw-book native forward stress, with Python used only as a test oracle."""
import importlib
import json
import os
import subprocess

import numpy as np
import polars as pl
import pytest

from portfolio_risk.core import quant_native, curve, vol
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import RunConfig, run_context
from test_native_mortgage_risk_owner import inputs, wire_request, models, prepay

stress = importlib.import_module('portfolio_risk.analytics.stress')
pytestmark = pytest.mark.skipif(not library_path().is_file(), reason='build native product backend')


def run(data, backend, paths=3, seed=27, oas=None, shocks=(-100., 0., 100.), horizon=4):
    with run_context(RunConfig(n_paths=paths, n_paths_base=paths+2, horizon=horizon, compute_backend=backend)):
        return stress.run_stress(*data, shocks_bp=shocks, seed=seed, oas=oas)


def equal_results(expected, actual):
    for a, b in zip(expected, actual):
        if a is None:
            assert b is None
            continue
        assert a.columns == b.columns and a.shape == b.shape
        if 'cusip' in a.columns:
            assert a['cusip'].to_list() == b['cusip'].to_list()
        numeric = [c for c in a.columns if c != 'cusip']
        np.testing.assert_allclose(b[numeric].to_numpy(), a[numeric].to_numpy(), rtol=1e-7, atol=1e-5)


def test_stress_owns_calibration_scheduling_checkpoints_and_aggregation(monkeypatch):
    data = inputs()
    expected = run(data, 'python')
    def forbidden(*args, **kwargs):
        raise AssertionError('Python financial stage executed in native mortgage stress')
    for module, names in [(stress, ['setup','CRN','build_paths','run_engine','solve_base_oas','shocked_paths','stress_engine']),
                          (models, ['fit_current_coupon','fit_ps_spread']),
                          (vol, ['calibrate_abcd','factor_loadings']),
                          (curve, ['bootstrap_curve']), (prepay, ['static_multipliers']),
                          (np.random, ['default_rng'])]:
        for name in names:
            monkeypatch.setattr(module, name, forbidden)
    calls = []
    original = quant_native.call
    def capture(op, *args, **kwargs):
        calls.append(op)
        return original(op, *args, **kwargs)
    monkeypatch.setattr(quant_native, 'call', capture)
    actual = run(data, 'rust')
    assert calls == [29]
    equal_results(expected, actual)


@pytest.mark.parametrize('paths,seed', [(1,0), (3,27), (8,2**64-1)])
@pytest.mark.parametrize('fixed', [False,True])
def test_final_stress_delays_hpi_matured_zero_face_and_fixed_oas(paths, seed, fixed):
    book, *rest = inputs()
    book = book.with_columns(pl.Series('wam', [2,120,200,300]),
        # The two-month instrument needs a price inside the supported OAS bracket.
        pl.Series('price', [100.,99.,98.,97.]),
        pl.Series('pay_delay_days', [0.,24.,45.,55.]), pl.Series('hpi_orig_ratio', [.9,1.,1.2,1.4]),
        pl.Series('current_face', [1e6,0.,2e6,3e6]))
    data = (book,*rest)
    oas = np.array([.002,.01,.012,.018]) if fixed else None
    expected = run(data,'python',paths,seed,oas)
    actual = run(data,'rust',paths,seed,oas)
    equal_results(expected,actual)
    pos, agg, profile = actual
    assert profile is not None
    assert pos.filter((pl.col('cusip') == book['cusip'][0]) & (pl.col('horizon_m') >= 2))['fwd_value_shock'].abs().sum() == 0
    independently_summed = pos.group_by(['horizon_m','shock_bp']).agg(pl.col('stress_pnl').sum()).sort(['shock_bp','horizon_m'])
    np.testing.assert_allclose(independently_summed['stress_pnl'],agg['pnl_$'],rtol=1e-12,atol=1e-8)


def test_stress_chunk_boundary_and_optional_profile():
    data = inputs(257)
    actual = run(data,'rust',paths=1,shocks=(0.,200.),horizon=2)
    rows = [0,255,256]
    expected = run((data[0][rows],*data[1:]),'python',paths=1,shocks=(0.,200.),horizon=2)
    selected = actual[0].filter(pl.col('cusip').is_in(data[0][rows]['cusip'].to_list()))
    equal_results((expected[0],None),(selected,None))
    assert actual[2] is None


@pytest.mark.parametrize('shocks', [(),(0.,0.),(np.inf,),(-120001.,)])
def test_invalid_scenarios_fail_without_fallback(shocks):
    with pytest.raises(ValueError,match='no fallback'):
        run(inputs(1),'rust',shocks=shocks)


def test_output_admission_precedes_ffi_allocation(monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError('oversized output reached the FFI allocator')
    monkeypatch.setattr(quant_native,'call',forbidden)
    with pytest.raises(ValueError,match='1 GiB admission'):
        run(inputs(1000),'rust',paths=1,shocks=tuple(range(200)),horizon=359)


def test_native_stress_is_identical_across_thread_counts():
    from numba import get_num_threads, set_num_threads
    initial = get_num_threads()
    data = inputs(7)
    try:
        set_num_threads(1)
        one = run(data,'rust',shocks=(100.,-100.,0.))
        set_num_threads(4)
        four = run(data,'rust',shocks=(100.,-100.,0.))
    finally:
        set_num_threads(initial)
    for a,b in zip(one,four):
        assert a.equals(b)


def test_standalone_stress_matches_ffi_and_rejects_invalid_horizons(monkeypatch):
    captured = []
    original = quant_native.call
    def capture(op,a,shapes):
        result = original(op,a,shapes)
        if op == 29:
            request = wire_request(a)
            request.update(schema='mortgage-stress-1',horizons=np.asarray(a[25]).tolist(),shocks_bp=np.asarray(a[26]).tolist())
            captured.append((request,result))
        return result
    monkeypatch.setattr(quant_native,'call',capture)
    run(inputs(2),'rust',paths=1)
    request, expected = captured[0]
    exe = library_path().parent / ('portfolio-mortgage-risk.exe' if os.name == 'nt' else 'portfolio-mortgage-risk')
    result = subprocess.run([str(exe)],input=json.dumps(request),capture_output=True,text=True,timeout=60)
    assert result.returncode == 0, result.stdout+result.stderr
    output = json.loads(result.stdout)['result']
    for name, values in zip(('base_value','base_price','shock_value','pnl','aggregate_base','aggregate_pnl','forward_dv01','oas'),expected):
        np.testing.assert_allclose(output[name],values.ravel(),rtol=1e-10,atol=1e-7)
    for horizons in ([0],[2,1],[1,1],[360],[]):
        request['horizons'] = horizons
        bad = subprocess.run([str(exe)],input=json.dumps(request),capture_output=True,text=True,timeout=10)
        assert bad.returncode == 1 and not json.loads(bad.stdout)['ok']
    request.update(book=request['book']*1000,horizons=list(range(1,360)),shocks_bp=list(range(200)))
    bad = subprocess.run([str(exe)],input=json.dumps(request),capture_output=True,text=True,timeout=10)
    assert bad.returncode == 1 and '1 GiB admission' in json.loads(bad.stdout)['error']
