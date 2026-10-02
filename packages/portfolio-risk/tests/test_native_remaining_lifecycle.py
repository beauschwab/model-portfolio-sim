"""Raw-product lifecycle ownership; reference callbacks are forbidden in Rust."""
import datetime as dt
import json
import os
import subprocess
import numpy as np
import polars as pl
import pytest
from portfolio_risk import demo
from portfolio_risk.core.runtime import RunConfig,run_context
from portfolio_risk.core.native import library_path
from portfolio_risk.core import lifecycle_native
from portfolio_risk.products import deposits
from test_native_products import equal

pytestmark=pytest.mark.skipif(not library_path().is_file(),reason='build native lifecycle')


@pytest.mark.parametrize('paths,seed',[(1,0),(3,29),(8,2**64+3)])
def test_raw_forward_program_lifecycle(paths,seed,monkeypatch):
    from portfolio_risk.strategy import strategies
    from portfolio_risk.core import scenarios,curve,vol
    rates,quotes=demo.demo_market()
    programs=[dict(name=f'p{i}',product='synthetic',side='asset' if i%2 else 'liability',
        rate_ref=ref,is_float=bool(i%2),spread_bp=-10.,term_m=18,start_m=i,end_m=30,
        amort=['bullet','annuity','cpr'][i%3],cpr_annual=.12,**(dict(monthly_notional=100.) if i%2 else dict(reinvest_frac=.7,reinvest_source='mbs')))
        for i,ref in enumerate(['short','s2','s5','s10','s30'])]
    def run(backend):
        with run_context(RunConfig(paths,paths,3,compute_backend=backend)):
            return strategies.run_strategies(programs,rates,quotes,{'mbs':np.arange(27)*10.},horizon=27,seed=seed)
    expected=run('python')
    def forbidden(*args,**kwargs): raise AssertionError('Python program finance executed')
    for module,names in [(strategies,['program_cashflows','fwd_dv01_profile','_amort_factors','_ref']),
                         (scenarios,['CRN','build_rate_paths']),(curve,['bootstrap_curve']),
                         (vol,['factor_loadings','calibrate_abcd'])]:
        for name in names: monkeypatch.setattr(module,name,forbidden)
    equal(expected,run('rust'),rtol=1e-10,atol=1e-8)


def test_native_saved_book_and_candidate_mapping(monkeypatch):
    from portfolio_risk.analytics.balance_workflow import from_accounting,add_candidate
    from test_balance_stress import small
    from test_balance_ledger import meta
    from copy import deepcopy
    spec=small();spec['positions']=[];spec['accounts'][0]['equity']=121.
    accounting=dict(instrument_openings=pl.DataFrame([dict(book='loans',id='x',balance=100.,book_adjustment=1.,side='asset')]),
        instrument_cashflows=pl.DataFrame([dict(book='loans',id='x',month=1,principal=0.,cash_interest=0.,accrual_interest=1.,book_amortization=-.1)]))
    lib=dict(units=[dict(template='loan',h=0,side=1.)],horizon=1,runoff=np.array([[.5]]),cash_interest=np.array([[.02]]),nii=np.array([[.02]]))
    def run(backend):
        with run_context(RunConfig(compute_backend=backend)):
            mapped=from_accounting(spec,accounting,{'loans:x':meta('loan')},1.)
            return add_candidate(mapped,[dict(template='loan',purchase_m=0,notional=10.)],lib,{'loan':meta('loan')},1.)
    original=deepcopy(spec)
    assert run('python')==run('rust')
    assert spec==original
    for scale in [0.,-1.,float('inf')]:
        with run_context(RunConfig(compute_backend='rust')),pytest.raises(ValueError):
            from_accounting(spec,accounting,{'loans:x':meta('loan')},scale)
    with run_context(RunConfig(compute_backend='rust')),pytest.raises(ValueError,match='duplicate'):
        add_candidate(run('rust'),[dict(template='loan',purchase_m=0,notional=10.)]*2,lib,{'loan':meta('loan')})


def deposit_run(backend,stress=False,paths=3,seed=29,fixed=False,book=None,segments=None,shocks=None):
    book=demo.demo_deposit_book(4) if book is None else book
    rates,quotes=demo.demo_market();history=demo.demo_deposit_history()
    with run_context(RunConfig(paths,paths,3,compute_backend=backend,deposit_segments=segments)):
        kwargs=dict(seed=seed,oas=np.full(len(book),-.015) if fixed else None)
        if stress: return deposits.run_deposit_stress(book,rates,quotes,history,shocks_bp=[-100.,0.,100.] if shocks is None else shocks,**kwargs)
        return deposits.run_deposit_risk(book,rates,quotes,history,**kwargs)


