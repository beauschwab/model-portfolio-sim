"""Reviewed acquired prepaid refund obligations: actual native buyer journal."""
from copy import deepcopy
import json
import os
from pathlib import Path
import pytest
from portfolio_risk.analytics.commercial_candidate_reference import verify
from portfolio_risk.core.commercial_candidate_native import run
ROOT=Path(__file__).resolve().parents[3]
@pytest.fixture(autouse=True)
def selected_debug_artifact(monkeypatch):
    selected=os.environ.get('SECONDARY_NATIVE_DEBUG_DIR')
    if selected:
        from portfolio_risk.core import product_strategy_native,model_native
        monkeypatch.setattr(product_strategy_native,'binary_path',lambda:Path(selected)/'portfolio-strategy.exe')
        monkeypatch.setattr(model_native,'binary_path',lambda:Path(selected)/'portfolio-lifecycle.exe')
def request(sign=1.,face=100.):
    raw=json.loads((ROOT/'examples/models/commercial-secondary-prepaid-candidate-journal-1.json').read_text());raw['allocation'][0]['face']=face
    if sign<0.:
        f=raw['commercial']['units'][0]['graph']['positions'][0]['fees'];f.update(original_signed_fee_quote=5.,total_purchase_cost=104.);f['allocations'][0]['allocated_cost']=5.
        for h in histories(raw):
            p=h['policy'];p['fees'][0]['entitlement']=-6.;p['transfer']['origin_service']['fees'][0]['entitlement']=-6.
            for e in p['events']+p['transfer']['origin_service']['events']:e['amount']=-6.
            for e in h.get('acquisition_releases',[]):e['amount']=-1.
    return raw
def histories(raw):
    f=raw['commercial']['units'][0]['graph']['positions'][0]['fees']
    return f['original_pricing']+[h for hs in f['histories']for role in('physical','pricing','pricing_down','pricing_up')for h in hs[role]]
def physical(result):
    g=result['preparation']['native_preparation']['source']['native_graphs'][0]
    return next(s for s in g['native_sources']if s['role']=='physical')['fee_projection']['bound_path']
@pytest.mark.parametrize('sign',[1.,-1.])
@pytest.mark.parametrize('face',[0.,50.,100.])
def test_signed_prepaid_cash_stock_cost_and_buyer_income_replay(sign,face):
    raw=request(sign,face);result,_=run(raw,timeout=60.);assert verify(raw,result)['purchased_journal_replayed']
    stock=next(g['next']for g in physical(result)['source']['groups']if g['event']['action']=='acquire')
    assert stock['transferred_net_stock']==-sign*6.
    assert stock['acquired_cost']==sign and stock.get('released_acquired_cost',0.)==0.
    assert stock['deferred_fee_income'if sign>0. else'prepaid_fee_expense']==-sign*6.
    assert stock['acquired_fee_liability_valuation_adjustment'if sign>0. else'acquired_fee_asset_valuation_adjustment']==sign
    assert all(r['cash']==pytest.approx(1000.+(4.-sign)*face/100.,abs=1e-8)for r in result['tables']['closing_statements'])
    assert not any(r['gl_account']=='pnl:commercial_nonintegral_fee_income'for r in result['tables']['journal'])
@pytest.mark.parametrize('target',['omitted','wrong_review','wrong_convention','wrong_exit','earned_claim','missing_prefix','opposite_prefix','counterfeit','refund_terms','continuing_service','wrong_service'])
def test_opposite_cash_requires_actual_sealed_reviewed_pure_prepaid_source_even_zero_allocation(target):
    raw=request(face=0.);h=histories(raw)[0];p=h['policy'];t=p['transfer']
    if target=='omitted':t.pop('purchase_cash_policy')
    elif target=='wrong_review':t['purchase_cash_policy']['review']['revision']=''
    elif target=='wrong_convention':t['purchase_cash_policy']['convention']='inferred'
    elif target=='wrong_exit':t['purchase_cash_policy']['exit_refunds']='forgive_the_rest'
    elif target=='earned_claim':t['origin_service']['events'][0].update(action='earn',reason='service')
    elif target=='missing_prefix':t['origin_service']['events']=t['origin_service']['events'][1:]
    elif target=='opposite_prefix':t['origin_service']['events'][0]['amount']=-6.
    elif target=='counterfeit':t['transferred_net_stock']=-6.
    elif target=='wrong_service':t['purchase_cash_policy']['remaining_service']='continue_without_policy'
    elif target=='continuing_service':p['events'][0].update(action='earn',reason='service')
    else:
        p['fees'][0]['prepaid_refunds_allowed']=False;t['origin_service']['fees'][0]['prepaid_refunds_allowed']=False
    with pytest.raises(ValueError):run(raw,timeout=60.)
