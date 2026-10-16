"""Durable joint loan/fee replay and final source/attempt publication guards."""
import json
from pathlib import Path
import pytest
from app import model_routes,persistence as db,worker
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]

def queued(raw=None):
    if raw is None:raw=json.loads((ROOT/'examples/models/commercial-fee-journal-1.json').read_text(encoding='utf-8'))
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0))
    db.WORKER='commercial-fee-test-worker';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER)


def earned_refund_raw():
    raw=json.loads((ROOT/'examples/models/commercial-fee-journal-1.json').read_text(encoding='utf-8'))
    for source,policy in zip(raw['loan']['commercial'],raw['fees'],strict=True):
        source['maturity_basis_policy']=dict(review=dict(id='reviewed-maturity',revision='1'),disposition='dispose_financial_basis_after_exact_legal_close-1')
        origin=source['source']['current']['asof_ordinal']
        policy['fees'][0]['earned_refunds_allowed']=True
        policy['events'][1]['ordinal']=origin+20
        policy['maturity']['refund_earned_fraction']=.4
    return raw


def test_earned_refund_job_persists_raw_policy_gross_rights_and_reconciled_income(durable):
    raw=earned_refund_raw();jid,job=queued(raw);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid))
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
    assert result['raw_source']==raw
    assert result['completion']==dict(complete=True,protocol=7)
    for paths in result['fee_sources']:
        state=paths[0]['source']['closing'][0]['state']
        assert state['earned']==5. and state['refunded_earned']==2. and state['net_service_cash']==3.
    assert saved['execution']['source_verification']['single_sim_ordered_journal_replayed']is True


@pytest.mark.parametrize('target',['right','review','posting'])
def test_rehashed_earned_refund_cannot_pass_persisted_publication(durable,monkeypatch,target):
    jid,job=queued(earned_refund_raw());dump=db.CODEC.dump;load=db.CODEC.load
    def changed(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='commercial-fee-journal-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='right':result['fee_sources'][0][0]['source']['closing'][0]['state']['refunded_earned']+=1.
        elif target=='review':result['raw_source']['fees'][0]['maturity']['refund_earned_fraction']=.9
        else:
            row=next(row for row in result['tables']['journal']if row['event']=='commercial_nonintegral_fee'and row['gl_account']=='cash'and row['value']<0.)
            row['value']-=1.
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',changed);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None

def test_fee_job_publishes_only_independently_replayed_source_and_shared_journal(durable):
    jid,job=queued();budget=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['publication_deadline_unix']==budget
    assert execution['source_verification']['single_sim_ordered_journal_replayed']is True
    assert execution['source_verification']['gross_fee_rights_reconciled']is True
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],execution['table_layouts'])
    assert result['completion']==dict(complete=True,protocol=7)
    assert result['fee_sources'][0][0]['source']['reporting'][0]['state']['fee_receivable']==5.
    assert result['fee_sources'][0][0]['source']['closing'][0]['state']['fee_receivable']==0.

@pytest.mark.parametrize('target',['right','posting','tail','review','flag','milliseconds','receipt','bound'])
def test_rehashed_joint_fee_persistence_tamper_never_publishes(durable,monkeypatch,target):
    jid,job=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def changed(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='commercial-fee-journal-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='right':result['tables']['commercial_fee_daily'][0]['native_fee_snapshot']['fee_receivable']+=1.
        elif target=='posting':result['tables']['journal'][-1]['value']+=1.
        elif target=='tail':result['tables']['commercial_fee_lineage'].pop()
        elif target=='review':result['fee_sources'][0][0]['source']['raw_request']['policy']['revision']='foreign'
        elif target=='flag':result['loan_carrying_value_changed']=True
        elif target=='milliseconds':saved['execution']['native_timeout_ms']=True
        elif target=='bound':result['admitted_work_upper']+=1
        else:saved['execution']['source_verification']['single_sim_ordered_journal_replayed']=False
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',changed);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None

def test_fee_job_preserves_original_queue_deadline(durable,monkeypatch):
    import time
    from types import SimpleNamespace
    from app import model_jobs
    jid,job=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    monkeypatch.setattr(model_jobs,'time',SimpleNamespace(time=lambda:deadline+1.,monotonic=time.monotonic))
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done'and row['result']is None and 'deadline'in row['detail'].lower()
