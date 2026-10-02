"""Native behavioral calibration and PCA ownership, including singular designs."""
import importlib
import numpy as np
import polars as pl
import pytest

from portfolio_risk import demo
from portfolio_risk.core import vol
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import RunConfig, run_context

models=importlib.import_module('portfolio_risk.models.models')
pytestmark=pytest.mark.skipif(not library_path().is_file(),reason='build native product backend')


@pytest.mark.parametrize('kind',['cc','ps','pca'])
def test_fits_do_not_call_numpy_linear_algebra(monkeypatch,kind):
    cc,ps=demo.demo_histories()
    functions={'cc':lambda:models.fit_current_coupon(cc),'ps':lambda:models.fit_ps_spread(ps),'pca':vol.factor_loadings}
    expected=functions[kind]()
    vol.factor_loadings.cache_clear()
    def forbidden(*a,**kw):
        raise AssertionError('Python linear algebra executed in native calibration')
    monkeypatch.setattr(np.linalg,'lstsq',forbidden)
    monkeypatch.setattr(np.linalg,'eigh',forbidden)
    with run_context(RunConfig(compute_backend='rust')):
        actual=functions[kind]()
    if isinstance(expected,dict):
        for key in expected: np.testing.assert_allclose(actual[key],expected[key],rtol=1e-10,atol=1e-12)
    else:
        np.testing.assert_allclose(actual,expected,rtol=1e-11,atol=1e-13)
        assert not actual.flags.writeable


@pytest.mark.parametrize('singular',[False,True])
@pytest.mark.parametrize('seed',range(5))
def test_coupon_fitted_function_and_adjustment_match(seed,singular):
    rng=np.random.default_rng(seed)
    features=rng.normal(size=(80,10))*.01
    if singular:
        features[:,2]=features[:,1]
        features[:,8]=0.
    fair=features@np.linspace(-.1,.4,10)+.03
    observed=np.empty(80);observed[0]=fair[0]
    for i in range(1,80): observed[i]=observed[i-1]+.35*(fair[i]-observed[i-1])+rng.normal()*1e-5
    hist=pl.DataFrame({name:features[:,i] for i,name in enumerate(models.CC_FEATURES)}|{'cc':observed})
    expected=models.fit_current_coupon(hist)
    with run_context(RunConfig(compute_backend='rust')): actual=models.fit_current_coupon(hist)
    design=np.column_stack([features,np.ones(80)])
    np.testing.assert_allclose(design@actual['beta'],design@expected['beta'],rtol=1e-11,atol=1e-13)
    np.testing.assert_allclose(actual['beta'],expected['beta'],rtol=1e-10,atol=1e-12)
    assert actual['lam']==pytest.approx(expected['lam'],rel=1e-10,abs=1e-12)


@pytest.mark.parametrize('phi',[-.3,.5,.9999,1.01])
def test_spread_fit_clipping_and_residual_variance(phi):
    rng=np.random.default_rng(19)
    history=[.012]
    for _ in range(90):history.append(phi*history[-1]+.001+rng.normal()*.0001)
    hist=pl.DataFrame({'ps':history})
    expected=models.fit_ps_spread(hist)
    with run_context(RunConfig(compute_backend='rust')): actual=models.fit_ps_spread(hist)
    for key in expected: assert actual[key]==pytest.approx(expected[key],rel=1e-10,abs=1e-12)


def test_factor_cache_keeps_backend_ownership_separate(monkeypatch):
    from portfolio_risk.core import quant_native
    vol.factor_loadings.cache_clear()
    reference=vol.factor_loadings()
    calls=[];original=quant_native.call
    def capture(op,*a,**kw):calls.append(op);return original(op,*a,**kw)
    monkeypatch.setattr(quant_native,'call',capture)
    with run_context(RunConfig(compute_backend='rust')):
        native=vol.factor_loadings()
        assert vol.factor_loadings() is native
    assert calls==[24]
    assert vol.factor_loadings() is reference
    np.testing.assert_allclose(native,reference,rtol=1e-11,atol=1e-13)


@pytest.mark.parametrize('op,inputs,shapes',[
    (22,[np.zeros((1,11))],[(11,),(2,)]),
    (22,[np.full((2,11),np.inf)],[(11,),(2,)]),
    (23,[[.01],1./12.],[(3,)]),
    (23,[[.01,.02,.03],0.],[(3,)]),
    (24,[161,4,.25,.1],[(161,4)]),
])
def test_bad_calibration_inputs_fail_closed(op,inputs,shapes):
    from portfolio_risk.core.quant_native import call
    with pytest.raises(ValueError):call(op,inputs,shapes)
