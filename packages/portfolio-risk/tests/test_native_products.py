"""Independent native product outputs, final drivers and failure contracts."""
import datetime as dt
import numpy as np
import pytest
import polars as pl

from portfolio_risk import demo
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.core.scenarios import CRN, setup, build_paths, run_engine
from portfolio_risk.core.native import library_path

pytestmark = pytest.mark.skipif(not library_path().is_file(), reason='build native product backend')


def both(fn):
    values=[]
    for backend in ('python','rust'):
        with run_context(RunConfig(n_paths=7,n_paths_base=9,horizon=3,compute_backend=backend)):
            values.append(fn())
    return values


def equal(a,b,rtol=1e-10,atol=1e-10):
    if isinstance(a,pl.DataFrame):
        assert a.columns==b.columns
        for col,dtype in a.schema.items():
            if dtype.is_numeric(): equal(a[col].to_numpy(),b[col].to_numpy(),rtol,atol)
            else: assert a[col].to_list()==b[col].to_list()
    elif isinstance(a,str) or a is None:
        assert a==b
    elif isinstance(a,dict):
        assert a.keys()==b.keys()
        for k in a: equal(a[k],b[k],rtol,atol)
    elif isinstance(a,(tuple,list)):
        assert len(a)==len(b)
        for x,y in zip(a,b): equal(x,y,rtol,atol)
    else:
        np.testing.assert_allclose(a,b,rtol=rtol,atol=atol)


@pytest.fixture(scope='module')
def env():
    port=demo.demo_portfolio(8);sr,vp=demo.demo_market();cc,ps=demo.demo_histories()
    models,b,abcd,sec,tgt,face=setup(port,sr,vp,cc,ps)
    crn=CRN(7,19)
    paths=build_paths(sr,vp,abcd,b,models,crn)
    return port,sr,vp,cc,ps,models,b,abcd,sec,crn,paths


def test_native_lmm_shared_draws(env):
    _,sr,vp,_,_,models,b,abcd,_,crn,_=env
    equal(*both(lambda:build_paths(sr,vp,abcd,b,models,crn)),rtol=1e-7,atol=1e-9)


@pytest.mark.parametrize('forward',[False,True])
def test_native_mortgage_all_outputs(env,forward):
    *_,sec,crn,paths=env
    equal(*both(lambda:run_engine(paths,sec,oas=np.linspace(-.01,.03,8),
        horizons=np.array([1,13,27]),want_fwd=forward)))


def test_native_mortgage_stress_restarts(env):
    from portfolio_risk.core.config import MOY, SEASONALITY, PREPAY_PARAMS
    from portfolio_risk.models.prepay import LTV_KNOTS,LTV_COEFS,SMM_LUT,SMM_SCALE,BURN_LUT,BURN_SCALE
    from portfolio_risk.core.kernels import stress_engine
    *_,sec,crn,paths=env
    oas=np.full(8,.012);hz=np.array([1,13,27])
    base=run_engine(paths,sec,oas=oas,horizons=hz,want_fwd=True)
    for i,h in enumerate(hz):
        equal(*both(lambda:stress_engine(paths['mtg'],paths['hpi'],paths['yoy'],paths['df'],
            MOY,SEASONALITY,PREPAY_PARAMS,LTV_KNOTS,LTV_COEFS,SMM_LUT,SMM_SCALE,BURN_LUT,BURN_SCALE,
            *sec,oas,h,i,base[3],base[4],True)))


@pytest.mark.parametrize('product',['corp','cd','deposit'])
def test_native_contract_cashflows(env,product):
    from portfolio_risk.products.corp import CorpDeck,_corp_full
    from portfolio_risk.products.cds import CDDeck,_cd_full
    from portfolio_risk.products.deposits import DepositDeck,LogisticBetaECM,_deposit_A
    paths=env[-1];asof=dt.date(2026,6,1)
    if product=='corp':
        deck=CorpDeck(demo.model_balance_sheet(scale=.001)['loans'].head(12),asof);fn=lambda:_corp_full(deck,paths)
    elif product=='cd':
        deck=CDDeck(demo.demo_cd_book(12,asof=asof),asof);fn=lambda:_cd_full(deck,paths)
    else:
        deck=DepositDeck(demo.demo_deposit_book(8));m=LogisticBetaECM();params=m.fit(demo.demo_deposit_history())
        dep=m.paths(paths['short'].astype(float),params,.01)
        fn=lambda:_deposit_A(deck,paths,dep,.01,oas=np.full(8,.012),horizons=np.array([1,13,27]),want_fwd=True)
    equal(*both(fn))


