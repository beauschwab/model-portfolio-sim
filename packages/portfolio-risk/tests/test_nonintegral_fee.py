"""Independent hand, native ownership and fee-tail tamper controls."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import pytest
from portfolio_risk.core import model_native
from portfolio_risk.analytics.nonintegral_fee_reference import verify

ROOT = Path(__file__).resolve().parents[3]


def raw():
    return json.loads((ROOT/'examples/models/nonintegral-fee-accounting-1.json').read_text())


def run(request):
    return model_native.run(request['schema'], request)[0]


def event(request, day, sequence, action, amount, reason):
    f=request['fees'][0]
    return dict(event_id=f'event-{sequence}',fee_id=f['fee_id'],source=deepcopy(f['source']),
                review=dict(id=f'review-{sequence}',revision='1'),ordinal=request['asof_ordinal']+day,
                sequence=sequence,action=action,amount=amount,reason=reason)


@pytest.fixture(scope='module')
def accepted():
    request=raw();return request,run(request)


def test_fee_income_rights_survive_report_and_opposite_fee_owners_are_not_netted(accepted):
    request,result=accepted;assert verify(request,result)['verified']
    assert result['reporting'][0]['state']['fee_receivable']==5.
    assert result['reporting'][1]['state']['earned']==-5.
    assert result['closing'][0]['state']['fee_receivable']==0.
    assert result['groups'][0]['postings'][1]['account']=='prepaid_fee_expense'
    assert result['loan_carrying_value_changed'] is False
    totals={}
    for g in result['groups']:
        assert sum(p['amount']for p in g['postings'])==0.
        for p in g['postings']:totals[p['account']]=totals.get(p['account'],0.)+p['amount']
    assert totals['cash']==0. and totals['fee_income']==0.
    # Equal aggregate income/cash still retains both independently earned sources.
    assert len(result['closing'])==2 and [s['state']['earned']for s in result['closing']]==[5.,-5.]


@pytest.mark.parametrize('sign',[1.,-1.])
@pytest.mark.parametrize('kind',['default','exercise','prepaid'])
def test_fee_dispositions_refunds_recovery_tails_and_signed_gross_snapshots(sign,kind):
    request=raw();request['fees']=request['fees'][:1];request['fees'][0]['entitlement']=sign*5.
    if kind=='default': steps=[(5,'earn',5.,'service'),(10,'waive_unpaid_earned',2.,'default'),(20,'cash',3.,'settlement'),(30,'recover_waived',2.,'default')]
    elif kind=='exercise':steps=[(5,'cash',5.,'settlement'),(10,'cancel_unearned',5.,'exercise'),(30,'refund_prepaid',5.,'settlement')]
    else:steps=[(5,'cash',5.,'settlement'),(20,'earn',5.,'service')]
    request['events']=[event(request,d,i,a,sign*x,r)for i,(d,a,x,r)in enumerate(steps)]
    result=run(request);assert verify(request,result)['verified']
    state=result['reporting'][0]['state']
    if kind=='default':assert state['fee_receivable' if sign>0 else 'fee_payable']==sign*3.
    else:assert state['deferred_fee_income'if sign>0 else 'prepaid_fee_expense']==-sign*5.


@pytest.mark.parametrize('target',['amount','ordinal','sequence','review','source','state','postings','reporting','tail','loan_flag','binding_flag','mixed_flag','bool_money'])
def test_independent_fee_replay_rejects_rehashed_source_money_and_evidence(accepted,target):
    request,result=accepted;changed=deepcopy(result)
    if target in {'amount','ordinal','sequence'}:changed['groups'][0]['event'][target]+=1
    elif target in {'review','source'}:changed['groups'][0]['event'][target]['revision']='foreign'
    elif target=='state':changed['groups'][0]['next']['prepaid_fee_expense']+=1.
    elif target=='postings':changed['groups'][0]['postings'].reverse()
    elif target=='reporting':changed['reporting'][0]['state']['fee_receivable']+=1.
    elif target=='tail':changed['groups'].pop()
    elif target=='bool_money':changed['groups'][0]['next']['fee_receivable']=False
    else:changed[{'loan_flag':'loan_carrying_value_changed','binding_flag':'native_credit_event_binding','mixed_flag':'mixed_ledger_adoption'}[target]]=True
    with pytest.raises(ValueError):verify(request,changed)


@pytest.mark.parametrize('kind',['pricing','checkpoint','sign','source','overpayment','duplicate','refund','reason'])
def test_native_fee_unsupported_or_unreviewed_sources_fail_without_fallback(kind):
    request=raw()
    if kind=='pricing':request['measure']='pricing'
    elif kind=='checkpoint':request['opening']={'fee_receivable':5.}
    elif kind=='sign':request['events'][0]['amount']=5.
    elif kind=='source':request['events'][0]['source']['revision']='foreign'
    elif kind=='overpayment':request['events'][0]['amount']=-5.000000000001
    elif kind=='duplicate':request['events'].insert(1,deepcopy(request['events'][0]))
    elif kind=='refund':request['events'][0]['action']='refund_prepaid'
    else:request['events'][0]['reason']='default'
    with pytest.raises(ValueError):run(request)


def test_fee_process_is_native_with_python_financial_callbacks_forbidden():
    script='''
from portfolio_risk.core import quant_native, pricing
def forbidden(*a,**k):raise AssertionError('Python financial callback')
quant_native.term_call=forbidden
pricing.pv_from_A=forbidden
pricing.solve_oas_from_A=forbidden
import json
from pathlib import Path
from portfolio_risk.analytics.model_contracts import account_nonintegral_fees
r=json.loads(Path('examples/models/nonintegral-fee-accounting-1.json').read_text())
out=account_nonintegral_fees(r)
assert out['reporting'][0]['state']['fee_receivable']==5.
'''
    child=subprocess.run([sys.executable,'-c',script],cwd=ROOT,capture_output=True,text=True)
    assert child.returncode==0,child.stderr


def test_fee_independent_replay_cancellation(accepted):
    request,result=accepted
    def stop():raise InterruptedError('cancelled during reference replay')
    with pytest.raises(InterruptedError):verify(request,result,stop)


@pytest.mark.parametrize('sign',[1.,-1.])
def test_partial_fee_cash_crosses_right_into_prepaid_without_netting_other_owners(sign):
    request=raw();request['fees']=request['fees'][:1];request['fees'][0]['entitlement']=sign*5.
    steps=[(5,'earn',3.,'service'),(10,'cash',5.,'settlement'),(20,'earn',2.,'service')]
    request['events']=[event(request,d,i,a,sign*x,r)for i,(d,a,x,r)in enumerate(steps)]
    result=run(request);assert verify(request,result)['verified']
    assert len(result['groups'][1]['postings'])==3
    assert result['reporting'][0]['state']['deferred_fee_income'if sign>0 else 'prepaid_fee_expense']==-sign*2.
    assert result['closing'][0]['state']['unearned_entitlement']==0.


def test_native_fee_strict_duplicate_source_and_clock_scalar_categories():
    request=raw();envelope=dict(schema=request['schema'],threads=1,execution_timeout_ms=1000,request=request)
    for field in ('ordinal','sequence','amount'):
        changed=deepcopy(envelope);changed['request']['events'][0][field]=True
        child=subprocess.run([str(model_native.binary_path())],input=json.dumps(changed).encode(),capture_output=True)
        assert child.returncode==1 and json.loads(child.stdout)['ok']is False
    body=json.dumps(envelope).replace('"amount": -5.0','"amount": -5.0, "amount": -5.0').encode()
    child=subprocess.run([str(model_native.binary_path())],input=body,capture_output=True)
    assert child.returncode==1 and json.loads(child.stdout)['ok']is False


def test_integer_money_is_valid_but_clock_and_boolean_categories_remain_exact():
    request=raw()
    for f in request['fees']:f['entitlement']=int(f['entitlement'])
    for e in request['events']:e['amount']=int(e['amount'])
    result=run(request);assert verify(request,result)['verified']
    changed=deepcopy(result);changed['raw_request']['events'][0]['ordinal']=float(changed['raw_request']['events'][0]['ordinal'])
    with pytest.raises(ValueError):verify(request,changed)
    request['events'][0]['amount']=True
    with pytest.raises(ValueError):verify(request,result)
