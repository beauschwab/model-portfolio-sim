"""One native mortgage lifecycle from raw books/history to final public risk."""
import importlib
import json
import os
import subprocess
import numpy as np
import polars as pl
import pytest

from portfolio_risk import demo
from portfolio_risk.core import quant_native, curve, vol, scenarios
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.core.native import library_path

risk = importlib.import_module('portfolio_risk.analytics.risk')
models = importlib.import_module('portfolio_risk.models.models')
prepay = importlib.import_module('portfolio_risk.models.prepay')
pytestmark = pytest.mark.skipif(not library_path().is_file(), reason='build native product backend')


def inputs(n=4):
    rates, quotes = demo.demo_market()
    return demo.demo_portfolio(n), rates, quotes, *demo.demo_histories()


def run(data, backend, paths=5, seed=19, oas=None):
    with run_context(RunConfig(n_paths=paths, n_paths_base=paths+2, compute_backend=backend)):
        return risk.run_risk(*data, seed=seed, oas=oas)


def equal_frames(expected, actual):
    assert actual.columns == expected.columns
    columns = [c for c in actual.columns if c not in ('cusip','state','channel')]
    np.testing.assert_allclose(actual[columns].to_numpy(), expected[columns].to_numpy(), rtol=1e-7, atol=1e-5)


def test_native_owns_all_financial_stages(monkeypatch):
    data = inputs()
    expected = run(data, 'python')
    def forbidden(*args, **kwargs):
        raise AssertionError('Python financial stage executed in native mortgage risk')
    for module, names in [(risk, ['setup','CRN','build_paths','run_engine','solve_base_oas','pv_from_A']),
                          (models, ['fit_current_coupon','fit_ps_spread']),
                          (vol, ['calibrate_abcd','factor_loadings']),
                          (curve, ['bootstrap_curve']),
                          (prepay, ['static_multipliers']),
                          (np.random, ['default_rng'])]:
        for name in names: monkeypatch.setattr(module, name, forbidden)
    calls=[];original=quant_native.call
    def capture(op,*args,**kwargs):
        calls.append(op)
        return original(op,*args,**kwargs)
    monkeypatch.setattr(quant_native,'call',capture)
    actual=run(data,'rust')
    assert calls==[28]
    equal_frames(expected,actual)


@pytest.mark.parametrize('seed,paths',[(0,1),(27,3),(2**64-1,8)])
@pytest.mark.parametrize('fixed',[False,True])
def test_final_risk_with_delays_hpi_and_fixed_spreads(seed,paths,fixed):
    book,*rest=inputs()
    book=book.with_columns(pl.Series('pay_delay_days',[0.,24.,45.,55.]),pl.Series('hpi_orig_ratio',[.9,1.,1.2,1.4]))
    oas=np.array([.002,.01,.012,.018]) if fixed else None
    data=(book,*rest)
    expected=run(data,'python',paths,seed,oas)
    actual=run(data,'rust',paths,seed,oas)
    equal_frames(expected,actual)
    krd=[c for c in actual.columns if c.startswith('krd01_')]
    np.testing.assert_allclose(actual[krd].to_numpy().sum(1),actual['dv01'].to_numpy(),rtol=0,atol=1e-9)
    if fixed: np.testing.assert_array_equal(actual['oas_bps'].to_numpy(),oas*1e4)


def test_base_chunk_boundary_does_not_change_calibration():
    data=inputs(257)
    actual=run(data,'rust',paths=1)
    rows=[0,255,256]
    subset=(data[0][rows],*data[1:])
    expected=run(subset,'python',paths=1)
    equal_frames(expected,actual[rows])


def test_category_assumption_tables_are_inputs(monkeypatch):
    monkeypatch.setitem(prepay.STATE_MULT,'OH',1.27)
    monkeypatch.setitem(prepay.CHANNEL_MULT,'C',0.95)
    book,*rest=inputs(2)
    book=book.with_columns(pl.Series('state',['OH','CA']),pl.Series('channel',['C','R']))
    data=(book,*rest)
    equal_frames(run(data,'python',paths=1),run(data,'rust',paths=1))


@pytest.mark.parametrize('column,value',[('price',-1.),('wam',0.),('wac',0.),('factor',0.),('pay_delay_days',-1.),('hpi_orig_ratio',0.)])
def test_invalid_book_fails_without_fallback(column,value):
    book,*rest=inputs(1)
    book=book.with_columns(pl.lit(value).alias(column))
    with pytest.raises(ValueError,match='no fallback'):
        run((book,*rest),'rust')


def wire_request(a):
    c=np.asarray(a[8]).tolist()
    keys=['base_paths','sensitivity_paths','months','forwards','factors','dt','tenor','shift','float32_paths','curve_bump','vol_bump']
    cfg=dict(zip(keys,c[:11]))
    for key in keys[:5]: cfg[key]=int(cfg[key])
    cfg['float32_paths']=bool(cfg['float32_paths'])
    cfg.update(hpi=c[11:14],incentive_lag=int(c[14]),ps_spot=c[15],rational_sigmoid=bool(c[16]))
    flat=lambda i:np.asarray(a[i]).ravel().tolist()
    return dict(schema='mortgage-risk-1',threads=2,tenors=flat(0),swap_rates=flat(1),vol_quotes=flat(2),cc_history=flat(3),ps_history=flat(4),book=flat(5),original_hpi=flat(6),seed=[int(x) for x in flat(7)],fixed_oas=flat(9),config=cfg,
        prepay=dict(month_of_year=flat(10),seasonality=flat(11),parameters=flat(12),ltv_knots=flat(13),ltv_coefficients=flat(14),smm_table=flat(15),smm_scale=a[16][0],burnout_scale=a[16][1],burnout_table=flat(17),cc_vol_points=flat(18),fico_x=flat(19),fico_y=flat(20),size_x=flat(21),size_y=flat(22),state_multipliers=flat(23),channel_multipliers=flat(24)))


def test_standalone_executable_matches_native_call(monkeypatch):
    original=quant_native.call;captured=[]
    def capture(op,a,shapes):
        result=original(op,a,shapes)
        if op==28: captured.append((wire_request(a),result))
        return result
    monkeypatch.setattr(quant_native,'call',capture)
    run(inputs(2),'rust',paths=1)
    exe=library_path().parent/('portfolio-mortgage-risk.exe' if os.name=='nt' else 'portfolio-mortgage-risk')
    assert exe.is_file(),'build standalone mortgage executable'
    request,expected=captured[0]
    process=subprocess.run([str(exe)],input=json.dumps(request),capture_output=True,text=True,timeout=60)
    assert process.returncode==0,process.stderr+process.stdout
    output=json.loads(process.stdout)
    assert output['ok']
    for name,values in zip(('oas','price','dv01','sensitivities'),expected):
        np.testing.assert_allclose(output['result'][name],values.ravel(),rtol=1e-10,atol=1e-7)
    request['schema']='unsupported'
    bad=subprocess.run([str(exe)],input=json.dumps(request),capture_output=True,text=True,timeout=10)
    assert bad.returncode==1 and not json.loads(bad.stdout)['ok']
