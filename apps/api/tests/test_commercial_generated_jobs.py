"""Generated exercise raw training/evaluation and immutable publication gates."""
import pytest
from app import model_routes,worker,persistence as db
from app.model_routes import ModelRunRequest
from test_persistence import durable


def queued():
    raw=model_routes.example('commercial-generated-exercise-1')['request']
    submitted=model_routes.submit(ModelRunRequest(expected_revision=0,schema_name='commercial-generated-exercise-1',request=raw))
    db.WORKER='commercial-generated-exercise-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER),raw


def test_commercial_generated_exercise_persists_verified_training_and_stopped_cash(durable):
    jid,job,raw=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix'];worker.execute(job)
    row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));receipt=saved['execution']['source_verification']
    assert receipt['verified'] and receipt['stopped_cash_replayed'] and receipt['training_evaluation_separated']
    assert receipt['empirical_validation'] is False and receipt['physical_journal_eligible'] is False
    assert saved['execution']['publication_deadline_unix']==deadline
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
    assert result['raw_source']['schema']==raw['schema']
    assert all(p['full_native_cash_replayed'] for p in result['evaluation_paths'])



@pytest.mark.parametrize('target',['coefficients','outcome','raw_source','nested_type','deadline'])
def test_commercial_generated_exercise_rehashed_persisted_financial_tamper_refuses(durable,monkeypatch,target):
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def mutate(value):
        ref=dump(value)
        if not isinstance(value,dict) or value.get('execution',{}).get('schema')!='commercial-generated-exercise-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='coefficients':result['regressions'][0]['coefficients'][0]+=1.
        elif target=='outcome':result['evaluation_paths'][0]['value']+=1.
        elif target=='raw_source':result['raw_source']['fixed_base_oas']+=1.
        elif target=='nested_type':result['evaluation_paths'][0]['full_native_cash_replayed']=1
        else:saved['execution']['publication_deadline_unix']+=3600.
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',mutate);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('target',['bytes','source','cancel','lease','queued_deadline'])
def test_commercial_generated_exercise_sql_lock_rechecks_verified_artifacts_and_original_authority(durable,monkeypatch,target):
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


@pytest.mark.parametrize('body',[b'{"schema_name":"commercial-generated-exercise-1","schema_name":"consumer-card-1","request":{}}',b'{"schema_name":"commercial-generated-exercise-1","request":{"x":1,"x":2}}'])
def test_generated_original_http_decoder_cannot_overwrite_selected_schema_or_source(body):
    import asyncio
    from starlette.requests import Request
    from fastapi import HTTPException
    async def receive():return dict(type='http.request',body=body,more_body=False)
    with pytest.raises(HTTPException,match='duplicate'):
        asyncio.run(model_routes._selected_cd_body(Request(dict(type='http',method='POST',path='/models/runs',headers=[]),receive)))


def test_generated_transport_enforces_input_limit_before_executable_identity(monkeypatch):
    from portfolio_risk.core import model_native
    monkeypatch.setattr(model_native,'binary_identity',lambda:(_ for _ in()).throw(AssertionError('oversized input reached executable identity')))
    with pytest.raises(ValueError,match='bounded native input'):model_native.run('commercial-generated-exercise-1',{'padding':'x'*(1024*1024)})
