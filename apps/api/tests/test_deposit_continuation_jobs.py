"""Actual default worker publication gates for explicitly revised deposit income."""
import json
import pytest
from app import main,worker,persistence as db,reporting,maintenance
from app.schemas import ModeledBalanceStressRequest
from app.artifacts import ObjectReference
from test_persistence import durable

SCHEMA='deposit-performing-income-ledger-1'


def queued():
    raw=main.modeled_balance_stress_example(SCHEMA)['specification']
    submitted=main.modeled_balance_stress(ModeledBalanceStressRequest(expected_revision=0,specification=raw))
    db.WORKER='revised-deposit-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted['id'],db.REPO.claim(db.WORKER),raw


def test_actual_default_continuation_job_retains_source_completion_and_original_yield(durable,monkeypatch):
    from portfolio_risk.analytics import model_ledger
    def forbidden(*args,**kwargs):raise AssertionError('unowned Python financial callback')
    monkeypatch.setattr(model_ledger,'validate_spec',forbidden)
    monkeypatch.setattr(model_ledger,'_finalize',forbidden)
    assert model_ledger.binary_path().parent.name=='release'
    jid,job,raw=queued();worker.execute(job);row=db.REPO.job(jid)
    assert row['status']=='done',row['detail']
    result=db.CODEC.load(db.result_ref(jid));e=result['execution'];r=e['validation']
    for key in ('physical_deposit_source_principal_recomputed','original_yield_continuation_recomputed','gross_origin_rights_reconciled','ordered_same_sim_journal_replayed'):assert r[key] is True
    for key in ('accounting_standard_approval','capital_ftp_eligible','installed_adoption'):assert r[key] is False
    source=json.loads(db.CODEC.objects.get(e['modeled_source']))
    assert model_ledger.exact_json_identity(source['source_receipt']['raw_request'],raw)
    assert source['source_receipt']['paths'][0]['native']['raw_request']['continuation']['anchor']==raw['runs'][0]['source']['continuation']['anchor']
    assert json.loads(db.CODEC.objects.get(e['producer_completion']))=={'complete':True,'protocol':3}
    for name in ('journal','deposit_revision','trial_balance','dated_rights'):
        assert reporting.table(jid,'/'+name,offset=0,limit=2)['rows']
    reachable=maintenance.reachable(db.REPO,db.CODEC.objects)
    assert e['producer_completion']['key'] in reachable and e['modeled_source']['key'] in reachable


@pytest.mark.parametrize('target',['input_sign_zero','input_bool','original_yield','completion_bool','protocol_float',
    'receipt_false','receipt_true','drop_table','extended_deadline','cancel','lease','source'])
def test_continuation_uploaded_objects_cannot_change_before_commit(durable,monkeypatch,target):
    from portfolio_risk.analytics import balance_stream
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def replace(value):
        ref=dump(value)
        if not isinstance(value,dict) or value.get('execution',{}).get('model_version')!=SCHEMA:return ref
        changed=load(ref);e=changed['execution']
        if target in {'input_sign_zero','input_bool','original_yield','completion_bool','protocol_float'}:
            key='input' if target.startswith('input_') else 'modeled_source' if target=='original_yield' else 'producer_completion'
            data=json.loads(db.CODEC.objects.get(e[key]))
            if target=='input_sign_zero':data['request']['specification']['positions'][0]['rate']=-0.
            elif target=='input_bool':data['request']['runs'][0]['source']['continuation']['anchor']['original_principal']=True
            elif target=='original_yield':data['source_receipt']['paths'][0]['native']['raw_request']['continuation']['anchor']['original_annual_effective_rate']+=1e-12
            elif target=='completion_bool':data['complete']=1
            else:data['protocol']=3.
            e[key]=ObjectReference(db.CODEC.objects.put(json.dumps(data,allow_nan=False).encode(),'bin'))
        elif target=='receipt_false':e['validation']['accounting_standard_approval']=0
        elif target=='receipt_true':e['validation']['original_yield_continuation_recomputed']=1
        elif target=='drop_table':changed.pop('deposit_revision')
        elif target=='extended_deadline':e['publication_deadline_unix']+=3600.
        elif target=='cancel':db.REPO.cancel(jid)
        elif target=='lease':db.REPO.release(db.WORKER)
        else:monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
        return dump(changed)
    monkeypatch.setattr(db.CODEC,'dump',replace)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None,row['detail']
    assert 'KeyError' not in str(row['detail'])
