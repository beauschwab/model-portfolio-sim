"""Installed/default gates proposed with the unapplied supplied-curve bridge."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.core import model_native,quant_native
from portfolio_risk.analytics import market_indexed_reference as reference
SCHEMA='market-indexed-adapter-1'
def raw():
    return json.loads((Path(__file__).resolve().parents[3]/'examples/models/market-indexed-adapter-1.json').read_text())['request']
def forbidden(*args,**kwargs):raise AssertionError('Python financial callback during native execution')
def native(request):return model_native.run(SCHEMA,request)[0]
def test_native_owns_projection_and_product_cash_before_independent_verification(monkeypatch):
    request=raw()
    with monkeypatch.context() as patch:
        patch.setattr(quant_native,'term_call',forbidden)
        patch.setattr(reference,'reconstruct',forbidden)
        result=native(request)
    receipt=reference.verify(request,result)
    assert receipt['verified'] and receipt['analytical_market_indexed_only']
    assert not result['capital_ftp_eligible'] and not receipt['persisted_journal_verified']
@pytest.mark.parametrize('variant',['signed','spread','distinct_daycounts','whole_json','escaped'])
def test_complete_generated_products_and_original_parent(variant):
    r=raw();k=r['bridge']['contract']
    if variant=='signed':k['fixed_rate']=-.04;k['margin']=-.1
    elif variant=='spread':r['bridge']['original']['locked_oas']=.02
    elif variant=='distinct_daycounts':k['fixed_day_count']='actual365_fixed'
    elif variant=='whole_json':k['notional']=100;k['margin']=0
    else:r['calendar']['revision']='escaped"\\µ'
    assert reference.verify(r,native(r))['verified']
@pytest.mark.parametrize('field',['money','date','rate','oas','source','flag','parent','admission'])
def test_reference_does_not_trust_producer_fields(field):
    r=raw();v=native(r)
    if field=='money':v['cases'][1]['swap_result']['schedule']['events'][0]['components']['cash_interest']+=1
    elif field=='date':v['cases'][1]['indexed_result']['schedule']['events'][0]['payment_ordinal']+=1
    elif field=='rate':v['cases'][1]['indexed_request']['periods'][0]['coupon']['observations'][0]['rate']+=1
    elif field=='oas':v['cases'][1]['original_locked_oas']+=1
    elif field=='source':v['cases'][1]['rate_source']['source_revision']='tampered'
    elif field=='flag':v['production_adoption']=True
    elif field=='parent':v['cache_parent']['bridge']['contract']['notional']+=1
    else:v['admission']['retained_bytes']=0
    with pytest.raises(ArithmeticError):reference.verify(r,v)
@pytest.mark.parametrize('side',['original','current_market'])
@pytest.mark.parametrize('tail',['reject','flat_zero','flat_forward'])
def test_new_curve_tail_enum_shape_rejects(side,tail):
    r=raw();m=r['bridge']['original']['market'] if side=='original' else r['bridge']['current_market']
    m['curves'][0]['right_tail']={tail:None}
    with pytest.raises(ValueError):native(r)
    with pytest.raises(ArithmeticError):reference.reconstruct(r)
def test_independent_cancel_prevents_acceptance():
    r=raw();result=native(r);calls=0
    def check():
        nonlocal calls;calls+=1;return calls==200
    with pytest.raises(InterruptedError):reference.verify(r,result,check)
def test_near_zero_forward_preserves_large_admitted_cash():
    r=raw();r['bridge']['contract']['notional']=1e12
    for m in [r['bridge']['original']['market'],r['bridge']['current_market']]:
        for curve in m['curves']:curve['discount_factors']=[1.,1.-1e-12,1.-2e-12]
    assert reference.verify(r,native(r))['verified']
    for m in [r['bridge']['original']['market'],r['bridge']['current_market']]:
        for curve in m['curves']:curve['times_years']=[0.,.6,1.]
    assert reference.verify(r,native(r))['verified']
