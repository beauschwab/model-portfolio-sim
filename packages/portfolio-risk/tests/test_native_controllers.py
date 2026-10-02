"""Public calibration/forecast controllers must not call Python financial stages."""
import datetime as dt
from dataclasses import replace
import numpy as np
import polars as pl
from polars.testing import assert_frame_equal
import pytest
from portfolio_risk import demo
from portfolio_risk.core.runtime import RunConfig,run_context
from portfolio_risk.core.native import library_path
from portfolio_risk.analytics import calibration,forecast,balance_calibration
from test_forecast import row

pytestmark=pytest.mark.skipif(not library_path().is_file(),reason='build native products')
CONFIG=RunConfig(7,9,3, compute_backend='python')

def forbidden(*args,**kwargs):raise AssertionError('Python financial stage called')

@pytest.fixture(scope='module')
def inputs():
    bs=demo.model_balance_sheet(scale=.001,basis='amortized_cost',include_markets_bs=True)
    for key in ('mbs','loans','debt','deposits','cds'):bs[key]=bs[key].head(3)
    return bs,*demo.demo_market(),demo.demo_deposit_history()

def test_public_base_calibration_and_spread_are_native(inputs,monkeypatch):
    bs,sr,vp,history=inputs
    with run_context(CONFIG):expected=calibration.calibrate_books(bs,sr,vp,history,seed=19)
    for name in ('factor_loadings','bootstrap_curve','CRN','setup','solve_base_oas'):
        monkeypatch.setattr(calibration,name,forbidden)
    with run_context(replace(CONFIG,compute_backend='rust')):
        actual=calibration.calibrate_books(bs,sr,vp,history,seed=19)
        changed=calibration.spread_oas(actual,.001)
    assert actual.keys()==expected.keys()
    for key in expected:
        np.testing.assert_allclose(actual[key],expected[key],rtol=1e-7,atol=1e-8)
        np.testing.assert_allclose(changed[key],actual[key]+(0 if key=='deposits' else .001),rtol=1e-14,atol=1e-14)

@pytest.mark.parametrize('mode',['empty','mortgage_only','unrelated'])
def test_calibration_keeps_empty_and_unrelated_book_boundaries(inputs,mode):
    bs,sr,vp,history=inputs
    if mode=='empty':book={'mbs':bs['mbs'].head(0),'loans':None,'deposits':None}
    elif mode=='mortgage_only':book={'mbs':bs['mbs'],'mbs_hists':bs['mbs_hists']}
    else:book={'loans':bs['loans'],'asof':bs['asof'],'mm':'outside calibration contract','hedges':'outside calibration contract'}
    with run_context(CONFIG):expected=calibration.calibrate_books(book,sr,vp,history)
    with run_context(replace(CONFIG,compute_backend='rust')):actual=calibration.calibrate_books(book,sr,vp,history)
    assert actual.keys()==expected.keys()
    for key in actual:np.testing.assert_allclose(actual[key],expected[key],rtol=1e-7,atol=1e-8)

@pytest.mark.parametrize('convention',['quarter_average','quarter_end','year_end','date_end'])
def test_raw_forecast_compilation_and_replay(inputs,monkeypatch,convention):
    bs,sr,vp,history=inputs
    rows=[row('2026-01-01','short_rate',4,convention),row('2026-04-01','short_rate',2,convention),
        row('2026-01-01','rate_5y',3,convention),row('2026-01-01','mortgage_rate',5,convention),
        row('2025-10-01','hpi',200,'quarter_end','history','index'),
        row('2026-01-01','hpi',180,'quarter_end',unit='index')]
    with run_context(CONFIG):
        plan=forecast.compile_forecast(rows,'baseline','2026-01-01',3)
        expected=forecast.run_forecast_nii(bs,sr,vp,history,plan,seed=19)
    from portfolio_risk.analytics import accounting
    monkeypatch.setattr(accounting,'run_balance_sheet_nii',forbidden)
    monkeypatch.setattr(forecast,'condition_paths',forbidden)
    monkeypatch.setattr(np,'interp',forbidden)
    with run_context(replace(CONFIG,compute_backend='rust')):
        native_plan=forecast.compile_forecast(rows,'baseline','2026-01-01',3)
        actual=forecast.run_forecast_nii(bs,sr,vp,history,native_plan,seed=19)
    for key in ('coverage','warnings','unused_variables'):assert native_plan[key]==plan[key]
    for key,value in plan['targets'].items():np.testing.assert_allclose(native_plan['targets'][key],value,rtol=1e-13,atol=1e-14)
    for key,value in expected.items():
        if isinstance(value,pl.DataFrame):assert_frame_equal(actual[key],value,check_dtypes=False,rel_tol=1e-7,abs_tol=1e-5)
        else:assert actual[key]==value

@pytest.mark.parametrize('singular',[False,True])
def test_joint_driver_fit_selection_and_holdout_are_native(monkeypatch,singular):
    rng=np.random.default_rng(4)
    values=rng.normal(size=(70,4))*.005+[.01,.01,.05,.2]
    if singular:values[:,3]=values[:,2]*2
    dates=[dt.date(2026,1,1)+dt.timedelta(days=i) for i in range(70)]
    history=pl.DataFrame({'date':dates,**dict(zip(balance_calibration.DRIVERS,values.T))})
    with run_context(CONFIG):
        expected=balance_calibration.fit_joint_drivers(history,dates[49],'fixture')
        scenarios=balance_calibration.empirical_joint_scenarios(history,dates[49])
    monkeypatch.setattr(np,'cov',forbidden);monkeypatch.setattr(np.linalg,'pinv',forbidden)
    with run_context(replace(CONFIG,compute_backend='rust')):
        actual=balance_calibration.fit_joint_drivers(history,dates[49],'fixture')
        selected=balance_calibration.empirical_joint_scenarios(history,dates[49])
        changed=history.with_columns(pl.when(pl.col('date')>dates[49]).then(pl.col('market_shock')+5).otherwise(pl.col('market_shock')).alias('market_shock'))
        held=balance_calibration.empirical_joint_scenarios(changed,dates[49])
    for key in ('mean','covariance','holdout_bias','holdout_rmse','holdout_outside_three_sigma'):
        np.testing.assert_allclose(actual.pop(key),expected.pop(key),rtol=1e-10,atol=1e-12)
    assert actual==expected and selected==scenarios and held==selected

@pytest.mark.parametrize('mutation',[
    lambda rows:rows.append(row('2026-01-01','short_rate',3)),
    lambda rows:rows.append(row('2026-01-01','hpi',200,'quarter_end',unit='index')),
    lambda rows:rows[0].update(unit='index'),
    lambda rows:rows[0].update(value=-3),
])
def test_forecast_invalid_raw_inputs_fail_on_both_backends(mutation):
    rows=[row('2026-01-01','short_rate',4)];mutation(rows)
    for backend in ('python','rust'):
        with run_context(replace(CONFIG,compute_backend=backend)),pytest.raises(ValueError):
            forecast.compile_forecast(rows,'baseline','2026-01-01',3)