@pytest.mark.parametrize('stress',[False,True])
@pytest.mark.parametrize('paths,seed',[(1,0),(3,29),(8,2**64+3)])
@pytest.mark.parametrize('fixed',[False,True])
def test_deposit_final_lifecycle_parity(stress,paths,seed,fixed):
    equal(deposit_run('python',stress,paths,seed,fixed),deposit_run('rust',stress,paths,seed,fixed),rtol=1e-7,atol=1e-5)


@pytest.mark.parametrize('stress',[False,True])
def test_deposit_lifecycle_has_no_python_financial_callbacks(monkeypatch,stress):
    expected=deposit_run('python',stress)
    def forbidden(*args,**kwargs): raise AssertionError('Python finance executed')
    for name in ['DepositDeck','factor_loadings','bootstrap_curve','calibrate_abcd','CRN','build_rate_paths','_deposit_A','deposit_shocked_paths','deposit_stress_engine','solve_oas_from_A','pv_from_A']:
        monkeypatch.setattr(deposits,name,forbidden)
    monkeypatch.setattr(deposits.LogisticBetaECM,'fit',forbidden)
    monkeypatch.setattr(deposits.LogisticBetaECM,'paths',forbidden)
    equal(expected,deposit_run('rust',stress),rtol=1e-7,atol=1e-5)


def test_deposit_raw_deck_override_unknown_segment_and_empty():
    book=demo.demo_deposit_book(4).with_columns(pl.Series('segment',['NEW','unknown','SAV','DDA']),pl.Series('avg_account_size',[.5,2e6,1000.,25000.]))
    segments={k:dict(v) for k,v in deposits.SEGMENTS.items()};segments['NEW']=dict(base=.031,amp=2.,b=190.,g0=.011)
    for rows in [book,book.head(0)]:
        values=[]
        for backend in ['python','rust']:
            with run_context(RunConfig(compute_backend=backend,deposit_segments=segments)):
                values.append(vars(deposits.DepositDeck(rows)))
        equal(*values,rtol=1e-13,atol=1e-13)
    equal(deposit_run('python',True,book=book,segments=segments),deposit_run('rust',True,book=book,segments=segments),rtol=1e-7,atol=1e-5)


@pytest.mark.parametrize('stress',[False,True])
def test_deposit_standalone_raw_protocol(monkeypatch,stress):
    calls=[];original=lifecycle_native.term_call
    def capture(schema,request):
        result=original(schema,request);calls.append((schema,request,result));return result
    monkeypatch.setattr(lifecycle_native,'term_call',capture)
    deposit_run('rust',stress,paths=1)
    assert len(calls)==1
    schema,request,expected=calls[0]
    exe=library_path().parent/('portfolio-lifecycle.exe' if os.name=='nt' else 'portfolio-lifecycle')
    result=subprocess.run([str(exe)],input=json.dumps(dict(schema=schema,threads=1,request=request)),capture_output=True,text=True,timeout=60)
    assert result.returncode==0,result.stdout+result.stderr
    equal(expected,json.loads(result.stdout)['result'],rtol=1e-12,atol=1e-9)


def test_deposit_analytic_derivatives_against_central_differences():
    theta = np.array([.001, .08, .6, 150., .025, .12, .38])
    ff = np.array([.01, .06, .04, .003, .07, .02, .045])
    def equilibrium(p):
        a, lo, hi, k, pivot = p
        return a + ff * (lo + (hi-lo) / (1+np.exp(-k*(ff-pivot))))
    def recurrence(p):
        eq = equilibrium(p[:5]); result = np.empty(len(ff)); result[0] = .004
        for i in range(1, len(ff)):
            gap = eq[i]-result[i-1]
            result[i] = result[i-1]+p[5 if gap>0 else 6]*gap
        return result
    for f, point, actual in [(equilibrium, theta[:5], deposits._equilibrium_jacobian(theta[:5], ff)),
                             (recurrence, theta, deposits._deposit_recurrence_jacobian(theta, ff, .004))]:
        expected = np.empty_like(actual)
        for j in range(len(point)):
            step = 1e-6 * max(abs(point[j]), .01)
            delta = np.zeros_like(point); delta[j] = step
            expected[:, j] = (f(point+delta)-f(point-delta))/(2*step)
        np.testing.assert_allclose(actual, expected, rtol=2e-6, atol=1e-9)


