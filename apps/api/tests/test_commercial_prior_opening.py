"""Authorized modeled prior openings, independent replay and final SQL fences."""
from copy import deepcopy
import time
import pytest
from app import model_routes,worker,persistence as db
from test_persistence import durable


def execute(jid,owner):
    db.WORKER=owner;worker.STOP.clear();assert db.REPO.acquire(owner,300.)
    job=db.REPO.claim(owner);assert job['id']==jid
    worker.execute(job);row=db.REPO.job(jid);db.REPO.release(owner)
    return row


def completed():
    raw=model_routes.example('commercial-holding-journal-1')['request']
    job=model_routes.submit(model_routes.ModelRunRequest(expected_revision=0,schema_name=raw['schema'],request=raw))
    row=execute(job.id,'prior-opening-source');assert row['status']=='done',row['detail']
    return job.id,raw


def selection(source,raw):
    revised=deepcopy(raw['revised_sources'])
    revised[1]['credit']['intervals'][1]['hazards_per_year'][0][3]=20.
    return model_routes.PublishedHoldingRequest(expected_revision=0,source_job_id=source,cut_ordinal=raw['cut_ordinal'],revised_sources=revised)


def queued():
    source,raw=completed();request=selection(source,raw)
    job=model_routes.published_holding(request)
    return source,raw,job.id


def test_commercial_prior_opening_source_and_future_journal_replayed(durable):
    source,raw,jid=queued();row=execute(jid,'prior-opening-current')
    assert row['status']=='done',row['detail']
    saved=db.CODEC.load(row['result']);e=saved['execution'];receipt=e['prior_opening_verification']
    assert receipt['modeled_prior_publication_verified']is True
    assert receipt['observed_bank_reconciliation']is False and receipt['cut_started_mixed_sim_adoption']is False
    assert receipt['provenance']['binding']['id']==source
    from app.model_tables import restore_sources
    native=restore_sources(saved['model_result'],e['table_layouts'])
    assert native['prior_publication_verified']is False # native producer cannot grant external authority
    assert native['raw_source']['origin_ledger']['commercial']==raw['revised_sources']
    prior=db.CODEC.load(db.REPO.job(source)['result']);p=restore_sources(prior['model_result'],prior['execution']['table_layouts'])
    assert native['window_tables']['opening']==p['window_tables']['opening']
    assert native['full_tables']['journal']!=p['full_tables']['journal']
    # The original completed job keeps its immutable objects live through GC.
    from app.maintenance import reachable
    refs=receipt['provenance']['objects'];live=reachable(db.REPO,db.CODEC.objects)
    assert {r['key']for r in refs}<=live
    db.stop();db.start(durable)
    assert db.CODEC.load(db.result_ref(jid))['execution']['prior_opening_verification']==receipt


def test_commercial_prior_expired_source_deadline_is_retained_not_reset(durable,monkeypatch):
    source,raw=completed();request=selection(source,raw)
    from app import commercial_prior_source as prior
    from types import SimpleNamespace
    captured=prior.capture(db.REPO,db.CODEC,source,raw['cut_ordinal'],lambda:False)
    saved=db.CODEC.load(db.REPO.job(source)['result']);deadline=saved['execution']['publication_deadline_unix']
    # Advancing only the source helper clock does not renew the old deadline;
    # current work is constrained by the new caller's check.
    monkeypatch.setattr(prior,'time',SimpleNamespace(monotonic=time.monotonic,time=lambda:deadline+1.))
    assert prior.reopen(db.REPO,db.CODEC,captured,lambda:False).provenance['source_deadline_unix']==deadline
    def expired():raise TimeoutError('NEW job deadline expired')
    with pytest.raises(TimeoutError,match='NEW job'):prior.reopen(db.REPO,db.CODEC,captured,expired)


@pytest.mark.parametrize('target',['wrong_cut','foreign_source','unchecked_flag','recursive'])
def test_commercial_prior_selection_rejects_invalid_authority(durable,target):
    from fastapi import HTTPException
    source,raw=completed();request=selection(source,raw)
    if target=='wrong_cut':request.cut_ordinal+=1
    elif target=='foreign_source':request.source_job_id='not-in-this-workspace'
    elif target=='unchecked_flag':
        with pytest.raises(ValueError):model_routes.PublishedHoldingRequest(**request.model_dump(),prior_publication_verified=True)
        return
    else:
        first=model_routes.published_holding(request);row=execute(first.id,'prior-opening-first');assert row['status']=='done',row['detail']
        request.source_job_id=first.id
    with pytest.raises(HTTPException):model_routes.published_holding(request)


