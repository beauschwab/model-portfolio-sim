"""Raw corporate/CD dates, schedules and final risk without Python financial stages."""
import datetime as dt
import ctypes
import json
import os
import subprocess

import numpy as np
import polars as pl
import pytest
from portfolio_risk import demo
from portfolio_risk.core import conventions, quant_native
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.products import corp, cds

pytestmark = pytest.mark.skipif(not library_path().is_file(), reason='build native products')
ASOF = dt.date(2023,12,29)


def book(product):
    maturity=[dt.date(2024,2,29),dt.date(2024,3,31),dt.date(2025,1,1),dt.date(2026,7,4),dt.date(2027,12,31),dt.date(2058,2,28)]
    common=dict(id=[f'T{i}' for i in range(6)],maturity=maturity,freq_months=[1,3,6,12,1,6],daycount=['ACT/365F']*6,price=[100.]*6)
    if product=='corporate':
        common.update(face=[1e6]*6,is_float=[0,1,0,1,0,1],coupon_or_spread=[.05,.01,.045,.015,.06,.02],
                      amort_type=['bullet','annuity','sink','bullet','sink','annuity'],cap=[.07]*6,floor=[.015]*6)
    else:
        common.update(balance=[1e6]*6,rate=[.04]*6,penalty_months=[3.]*6,
                      channel=['retail','brokered']*3,ew_mult=[1.,2.,.7,1.,0.,1.],freq_months=[0,1,3,6,12,0])
    out=pl.DataFrame(common)
    options=[[(dt.date(2024,6,28),1.02),(dt.date(2024,6,30),1.),(dt.date(2025,1,1),1.)]]*6
    out=out.with_columns(pl.Series('call_schedule',options,dtype=pl.Object))
    if product=='corporate':
        out=out.with_columns(pl.Series('put_schedule',options,dtype=pl.Object),
            pl.Series('sink_schedule',[[(dt.date(2024,6,30),.2),(dt.date(2025,1,1),.25),(dt.date(2025,1,1),.3)]]*6,dtype=pl.Object))
    return out


def deck(data,product,backend,cal=None,bdc=conventions.BDC.MODIFIED_FOLLOWING):
    with run_context(RunConfig(n_paths=3,n_paths_base=3,compute_backend=backend)):
        return (corp.CorpDeck if product=='corporate' else cds.CDDeck)(data,ASOF,cal=cal,bdc=bdc)


@pytest.mark.parametrize('product',['corporate','cd'])
@pytest.mark.parametrize('basis',list(conventions.DayCount))
@pytest.mark.parametrize('bdc',list(conventions.BDC))
def test_complete_deck_parity_on_stubs_holidays_leaps_and_long_maturities(product,basis,bdc):
    data=book(product).with_columns(pl.lit(basis.value).alias('daycount'))
    cal=conventions.Calendar('US',extra_holidays={dt.date(2024,2,29),dt.date(2026,7,6)})
    expected=deck(data,product,'python',cal,bdc)
    actual=deck(data,product,'rust',cal,bdc)
    assert set(vars(expected))==set(vars(actual))
    for name,value in vars(expected).items():
        np.testing.assert_allclose(getattr(actual,name),value,rtol=1e-13,atol=1e-13,err_msg=name)


@pytest.mark.parametrize('product',['corporate','cd'])
def test_deck_owns_date_and_schedule_financial_work(monkeypatch,product):
    data=book(product)
    cal=conventions.Calendar('weekends',extra_holidays={dt.date(2024,3,29)})
    expected=deck(data,product,'python',cal)
    def forbidden(*args,**kwargs): raise AssertionError('Python schedule stage executed')
    for module,names in [(corp,['gen_schedule']), (cds,['gen_schedule']),
                         (conventions,['year_fraction','us_bond_holidays','_add_months'])]:
        for name in names: monkeypatch.setattr(module,name,forbidden)
    monkeypatch.setattr(conventions.Calendar,'adjust',forbidden)
    actual=deck(data,product,'rust',cal)
    for name,value in vars(expected).items():
        np.testing.assert_allclose(getattr(actual,name),value,rtol=1e-13,atol=1e-13)


def risk_inputs(product,n=6):
    asof=dt.date(2026,6,10)
    data=demo.model_balance_sheet(scale=.001,asof=asof)['loans'].head(n) if product=='corporate' else demo.demo_cd_book(n,asof=asof)
    return data.with_columns(pl.lit(100.).alias('price')),asof,*demo.demo_market()


