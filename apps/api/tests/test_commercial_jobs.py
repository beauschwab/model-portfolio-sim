"""Commercial complete raw source and persisted journal durable publication."""
import io
import json
import pytest
import polars as pl
from app import main,worker,persistence as db,reporting,maintenance
from app.schemas import ModeledBalanceStressRequest
from app.artifacts import ObjectReference
from test_persistence import durable

SCHEMA='commercial-credit-ledger-1'


@pytest.mark.parametrize('target',['duplicate','raw_bytes'])
def test_commercial_ledger_http_original_body_rejects_before_enqueue(durable,target):
    from fastapi.testclient import TestClient
    raw=main.modeled_balance_stress_example(SCHEMA)['specification']
    body=json.dumps(dict(expected_revision=0,specification=raw),separators=(',',':'))
    if target=='duplicate':body=body.replace('"schema":"commercial-credit-ledger-1"','"schema":"commercial-credit-ledger-1","schema":"commercial-credit-ledger-1"',1)
    else:body+=' '*(1024*1024)
    with TestClient(main.app) as client:response=client.post('/balance-stress/modeled',content=body,headers={'content-type':'application/json'})
    assert response.status_code==422
    assert ('duplicate' if target=='duplicate' else 'raw bytes') in response.text

def queued():
    raw=main.modeled_balance_stress_example(SCHEMA)['specification']
    submitted=main.modeled_balance_stress(ModeledBalanceStressRequest(expected_revision=0,specification=raw))
    db.WORKER='commercial-publication-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted['id'],db.REPO.claim(db.WORKER),raw


def test_actual_commercial_worker_publishes_and_reopens_verified_full_tail(durable,monkeypatch):
    from portfolio_risk.analytics import model_ledger,balance_stress
    def forbidden(*a,**k):raise AssertionError('Python production finance callback')
    monkeypatch.setattr(model_ledger,'validate_spec',forbidden);monkeypatch.setattr(model_ledger,'_finalize',forbidden);monkeypatch.setattr(balance_stress,'_simulate',forbidden)
    jid,job,raw=queued();accepted=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix'];worker.execute(job)
    row=db.REPO.job(jid);assert row['status']=='done',row['detail'];result=db.CODEC.load(db.result_ref(jid));execution=result['execution']
    assert execution['publication_deadline_unix']==accepted
    for name in ('commercial_source_recomputed','commercial_credit_clock_recomputed','gross_coupon_origins_reconciled','ordered_same_sim_journal_replayed','persisted_partitions_replayed'):assert execution['validation'][name] is True
    for name in ('accounting_standard_approval','empirical_validation','capital_ftp_eligible'):assert execution['validation'][name] is False
    assert json.loads(db.CODEC.objects.get(execution['input']))['request']==raw
    assert json.loads(db.CODEC.objects.get(execution['producer_completion']))==dict(complete=True,protocol=6)
    for name in ('journal','commercial_daily','commercial_closing','commercial_source_lineage'):assert reporting.table(jid,'/'+name,offset=0,limit=2)['rows']
    assert execution['modeled_source']['key'] in maintenance.reachable(db.REPO,db.CODEC.objects)
    before=db.CODEC.load(db.result_ref(jid));db.stop();db.start(durable);assert db.CODEC.load(db.result_ref(jid))==before


@pytest.mark.parametrize('target',['nested_money','clock','journal','scalar_bool','completion','extended_deadline','cancel','lease','dropped_table'])
def test_commercial_worker_refuses_rehashed_persisted_financial_or_authority_tamper(durable,monkeypatch,target):
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def tamper(value):
        ref=dump(value)
        if not isinstance(value,dict) or value.get('execution',{}).get('model_version')!=SCHEMA:return ref
        changed=load(ref);execution=changed['execution']
        if target in ('nested_money','clock','scalar_bool'):
            header=json.loads(db.CODEC.objects.get(execution['modeled_source']));source=header['source_receipt']['sources'][0]
            if target=='clock':source['carry']['carry_points'][1]['year']+=1e-12
            else:source['accounting']['groups'][0]['next']['principal']=True if target=='scalar_bool' else 101.
            execution['modeled_source']=ObjectReference(db.CODEC.objects.put(json.dumps(header).encode(),'bin'))
        elif target=='completion':execution['producer_completion']=ObjectReference(db.CODEC.objects.put(b'{"complete":true,"protocol":6.0}','bin'))
        elif target=='journal':
            part=changed['journal']['parts'][0];frame=pl.read_parquet(io.BytesIO(db.CODEC.objects.get(part['ref'])));rows=frame.to_dicts()
            for row in rows[:2]:row['debit']+=1. if row['debit'] else 0.;row['credit']+=1. if row['credit'] else 0.
            stream=io.BytesIO();pl.DataFrame(rows,schema=frame.schema).write_parquet(stream);part['ref']=db.CODEC.objects.put(stream.getvalue(),'parquet')
        elif target=='extended_deadline':execution['publication_deadline_unix']+=3600.
        elif target=='cancel':db.REPO.cancel(jid)
        elif target=='lease':db.REPO.release(db.WORKER)
        else:changed.pop('commercial_daily')
        return dump(changed)
    monkeypatch.setattr(db.CODEC,'dump',tamper);worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('target',['late_source','late_partition','late_stop','queued_request','attempt'])
def test_commercial_ledger_sql_lock_rechecks_verified_objects_and_attempt(durable,monkeypatch,target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid,job,_=queued();complete=db.REPO.complete
    def commit(bound,**kw):
        if kw.get('guard'):
            if target=='late_stop':worker.STOP.set()
            elif target=='late_source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            elif target=='late_partition':
                result=db.CODEC.load(kw['result']);descriptor=result['journal']['parts'][0]['ref']
                (db.CODEC.objects.root/descriptor['key']).write_bytes(b'changed-after-independent-replay')
            else:
                with db.REPO.engine.begin() as connection:
                    if target=='attempt':changes=dict(attempt=bound['attempt']+1)
                    else:
                        payload=db.CODEC.load(bound['request']);payload['args'][0]['publication_deadline_unix']+=3600.;changes=dict(request=db.CODEC.dump(payload))
                    connection.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(**changes))
        return complete(bound,**kw)
    monkeypatch.setattr(db.REPO,'complete',commit)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert row['status']!='done' and row['result'] is None
