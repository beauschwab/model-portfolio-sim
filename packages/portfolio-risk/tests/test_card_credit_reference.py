"""Independent hand, strict boundary, complete source and intent tamper checks."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest

ROOT=Path(__file__).resolve().parents[3]
from portfolio_risk.analytics import card_credit_reference as oracle



def raw():return json.loads((ROOT/'examples/models/card-credit-accounting-1.json').read_text())


def closure():
    r=raw();start=r['contract']['as_of_ordinal'];r['acquisition']['cost']=101.
    r['contract']['cycles']=[dict(statement_ordinal=start+10,due_ordinal=start+15)]
    r['contract']['steps']=[dict(ordinal=start+10,action=dict(kind='payment',amount=100.)),
        dict(ordinal=start+11,action=dict(kind='payment',amount=1.)),dict(ordinal=start+11,action=dict(kind='set_limit',amount=0.))]
    r['termination']=dict(kind='reviewed_zero_rights_closure',ordinal=start+11,review=dict(id='reviewed-card-closure',revision='r1'))
    r['chargeoffs']=[];r['opening_allowance']=0.;return r


def case(name):
    r=closure() if name in {'closure','premium','negative_eir'} else raw();c=r['contract'];start=c['as_of_ordinal']
    if name=='billed':
        c['cycles']=[dict(statement_ordinal=start+10,due_ordinal=start+15)];c['steps'][0]['ordinal']=start+11
    elif name=='premium':r['acquisition'].update(cost=103.,original_annual_effective_rate=.02)
    elif name=='negative_eir':
        r['acquisition'].update(cost=90.,original_annual_effective_rate=-.01)
        c['opening_principal']=[0.,0.,100.];c['rates'].update(cash_advance=0.,promotional=.36,promotional_expiry_ordinal=start+10,post_promotion=0.)
        c['steps']=[dict(ordinal=start+11,action=dict(kind='payment',amount=101.)),dict(ordinal=start+11,action=dict(kind='set_limit',amount=0.))]
    elif name=='partial':
        c['steps']=[dict(ordinal=start+10,action=dict(kind='payment',amount=50.)),dict(ordinal=start+10,action=dict(kind='default'))]
        r['chargeoffs'][0]['principal_amount']=20.;r['opening_allowance']=20.
    elif name=='same_day':c['recovery_delay_days']=0;r['chargeoffs'][0]['recognition_ordinal']=start+10
    elif name=='transferred':
        r['policy']['accounting'].update(accrued='transfer_until_reviewed_reversal',basis='transfer_until_reviewed_chargeoff');r['chargeoffs']=[]
    elif name=='zero_net':c['collection_cost']=60.
    elif name=='classes':
        c['opening_principal']=[20.,30.,50.];c['rates'].update(purchase=.18,cash_advance=.36,promotional=0.,promotional_expiry_ordinal=start+5,post_promotion=.72)
    elif name=='compound':
        c['cycles']=[dict(statement_ordinal=start+5,due_ordinal=start+9)];c['compound_billed_interest']=True
    elif name=='ordered':
        c['steps']=[dict(ordinal=start+10,action=dict(kind='payment',amount=20.)),dict(ordinal=start+10,action=dict(kind='payment',amount=30.)),dict(ordinal=start+10,action=dict(kind='default'))]
        r['chargeoffs'][0]['principal_amount']=20.;r['opening_allowance']=20.
    elif name=='act365':c['day_count']='act365_fixed'
    elif name=='allowance_review':
        r['opening_allowance']=0.;r['allowance_reviews']=[dict(review=dict(id='new-allowance',revision='r1'),recognition_ordinal=start+31,phase='before_credit',target_amount=40.)]
    elif name=='minimum':
        c['opening_principal']=[20.,30.,50.];c['rates'].update(purchase=.18,cash_advance=.36,promotional=0.)
        c['cycles']=[dict(statement_ordinal=start+5,due_ordinal=start+9)]
        c['steps']=[dict(ordinal=start+6,action=dict(kind='payment',amount=10.)),dict(ordinal=start+10,action=dict(kind='default'))]
        r['chargeoffs']=[]
    return r


ACCEPTED=('canonical','billed','closure','premium','negative_eir','partial','same_day','transferred','zero_net','classes','compound','ordered','act365','allowance_review','minimum')


def total(result,code):return sum(p['amount'] for i in result['intents'] for p in i['postings'] if p['account']==code)


@pytest.mark.parametrize('name',ACCEPTED)
def test_pure_card_complete_source_and_accounting_conservation(name):
    r=case(name);result=oracle.reconstruct(r)
    assert total(result,'cash')==pytest.approx(sum(x['principal']+x['interest']+x['fees'] for x in result['economic']['cashflows']))
    assert len(result['intents'])==len(result['lineage'])
    for intent,lineage in zip(result['intents'],result['lineage']):
        assert sum(p['amount'] for p in intent['postings'])==pytest.approx(0.,abs=1e-10)
        assert intent['recognition_ordinal']==lineage['ordinal']
    assert not result['mixed_ledger_adoption'] and not result['empirical_validation']


def test_hand_default_unbilled_and_gross_cost_never_second_loss():
    result=oracle.reconstruct(raw());d=result['economic']['default']
    assert d['recognized_accrued_interest']==pytest.approx(100*.36*10/360)
    assert result['economic']['statements']==[]
    assert result['daily'][9]['source_unbilled_interest']==pytest.approx([0.,.9,0.])
    assert total(result,'cash')==50. and total(result,'credit_loss_expense')==10.
    assert result['recovery']['principal_only_loss']==40. and result['recovery']['total_economic_loss']==50.
    assert sum(x['earnings_delta'] for x in result['daily'])==pytest.approx(-10.)
    assert result['commitment_source']['funded_contingent_draw']==0.
    assert result['daily'][9]['source_undrawn']==100. and result['daily'][9]['owned']['live'][0]['undrawn']==0.


def test_hand_statement_reclassifies_coupon_once_with_exact_recognition_indices():
    result=oracle.reconstruct(case('billed'))
    assert len(result['statement_lineage'])==1 and result['statement_lineage'][0]['interest_recognized_again'] is False
    assert result['daily'][10]['owned']['live'][0]['accrued_interest']==pytest.approx(1.)
    assert result['daily'][10]['source_billed_interest']==pytest.approx([0.,1.,0.])
    assert sum(x['contractual_interest_earned'] for x in result['income_prefix']['rows'])==pytest.approx(1.1)
    indices=[i for line in result['lineage'] for i in line['source_recognition_indices']]
    assert indices==list(range(11))


def test_hand_interest_only_tail_original_yield_and_signed_retirement():
    result=oracle.reconstruct(case('premium'));first=103*(1.02**(10/365)-1);last=(103+first-100)*(1.02**(1/365)-1)
    assert -total(result,'interest_income')==pytest.approx(first+last,abs=1e-11)
    assert total(result,'basis_recognition_expense')==pytest.approx(2+first+last,abs=1e-11)
    assert sum(x['earnings_delta'] for x in result['daily'])==pytest.approx(-2.)
    assert result['daily'][10]['owned']['live'][0]['principal']==0.
    assert result['daily'][10]['owned']['live'][0]['accrued_interest']==pytest.approx(1.)
    assert result['daily'][10]['accounting_status']=='live' and result['daily'][11]['accounting_status']=='reviewed_closed'
    assert result['daily'][11]['economic_state']=='performing'


def test_hand_classes_promo_days_and_credit_state_separate():
    result=oracle.reconstruct(case('classes'));coupon=(20*.18+30*.36)*10/360+50*.72*5/360
    assert result['economic']['default']['recognized_accrued_interest']==pytest.approx(coupon)
    assert [s['principal'] for s in result['acquisition_opening']['live']]==[100.,0.,0.]


def test_hand_zero_net_uses_actual_default_origin_without_fictitious_cash():
    result=oracle.reconstruct(case('zero_net'));assert result['economic']['cashflows']==[]
    cost=next(x for x in result['lineage'] if x['phase']=='card_reviewed_recovery_cost')
    assert cost['source_event_ids']==['default:738926']
    assert total(result,'cash')==0. and total(result,'credit_loss_expense')==60.


@pytest.mark.parametrize('target',['source_cash','source_recognition','statement','recognition_index','claim','basis','original_yield','cost','intent','lineage','commitment','report','work_float','date_float'])
def test_card_source_and_intent_tamper_reject(target):
    r=case('billed');result=oracle.reconstruct(r)
    if target=='source_cash':result['economic']['cashflows'][0]['principal']+=.01
    elif target=='source_recognition':result['economic']['recognition'][0]['interest']+=.01
    elif target=='statement':result['statement_lineage'][0]['interest_recognized_again']=True
    elif target=='recognition_index':result['lineage'][0]['source_recognition_indices']=[1]
    elif target=='claim':result['closing']['claims'][0]['outstanding_principal']+=.01
    elif target=='basis':result['income_prefix']['rows'][0]['basis_amortization']+=.01
    elif target=='original_yield':result['raw_request']['acquisition']['original_annual_effective_rate']=.01
    elif target=='cost':result['recovery']['cost_cash']+=.01
    elif target=='intent':result['intents'][0]['postings'][0]['amount']+=.01
    elif target=='lineage':result['lineage'][0]['economic_source']['revision']='different'
    elif target=='commitment':result['commitment_source']['funded_contingent_draw']=1.
    elif target=='report':result['reporting']['owned']['claims'][0]['outstanding_principal']+=.01
    elif target=='work_float':result['admitted_work']=float(result['admitted_work'])
    else:result['statement_lineage'][0]['ordinal']=float(result['statement_lineage'][0]['ordinal'])
    with pytest.raises(ArithmeticError):oracle.verify(r,result)


def invalid(name):
    r=closure() if name in {'negative_tail_basis','missing_limit','unpaid_closure'} else raw();c=r['contract']
    if name=='negative_tail_basis':r['acquisition']['cost']=100.
    elif name=='missing_limit':c['steps'].pop()
    elif name=='unpaid_closure':c['steps'][1]['action']['amount']=0.
    elif name=='grace':c['initial_purchase_grace']=True
    elif name=='regain':c['regain_grace_after_full_statement_payment']=True
    elif name=='draw':c['steps'][0]['action']=dict(kind='draw',balance_class='purchase',amount=1.)
    elif name=='fee':c['steps'][0]['action']=dict(kind='fee',balance_class='purchase',amount=1.)
    elif name=='state':c['steps'][0]['action']=dict(kind='set_state',state='nonaccrual')
    elif name=='pricing':c['identity']['measure']='pricing'
    elif name=='face':r['acquisition']['principal']=99.
    elif name=='revision':r['economic_source']['revision']='bad'
    elif name=='opening_right':c['opening_unbilled_interest'][1]=1.
    elif name=='uncovered_cost':c['collection_cost']=70.
    elif name=='missing_tail':c['horizon_days']=29
    elif name=='asof_action':c['steps'][0]['ordinal']=c['as_of_ordinal']
    elif name=='ccf':r['policy']['commitment']='infer_ccf'
    elif name=='no_exit':c['steps']=[]
    elif name=='payment_after_default':c['steps'].append(dict(ordinal=c['steps'][0]['ordinal'],action=dict(kind='payment',amount=0.)))
    elif name=='float_date':c['steps'][0]['ordinal']=float(c['steps'][0]['ordinal'])
    elif name=='unknown':r['caller_verified']=True
    return r


REJECTED=('negative_tail_basis','missing_limit','unpaid_closure','grace','regain','draw','fee','state','pricing','face','revision','opening_right','uncovered_cost','missing_tail','asof_action','ccf','no_exit','payment_after_default','float_date','unknown')


@pytest.mark.parametrize('name',REJECTED)
def test_independent_fail_closed_rejected_domain(name):
    with pytest.raises(ArithmeticError):oracle.reconstruct(invalid(name))


def test_deadline_and_work_admission_before_card_source_projection(monkeypatch):
    with pytest.raises(InterruptedError):oracle.reconstruct(raw(),lambda:True)
    r=raw();c=r['contract'];c['horizon_days']=1080
    c['steps']=[dict(ordinal=c['as_of_ordinal']+1,action=dict(kind='payment',amount=0.)) for _ in range(3000)]+c['steps']
    monkeypatch.setattr(oracle,'project',lambda *a,**k:pytest.fail('card projected before bounded admission'))
    with pytest.raises(ArithmeticError,match='work'):oracle.reconstruct(r)
