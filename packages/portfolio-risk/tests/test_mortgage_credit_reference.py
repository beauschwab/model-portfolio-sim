"""Independent public mortgage original-anchor, source, carry and failure gates."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import mortgage_credit_reference as oracle

HERE=Path(__file__).resolve().parent/'fixtures/mortgage_credit'


def raw(y=.02,default=.2,turnover=0.,refi=0.,recognized=1.):
    c=json.loads((HERE/'source.json').read_text())['contract'];start=c['as_of_ordinal']
    c['ownership'].update(remittance_lag_days=0,recognized_accrual_fraction=[recognized]*3)
    c['payment_dates']=[start+10,start+20];c['borrower_collection_fraction']=[1.]*3
    c['path'][0]['credit_hazards_per_year'][0][3]=default;c['path'][0]['turnover_hazard_per_year']=turnover;c['refinance']['incentive_slope']=refi
    ident=lambda name:dict(id=name,revision='synthetic-v1')
    p=dict(identity=ident('policy'),**oracle.POLICY,default_principal_book_review=ident('book-loss'),economic_principal_disposal_review=ident('economic-P'),economic_principal_disposal_lag_days=30,
        interest=dict(review=ident('contract-interest'),accounting_accrued='reverse_income_at_default',collection_fraction=[.5]*3,
        complete_collection_tails=[[dict(lag_days=5,weight=.25),dict(lag_days=15,weight=.75)] for _ in range(3)],economic_remainder='reviewed_economic_disposal',economic_disposal_review=ident('economic-A'),disposal_lag_days=20))
    source=oracle.boundary_source(c);projection=source['native_projection'];discount=lambda d,x:x*math.exp(-math.log1p(y)*(d-start)/365.)
    quote=sum(discount(x['ordinal'],x['principal']+x['interest']) for x in projection['cashflows'])
    for claim in projection['defaults']:
        for t in p['interest']['complete_collection_tails'][claim['origin']]:quote+=discount(claim['default_ordinal']+t['lag_days'],claim['borrower_contract_interest']*.5*t['weight'])
    return dict(schema=oracle.SCHEMA,contract=c,original_contract=deepcopy(c),original_economic_source=ident('original-economic'),pairing=dict(review=ident('physical-pairing'),original_run_id=c['identity']['run_id'],current_run_id=c['identity']['run_id'],policy='supplied_physical_original_contract_unchanged-1'),path_id='whole-loan',economic_source=ident('economic-source'),
        acquisition=dict(identity=ident('acquisition'),original_yield_anchor=ident('original-yield'),income_review=ident('income'),purchase_ordinal=start,actual_cost=quote,original_annual_effective_rate=y,opening_gross_carrying=[quote,0.,0.],accounting_basis='supplied_non_pcd_gross_cost-1'),policy=p)


def native_shape(expected):
    result=deepcopy(expected)
    for interval in result['intervals']:interval['operator']['work_multiplications']=0 if result['raw_request']['acquisition']['original_annual_effective_rate']==0. else 1
    return result


def test_zero_credit_zero_eir_hand_two_actual_coupons():
    r=raw(y=0.,default=0.);o=oracle.reconstruct(r)
    assert o['actual_acquisition_cost']==pytest.approx(100.1661,abs=1e-12)
    assert [x['principal']+x['interest'] for x in o['source']['native_projection']['cashflows']]==pytest.approx([34.,66.1661])
    assert o['total_earned_eir']==0. and o['total_accounting_income']==pytest.approx(0.,abs=1e-12)
    assert o['closing_live_gross_carrying']==[0.,0.,0.]
    assert oracle.verify(r,native_shape(o))['sequential_single_acquisition_carry_recomputed']

@pytest.mark.parametrize('kwargs',[dict(default=0.),dict(y=-.01),dict(y=.2),dict(turnover=.4),dict(refi=30.),dict(recognized=.25),dict(recognized=0.),dict(default=1.,turnover=.5,refi=10.)])
def test_one_cost_yield_anchor_all_interval_cash_and_claims(kwargs):
    r=raw(**kwargs);o=oracle.reconstruct(r);assert oracle.verify(r,native_shape(o))['complete_gross_claim_tail_verified']
    assert o['total_actual_cash']==pytest.approx(r['acquisition']['actual_cost']+o['total_accounting_income'],abs=1e-9)
    for left,right in zip(o['intervals'],o['intervals'][1:]):assert right['opening']['gross_carrying'][:3]==left['closing_live_gross_carrying']
    for claim in o['default_closeouts']:
        assert claim['closing_economic_principal']==claim['closing_economic_interest']==0.
        assert claim['gross_principal']==pytest.approx(claim['principal_cash']+claim['principal_economic_disposal'])
        assert claim['gross_contract_interest']==pytest.approx(claim['interest_cash']+claim['interest_economic_disposal'])

@pytest.mark.parametrize('bad',['remittance','pricing','agency','opening_accrued','orphan_carry','wrong_cost','yield_anchor','principal_disposal_early','interest_disposal_early','weight_mass','unknown_policy','unreviewed_disposal','empty_collection','future_purchase','unknown_field','float_date','float_interval','large_calendar','source_revision','metadata'])
def test_explicit_raw_policy_float_disposal_boundaries_fail(bad):
    r=raw();c=r['contract'];a=r['acquisition'];p=r['policy'];i=p['interest']
    if bad=='remittance':c['ownership']['remittance_lag_days']=1
    elif bad=='pricing':c['identity']['measure']='pricing'
    elif bad=='agency':c['ownership']=dict(kind='agency',remittance_lag_days=0,servicing_and_guarantee_spread=0.,default_buyout_lag_days=1)
    elif bad=='opening_accrued':c['opening_investor_interest'][0]=1.
    elif bad=='orphan_carry':a['opening_gross_carrying'][1]=1.
    elif bad=='wrong_cost':a['actual_cost']+=1.
    elif bad=='yield_anchor':a['original_annual_effective_rate']+=.1
    elif bad=='principal_disposal_early':p['economic_principal_disposal_lag_days']=1
    elif bad=='interest_disposal_early':i['disposal_lag_days']=1
    elif bad=='weight_mass':i['complete_collection_tails'][0][0]['weight']=.9
    elif bad=='unknown_policy':p['default_basis']='keep'
    elif bad=='unreviewed_disposal':i['economic_disposal_review']=None
    elif bad=='empty_collection':i['complete_collection_tails'][0]=[]
    elif bad=='future_purchase':a['purchase_ordinal']+=1
    elif bad=='unknown_field':r['producer']=True
    elif bad=='float_date':a['purchase_ordinal']=float(a['purchase_ordinal'])
    elif bad=='float_interval':c['horizon_days']=20.
    elif bad=='large_calendar':c['payment_dates']=[c['as_of_ordinal']+d for d in range(1,21)]
    elif bad=='source_revision':r['economic_source']['revision']='other'
    else:p['identity']['id']='escaped"\\'*100000
    with pytest.raises(ArithmeticError):oracle.reconstruct(r)

@pytest.mark.parametrize('target',['cash','scheduled','recognized','boundary_P','boundary_A','boundary_order','default_id','recovery_id','cash_link','carry_chain','carry_A','carry_G','eir','basis','quote','income','principal_disposal','interest_disposal','original_anchor','measure','source_hash','work_float','bytes','flags','tail_date'])
def test_complete_independent_expected_source_action_tail_tamper(target):
    r=raw(turnover=.2);o=native_shape(oracle.reconstruct(r))
    if target=='cash':o['source']['native_projection']['cashflows'][0]['principal']+=1.
    elif target=='scheduled':o['source']['capture']['boundaries'][0]['scheduled'][0]['principal']+=1.
    elif target=='recognized':o['source']['capture']['boundaries'][0]['scheduled'][0]['recognized_interest']+=1.
    elif target=='boundary_P':o['source']['capture']['boundaries'][0]['transported_investor_principal'][0]+=1.
    elif target=='boundary_A':o['source']['capture']['boundaries'][0]['transported_investor_interest'][0]+=1.
    elif target=='boundary_order':o['source']['capture']['boundaries'].reverse()
    elif target=='default_id':o['default_closeouts'][0]['claim_id']='other'
    elif target=='recovery_id':o['source']['capture']['boundaries'][0]['default_recoveries'][0]['entitlement_id']='other'
    elif target=='cash_link':o['source']['capture']['cash_links'][0]['cash_event_id']='other'
    elif target=='carry_chain':o['intervals'][1]['opening']['gross_carrying'][0]+=1.
    elif target=='carry_A':o['intervals'][0]['transport']['closing']['accrued_interest'][3]+=1.
    elif target=='carry_G':o['intervals'][0]['transport']['closing']['gross_carrying'][3]+=1.
    elif target=='eir':o['total_earned_eir']+=1.
    elif target=='basis':o['default_closeouts'][0]['signed_default_basis']+=1.
    elif target=='quote':o['original_quote_pv']+=1.
    elif target=='income':o['actions'][0]['income']+=1.
    elif target=='principal_disposal':o['default_closeouts'][0]['principal_economic_disposal']+=1.
    elif target=='interest_disposal':o['default_closeouts'][0]['interest_economic_disposal']+=1.
    elif target=='original_anchor':o['raw_request']['acquisition']['original_yield_anchor']['revision']='other'
    elif target=='measure':o['intervals'][0]['opening']['measure']='pricing'
    elif target=='source_hash':o['source']['intervals'][0]['financial_source_sha256']='other'
    elif target=='work_float':o['admission']['work_bound']=float(o['admission']['work_bound'])
    elif target=='bytes':o['admission']['retained_byte_bound']-=1
    elif target=='flags':o['production_adoption']=True
    else:o['full_end_ordinal']=float(o['full_end_ordinal'])
    with pytest.raises(ArithmeticError):oracle.verify(r,o)


def test_reference_cancellation_no_native_financial_callback():
    with pytest.raises(InterruptedError):oracle.reconstruct(raw(),check=lambda:True)


@pytest.mark.parametrize('path',[
    ('raw_request','acquisition','original_annual_effective_rate'),
    ('raw_request','acquisition','actual_cost'),
    ('raw_request','original_contract','current_note_rate'),
    ('raw_request','contract','current_note_rate'),
    ('raw_request','original_contract','original_payment'),
    ('source','native_projection','original_principal'),
    ('original_source','native_projection','ownership','recognized_accrual_fraction',0),
    ('source','intervals',0,'ownership','recognized_accrual_fraction',0),
    ('intervals',0,'native_carry_request','original_annual_effective_rate'),
])
@pytest.mark.parametrize('mutation',['small_drift','numeric_type'])
def test_immutable_input_clones_reject_subtolerance_drift_and_type(path,mutation):
    r=raw();o=native_shape(oracle.reconstruct(r));parent=o
    for key in path[:-1]:parent=parent[key]
    original=parent[path[-1]]
    parent[path[-1]]=original+1e-10 if mutation=='small_drift' else int(original)
    if path[0] in ('source','original_source'):
        name='current_source_receipt_hash' if path[0]=='source' else 'original_source_receipt_hash'
        o[name]=oracle.source_hash(o[path[0]])
    with pytest.raises(ArithmeticError,match='exact input'):
        oracle.verify(r,o)


@pytest.mark.parametrize('path',[
    ('raw_request','acquisition','original_annual_effective_rate'),
    ('raw_request','contract','opening_investor_interest',0),
    ('intervals',0,'native_carry_request','original_annual_effective_rate'),
])
def test_retained_signed_zero_input_identity_is_preserved(path):
    r=raw(y=0.);o=native_shape(oracle.reconstruct(r));parent=o
    for key in path[:-1]:parent=parent[key]
    assert parent[path[-1]]==0.
    parent[path[-1]]=-0.
    with pytest.raises(ArithmeticError,match='exact input'):
        oracle.verify(r,o)


@pytest.mark.parametrize('shock',['default','turnover','refinance','lgd','recovery_dates','path_boundary'])
def test_changed_current_economics_keep_original_purchase_cost_and_yield(shock):
    r=raw(refi=10. if shock=='refinance' else 0.);original_cost=r['acquisition']['actual_cost'];c=r['contract'];c['identity']['run_id']='shocked';r['pairing']['current_run_id']='shocked'
    if shock=='default':c['path'][0]['credit_hazards_per_year'][0][3]=.8
    elif shock=='turnover':c['path'][0]['turnover_hazard_per_year']=.4
    elif shock=='refinance':c['path'][0]['refinancing_market_rate']=0.
    elif shock=='lgd':c['loss_given_default'][0]=.8
    elif shock=='recovery_dates':
        for tail in c['complete_recovery_tails']:
            tail[0]['lag_days']=10;tail[1]['lag_days']=25
    else:
        next_path=deepcopy(c['path'][0]);next_path['start_ordinal']+=5;next_path['credit_hazards_per_year'][0][3]=.5;c['path'].append(next_path)
    o=oracle.reconstruct(r);assert oracle.verify(r,native_shape(o))['verified']
    assert o['actual_acquisition_cost']==original_cost and o['original_quote_pv']==pytest.approx(original_cost,abs=1e-9)
    assert o['total_actual_cash']==pytest.approx(original_cost+o['total_accounting_income'],abs=1e-9)
    assert o['original_source_raw_hash']!=o['current_source_raw_hash']
    for left,right in zip(o['intervals'],o['intervals'][1:]):assert right['opening']['earned_eir'][:3]==left['closing_live_earned_eir']
    assert o['cumulative_earned_eir_conservation_error']==pytest.approx(0.,abs=1e-12)

@pytest.mark.parametrize('field',['current_note_rate','original_payment','day_count','opening_probability','payment_dates','identity','ownership'])
def test_original_terms_cannot_drift_under_pairing(field):
    r=raw();c=r['contract']
    if field=='current_note_rate':c[field]+=.01
    elif field=='original_payment':c[field]+=1.;c['current_payment']+=1.
    elif field=='day_count':c[field]='act360'
    elif field=='opening_probability':c[field]=[.5,0.,0.]
    elif field=='payment_dates':c[field][0]+=1
    elif field=='identity':c[field]['original_loan_id']='other'
    else:c[field]['recognized_accrual_fraction'][0]=.5
    with pytest.raises(ArithmeticError,match='pairing'):oracle.reconstruct(r)

@pytest.mark.parametrize('field',['original_source','original_source_raw_hash','current_source_raw_hash','original_source_receipt_hash','current_source_receipt_hash','original_interval_count','closing_live_earned_eir','quote_basis'])
def test_immutable_quote_source_and_current_reward_lineage_tamper(field):
    r=raw();o=native_shape(oracle.reconstruct(r))
    if field=='original_source':o[field]['native_projection']['cashflows'][0]['interest']+=1.
    elif field=='original_interval_count':o['admission'][field]=float(o['admission'][field])
    elif field=='closing_live_earned_eir':o['intervals'][0][field][0]+=1.
    else:o[field]='caller'
    with pytest.raises(ArithmeticError):oracle.verify(r,o)


def test_absolute_monetary_principal_not_probability_weighted_again():
    r=raw(default=0.);c=r['contract'];c['opening_probability']=[.4,0.,0.];r['original_contract']=deepcopy(c)
    source=oracle.boundary_source(c);p=r['policy']['interest'];y=r['acquisition']['original_annual_effective_rate'];start=c['as_of_ordinal']
    cost=sum((x['principal']+x['interest'])*math.exp(-math.log1p(y)*(x['ordinal']-start)/365.) for x in source['native_projection']['cashflows'])
    r['acquisition'].update(actual_cost=cost,opening_gross_carrying=[cost,0.,0.]);o=oracle.reconstruct(r)
    assert o['intervals'][0]['transport']['closing']['principal'][0]==100.
    assert o['intervals'][0]['transport']['closing']['probability'][0]==.4
    assert o['total_actual_cash']>100.


@pytest.mark.parametrize('target',['original_cash','original_disposal','original_recovery_id','original_full_end','original_full_end_float'])
def test_original_gross_closeout_and_cash_receipts_independent(target):
    r=raw();o=native_shape(oracle.reconstruct(r))
    if target=='original_cash':o['original_reviewed_interest_cash'][0]['amount']+=1.
    elif target=='original_disposal':o['original_economic_closeouts'][0]['interest_economic_disposal']+=1.
    elif target=='original_recovery_id':o['original_economic_closeouts'][0]['native_principal_cash_ids'][0]='foreign'
    elif target=='original_full_end':o['original_full_end_ordinal']+=1
    else:o['original_full_end_ordinal']=float(o['original_full_end_ordinal'])
    with pytest.raises(ArithmeticError):oracle.verify(r,o)


def test_original_quote_gross_claim_closure_remains_separate_from_current_shock():
    r=raw();r['contract']['loss_given_default'][0]=.9;o=oracle.reconstruct(r)
    original=o['original_economic_closeouts'][0];current=o['default_closeouts'][0]
    assert original['principal_cash']>current['principal_cash']
    assert original['closing_economic_principal']==original['closing_economic_interest']==0.
    assert o['original_quote_pv']==pytest.approx(r['acquisition']['actual_cost'],abs=1e-9)
    assert o['original_full_end_ordinal']==r['contract']['as_of_ordinal']+50


def test_equal_amount_multiorigin_cash_identity_keeps_original_append_order():
    r=raw();c=r['contract'];c['opening_borrower_principal']=[100./3]*3;c['opening_investor_principal']=[100./3]*3;c['opening_probability']=[1./3]*3
    c['path'][0]['credit_hazards_per_year']=[[0.,0.,0.,.2,0.],[0.,0.,0.,.2,0.],[0.,0.,0.,.2,0.]]
    r['original_contract']=deepcopy(c);source=oracle.boundary_source(c);original_cash,_,_=oracle.original_economics(r,source,lambda:False);start=c['as_of_ordinal'];k=math.log1p(r['acquisition']['original_annual_effective_rate'])
    cost=sum((x['principal']+x['interest'])*math.exp(-k*(x['ordinal']-start)/365.) for x in source['native_projection']['cashflows'])+sum(x['amount']*math.exp(-k*(x['ordinal']-start)/365.) for x in original_cash)
    r['acquisition'].update(actual_cost=cost,opening_gross_carrying=[cost/3]*3);o=oracle.reconstruct(r)
    assert oracle.verify(r,native_shape(o))['verified']
    ids=[x['native_principal_cash_ids'] for x in o['default_closeouts']]
    assert len(ids)==6 and len({x for group in ids for x in group})==12
    b=o['source']['capture']['boundaries'][1]
    assert [int(x['entitlement_id'].rsplit('-',1)[1]) for x in b['default_recoveries']]==sorted(int(x['entitlement_id'].rsplit('-',1)[1]) for x in b['default_recoveries'])


def test_complete_comparison_cancellation_after_raw_projection():
    r=raw();o=native_shape(oracle.reconstruct(r));calls=0
    def check():
        nonlocal calls
        calls+=1
        return calls>1500
    with pytest.raises(InterruptedError):oracle.verify(r,o,check=check)