def test_native_oas_roundtrip_and_failed_solve():
    from portfolio_risk.core.pricing import pv_from_A,solve_oas_from_A
    rng=np.random.default_rng(10);a=rng.uniform(0,.01,(9,360));o=np.linspace(-.02,.04,9);delay=np.arange(9)/365
    target=pv_from_A(a,o,7,delay)
    equal(*both(lambda:solve_oas_from_A(a,7,target,delay_y=delay)),rtol=1e-10,atol=1e-10)
    with run_context(RunConfig(compute_backend='rust')):
        with pytest.raises(ValueError,match='no fallback'):
            solve_oas_from_A(a,7,np.full(9,1e20))


def test_native_boundary_atomic_failure_and_input_ownership():
    import ctypes
    from portfolio_risk.core.quant_native import Buffer, _load, call
    source=np.array([[.01,.02],[.03,.04]])
    before=source.copy()
    call(16,[source,[100.],[0.],[1.],2],[(2,),(2,)])
    np.testing.assert_array_equal(source,before)
    with pytest.raises(ValueError,match='no fallback'):
        call(16,[source,[100.,200.],[0.],[1.],2],[(2,),(2,)])
    # Invalid result shape must not publish even the first valid output.
    inputs=[source,np.array([100.]),np.array([0.]),np.array([1.]),np.array([2.])]
    outputs=[np.full(2,123.),np.full(3,456.)]
    def descriptors(arrays):
        return (Buffer*len(arrays))(*(Buffer(a.ctypes.data_as(ctypes.POINTER(ctypes.c_double)),a.size,
            (ctypes.c_size_t*3)(*a.shape,*([1]*(3-a.ndim)))) for a in arrays))
    _,invoke=_load(str(library_path()))
    assert invoke(16,descriptors(inputs),5,descriptors(outputs),2,1)==1
    np.testing.assert_array_equal(outputs[0],[123.,123.])
    np.testing.assert_array_equal(outputs[1],[456.,456.,456.])


def test_native_products_do_not_invoke_reference_and_threads_are_deterministic(env):
    from numba import get_num_threads,set_num_threads,config
    from portfolio_risk.core.quant_native import dispatch
    from portfolio_risk.core import kernels
    import inspect
    original=kernels.engine.__wrapped__
    def forbidden(*args,**kwargs):
        raise AssertionError('Python product fallback was executed')
    forbidden.__signature__=inspect.signature(original)
    native=dispatch('mbs',forbidden)
    # Reconstruct the normal wrapper's inputs with a capture callable.
    from unittest.mock import patch
    captured=[]
    with patch('portfolio_risk.core.scenarios.engine',side_effect=lambda *a:captured.append(a)):
        run_engine(env[-1],env[-3])
    old=get_num_threads()
    try:
        with run_context(RunConfig(compute_backend='rust')):
            set_num_threads(1);a=native(*captured[0])
            set_num_threads(min(2,config.NUMBA_NUM_THREADS));b=native(*captured[0])
        equal(a,b,rtol=0,atol=0)
    finally: set_num_threads(old)


def test_custom_models_fail_explicitly(env):
    from portfolio_risk.core.interfaces import ModelSuite
    from portfolio_risk.models.models import TrendingCC
    class CustomCC(TrendingCC): pass
    suite=ModelSuite.default();suite.cc=CustomCC()
    _,sr,vp,_,_,models,b,abcd,_,crn,_=env
    with run_context(RunConfig(compute_backend='rust')):
        with pytest.raises(ValueError,match='Custom Python model'):
            build_paths(sr,vp,abcd,b,models,crn,suite=suite)


def test_backend_switch_cannot_reuse_python_product_cache(env,small_book):
    from portfolio_risk.analytics.incremental import price_books,SUPPORTED_BOOKS
    from portfolio_risk.core.dependency import DependencyCache
    from test_incremental import equivalent,computed
    _,sr,vp,*_=env
    args=dict(books={k:small_book[k] for k in SUPPORTED_BOOKS},asof=small_book['asof'],
              swap_rates=sr,vol_pts=vp,dep_hist=demo.demo_deposit_history(),mbs_hists=small_book['mbs_hists'])
    cache=DependencyCache()
    python=price_books(**args,config=RunConfig(7,9,3, compute_backend='python'),cache=cache)
    assert cache._native is None and cache._entries and cache._shared
    config=RunConfig(7,9,3,compute_backend='rust')
    rust=price_books(**args,config=config,cache=cache)
    # Shared paths now live in the separate Rust market cache, so Python graph
    # path counters no longer describe this boundary. Check ownership directly.
    assert cache._native is not None and not cache._entries and not cache._shared
    assert computed(rust,'cashflows:')==sum(len(v) for v in args['books'].values())
    equivalent(python,rust)
    warm=price_books(**args,config=config,cache=cache)
    assert sum(s['computed'] for s in warm['graph'].values())==0