def risk(data,product,backend,paths=3,seed=27,oas=None,cal=None,ewp=None):
    with run_context(RunConfig(paths,paths,3,compute_backend=backend,cd_ew_params=ewp)):
        if product=='corporate': return corp.run_corp_risk(*data,None,None,seed=seed,oas=oas,cal=cal)
        return cds.run_cd_risk(*data,seed=seed,oas=oas,cal=cal)


def equal_risk(expected,actual):
    assert expected.columns==actual.columns and expected.shape==actual.shape
    cols=[c for c in expected.columns if c in ('oas_bps','model_price','dv01') or c.startswith(('krd01_','vega_'))]
    np.testing.assert_allclose(actual[cols].to_numpy(),expected[cols].to_numpy(),rtol=1e-7,atol=1e-5)


def exact_frames(expected,actual):
    assert expected.schema==actual.schema
    for column,dtype in expected.schema.items():
        if dtype==pl.Object:
            assert expected[column].to_list()==actual[column].to_list()
        else:
            assert expected[column].equals(actual[column]),column


@pytest.mark.parametrize('product',['corporate','cd'])
def test_risk_native_ownership_one_raw_batch(monkeypatch,product):
    data=risk_inputs(product)
    expected=risk(data,product,'python')
    def forbidden(*args,**kwargs): raise AssertionError('Python financial stage executed in native term risk')
    for module,names in [(corp,['CorpDeck','factor_loadings','bootstrap_curve','calibrate_abcd','CRN','build_rate_paths','_corp_A','corp_pv','corp_solve_oas']),
                         (cds,['CDDeck','factor_loadings','bootstrap_curve','calibrate_abcd','CRN','build_rate_paths','_cd_A','corp_pv','corp_solve_oas']),
                         (np.random,['default_rng'])]:
        for name in names: monkeypatch.setattr(module,name,forbidden)
    calls=[];original=quant_native.term_call
    def capture(schema,request): calls.append(schema); return original(schema,request)
    monkeypatch.setattr(quant_native,'term_call',capture)
    actual=risk(data,product,'rust')
    assert calls==['term-risk-1']
    equal_risk(expected,actual)


@pytest.mark.parametrize('product',['corporate','cd'])
@pytest.mark.parametrize('seed,paths',[(0,1),(27,3),(2**64+19,8)])
@pytest.mark.parametrize('fixed',[False,True])
def test_final_risk_and_fixed_spread_parity(product,seed,paths,fixed):
    data=risk_inputs(product)
    spread=np.linspace(.002,.015,len(data[0])) if fixed else None
    cal=conventions.Calendar('US',extra_holidays={dt.date(2027,6,10)})
    ewp=(.03,3.,220.,.008,.5) if product=='cd' else None
    expected=risk(data,product,'python',paths,seed,spread,cal,ewp)
    actual=risk(data,product,'rust',paths,seed,spread,cal,ewp)
    equal_risk(expected,actual)
    if fixed: np.testing.assert_array_equal(actual['oas_bps'],spread*1e4)
    cols=[c for c in actual.columns if c.startswith('krd01_')]
    np.testing.assert_allclose(actual['dv01'],actual[cols].to_numpy().sum(axis=1),rtol=0,atol=1e-8)


@pytest.mark.parametrize('product',['corporate','cd'])
def test_standalone_risk_and_deck_protocol(monkeypatch,product):
    captured=[];original=quant_native.term_call
    def capture(schema,request):
        result=original(schema,request);captured.append((schema,request,result));return result
    monkeypatch.setattr(quant_native,'term_call',capture)
    data=risk_inputs(product,2)
    risk(data,product,'rust',paths=1)
    deck(book(product),product,'rust')
    exe=library_path().parent/('portfolio-term-risk.exe' if os.name=='nt' else 'portfolio-term-risk')
    for schema,request,expected in captured:
        payload=dict(schema=schema,threads=1,request=request)
        result=subprocess.run([str(exe)],input=json.dumps(payload,default=lambda d:d.toordinal()),capture_output=True,text=True,timeout=60)
        assert result.returncode==0,result.stdout+result.stderr
        actual=json.loads(result.stdout)['result']
        for name,values in expected.items():
            np.testing.assert_allclose(actual[name],values,rtol=1e-12,atol=1e-9)
    payload['request']['unknown_financial_input']=1
    bad=subprocess.run([str(exe)],input=json.dumps(payload,default=lambda d:d.toordinal()),capture_output=True,text=True,timeout=10)
    assert bad.returncode==1 and not json.loads(bad.stdout)['ok']