def hedge_run(backend, paths=3, seed=29, options=True, horizon=27):
    from portfolio_risk.products.hedges import run_hedge_risk
    asof = dt.date(2026, 6, 10)
    swaps, swaptions = demo.demo_hedge_book(asof=asof)
    rates, quotes = demo.demo_market()
    with run_context(RunConfig(paths, paths, 3, compute_backend=backend)):
        return run_hedge_risk(swaps, swaptions if options else None, asof, rates, quotes, seed, horizon)


@pytest.mark.parametrize('paths,seed,horizon', [(1,0,1),(3,29,27),(8,2**64+3,48)])
@pytest.mark.parametrize('options', [False, True])
def test_hedge_final_lifecycle_parity(paths,seed,horizon,options):
    equal(hedge_run('python',paths,seed,options,horizon),hedge_run('rust',paths,seed,options,horizon),rtol=1e-7,atol=1e-5)


def test_hedge_lifecycle_has_no_python_financial_callbacks(monkeypatch):
    from portfolio_risk.products import hedges
    expected = hedge_run('python')
    def forbidden(*args, **kwargs): raise AssertionError('Python hedge finance executed')
    for name in ['HedgeDeck','CorpDeck','factor_loadings','bootstrap_curve','calibrate_abcd','CRN','build_rate_paths','swap_mtm_and_carry','swaption_value']:
        monkeypatch.setattr(hedges, name, forbidden)
    equal(expected, hedge_run('rust'), rtol=1e-7, atol=1e-5)


def test_hedge_standalone_raw_protocol(monkeypatch):
    calls=[]; original=lifecycle_native.term_call
    def capture(schema, request):
        result=original(schema,request); calls.append((schema,request,result)); return result
    monkeypatch.setattr(lifecycle_native, 'term_call', capture)
    hedge_run('rust', paths=1)
    assert len(calls)==1
    schema, request, expected=calls[0]
    request['asof']=request['asof'].toordinal()
    for row in request['swaps']: row['maturity']=row['maturity'].toordinal()
    exe=library_path().parent/('portfolio-lifecycle.exe' if os.name=='nt' else 'portfolio-lifecycle')
    result=subprocess.run([str(exe)],input=json.dumps(dict(schema=schema,threads=1,request=request)),capture_output=True,text=True,timeout=60)
    assert result.returncode==0,result.stdout+result.stderr
    equal(expected,json.loads(result.stdout)['result'],rtol=1e-12,atol=1e-9)


def accounting_run(backend, paths=3, basis='market', conditional=False, subset=None):
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.core.scenarios import CRN
    bs=demo.model_balance_sheet(scale=.001,basis=basis,asof=dt.date(2026,6,10))
    bs={key:(value.head(3) if isinstance(value,pl.DataFrame) else value) for key,value in bs.items()}
    bs['hedges']=demo.demo_hedge_book(scale=.001,asof=bs['asof'])
    if subset is not None:
        bs={key:value for key,value in bs.items() if key in subset or key in ['asof','mbs_hists']}
    sr,vp=demo.demo_market();hist=demo.demo_deposit_history()
    with run_context(RunConfig(paths,paths,13,compute_backend=backend)):
        kwargs=dict(horizon=13,seed=29,asof=bs['asof'],capture_anchor=True,capture_cashflows=True)
        base=run_balance_sheet_nii(bs,sr,vp,hist,**kwargs)
        if not conditional:return base
        targets=dict(short_rate=np.linspace(.04,.02,360),rate_5y=np.full(360,.028),
                     rate_10y=np.full(360,.035),mortgage_rate=np.linspace(.06,.04,360),hpi=np.exp(np.arange(1,361)*.002))
        return run_balance_sheet_nii(bs,sr,vp,hist,**kwargs,crn=CRN(paths,29),
                                    forecast_plan={'targets':targets},accounting_anchor=base['accounting_anchor'])


@pytest.mark.parametrize('paths',[1,3,8])
@pytest.mark.parametrize('basis',['market','amortized_cost'])
@pytest.mark.parametrize('conditional',[False,True])
def test_raw_accounting_final_parity(paths,basis,conditional):
    equal(accounting_run('python',paths,basis,conditional),accounting_run('rust',paths,basis,conditional),rtol=1e-7,atol=1e-5)