def default_request(fraction=1.,sign=1.):
    raw=request(sign);position=raw['commercial']['units'][0]['graph']['positions'][0]
    for hs in position['histories']:
        for h in hs['physical']:h['source']['credit']['intervals'][0]['hazards_per_year'][0][3]=100.
    for h in histories(raw):h['policy']['default']['refund_prepaid_fraction']=fraction
    return raw
@pytest.mark.parametrize('sign',[1.,-1.])
def test_default_before_cash_refunds_once_and_preserves_actual_native_clock(sign):
    raw=default_request(sign=sign);result,_=run(raw,timeout=60.);assert verify(raw,result)['purchased_journal_replayed']
    bound=physical(result);refunds=[(t,g)for t,g in zip(bound['native_years'],bound['source']['groups'],strict=True)if g['event']['action']=='refund_prepaid']
    assert len(refunds)==1 and refunds[0][0]==bound['exit']['native_year']
    assert refunds[0][1]['event']['event_id'].startswith('native-fee:')and refunds[0][1]['event']['amount']==sign*6.
    assert not any(g['event']['event_id']=='buyer-refunds-prepaid-consideration'for g in bound['source']['groups'])
def test_partial_exit_refund_cannot_cancel_surviving_obligation_or_invent_forgiveness():
    with pytest.raises(ValueError,match='complete|remaining|rights'):run(default_request(.5),timeout=60.)
@pytest.mark.parametrize('target',['refund_suppression','exit_review','gross','contra','cost','cash'])
def test_rehashed_actual_source_and_journal_tamper_refuses(target):
    raw=default_request();result,_=run(raw,timeout=60.);bound=physical(result)
    group=next(g for g in bound['source']['groups']if g['event']['action']=='refund_prepaid')
    if target=='refund_suppression':group['event']['event_id']='buyer-refunds-prepaid-consideration'
    elif target=='exit_review':group['event']['review']['revision']='counterfeit'
    elif target=='gross':next(g['next']for g in bound['source']['groups']if g['event']['action']=='acquire')['deferred_fee_income']-=1.
    elif target=='contra':next(g['next']for g in bound['source']['groups']if g['event']['action']=='acquire')['acquired_fee_liability_valuation_adjustment']+=1.
    elif target=='cost':bound['source']['native_transfer']['states'][0]['transferred_net_stock']-=1.
    else:next(r for r in result['tables']['journal']if r['gl_account']=='cash'and r['value']<0.)['value']-=1.
    with pytest.raises((ArithmeticError,ValueError,KeyError)):verify(raw,result)
def test_null_policy_preserves_typed_receipts_and_exact_immutable_raw_source():
    raw=json.loads((ROOT/'examples/models/commercial-secondary-candidate-journal-1.json').read_text());out,_=run(raw,timeout=60.)
    for h in histories(raw):h['policy']['transfer']['purchase_cash_policy']=None
    null,_=run(raw,timeout=60.)
    assert verify(raw,null)['purchased_journal_replayed']
    # The outer raw Value deliberately retains its exact null spelling.
    assert null['preparation']['raw_request']==raw
    a=deepcopy(out);b=deepcopy(null)
    a['preparation'].pop('raw_request');b['preparation'].pop('raw_request')
    assert a==b

@pytest.mark.parametrize('sign',[1.,-1.])
def test_prepaid_premium_residual_keeps_normal_asset_liability_classification(sign):
    import math
    raw=request(sign);f=raw['commercial']['units'][0]['graph']['positions'][0]['fees']
    f.update(original_signed_fee_quote=-sign*7.,total_purchase_cost=99.-sign*7.,discount_spread=math.log(6./7.)*365./20.);f['allocations'][0]['allocated_cost']=-sign*7.
    for h in histories(raw):
        for e in h.get('acquisition_releases',[]):e['amount']=-sign
    result,_=run(raw,timeout=60.);assert verify(raw,result)['purchased_journal_replayed']
    stock=next(g['next']for g in physical(result)['source']['groups']if g['event']['action']=='acquire')
    assert stock['acquired_fee_liability'if sign>0. else'acquired_fee_asset']==-sign
    assert stock.get('acquired_fee_liability_valuation_adjustment',0.)==0.
    assert stock.get('acquired_fee_asset_valuation_adjustment',0.)==0.
    assert all(c['cash']==pytest.approx(1000.+4.+sign,abs=1e-8)for c in result['tables']['closing_statements'])