@pytest.mark.parametrize('product',['corporate','cd'])
def test_calendar_subclasses_fail_explicitly(product):
    class Custom(conventions.Calendar): pass
    with pytest.raises(ValueError,match='Custom Python calendar'):
        risk(risk_inputs(product),product,'rust',cal=Custom())


@pytest.mark.parametrize('column,value',[('maturity',dt.date(2020,1,1)),('price',-1.),('freq_months',0),('cap',-.1)])
def test_invalid_corporate_contracts_fail(column,value):
    data=risk_inputs('corporate',2)
    changed=data[0].with_columns(pl.lit(value).alias(column))
    if column=='cap': changed=changed.with_columns(pl.lit(.1).alias('floor'))
    with pytest.raises(ValueError,match='no fallback'):
        risk((changed,*data[1:]),'corporate','rust',paths=1)


@pytest.mark.parametrize('product',['corporate','cd'])
def test_chunk_boundary_and_option_rich_risk(product):
    data=book(product)
    # Cross the 256-row boundary with nonuniform terms and fixed spreads.
    data=pl.concat([data]*43).head(257)
    inputs=(data,ASOF,*demo.demo_market())
    spread=np.linspace(.001,.02,len(data))
    equal_risk(risk(inputs,product,'python',paths=1,oas=spread),
               risk(inputs,product,'rust',paths=1,oas=spread))


@pytest.mark.parametrize('product',['corporate','cd'])
def test_native_market_reuse_and_edit_invalidation(product):
    stats=quant_native.mortgage_cache_statistics
    inputs=risk_inputs(product,2)
    stats(clear=True)
    first=risk(inputs,product,'rust',paths=1)
    cold=stats()
    exact_frames(first,risk(inputs,product,'rust',paths=1))
    assert stats()['misses']==cold['misses'] and stats()['hits']>cold['hits']
    column='face' if product=='corporate' else 'balance'
    edited=(inputs[0].with_columns((pl.col(column)*1.03).alias(column)),*inputs[1:])
    result=risk(edited,product,'rust',paths=1)
    assert stats()['misses']==cold['misses']
    np.testing.assert_allclose(result['dv01'],first['dv01']*1.03,rtol=1e-12,atol=1e-8)
    # Each economically relevant path input must change cache identity.
    for change in ['rates','quotes','seed','paths']:
        book_,asof,rates,quotes=inputs
        seed,paths=27,1
        if change=='rates':
            rates=rates.copy();rates[3]+=.0005
        elif change=='quotes':
            quotes=quotes.copy();quotes[2,2]+=.005
        elif change=='seed': seed=2**64+39
        else: paths=3
        before=stats()['misses']
        changed=(book_,asof,rates,quotes)
        actual=risk(changed,product,'rust',paths=paths,seed=seed)
        assert stats()['misses']>before
        stats(clear=True)
        exact_frames(actual,risk(changed,product,'rust',paths=paths,seed=seed))


@pytest.mark.parametrize('product',['corporate','cd'])
def test_empty_deck_retains_reference_contract(product):
    expected=deck(book(product).head(0),product,'python')
    actual=deck(book(product).head(0),product,'rust')
    for name,value in vars(expected).items():
        np.testing.assert_array_equal(getattr(actual,name),value)


@pytest.mark.parametrize('failure',['contract','output_shape'])
def test_numeric_output_failure_does_not_publish_partial_results(monkeypatch,failure):
    captured=[];original=quant_native.term_call
    def capture(schema,request):
        captured.append(request);return original(schema,request)
    monkeypatch.setattr(quant_native,'term_call',capture)
    risk(risk_inputs('cd',1),'cd','rust',paths=1)
    request=captured[0]
    if failure=='contract': request['deck']['contracts'][0]['price']=-1.
    count=len(request['tenors'])+len(request['vol_quotes'])//3
    arrays=[np.full(size,-987.) for size in (1,1,1,count if failure=='contract' else count+1)]
    descriptors=(quant_native.Buffer*4)(*(quant_native.Buffer(
        a.ctypes.data_as(ctypes.POINTER(ctypes.c_double)),a.size,
        (ctypes.c_size_t*3)(a.size,1,1)) for a in arrays))
    payload=json.dumps(dict(schema='term-risk-1',threads=1,request=request),default=lambda d:d.toordinal()).encode()
    lib,_=quant_native._load(str(library_path()))
    pointer=lib.portfolio_term_risk_into(payload,len(payload),descriptors,4)
    try:
        response=json.loads(ctypes.string_at(pointer))
    finally:
        lib.portfolio_term_free(pointer)
    assert not response['ok']
    for array in arrays: np.testing.assert_array_equal(array,np.full(array.size,-987.))
