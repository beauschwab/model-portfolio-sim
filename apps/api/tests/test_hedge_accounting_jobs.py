"""Durable formal hedge relationship ownership and independent persisted publication gates."""
import json
from pathlib import Path
import pytest
from app import model_routes,persistence as db,worker
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]


def queued():
    raw=json.loads((ROOT/'examples/models/hedge-accounting-1.json').read_text())
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0))
    db.WORKER='hedge-test-worker';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER)


def test_hedge_job_retains_original_deadline_and_independent_replay(durable):
    jid,job=queued();budget=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['publication_deadline_unix']==budget
    receipt=execution['source_verification']
    assert receipt['formal_hedge_relationships_replayed'] is True and receipt['derivative_source_recomputed'] is True
    assert receipt['accounting_standard_approval'] is False and receipt['saved_book_adoption'] is False
    assert execution['summary']['accounting_standard_approval'] is False and execution['summary']['rule_version']=='asc815-asu2017-12'
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],execution['table_layouts'])
    assert [a['qualified'] for a in result['assessments']]==[True,True,True]
    assert abs(result['closing'][1]['oci_closing'])<1e-9 and result['relationships'][0]['status']=='qualified'


@pytest.mark.parametrize('target',['ratio','posting','tail','flag','milliseconds','receipt','status'])
def test_rehashed_hedge_persistence_tamper_cannot_publish(durable,monkeypatch,target):
    jid,job=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def changed(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='hedge-accounting-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='ratio':result['assessments'][0]['ratio']+=1e-6
        elif target=='posting':result['intents'][-1]['postings'][0]['debit_amount']+=1e-6
        elif target=='tail':result['intents'].pop()
        elif target=='flag':result['accounting_standard_approval']=True
        elif target=='milliseconds':saved['execution']['native_timeout_ms']=True
        elif target=='status':result['relationships'][0]['status']='discontinued'
        else:saved['execution']['source_verification']['formal_hedge_relationships_replayed']=False
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',changed);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None


def test_hedge_queue_cannot_restart_expired_deadline(durable,monkeypatch):
    import time
    from types import SimpleNamespace
    from app import model_jobs
    jid,job=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    monkeypatch.setattr(model_jobs,'time',SimpleNamespace(time=lambda:deadline+1.,monotonic=time.monotonic))
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done'and row['result']is None and 'deadline'in row['detail'].lower()


def test_hedge_contract_is_listed_with_its_synthetic_example(durable):
    assert 'hedge-accounting-1' in model_routes.contracts()['schemas']
    example=model_routes.example('hedge-accounting-1')
    assert example['synthetic'] is True and example['request']['policy']['rule_version']=='asc815-asu2017-12'


def test_hedge_job_survives_worker_loss_with_its_original_deadline(durable):
    jid,job=queued();budget=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    db.REPO.release(db.WORKER);db.WORKER='hedge-replacement-worker';assert db.REPO.acquire(db.WORKER)
    retry=db.REPO.claim(db.WORKER);assert retry['id']==jid and retry['attempt']==2
    assert db.CODEC.load(retry['request'])['args'][0]['publication_deadline_unix']==budget
    worker.execute(retry);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
