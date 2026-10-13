"""Commercial analytical raw graph/coefficient sources through durable Model Lab."""
import json
import pytest
import polars as pl
from app import model_routes,worker,persistence as db
from app.model_routes import ModelRunRequest
from app.artifacts import ObjectReference
from test_persistence import durable


def test_commercial_library_bounded_capture_retains_numerical_dtype_and_shape(durable):
    import numpy as np
    from app.commercial_analysis_jobs import _library_bounded
    class Budget:
        def check(self):pass
        def checkpoint(self):return False
    value={'values':np.arange(24,dtype=np.float64).reshape(2,3,4),'ids':['a','b']}
    saved,captured=_library_bounded(db.CODEC.dump(value),Budget())
    assert saved['values'].dtype==value['values'].dtype and np.array_equal(saved['values'],value['values'])
    assert len(captured)==2


@pytest.mark.parametrize('change',['shape','object_dtype','excess_bytes','bool_shape'])
def test_commercial_library_bounded_array_prepass_rejects_injected_metadata(durable,change):
    import numpy as np
    from app.commercial_analysis_jobs import _library_bounded
    class Budget:
        def check(self):pass
        def checkpoint(self):return False
    ref=db.CODEC.dump({'values':np.arange(3,dtype=np.float64)});manifest=json.loads(db.CODEC.objects.get(ref));array=manifest['value']['items'][0][1]
    if change=='shape':array['shape']=[4]
    elif change=='object_dtype':array['dtype']='O'
    elif change=='excess_bytes':array['shape']=[250000,250000]
    else:array['shape']=[True]
    altered=db.CODEC.objects.put(json.dumps(manifest).encode(),'json')
    with pytest.raises(ValueError):_library_bounded(altered,Budget())


def queued(schema):
    raw=model_routes.example(schema)['request']
    submitted=model_routes.submit(ModelRunRequest(expected_revision=0,schema_name=schema,request=raw))
    db.WORKER='commercial-analysis-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER),raw


@pytest.mark.parametrize('schema',['commercial-product-graph-1','commercial-units-1'])
def test_actual_commercial_analysis_replays_uploaded_sources_and_original_budget(durable,schema):
    jid,job,raw=queued(schema);accepted=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix'];worker.execute(job)
    row=db.REPO.job(jid);assert row['status']=='done',row['detail'];result=db.CODEC.load(db.result_ref(jid));execution=result['execution']
    assert execution['publication_deadline_unix']==accepted;assert execution['source_verification']['verified'] is True
    assert execution['source_verification']['empirical_validation'] is False
    from app.model_tables import restore_sources
    restored=restore_sources(result['model_result'],execution['table_layouts'])
    from portfolio_risk.analytics.commercial_graph_reference import _raw
    _raw(raw,restored['raw_source'])
    assert db.CODEC.load(job['request'])['args'][0]['request']==raw
    before=result;db.stop();db.start(durable);assert db.CODEC.load(db.result_ref(jid))['execution']==before['execution']


@pytest.mark.parametrize('schema',['commercial-product-graph-1','commercial-units-1'])
@pytest.mark.parametrize('target',['money','nested_bool','deadline','completion_receipt','cancel','lease'])
def test_actual_commercial_analysis_rehashed_source_and_queued_authority_tamper_reject(durable,monkeypatch,schema,target):
    jid,job,_=queued(schema);dump=db.CODEC.dump;load=db.CODEC.load
    def mutate(value):
        ref=dump(value)
        if not isinstance(value,dict) or value.get('execution',{}).get('schema')!=schema:return ref
        result=load(ref);execution=result['execution']
        if target in ('money','nested_bool'):
            from app.model_tables import restore_sources,format_sources
            restored=restore_sources(result['model_result'],execution['table_layouts'])
            if schema=='commercial-product-graph-1':restored['metrics'][0]['pv']=True if target=='nested_bool' else restored['metrics'][0]['pv']+1.
            else:restored['values'][0][0][0][0]=True if target=='nested_bool' else restored['values'][0][0][0][0]+1.
            result['model_result'],execution['table_layouts']=format_sources(restored)
        elif target=='deadline':execution['publication_deadline_unix']+=3600.
        elif target=='completion_receipt':execution['source_verification']['verified']=1
        elif target=='cancel':db.REPO.cancel(jid)
        else:db.REPO.release(db.WORKER)
        return dump(result)
    monkeypatch.setattr(db.CODEC,'dump',mutate);worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('schema',['commercial-product-graph-1','commercial-units-1'])
@pytest.mark.parametrize('target',['late_source','late_partition','late_stop','queued_request','attempt'])
def test_commercial_analysis_sql_lock_rechecks_all_verified_authority(durable,monkeypatch,schema,target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid,job,_=queued(schema);complete=db.REPO.complete
    def commit(bound,**kw):
        if kw.get('guard'):
            if target=='late_stop':worker.STOP.set()
            elif target=='late_source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            elif target=='late_partition':
                descriptor=next(iter(db.CODEC.tables(kw['result']).values()))
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


@pytest.mark.parametrize('schema',['commercial-holding-continuation-1','commercial-holding-journal-1','nonintegral-fee-accounting-1'])
@pytest.mark.parametrize('target',['late_source','late_partition','late_stop','queued_request','attempt','producer_deadline'])
def test_commercial_holding_sql_transaction_keeps_original_authority(durable,monkeypatch,schema,target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid,job,_=queued(schema);complete=db.REPO.complete
    if target=='producer_deadline':
        from app import model_jobs
        run=model_jobs.run
        def changed(*args):
            result=run(*args);result['execution']['publication_deadline_unix']+=3600.;return result
        monkeypatch.setattr(model_jobs,'run',changed)
    else:
        def commit(bound,**kw):
            if kw.get('result')is not None:
                assert kw.get('guard')is not None,'holding job skipped its SQL guard'
                if target=='late_stop':worker.STOP.set()
                elif target=='late_source':
                    from portfolio_risk.analytics import balance_stream
                    monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
                elif target=='late_partition':
                    descriptor=next(iter(db.CODEC.tables(kw['result']).values()))
                    (db.CODEC.objects.root/descriptor['key']).write_bytes(b'late-holding-mutation')
                else:
                    with db.REPO.engine.begin()as connection:
                        if target=='attempt':changes=dict(attempt=bound['attempt']+1)
                        else:
                            payload=db.CODEC.load(bound['request']);payload['args'][0]['publication_deadline_unix']+=3600.;changes=dict(request=db.CODEC.dump(payload))
                        connection.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(**changes))
            return complete(bound,**kw)
        monkeypatch.setattr(db.REPO,'complete',commit)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None


def test_commercial_http_original_duplicate_keys_reject_before_enqueue(durable):
    from fastapi.testclient import TestClient
    from app import main
    raw=model_routes.example('commercial-product-graph-1')['request']
    body=json.dumps(dict(expected_revision=0,schema_name='commercial-product-graph-1',request=raw),separators=(',',':'))
    body=body.replace('"bump_bp":1.0','"bump_bp":1.0,"bump_bp":2.0',1)
    with TestClient(main.app) as client:response=client.post('/models/runs',content=body,headers={'content-type':'application/json'})
    assert response.status_code==422;assert 'duplicate' in response.text