def test_accounting_without_python_financial_callbacks(monkeypatch):
    import importlib
    expected=accounting_run('python')
    def forbidden(*args,**kwargs):raise AssertionError('Python accounting finance executed')
    for module,names in {
        'core.scenarios':['setup','run_engine','build_paths','build_rate_paths','solve_base_oas','CRN'],
        'products.corp':['CorpDeck','_corp_full'], 'products.cds':['CDDeck','_cd_full'],
        'products.deposits':['DepositDeck','_deposit_A'], 'products.hedges':['HedgeDeck','swap_mtm_and_carry'],
        'products.mm':['MMDeck','mm_income','mm_earning_assets'],
        'analytics.accounting':['effective_income','book_yield','smear_csr','bucket_csr'],
    }.items():
        mod=importlib.import_module('portfolio_risk.'+module)
        for name in names:monkeypatch.setattr(mod,name,forbidden)
    equal(expected,accounting_run('rust'),rtol=1e-7,atol=1e-5)


def kpi_run(backend,paths=3,fixed=False,component=None):
    from portfolio_risk.analytics import kpis
    bs=demo.model_balance_sheet(scale=.001,asof=dt.date(2026,6,10))
    bs={key:(value.head(3) if isinstance(value,pl.DataFrame) else value) for key,value in bs.items()}
    bs['hedges']=demo.demo_hedge_book(scale=.001,asof=bs['asof'])
    sr,vp=demo.demo_market();hist=demo.demo_deposit_history()
    nii=pl.DataFrame({'nii':[1e5,-2e5,3e5,4e5,-3e5,4e5,2e5]})
    oas={key:np.full(len(bs[key]),-.01 if key=='deposits' else .015) for key in ['mbs','loans','debt','deposits','cds']} if fixed else None
    with run_context(RunConfig(paths,paths+2,13,compute_backend=backend)):
        if component=='capital':return kpis.capital(bs,nii,stress_aoci_q=[1e4,-3e4,5e4])
        if component in ['lcr','nsfr']:return getattr(kpis,component)(bs,bs['asof'])
        if component=='parallel':
            valued={};dv=kpis.parallel_dv01s(bs,sr,vp,hist,29,oas_by_book=oas,valued_books=valued)
            return dv,valued
        return kpis.compute_kpis(bs,sr,vp,hist,nii,29,oas_by_book=oas)


@pytest.mark.parametrize('paths',[1,3,8])
@pytest.mark.parametrize('fixed',[False,True])
def test_kpi_raw_lifecycle_final_parity(paths,fixed):
    equal(kpi_run('python',paths,fixed),kpi_run('rust',paths,fixed),rtol=1e-7,atol=1e-5)


@pytest.mark.parametrize('component',['capital','lcr','nsfr','parallel'])
def test_kpi_component_parity(component):
    equal(kpi_run('python',fixed=True,component=component),kpi_run('rust',fixed=True,component=component),rtol=1e-7,atol=1e-5)


def test_kpi_without_python_financial_callbacks(monkeypatch):
    from portfolio_risk.analytics import kpis
    expected=kpi_run('python',fixed=True)
    def forbidden(*a,**kw):raise AssertionError('Python KPI finance executed')
    for name in ['parallel_dv01s','eve_summary','lcr','nsfr','capital','_mv']:
        monkeypatch.setattr(kpis,name,forbidden)
    equal(expected,kpi_run('rust',fixed=True),rtol=1e-7,atol=1e-5)


def unit_run(backend,paths=3,names=None,grid=None):
    from portfolio_risk.strategy.unitlib import build_unit_library
    sr,vp=demo.demo_market()
    with run_context(RunConfig(paths,paths+2,13,compute_backend=backend)):
        return build_unit_library(sr,vp,demo.demo_histories(),demo.demo_deposit_history(),
            horizon=13,seed=29,asof=dt.date(2026,6,10),template_names=names,grid_m=grid,
            template_overrides={'agency_mbs':{'spread_bp':145},'mmda_growth':{'spread_bp':25}})


@pytest.mark.parametrize('paths',[1,3,8])
@pytest.mark.parametrize('names,grid',[(None,None),(['mmda_growth','cml_float_3y','agency_mbs'],[12,3,0,3]),(['cd_2y'],[-1,30])])
def test_unit_library_raw_lifecycle_parity(paths,names,grid):
    equal(unit_run('python',paths,names,grid),unit_run('rust',paths,names,grid),rtol=1e-7,atol=1e-8)


def test_unit_library_no_python_financial_callbacks(monkeypatch):
    from portfolio_risk.strategy import unitlib
    import importlib
    expected=unit_run('python')
    def forbidden(*a,**kw):raise AssertionError('Python unit library finance executed')
    for name in ['_fwd_par','prepare_library','allocation_vectors']:monkeypatch.setattr(unitlib,name,forbidden)
    for module,names in {
        'core.scenarios':['setup','run_engine','build_paths','build_rate_paths','solve_base_oas','CRN'],
        'products.corp':['CorpDeck','_corp_full','corp_pv','corp_solve_oas'],
        'products.cds':['CDDeck','_cd_full'],'products.deposits':['DepositDeck','_deposit_A'],
        'analytics.accounting':['smear_csr','bucket_csr'],
    }.items():
        mod=importlib.import_module('portfolio_risk.'+module)
        for name in names:monkeypatch.setattr(mod,name,forbidden)
    equal(expected,unit_run('rust'),rtol=1e-7,atol=1e-8)