def test_native_optimizer_monthly_funding_and_late_purchase_replay():
    from test_review_regressions import (test_optimizer_requires_funding_by_default,
                                        test_optimizer_replays_late_purchases_and_base_earnings)
    with run_context(RunConfig(compute_backend='rust')):
        test_optimizer_requires_funding_by_default()
        test_optimizer_replays_late_purchases_and_base_earnings()


@pytest.mark.parametrize('binding',['lcr','nsfr','commercial'])
def test_native_optimizer_binding_limits(binding):
    from test_review_regressions import synthetic_library
    from portfolio_risk.strategy.optimizer import optimize_balance_sheet
    lib,base=synthetic_library();constraints={}
    if binding=='lcr':
        lib['templates']['asset']['outflow30']=1.
        base['lcr'].update({'hqla_$':30.,'hqla_l1_$':30.,'net_outflows_$':10.})
    elif binding=='nsfr':
        lib['templates']['asset']['rsf']=1.
        base['nsfr']={'asf_$':30.,'rsf_$':10.}
    else:
        constraints['commercial']=[dict(label='asset cap',template='asset',sense='<=',rhs=20.)]
    for result in both(lambda:optimize_balance_sheet([(lib,base)],lcr_min=1.,nsfr_min=1.,
        max_total_assets=100.,cash_budget=100.,**constraints)):
        assert result['validated']
        assert result['total_new_assets_$']==pytest.approx(20.,abs=1e-7)


@pytest.fixture(scope='module')
def small_book():
    bs=demo.model_balance_sheet(scale=.001)
    for key in ('mbs','loans','debt','cds','deposits'):
        bs[key]=bs[key].head(3)
    return bs


@pytest.mark.parametrize('product',['mbs','corp','cd','deposit','mbs_stress','deposit_stress','nii','kpis','unitlib','hedges'])
def test_final_native_driver_parity(env,small_book,product):
    from portfolio_risk.analytics.risk import run_risk
    from portfolio_risk.analytics.stress import run_stress
    from portfolio_risk.products.corp import run_corp_risk
    from portfolio_risk.products.cds import run_cd_risk
    from portfolio_risk.products.deposits import run_deposit_risk,run_deposit_stress
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.analytics.kpis import compute_kpis
    from portfolio_risk.strategy.unitlib import build_unit_library
    from portfolio_risk.products.hedges import run_hedge_risk
    _,sr,vp,cc,ps,*_=env;bs=small_book;dh=demo.demo_deposit_history();asof=dt.date(2026,6,10)
    swaps,options=demo.demo_hedge_book(.001,asof)
    calls={
        'mbs':lambda:run_risk(bs['mbs'],sr,vp,cc,ps),
        'corp':lambda:run_corp_risk(bs['loans'],asof,sr,vp,cc,ps),
        'cd':lambda:run_cd_risk(bs['cds'],asof,sr,vp),
        'deposit':lambda:run_deposit_risk(bs['deposits'],sr,vp,dh),
        'mbs_stress':lambda:run_stress(bs['mbs'],sr,vp,cc,ps,shocks_bp=[0,200]),
        'deposit_stress':lambda:run_deposit_stress(bs['deposits'],sr,vp,dh,shocks_bp=[0,200]),
        'nii':lambda:run_balance_sheet_nii(bs,sr,vp,dh,horizon=3),
        'kpis':lambda:compute_kpis(bs,sr,vp,dh),
        'unitlib':lambda:build_unit_library(sr,vp,(cc,ps),dh,grid_m=[0,2],horizon=3),
        'hedges':lambda:run_hedge_risk(swaps,options,asof,sr,vp,horizon=3),
    }
    equal(*both(calls[product]),rtol=1e-7,atol=1e-5)


