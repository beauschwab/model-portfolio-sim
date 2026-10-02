"""Rust market-context ownership and independent curve/path reference gates."""
import copy
import importlib

import numpy as np
import pytest

from portfolio_risk.core import curve, scenarios, quant_native
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.core.native import library_path
mdl = importlib.import_module('portfolio_risk.models.models')
from test_native_products import env, equal  # shared calibrated reference fixture

pytestmark = pytest.mark.skipif(not library_path().is_file(), reason='build native product backend')


@pytest.mark.parametrize('mortgage', [False, True])
def test_market_context_owns_every_path_stage(env, monkeypatch, mortgage):
    _,sr,vp,_,_,models,b,p,_,crn,_ = env
    fn = scenarios.build_paths if mortgage else scenarios.build_rate_paths
    args = (sr,vp,p,b,models,crn) if mortgage else (sr,vp,p,b,crn)
    reference = fn(*args)
    def forbidden(*a, **kw):
        raise AssertionError('Python financial stage executed inside native market context')
    for name in ('bootstrap_curve','forwards_from_dfs','simulate_rates','vol_feature_paths','calibrate_abcd'):
        monkeypatch.setattr(scenarios,name,forbidden)
    for name in ('cc_paths','ps_paths','hpi_paths','yoy_from_hpi'):
        monkeypatch.setattr(mdl,name,forbidden)
    calls = []
    original = quant_native.call
    def capture(op,*a,**kw):
        calls.append(op)
        return original(op,*a,**kw)
    monkeypatch.setattr(quant_native,'call',capture)
    with run_context(RunConfig(compute_backend='rust')):
        actual = fn(*args)
    assert calls == [20 if mortgage else 19]
    equal(reference,actual,rtol=1e-7,atol=1e-9)


@pytest.mark.parametrize('seed', range(6))
def test_native_curve_matches_scipy_and_reprices_quotes(seed, monkeypatch):
    from portfolio_risk.core.config import SWAP_TENORS, TENOR
    rng = np.random.default_rng(seed)
    # Smooth quote sets remain inside both implementations' root bracket.
    rates = .035 + np.cumsum(rng.uniform(-.0005,.0005,len(SWAP_TENORS)))
    expected = curve.bootstrap_curve(SWAP_TENORS,rates)
    def forbidden(*a,**kw):
        raise AssertionError('native bootstrap invoked scipy root solver')
    monkeypatch.setattr(curve,'brentq',forbidden)
    with run_context(RunConfig(compute_backend='rust')):
        actual = curve.bootstrap_curve(SWAP_TENORS,rates)
    np.testing.assert_allclose(actual,expected,rtol=1e-11,atol=1e-12)
    for t,r in zip(SWAP_TENORS,rates):
        annual=actual[(np.arange(1,int(t)+1)/TENOR).astype(int)]
        assert r*annual.sum()+annual[-1] == pytest.approx(1.,abs=1e-12)


def test_native_market_reuse_invalidates_changed_curves_and_draws(env, monkeypatch):
    _,sr,vp,_,_,models,b,p,_,crn,_ = env
    tape = crn.Z.copy()
    calls=[]
    original=quant_native.call
    def capture(op,*a,**kw):
        calls.append(op)
        return original(op,*a,**kw)
    monkeypatch.setattr(quant_native,'call',capture)
    with run_context(RunConfig(compute_backend='rust')) as ctx:
        a=scenarios.build_paths(sr,vp,p,b,models,crn)
        cached=scenarios.build_paths(sr.copy(),vp,p,b,copy.deepcopy(models),crn)
        equal(a,cached,rtol=0,atol=0)
        assert calls == [20] and ctx.hits == 1
        changed=scenarios.build_paths(sr+.001,vp,p,b,models,crn)
        assert not np.array_equal(a['df'],changed['df'])
        other=copy.deepcopy(crn);other.Z[0,0,0] += .1
        changed_draws=scenarios.build_paths(sr,vp,p,b,models,other)
        assert not np.array_equal(a['swaps'],changed_draws['swaps'])
        assert calls == [20]*3
    with run_context(RunConfig(compute_backend='rust')):
        scenarios.build_paths(sr,vp,p,b,models,crn)
    assert calls == [20]*4
    np.testing.assert_array_equal(crn.Z,tape)


@pytest.mark.parametrize('change',['empty','shape','nonfinite','duplicate','unbracketed'])
def test_native_bootstrap_rejects_bad_inputs(change):
    tenors=np.array([1.,2.,5.]); rates=np.array([.03,.035,.04])
    if change=='empty': tenors=rates=np.array([])
    if change=='shape': rates=rates[:-1]
    if change=='nonfinite': rates[0]=np.inf
    if change=='duplicate': tenors[1]=tenors[0]
    if change=='unbracketed': rates[0]=-2.
    with run_context(RunConfig(compute_backend='rust')):
        with pytest.raises(ValueError): curve.bootstrap_curve(tenors,rates)


def test_native_curve_recalibration_remains_explicit(env, monkeypatch):
    _,sr,vp,_,_,models,b,p,_,crn,_=env
    called=[]
    def calibration(*args,**kwargs):
        called.append(kwargs.get('x0'))
        return p
    monkeypatch.setattr(scenarios,'calibrate_abcd',calibration)
    with run_context(RunConfig(compute_backend='rust')):
        a=scenarios.build_paths(sr,vp,p,b,models,crn)
        c=scenarios.build_paths(sr,vp,p,b,models,crn,recalibrate=True,abcd_warm=p)
    assert len(called)==1
    equal(a,c,rtol=0,atol=0)


@pytest.mark.parametrize('shock',[-200.,0.,200.])
def test_forward_shock_lifecycle_runs_without_python_models(env,monkeypatch,shock):
    *_,crn,paths=env
    models=env[5]
    expected=scenarios.shocked_paths(paths,6,shock,models)
    def forbidden(*a,**kw):
        raise AssertionError('native parallel shock invoked Python behavioral logic')
    monkeypatch.setattr(mdl.TrendingCC,'shock_response',forbidden)
    monkeypatch.setattr(mdl.RateLinkedHPI,'shock_multiplier',forbidden)
    monkeypatch.setattr(mdl,'yoy_from_hpi',forbidden)
    with run_context(RunConfig(compute_backend='rust')):
        actual=scenarios.shocked_paths(paths,6,shock,models)
    equal(expected,actual,rtol=1e-7,atol=1e-9)
    assert actual['short'] is paths['short'] and actual['swaps'] is paths['swaps']