@pytest.mark.parametrize('target',['money','receipt','summary','input_identity','deadline_identity'])
def test_commercial_prior_rehashed_saved_source_needs_independent_replay(durable,monkeypatch,target):
    import sqlalchemy as sa
    from app.repository import jobs
    source,raw,jid=queued();old=db.REPO.job(source);saved=db.CODEC.load(old['result'])
    if target=='money':
        from app.model_tables import restore_sources,format_sources
        native=restore_sources(saved['model_result'],saved['execution']['table_layouts']);native['window_tables']['opening'][0]['balance']+=1.
        saved['model_result'],saved['execution']['table_layouts']=format_sources(native)
    elif target=='receipt':saved['execution']['source_verification']['verified']=1
    elif target=='summary':saved['execution']['summary']['cut_day']+=1
    elif target=='input_identity':saved['execution']['executed_input_sha256']='0'*64
    else:saved['execution']['publication_deadline_unix']+=3600.
    ref=db.CODEC.dump(saved)
    # Match the new source descriptor in the captured request too, to test
    # source recomputation rather than only its first immutable binding fence.
    payload=db.CODEC.load(db.REPO.job(jid)['request']);payload['args'][0]['prior_publication']['binding']['result']=ref
    request_ref=db.CODEC.dump(payload)
    with db.REPO.engine.begin()as conn:
        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==source).values(result=ref))
        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(request=request_ref))
    row=execute(jid,'prior-opening-tamper')
    assert row['status']!='done'and row['result']is None


@pytest.mark.parametrize('target',['source_result','source_attempt','source_partition','new_attempt','stop','evidence'])
def test_commercial_prior_sql_transaction_rechecks_source_and_new_owner(durable,monkeypatch,target):
    import sqlalchemy as sa
    from app.repository import jobs
    source,raw,jid=queued();complete=db.REPO.complete
    if target=='evidence':
        dump=db.CODEC.dump
        def changed(value):
            if isinstance(value,dict)and value.get('execution',{}).get('prior_opening_verification'):
                value=deepcopy(value);value['execution']['prior_opening_verification']['observed_bank_reconciliation']=True
            return dump(value)
        monkeypatch.setattr(db.CODEC,'dump',changed)
    else:
        def commit(job,**kw):
            if job['id']==jid and kw.get('result')is not None:
                assert kw.get('guard')is not None
                if target=='stop':worker.STOP.set()
                elif target=='source_partition':
                    ref=next(iter(db.CODEC.tables(db.REPO.job(source)['result']).values()));(db.CODEC.objects.root/ref['key']).write_bytes(b'late-source-bytes')
                else:
                    with db.REPO.engine.begin()as conn:
                        sid=jid if target=='new_attempt'else source;row=db.REPO.job(sid)
                        changes=dict(attempt=row['attempt']+1)if target.endswith('attempt')else dict(result=db.CODEC.dump({'replaced':True}))
                        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==sid).values(**changes))
            return complete(job,**kw)
        monkeypatch.setattr(db.REPO,'complete',commit)
    try:row=execute(jid,'prior-opening-final')
    finally:worker.STOP.clear()
    assert row['status']!='done'and row['result']is None


@pytest.mark.parametrize('target',['duplicate','bool_cut','checkpoint','string_source'])
def test_commercial_prior_http_original_authority_fields_are_strict(durable,target):
    import json
    from fastapi.testclient import TestClient
    from app import main
    # All reject before source lookup or enqueue, so no completed-source fixture
    # can disguise an intake coercion or a caller-provided financial checkpoint.
    value=dict(expected_revision=0,source_job_id='missing-source',cut_ordinal=739900,revised_sources=[{}])
    if target=='bool_cut':value['cut_ordinal']=True
    elif target=='checkpoint':value['opening']={'cash':1000.}
    elif target=='string_source':value['source_job_id']=123
    body=json.dumps(value)
    if target=='duplicate':body=body.replace('"cut_ordinal": 739900','"cut_ordinal": 739900,"cut_ordinal": 739901')
    with TestClient(main.app)as client:response=client.post('/models/holding-journals',content=body,headers={'content-type':'application/json'})
    assert response.status_code==422,response.text