@pytest.mark.parametrize('floating',[False,True])
def test_native_forward_program(env,floating):
    from portfolio_risk.strategy.strategies import program_cashflows,fwd_dv01_profile
    prog=dict(rate_ref='s5',spread_bp=35,term_m=8,start_m=2,end_m=7,side='liability',
              amort='cpr',cpr_annual=.13,reinvest_frac=.5,is_float=floating)
    equal(*both(lambda:(program_cashflows(prog,env[-1],12,np.arange(12)*1000),
                       fwd_dv01_profile(prog,env[-1],12,np.arange(12)*1000))))


def test_analytic_vol_jacobian_and_calibration(env):
    from portfolio_risk.core.vol import model_swaption_value_jac,model_swaption_vol,calibrate_abcd
    from portfolio_risk.core.curve import bootstrap_curve,forwards_from_dfs
    from portfolio_risk.core.config import SWAP_TENORS
    _,sr,vp,_,_,_,b,p,*_=env;df=bootstrap_curve(SWAP_TENORS,sr);f=forwards_from_dfs(df)
    values=both(lambda:model_swaption_value_jac(0.,3.,10.,p,f,df,b));equal(*values,atol=1e-12)
    for j in range(4):
        bump=np.zeros(4);bump[j]=1e-5
        finite=(model_swaption_vol(0.,3.,10.,p+bump,f,df,b)-model_swaption_vol(0.,3.,10.,p-bump,f,df,b))/2e-5
        assert finite==pytest.approx(values[0][1][j],rel=1e-6,abs=1e-9)
    equal(*both(lambda:calibrate_abcd(vp,f,df,b)),rtol=1e-8,atol=1e-9)


def test_public_optimizer_native_parity_and_infeasibility(env,small_book):
    from portfolio_risk.strategy.unitlib import build_unit_library
    from portfolio_risk.analytics.kpis import compute_kpis
    from portfolio_risk.strategy.optimizer import optimize_balance_sheet
    from test_decision import LIMITS
    _,sr,vp,cc,ps,*_=env;dh=demo.demo_deposit_history()
    with run_context(RunConfig(7,9,3, compute_backend='python')):
        lib=build_unit_library(sr,vp,(cc,ps),dh,grid_m=[0,2],horizon=3)
        base=compute_kpis(small_book,sr,vp,dh)
    a,b=both(lambda:optimize_balance_sheet([(lib,base)],**LIMITS))
    assert a['validated'] and b['validated']
    equal(a['worst_case_nii_$'],b['worst_case_nii_$'],rtol=1e-9,atol=.01)
    impossible=LIMITS|{'commercial':[dict(label='impossible',template='ALL_ASSET',sense='>=',rhs=1e10)]}
    for result in both(lambda:optimize_balance_sheet([(lib,base)],**impossible)):
        assert not result['feasible']


def test_saved_book_and_candidate_native_parity(env,small_book):
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.analytics.balance_workflow import run_saved_book_stress
    from test_balance_ledger import meta
    from test_balance_stress import small
    _,sr,vp,*_=env;bs=small_book;dh=demo.demo_deposit_history()
    with run_context(RunConfig(7,9,3, compute_backend='python')):
        accounting=run_balance_sheet_nii(bs,sr,vp,dh,horizon=1,capture_cashflows=True)
    opening=accounting['instrument_openings'].to_dicts();scale=1e-6
    net=sum((r['balance']+r['book_adjustment'])*(1 if r['side']=='asset' else -1) for r in opening)*scale
    spec=small();spec.update(horizon_days=30,positions=[])
    cash=max(-net,0)+100;spec['accounts'][0].update(cash=cash,equity=cash+net)
    mapping={f"{r['book']}:{r['id']}":meta('loan' if r['side']=='asset' else 'funding') for r in opening}
    request=dict(specification=spec,position_mapping=mapping,amount_scale=scale,
        allocation=[dict(template='cml_fixed_5y',purchase_m=0,notional=1e6)],
        template_mapping={'cml_fixed_5y':meta('loan')})
    a,b=both(lambda:run_saved_book_stress(bs,sr,vp,dh,request))
    assert b['execution']['financial_events']=='rust'
    assert b['execution']['partition_validation']['journal_replayed']
    for key,value in a.items():
        if isinstance(value,pl.DataFrame):
            # Legacy empty reports infer no columns; partitioned output has an
            # explicit schema. Compare named financial columns in row order.
            assert value.height==b[key].height
            if value.width: equal(value,b[key].select(value.columns),rtol=1e-10,atol=1e-8)