def test_secondary_prepaid_joint_exercise_units_replay_same_buyer_and_refund_owners():
    from portfolio_risk.analytics import model_contracts
    from portfolio_risk.analytics.commercial_graph_reference import verify_units
    raw=json.loads((ROOT/'examples/models/commercial-secondary-prepaid-exercise-units-1.json').read_text())
    out=model_contracts.prepare_commercial_units(raw)
    assert verify_units(raw,out)['verified']
    graph=out['native_graphs'][0];policy=graph['exercise_policies'][0]['policy']
    assert policy['nodes'][0]['decision']=='call'
    assert policy['joint_fee_values']['decision_criterion']=='root_total_loan_and_fee_in_loan_numeraire-1'
    for source in graph['native_sources']:
        fee=source['fee_projection']['bound_path'];refunds=[g for g in fee['source']['groups']if g['event']['action']=='refund_prepaid']
        assert len(refunds)==1 and refunds[0]['event']['event_id'].startswith('native-fee:')
        assert fee['exit']['exercise_timing']=='before_scheduled_cash'
        assert fee['source']['closing'][0]['state']['deferred_fee_income']==0.

def test_secondary_prepaid_complete_origin_joint_candidate_preserves32m_refusal():
    raw=json.loads((ROOT/'packages/portfolio-risk/tests/fixtures/commercial-secondary-prepaid-exercise-candidate-refusal.json').read_text())
    expected=json.loads((ROOT/'packages/portfolio-risk/tests/fixtures/commercial-secondary-prepaid-exercise-candidate-refusal.expected.json').read_text())
    with pytest.raises(ValueError,match='joined 32M/128MiB preflight')as error:run(raw,timeout=60.)
    assert str(error.value).endswith(expected['error'])

@pytest.mark.parametrize('sign',[1.,-1.])
def test_prepaid_physical_weights_and_allocation_apply_once_with_full_unweighted_stock(sign):
    raw=request(sign,50.);p=raw['commercial']['units'][0]['graph']['positions'][0]
    for hs in p['histories']:
        h=hs['physical'][0];h['weight']=.25;other=deepcopy(h);other.update(path_id='borrower-two',weight=.75)
        other['source']['credit']['paths'][0].update(path_id='borrower-two',path_key=18);hs['physical'].append(other)
    for hs in p['fees']['histories']:
        other=deepcopy(hs['physical'][0]);other['path_id']='borrower-two';hs['physical'].append(other)
    from portfolio_risk.analytics import model_contracts
    from portfolio_risk.analytics.commercial_graph_reference import verify_units
    out=model_contracts.prepare_commercial_units(raw['commercial']);assert verify_units(raw['commercial'],out)['verified']
    sources=[r for r in out['native_graphs'][0]['native_sources']if r['role']=='physical'and r['scenario_id']=='baseline']
    assert len(sources)==2 and sorted(r['weight']for r in sources)==[.25,.75]
    field='deferred_fee_income'if sign>0. else'prepaid_fee_expense'
    for source in sources:
        stock=next(g['next']for g in source['fee_projection']['bound_path']['source']['groups']if g['event']['action']=='acquire')
        assert stock[field]==-sign*6.
    cash=[e for e in out['funding']['cash_events']if e['scenario_id']=='baseline'and e['fee_cash']!=0.]
    assert len(cash)==2 and sorted(e['source_weight']for e in cash)==[.25,.75]
    # These monetary coefficients already include source weights. Allocating50
    # applies only face; applying source weights again would understate refunds.
    assert sum(e['fee_cash']for e in cash)*50.==pytest.approx(-sign*3.,abs=1e-12)
    with pytest.raises(ValueError,match='joined 32M/128MiB preflight'):run(raw,timeout=60.)
