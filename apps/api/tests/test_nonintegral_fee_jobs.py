"""Durable separate fee ownership and independent persisted publication gates."""
import json
from pathlib import Path
import pytest
from app import model_routes,persistence as db,worker
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]


def queued(integer_money=False):
    raw=json.loads((ROOT/'examples/models/nonintegral-fee-accounting-1.json').read_text())
    if integer_money:
        for f in raw['fees']:f['entitlement']=int(f['entitlement'])
        for e in raw['events']:e['amount']=int(e['amount'])
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0))
    db.WORKER='fee-test-worker';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER)


@pytest.mark.parametrize('integer_money',[False,True])
def test_fee_job_retains_original_deadline_and_independent_source_tail(durable,integer_money):
    jid,job=queued(integer_money);budget=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['publication_deadline_unix']==budget
    assert execution['source_verification']['ordered_fee_intents_replayed'] is True
    assert execution['source_verification']['native_credit_event_binding'] is False
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],execution['table_layouts'])
    assert result['reporting'][0]['state']['fee_receivable']==5.
    assert result['closing'][0]['state']['fee_receivable']==0.


@pytest.mark.parametrize('target',['right','posting','tail','evidence','review','milliseconds','receipt'])
def test_rehashed_fee_persistence_tamper_cannot_publish(durable,monkeypatch,target):
    jid,job=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def changed(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='nonintegral-fee-accounting-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='right':result['reporting'][0]['state']['fee_receivable']+=1.
        elif target=='posting':result['groups'][0]['postings'][0]['amount']+=1.
        elif target=='tail':result['groups'].pop()
        elif target=='evidence':result['native_credit_event_binding']=True
        elif target=='review':result['groups'][0]['event']['review']['revision']='foreign'
        elif target=='milliseconds':saved['execution']['native_timeout_ms']=True
        else:saved['execution']['source_verification']['ordered_fee_intents_replayed']=False
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',changed);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None


def test_fee_queue_cannot_restart_expired_deadline(durable,monkeypatch):
    import time
    from types import SimpleNamespace
    from app import model_jobs
    jid,job=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    monkeypatch.setattr(model_jobs,'time',SimpleNamespace(time=lambda:deadline+1.,monotonic=time.monotonic))
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done'and row['result']is None and 'deadline'in row['detail'].lower()