@pytest.mark.parametrize('with_base',[False,True])
@pytest.mark.parametrize('allocation',[[],[{'template':'agency_mbs','purchase_m':2,'notional':2e6},
                                         {'template':'cd_2y','purchase_m':4,'notional':3e6},
                                         {'template':'mmda_growth','purchase_m':99,'notional':1e5}]])
def test_interactive_native_coefficients_only(monkeypatch,with_base,allocation):
    from portfolio_risk.strategy import unitlib
    library=unit_run('rust')
    # Positive, independently supplied baseline; no pricing belongs in evaluation.
    base=dict(eve={'eve_$':1e7,'dv01_net_$':450.,'mv_assets_$':1e8},
              lcr={'hqla_$':2e7,'hqla_l1_$':1.5e7,'hqla_l2a_uncapped_$':7e6,'net_outflows_$':1e7},
              nsfr={'asf_$':7e7,'rsf_$':5e7},capital={'cet1_path':[{'cet1_$':8e6}],'rwa_total_$':5e7}) if with_base else None
    with run_context(RunConfig(compute_backend='python')):expected=unitlib.evaluate_strategy(library,allocation,base)
    def forbidden(*a,**kw):raise AssertionError('Python interpolation or pricing called during interactive evaluation')
    monkeypatch.setattr(unitlib,'allocation_vectors',forbidden)
    monkeypatch.setattr(unitlib,'build_unit_library',forbidden)
    with run_context(RunConfig(compute_backend='rust')):actual=unitlib.evaluate_strategy(library,allocation,base)
    equal(expected,actual,rtol=1e-12,atol=1e-8)


@pytest.mark.parametrize('runner',[accounting_run,kpi_run,unit_run])
def test_standalone_mixed_lifecycles_and_invalid_inputs(monkeypatch,runner):
    calls=[];original=lifecycle_native.term_call
    def capture(schema,request):
        result=original(schema,request);calls.append((schema,request,result));return result
    monkeypatch.setattr(lifecycle_native,'term_call',capture)
    runner('rust',paths=1)
    schema,request,expected=calls[-1]
    exe=library_path().parent/('portfolio-lifecycle.exe' if os.name=='nt' else 'portfolio-lifecycle')
    def encode(value):
        if isinstance(value,dt.date):return value.toordinal()
        raise TypeError(type(value).__name__)
    def invoke(request):
        return subprocess.run([str(exe)],input=json.dumps(dict(schema=schema,threads=1,request=request),default=encode),capture_output=True,text=True,timeout=60)
    result=invoke(request)
    assert result.returncode==0,result.stdout+result.stderr
    equal(expected,json.loads(result.stdout)['result'],rtol=1e-12,atol=1e-7)
    bad=dict(request)
    if schema=='kpi-1':bad['weights']=dict(request['weights'],l2_cap=1.)
    else:bad['horizon']=0
    result=invoke(bad)
    assert result.returncode==1 and not json.loads(result.stdout)['ok']
    # A rejected request does not poison later requests or alter an input table.
    equal(expected,original(schema,request),rtol=0,atol=0)


@pytest.mark.parametrize('field',['vol_quotes','tenors','seed','months','withdrawal_parameters','ids'])
def test_mixed_risk_rejects_inconsistent_raw_inputs(monkeypatch,field):
    from copy import deepcopy
    calls=[];original=lifecycle_native.term_call
    def capture(schema,request):
        calls.append((schema,deepcopy(request)));return original(schema,request)
    monkeypatch.setattr(lifecycle_native,'term_call',capture)
    kpi_run('rust',paths=1)
    schema,raw=calls[-1];bad=deepcopy(raw);books=bad['risk']['books']
    if field in ('vol_quotes','tenors','seed'):
        books['mbs']['request'][field][0]+=1
    elif field=='months':books['terms'][0]['deck']['months']+=1
    elif field=='ids':books['terms'][0]['ids']=[]
    else:books['withdrawal_parameters']=[]
    with pytest.raises(ValueError):original(schema,bad)
    equal(original(schema,raw),original(schema,raw),rtol=0,atol=0)
