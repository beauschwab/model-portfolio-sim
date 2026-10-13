"""Independent consumer-credit financial hand cases; no native execution."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest

from portfolio_risk.analytics import consumer_credit_reference as oracle

ROOT=Path(__file__).resolve().parents[3]


def ident(name):
    return {'id':name,'revision':'review-1'}


def raw_default(*,auto=False,delay=10,chargeoff=True):
    contract=json.loads((ROOT/'examples/models/consumer-installment-1.json').read_text())
    start=contract['as_of_ordinal']
    contract.update(horizon_days=30,original_principal=100.,opening_principal=100.,note_rate=.36,
                    disclosed_apr=.4,day_count='act360',original_payment=101.,payment_dates=[start+10,start+30],
                    steps=[{'ordinal':start+10,'action':{'kind':'default'}}])
    contract['product'].update(collection_fraction=.6,collection_cost=10.,recovery_delay_days=delay)
    if auto:
        contract['product']=dict(kind='auto',new_vehicle=False,vintage_ordinal=start-365,original_ltv=1.,channel='branch',
            collateral_value_as_of=60.,annual_depreciation=0.,sale_haircut=0.,repossession_cost=10.,recovery_delay_days=delay)
    source=ident('economic');source['revision']=contract['identity']['source_revision']
    raw=dict(schema='consumer-credit-accounting-1',contract=contract,path_id='path-1',economic_source=source,
        acquisition=dict(identity=ident('purchase'),original_yield_anchor=ident('original-yield'),income_review=ident('eir-policy'),
            accounting_basis='supplied_non_pcd_gross_cost-1',purchase_ordinal=start,principal=100.,cost=100.,original_annual_effective_rate=0.),
        policy=dict(accounting=dict(identity=ident('credit-policy'),accrued='reverse_income_at_default',
            basis='recognize_in_earnings_at_default',allowance_shortfall='reject',excess_cash_recovery='credit_allowance'),
            recovery_review=ident('recovery-policy'),cost_review=ident('cost-policy'),original_income='frozen_original_eir_until_exit-1',
            terminal_basis='recognize_in_earnings_at_payoff-1',recovery_cost='gross_recovery_with_credit_loss_cost-1'),
        claim_id='claim-1',reporting_end_ordinal=start+15,opening_allowance=40.,allowance_reviews=[],chargeoffs=[])
    if chargeoff:
        raw['chargeoffs']=[dict(review=ident('chargeoff-policy'),claim_id='claim-1',recognition_ordinal=start+10,
            principal_amount=40.,book_adjustment_amount=0.,accrued_interest_reversal=0.)]
    return raw


def postings(result,event):
    return [p for intent in result['intents'] if intent['event']==event for p in intent['postings']]


@pytest.mark.parametrize('auto',[False,True])
def test_gross_recovery_and_separate_cost_handcalc_default_suppresses_due(auto):
    raw=raw_default(auto=auto);result=oracle.reconstruct(raw)
    economic=result['economic'];d=economic['default'];claim=result['closing']['claims'][0]
    # P100 * 36% *10/360=1; P100-gross60=40; gross60-cost10=net50.
    assert d['recognized_accrued_interest']==pytest.approx(1.)
    assert [c['kind'] for c in economic['cashflows']]==['recovery']
    assert result['recovery']['principal_only_loss']==40.
    assert result['recovery']['total_economic_loss']==50.
    assert economic['cashflows'][0]['principal']==50.
    assert claim['economic_principal_loss']==40.
    assert claim['principal_charged_off']==40.
    assert claim['recovery_cash_collected']==60.
    assert claim['outstanding_principal']==0.
    assert result['closing']['allowance']==0.
    assert sum(d['cash_delta'] for d in result['daily'])==50.
    assert sum(d['earnings_delta'] for d in result['daily'])==pytest.approx(-10.)
    assert postings(result,'product_reviewed_recovery_cost')==[{'account':'cash','amount':-10.},{'account':'credit_loss_expense','amount':10.}]
    assert postings(result,'reviewed_credit_chargeoff')==[{'account':'default_principal','amount':-40.},{'account':'allowance','amount':40.}]
    assert result['reporting']['owned']['claims'][0]['outstanding_principal']==60.
    assert result['reporting']['owned']['claims'][0]['remaining_recoveries']==[{'payment_ordinal':raw['contract']['as_of_ordinal']+20,'amount':60.}]


def test_economic_loss_is_not_an_automatic_allowance_or_writeoff_journal():
    raw=raw_default(chargeoff=False);raw['opening_allowance']=0.
    result=oracle.reconstruct(raw);claim=result['closing']['claims'][0]
    assert claim['outstanding_principal']==40. and claim['principal_charged_off']==0.
    assert claim['remaining_recoveries']==[] and result['closing']['allowance']==0.
    assert not postings(result,'reviewed_credit_chargeoff')
    assert not postings(result,'reviewed_allowance_target')
    assert sum(d['cash_delta'] for d in result['daily'])==50.
    assert sum(d['earnings_delta'] for d in result['daily'])==pytest.approx(-10.)


def test_payoff_cash_once_and_reviewed_original_basis_retirement_handcalc():
    raw=raw_default(chargeoff=False);raw['opening_allowance']=0.;raw['acquisition']['cost']=90.
    raw['contract']['steps'][0]['action']={'kind':'payoff'}
    result=oracle.reconstruct(raw)
    assert result['recovery'] is None and result['closing']['claims']==[]
    assert sum(d['cash_delta'] for d in result['daily'])==pytest.approx(101.)
    assert sum(d['earnings_delta'] for d in result['daily'])==pytest.approx(11.)
    assert result['income_prefix']['remaining_gross_carrying_value']==90.
    assert result['income_prefix']['rows'][-1]['closing_book_adjustment']==pytest.approx(-11.)
    assert result['income_prefix_request']['events'][-1]['principal_cash']==0.
    for live in result['closing']['live']:
        assert live==pytest.approx(oracle._empty_live(),abs=1e-12)
    assert postings(result,'product_prepayment_basis_retirement')[1]['amount']==pytest.approx(-11.)


def test_positive_and_negative_original_eir_prefix_use_retained_acquisition_cost():
    for yield_rate in (.02,-.02):
        raw=raw_default(chargeoff=False);raw['acquisition']['cost']=90.
        raw['acquisition']['original_annual_effective_rate']=yield_rate
        result=oracle.reconstruct(raw)
        expected=90.*math.expm1(math.log1p(yield_rate)*10/365.)
        assert sum(d['effective_interest'] for d in result['daily'])==pytest.approx(expected,abs=1e-12)
        assert result['income_prefix']['remaining_gross_carrying_value']==pytest.approx(90.+expected)
        # The reviewed accrued reversal and basis retirement reconcile original cost at exit.
        assert sum(d['earnings_delta'] for d in result['daily'])==pytest.approx(0.,abs=1e-12)


def test_same_day_default_chargeoff_recovery_cost_has_explicit_order_and_lineage():
    raw=raw_default(delay=0);result=oracle.reconstruct(raw);day=raw['contract']['as_of_ordinal']+10
    events=[i['event'] for i in result['intents'] if i['recognition_ordinal']==day]
    assert events==['product_original_effective_interest','economic_default_transfer','default_accrued_income_reversal',
        'default_basis_earnings','reviewed_credit_chargeoff','modeled_physical_recovery_cash','product_reviewed_recovery_cost']
    assert result['daily'][10]['cash_delta']==50.
    for index,(intent,lineage) in enumerate(zip(result['intents'],result['lineage'])):
        assert lineage['intent_index']==index and lineage['phase']==intent['event']
        assert math.fsum(p['amount'] for p in intent['postings'])==pytest.approx(0.,abs=1e-12)


def test_allowance_before_default_then_reviewed_writeoff_not_inferred():
    raw=raw_default();raw['opening_allowance']=0.;day=raw['contract']['as_of_ordinal']+10
    raw['allowance_reviews']=[dict(review=ident('allowance-1'),recognition_ordinal=day,phase='before_credit',target_amount=40.)]
    result=oracle.reconstruct(raw)
    events=[i['event'] for i in result['intents'] if i['recognition_ordinal']==day]
    assert events.index('reviewed_allowance_target')<events.index('economic_default_transfer')<events.index('reviewed_credit_chargeoff')
    assert postings(result,'reviewed_allowance_target')==[dict(account='credit_loss_expense',amount=40.),dict(account='allowance',amount=-40.)]
    assert sum(d['earnings_delta'] for d in result['daily'])==pytest.approx(-50.)


def test_partial_payment_then_default_consumes_only_actual_remaining_principal():
    raw=raw_default(chargeoff=False);c=raw['contract'];start=c['as_of_ordinal']
    c['original_payment']=50.;c['steps'][0]['ordinal']=start+20;c['product']['recovery_delay_days']=5
    result=oracle.reconstruct(raw)
    # Day10 paid50: interest1 + principal49; next10d interest51*.36*10/360=.51.
    d=result['economic']['default'];assert d['principal']==51.
    assert d['recognized_accrued_interest']==pytest.approx(.51)
    assert result['recovery']['gross_cash']==pytest.approx(30.6)
    assert result['recovery']['principal_only_loss']==pytest.approx(20.4)
    assert result['recovery']['total_economic_loss']==pytest.approx(30.4)
    assert sum(x['cash_delta'] for x in result['daily'])==pytest.approx(70.6)
    assert sum(x['earnings_delta'] for x in result['daily'])==pytest.approx(-9.)


def test_auto_delayed_repossession_uses_actual_elapsed_collateral_decay():
    raw=raw_default(auto=True,chargeoff=False);c=raw['contract'];start=c['as_of_ordinal']
    c.update(horizon_days=365,payment_dates=[start+365]);raw['reporting_end_ordinal']=start+10
    c['product'].update(collateral_value_as_of=80.,annual_depreciation=.25,recovery_delay_days=355)
    result=oracle.reconstruct(raw)
    # Sold one actual365d year after valuation:80*(1-.25)=60 gross; cost10.
    assert result['recovery']['ordinal']==start+365
    assert result['recovery']['gross_cash']==pytest.approx(60.)
    assert result['reporting']['owned']['claims'][0]['outstanding_principal']==100.
    assert result['closing']['claims'][0]['outstanding_principal']==pytest.approx(40.)


def test_transferred_gross_accrual_and_negative_basis_survive_principal_recovery():
    raw=raw_default(chargeoff=False);p=raw['policy']['accounting']
    p.update(accrued='transfer_until_reviewed_reversal',basis='transfer_until_reviewed_chargeoff')
    result=oracle.reconstruct(raw);q=result['closing']['claims'][0]
    assert q['outstanding_principal']==40.
    assert q['outstanding_accrued_interest']==pytest.approx(1.)
    assert q['outstanding_book_adjustment']==pytest.approx(-1.)
    assert q['basis_recognized']==0. and q['accrued_reversed']==0.
    assert postings(result,'default_accrued_transfer')[1]['account']=='default_accrued_interest'
    assert postings(result,'default_basis_transfer')[1]['account']=='default_book_adjustment'
    assert not postings(result,'default_accrued_income_reversal')


def test_reviewed_signed_basis_and_accrued_retirement_conserve_full_claim():
    raw=raw_default(chargeoff=False);day=raw['contract']['as_of_ordinal']+10
    raw['policy']['accounting'].update(accrued='transfer_until_reviewed_reversal',basis='transfer_until_reviewed_chargeoff')
    raw['chargeoffs']=[dict(review=ident('signed-retirement'),claim_id='claim-1',recognition_ordinal=day,
        principal_amount=40.,book_adjustment_amount=-1.,accrued_interest_reversal=1.)]
    result=oracle.reconstruct(raw);q=result['closing']['claims'][0]
    assert q['outstanding_principal']==0.
    assert q['outstanding_book_adjustment']==pytest.approx(0.,abs=1e-12)
    assert q['outstanding_accrued_interest']==pytest.approx(0.,abs=1e-12)
    assert result['closing']['allowance']==pytest.approx(1.)
    assert q['initial_principal']==q['principal_charged_off']+q['recovery_applied_to_principal']+q['outstanding_principal']


def test_after_credit_allowance_review_does_not_fund_prior_reject_policy_chargeoff():
    raw=raw_default();day=raw['contract']['as_of_ordinal']+10;raw['opening_allowance']=0.
    raw['allowance_reviews']=[dict(review=ident('too-late'),recognition_ordinal=day,phase='after_credit',target_amount=40.)]
    with pytest.raises(ArithmeticError,match='exceeds allowance'):
        oracle.reconstruct(raw)


def test_recovery_excess_disposition_requires_explicit_policy():
    for policy,account in [('credit_allowance','allowance'),('recognize_in_earnings','recovery_income')]:
        raw=raw_default();raw['opening_allowance']=100.;raw['chargeoffs'][0]['principal_amount']=100.
        raw['policy']['accounting']['excess_cash_recovery']=policy
        result=oracle.reconstruct(raw)
        assert postings(result,'modeled_physical_recovery_cash')==[dict(account='cash',amount=60.),dict(account=account,amount=-60.)]
        assert result['closing']['allowance']==(60. if policy=='credit_allowance' else 0.)


@pytest.mark.parametrize('change',[
    lambda r:r.update(unknown=True),
    lambda r:r['policy']['accounting'].update(accrued='infer'),
    lambda r:r['contract']['identity'].update(measure='pricing'),
    lambda r:r['contract'].update(note_rate=3.),
    lambda r:r['contract']['steps'][0].update(ordinal=float(r['contract']['steps'][0]['ordinal'])),
    lambda r:r['contract']['product'].update(collection_fraction=1.1),
    lambda r:r['contract']['product'].update(collection_cost=61.),
    lambda r:r['contract']['product'].update(recovery_delay_days=21),
    lambda r:r.update(opening_allowance=0.),
    lambda r:r['chargeoffs'][0].update(claim_id='other'),
    lambda r:r['economic_source'].update(id='hidden\n'),
    lambda r:r['contract']['steps'][0].update(action={'kind':'change_payment','amount':10.}),
])
def test_unsupported_domains_and_incomplete_tail_reject(change):
    raw=raw_default();change(raw)
    with pytest.raises(ArithmeticError):
        oracle.reconstruct(raw)


@pytest.mark.parametrize('path',[
    ('raw_request','economic_source','revision'),('economic','default','principal_loss'),
    ('recovery','gross_cash'),('recovery','cost_cash'),('income_prefix','rows',0,'effective_interest'),
    ('intents',0,'postings',0,'amount'),('closing','claims',0,'economic_principal_loss'),
    ('lineage',0,'intent_index'),('reporting','ordinal'),
])
def test_any_retained_finance_or_identity_tampering_rejects(path):
    raw=raw_default();result=oracle.reconstruct(raw);node=result
    for key in path[:-1]:node=node[key]
    last=path[-1];node[last]=node[last]+'tampered' if isinstance(node[last],str) else node[last]+1
    with pytest.raises(ArithmeticError):oracle.verify(raw,result)


def test_exact_integer_dates_and_no_native_callback(monkeypatch):
    from portfolio_risk.core import quant_native
    def forbidden(*args,**kwargs):
        pytest.fail('independent artifact oracle called native finance')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    raw=raw_default();result=oracle.reconstruct(raw)
    receipt=oracle.verify(raw,json.loads(json.dumps(result)))
    assert receipt['verified'] and not receipt['mixed_ledger_adoption'] and not receipt['empirical_validation']
    result['daily'][1]['ordinal']=float(result['daily'][1]['ordinal'])
    with pytest.raises(ArithmeticError):oracle.verify(raw,result)


def test_cooperative_cancellation():
    with pytest.raises(Exception,match='cancel'):
        oracle.reconstruct(raw_default(),check=lambda:True)


def test_canonical_complete_native_source_example_independent_handcash():
    raw=json.loads((ROOT/'examples/models/consumer-credit-accounting-1.json').read_text())
    result=oracle.reconstruct(raw)
    assert sum(d['cash_delta'] for d in result['daily'])==50.
    assert result['closing']['claims'][0]['outstanding_principal']==0.
    assert result['reporting']['owned']['claims'][0]['outstanding_principal']==100.
    assert all(i['policy']==raw['acquisition']['income_review'] for i in result['intents'] if i['event']=='product_original_effective_interest')
    cost=next(i for i in result['intents'] if i['event']=='product_reviewed_recovery_cost')
    assert cost['claim_id']==raw['claim_id'] and cost['policy']==raw['policy']['cost_review']


def test_unfinished_live_full_horizon_rejects_but_default_claim_is_retained():
    raw=raw_default(chargeoff=False);c=raw['contract'];start=c['as_of_ordinal'];c['payment_dates']=[start+30]
    c['steps']=[dict(ordinal=start+30,action=dict(kind='payment',amount=1.))]
    with pytest.raises(ArithmeticError,match='unfinished live'):
        oracle.reconstruct(raw)
    default=oracle.reconstruct(raw_default(chargeoff=False))
    assert default['closing']['claims'][0]['outstanding_principal']==40.


def test_zero_net_cash_retains_actual_default_origin_without_invented_cash_event():
    raw=raw_default();raw['contract']['product']['collection_cost']=60.
    result=oracle.reconstruct(raw)
    assert result['economic']['cashflows']==[]
    assert result['recovery']['total_economic_loss']==100.
    assert sum(d['cash_delta'] for d in result['daily'])==0.
    assert sum(d['earnings_delta'] for d in result['daily'])==pytest.approx(-60.)
    cost=next(l for l in result['lineage'] if l['phase']=='product_reviewed_recovery_cost')
    assert cost['product_event_ids']==[f"default:{raw['contract']['as_of_ordinal']+10}"]

