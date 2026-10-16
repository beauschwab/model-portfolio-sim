"""Complete-origin acquired loan/fee anchors and exact prospective windows."""
from copy import deepcopy
import json
from pathlib import Path

import pytest

from portfolio_risk.core.commercial_candidate_holding_native import run
from portfolio_risk.analytics.commercial_candidate_holding_reference import verify,raw_prefix

ROOT=Path(__file__).resolve().parents[3]


def request(sign=1.,face=50.):
    raw=json.loads((ROOT/'examples/models/commercial-candidate-holding-journal-1.json').read_text())
    for candidate in(raw['completed_candidate'],raw['revised_candidate']):
        candidate['allocation'][0]['face']=face
        p=candidate['commercial']['units'][0]['graph']['positions'][0];fees=p['fees']
        fees['original_signed_fee_quote']*=sign;fees['total_purchase_cost']=p['original_signed_cash_quote']+fees['original_signed_fee_quote']
        for a in fees['allocations']:a['allocated_cost']*=sign
        histories=fees['original_pricing']+fees.get('original_physical',[])+[h for scene in fees['histories']for role in('physical','pricing','pricing_down','pricing_up')for h in scene[role]]
        for h in histories:
            for f in h['policy']['fees']:f['entitlement']*=sign
            for e in h['policy']['events']:e['amount']*=sign
            for e in h.get('acquisition_releases',[]):e['amount']*=sign
    return raw


@pytest.mark.parametrize('sign',[1.,-1.])
@pytest.mark.parametrize('face',[0.,50.])
def test_complete_origin_fee_holding_replays_signed_gross_cut_and_once_only_future(sign,face):
    raw=request(sign,face);actual,_=run(raw);receipt=verify(raw,actual)
    assert receipt['window_gross_rights_basis_replayed']and receipt['exact_ordered_monetary_prefix_retained']
    old=actual['completed_candidate'];new=actual['revised_candidate'];window=actual['window_tables']
    assert new['producer_completion']==dict(complete=True,protocol=7)
    assert actual['full_end_ordinal']==738931 and actual['cut_day']==15
    assert old['preparation']['native_preparation']['source']['native_graphs'][0]['original_oas']==new['preparation']['native_preparation']['source']['native_graphs'][0]['original_oas']
    asset='commercial_acquired_fee_asset'if sign>0. else'commercial_acquired_fee_liability'
    right='commercial_fee_receivable'if sign>0. else'commercial_fee_payable'
    opening=lambda account:sum(r['balance']for r in window['opening']if r['scenario']=='baseline'and r['gl_account']==account)
    assert opening(asset)==pytest.approx(sign*3.*face/100.,abs=1e-10)
    assert opening(right)==pytest.approx(sign*5.*face/100.,abs=1e-10)
    assert sum(r['signed_prior_pnl']for r in window['retained_prior_pnl']if r['scenario']=='baseline')==pytest.approx(-sign*3.*face/100.,abs=1e-10)
    future=[r for r in window['journal']if r['origin_transaction_number']is not None]
    assert len(future)==len([r for r in new['tables']['journal']if r['day']>15])
    assert all(r['event']not in('opening','reviewed_opening_snapshot')for r in future)
    assert sum(r['value']for r in future if r['scenario']=='baseline'and r['event']=='commercial_nonintegral_fee'and r['gl_account']=='cash')==pytest.approx(sign*5.*face/100.,abs=1e-10)
    assert all(r['day']>=15 for r in window['commercial_fee_daily'])
    assert any(r['native_fee_snapshot']['acquired_cost']==sign*5. for r in window['commercial_fee_daily'])
    assert actual['prior_publication_verified']is False and actual['cut_started_mixed_sim_adoption']is False


@pytest.mark.parametrize('target',['past_fee_review','past_release','original_quote','fee_spread','allocation','credit_stream','fee_contract','full_tail'])
def test_candidate_holding_rejects_changed_observed_or_original_authority_before_adoption(target):
    raw=request();candidate=raw['revised_candidate'];p=candidate['commercial']['units'][0]['graph']['positions'][0];fees=p['fees'];physical=fees['histories'][0]['physical'][0]
    if target=='past_fee_review':physical['policy']['events'][0]['review']['revision']='changed-past'
    elif target=='past_release':physical['acquisition_releases'][0]['review']['revision']='changed-past'
    elif target=='original_quote':p['original_signed_cash_quote']+=1.
    elif target=='fee_spread':fees['discount_spread']+=.001
    elif target=='allocation':candidate['allocation'][0]['face']=25.
    elif target=='credit_stream':p['histories'][0]['physical'][0]['source']['credit']['draw_stream']['stream_revision']='changed'
    elif target=='fee_contract':physical['policy']['fees'][0]['classification_review']['revision']='changed'
    else:physical['policy']['complete_end_ordinal']+=1
    with pytest.raises(ArithmeticError):raw_prefix(raw)
    with pytest.raises((ValueError,RuntimeError)):run(raw)


@pytest.mark.parametrize('target',['cut_basis','prior_pnl','future_cash','same_balance_prefix_order','fee_source','raw_fee_quote'])
def test_candidate_holding_independent_replay_refuses_nested_source_and_window_corruption(target):
    raw=request();actual,_=run(raw)
    if target=='cut_basis':
        row=next(r for r in actual['window_tables']['opening']if r['gl_account']=='commercial_acquired_fee_asset');row['balance']+=1.
    elif target=='prior_pnl':actual['window_tables']['retained_prior_pnl'][0]['signed_prior_pnl']+=1.
    elif target=='future_cash':
        row=next(r for r in actual['window_tables']['journal']if r['event']=='commercial_nonintegral_fee'and r['gl_account']=='cash');row['value']+=1.
    elif target=='same_balance_prefix_order':
        rows=actual['completed_candidate']['tables']['journal'];rows[0],rows[1]=rows[1],rows[0]
    elif target=='fee_source':actual['revised_candidate']['preparation']['native_preparation']['source']['native_graphs'][0]['native_sources'][0]['fee_projection']['raw_fee_history']['acquisition_releases'][0]['amount']+=1.
    else:actual['raw_source']['completed_candidate']['commercial']['units'][0]['graph']['positions'][0]['fees']['original_signed_fee_quote']+=1.
    with pytest.raises((ArithmeticError,ValueError)):verify(raw,actual)


def test_candidate_holding_keeps_bounded_raw_input_and_original_deadline():
    raw=request();raw['oversized']='x'*(2*1024*1024)
    with pytest.raises(ValueError,match='2MiB'):run(raw)
    with pytest.raises((TimeoutError,ValueError)):run(request(),timeout=.001)


def test_larger_complete_sources_refuse_joined_capacity_without_dropping_either_replay():
    raw=request()
    for candidate in(raw['completed_candidate'],raw['revised_candidate']):
        g=candidate['commercial']['units'][0]['graph'];p=g['positions'][0]
        scenario=deepcopy(g['scenarios'][0]);scenario.update(scenario_id='extra',run_id='extra-run');g['scenarios'].append(scenario)
        for histories in(p['histories'],p['fees']['histories']):
            h=deepcopy(histories[0]);h['scenario_id']='extra';histories.append(h)
        candidate['specification']['scenarios'].append(dict(name='extra',start_day=1))
        candidate['run_mapping'].append(dict(source_scenario_id='extra',ledger_scenario_id='extra',severity=0.))
    raw_prefix(raw)
    with pytest.raises(ValueError,match='joined.*before dual source generation'):run(raw)
