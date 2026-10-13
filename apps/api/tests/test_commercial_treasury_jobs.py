"""Commercial treasury original deadline, raw source and SQL publication gates."""
import pytest
from app import model_routes,worker,persistence as db
from app.model_routes import ModelRunRequest
from test_persistence import durable


def queued():
    raw=model_routes.example('commercial-treasury-1')['request']
    submitted=model_routes.submit(ModelRunRequest(expected_revision=0,schema_name='commercial-treasury-1',request=raw))
    db.WORKER='commercial-treasury-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER),raw


def test_commercial_treasury_persists_verified_complete_raw_journal_and_funding(durable):
    jid,job,raw=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix'];worker.execute(job)
    row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));receipt=saved['execution']['source_verification']
    assert receipt['verified'] and receipt['closing_capital_verified'] and receipt['daily_ftp_verified']
    assert receipt['economic_loss_deducted_again'] is False
    assert saved['execution']['publication_deadline_unix']==deadline
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
    assert result['raw_request']==raw and result['ledger_messages'][-1]==dict(complete=True,protocol=6)


@pytest.mark.parametrize('target',['capital','ftp','raw_source','nested_type','deadline'])
def test_commercial_treasury_rehashed_persisted_financial_tamper_refuses(durable,monkeypatch,target):
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def mutate(value):
        ref=dump(value)
        if not isinstance(value,dict) or value.get('execution',{}).get('schema')!='commercial-treasury-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='capital':result['capital']['capital_bridge'][0]['assets']+=1.
        elif target=='ftp':result['ftp']['ftp_positions'][0]['external_profit']+=1.
        elif target=='raw_source':result['raw_request']['raw_commercial']['commercial'][0]['source']['current']['opening_principal']+=1.
        elif target=='nested_type':result['funding_paths'][0]['preparation']['raw_request']['states'][0]['channels'][0]['amount']=True
        else:saved['execution']['publication_deadline_unix']+=3600.
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',mutate);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('target',['bytes','source','cancel','lease','queued_deadline'])
def test_commercial_treasury_sql_lock_rechecks_verified_artifacts_and_original_authority(durable,monkeypatch,target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid,job,_=queued();complete=db.REPO.complete
    def commit(bound,**kw):
        if kw.get('guard'):
            if target=='bytes':
                descriptor=next(iter(db.CODEC.tables(kw['result']).values()));(db.CODEC.objects.root/descriptor['key']).write_bytes(b'changed-after-commercial-verification')
            elif target=='source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            elif target=='cancel':worker.STOP.set()
            elif target=='lease':db.REPO.release(db.WORKER)
            else:
                payload=db.CODEC.load(bound['request']);payload['args'][0]['publication_deadline_unix']+=3600.
                with db.REPO.engine.begin() as connection:connection.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(request=db.CODEC.dump(payload)))
        return complete(bound,**kw)
    monkeypatch.setattr(db.REPO,'complete',commit)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert row['status']!='done' and row['result'] is None
