"""Actual candidate prior publication, gross prefix and final authority controls."""
from copy import deepcopy
import json
import time
import pytest
from app import model_routes,worker,persistence as db
from app.model_tables import restore_sources,format_sources
from test_persistence import durable
from test_commercial_prior_opening import execute


def completed(zero=False):
    raw=model_routes.example('commercial-candidate-holding-journal-1')['request']
    if zero:
        for key in('completed_candidate','revised_candidate'):raw[key]['allocation'][0]['face']=0.
    job=model_routes.submit(model_routes.ModelRunRequest(expected_revision=0,schema_name=raw['schema'],request=raw))
    row=execute(job.id,'candidate-prior-source');assert row['status']=='done',row['detail']
    return job.id,raw


def selection(source,raw):
    revised=deepcopy(raw['revised_candidate'])
    # A future Physical cash-date revision; original quote, fee basis and all
    # observations through the published cut remain exactly unchanged.
    for unit in revised['commercial']['units']:
        for position in unit['graph']['positions']:
            for history in position['fees']['histories']:
                for path in history['physical']:
                    next(e for e in path['policy']['events']if e['action']=='cash')['ordinal']+=1
    return model_routes.PublishedCandidateHoldingRequest(expected_revision=0,source_job_id=source,
        cut_ordinal=raw['cut_ordinal'],revised_candidate=revised)


def queued(zero=False):
    source,raw=completed(zero);request=selection(source,raw)
    job=model_routes.published_candidate_holding(request)
    return source,raw,job.id


@pytest.mark.parametrize('zero',[False,True])
def test_candidate_prior_replays_gross_prefix_and_reopens(durable,zero):
    source,raw,jid=queued(zero);row=execute(jid,'candidate-prior-current')
    assert row['status']=='done',row['detail']
    saved=db.CODEC.load(row['result']);e=saved['execution'];receipt=e['prior_opening_verification']
    assert receipt['schema']=='commercial-candidate-prior-opening-verification-1'
    assert receipt['modeled_prior_publication_verified']is True
    assert receipt['gross_unweighted_loan_and_fee_prefix_matched']is True
    assert all(receipt[k]is False for k in('observed_bank_reconciliation','cut_started_mixed_sim_adoption','secondary_acquisition_adoption','empirical_validation'))
    assert receipt['provenance']['binding']['id']==source
    native=restore_sources(saved['model_result'],e['table_layouts'])
    assert native['prior_publication_verified']is False
    assert native['raw_source']['completed_candidate']==raw['revised_candidate']
    prior=db.CODEC.load(db.REPO.job(source)['result']);p=restore_sources(prior['model_result'],prior['execution']['table_layouts'])
    from app.commercial_candidate_prior_source import _prefix
    assert _prefix(native)==_prefix(p)
    assert native['window_tables']['opening']==p['window_tables']['opening']
    if zero:
        # The comparison contains real unweighted fee money even though the
        # participation multiplier makes the actual fee journal amounts zero.
        fee=_prefix(native)['source_tables']['commercial_fee_daily']
        assert fee and any(r['native_fee_snapshot']['acquired_fee_asset']>0 for r in fee)
    else:assert native['window_tables']['journal']!=p['window_tables']['journal']
    from app.maintenance import reachable
    assert {r['key']for r in receipt['provenance']['objects']}<=reachable(db.REPO,db.CODEC.objects)
    db.stop();db.start(durable)
    assert db.CODEC.load(db.result_ref(jid))['execution']['prior_opening_verification']==receipt


def test_candidate_prior_old_clock_is_provenance_new_deadline_is_enforced(durable,monkeypatch):
    from app import commercial_candidate_prior_source as source,commercial_prior_source as shared
    from types import SimpleNamespace
    sid,raw=completed();captured=source.capture(db.REPO,db.CODEC,sid,raw['cut_ordinal'],lambda:False)
    old=db.CODEC.load(db.REPO.job(sid)['result'])['execution']['publication_deadline_unix']
    monkeypatch.setattr(shared,'time',SimpleNamespace(monotonic=time.monotonic,time=lambda:old+1.))
    assert source.reopen(db.REPO,db.CODEC,captured,lambda:False).provenance['source_deadline_unix']==old
    def expired():raise TimeoutError('NEW candidate deadline expired')
    with pytest.raises(TimeoutError,match='NEW candidate'):source.reopen(db.REPO,db.CODEC,captured,expired)
    def cancelled():raise InterruptedError('candidate replay cancelled')
    with pytest.raises(InterruptedError,match='cancelled'):source.reopen(db.REPO,db.CODEC,captured,cancelled)


@pytest.mark.parametrize('phase',['cancel','deadline'])
def test_candidate_prior_worker_budget_covers_historical_replay(durable,monkeypatch,phase):
    from app import commercial_candidate_prior_source as source,model_jobs
    from portfolio_risk.core import commercial_candidate_holding_native as runtime
    from types import SimpleNamespace
    sid,raw,jid=queued();verify=source._verify;entered=[]
    deadline=db.CODEC.load(db.REPO.job(jid)['request'])['args'][0]['publication_deadline_unix']
    def replay(raw,native,check):
        entered.append(True)
        if phase=='cancel':worker.STOP.set()
        else:monkeypatch.setattr(model_jobs,'time',SimpleNamespace(time=lambda:deadline+1.,monotonic=time.monotonic))
        return verify(raw,native,check)
    def forbidden(*args,**kwargs):raise AssertionError('native execution began before verified prior replay')
    monkeypatch.setattr(source,'_verify',replay);monkeypatch.setattr(runtime,'run',forbidden)
    try:row=execute(jid,'candidate-prior-replay-budget')
    finally:worker.STOP.clear()
    assert entered and row['status']!='done'and row['result']is None


@pytest.mark.parametrize('target',['wrong_cut','foreign_source','wrong_schema','recursive','completed_override'])
def test_candidate_prior_admission_rejects_invalid_authority(durable,target):
    from fastapi import HTTPException
    sid,raw=completed();request=selection(sid,raw)
    if target=='wrong_cut':request.cut_ordinal+=1
    elif target=='foreign_source':request.source_job_id='not-in-this-workspace'
    elif target=='wrong_schema':
        from test_commercial_prior_opening import completed as old_completed
        request.source_job_id=old_completed()[0]
    elif target=='completed_override':
        with pytest.raises(ValueError):model_routes.PublishedCandidateHoldingRequest(**request.model_dump(),completed_candidate=raw['completed_candidate'])
        return
    else:
        first=model_routes.published_candidate_holding(request);row=execute(first.id,'candidate-prior-first');assert row['status']=='done',row['detail']
        request.source_job_id=first.id
    with pytest.raises(HTTPException):model_routes.published_candidate_holding(request)


@pytest.mark.parametrize('target',['opening','gross_fee','prefix_order','raw_anchor','receipt','summary','input_identity','deadline_identity','clock_type'])
def test_candidate_prior_rehashed_source_requires_independent_replay(durable,target):
    import sqlalchemy as sa
    from app.repository import jobs
    sid,raw,jid=queued();old=db.REPO.job(sid);saved=db.CODEC.load(old['result'])
    if target in('opening','gross_fee','prefix_order','raw_anchor'):
        native=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='opening':native['window_tables']['opening'][0]['balance']+=1.
        elif target=='gross_fee':
            rows=native['revised_candidate']['tables']['commercial_fee_daily'];rows[0]['native_fee_snapshot']['acquired_fee_asset']+=1.
        elif target=='prefix_order':
            rows=native['revised_candidate']['tables']['journal'];rows[0],rows[1]=rows[1],rows[0]
        else:native['raw_source']['revised_candidate']['allocation'][0]['face']+=1.
        saved['model_result'],saved['execution']['table_layouts']=format_sources(native)
    elif target=='receipt':saved['execution']['source_verification']['verified']=1
    elif target=='summary':saved['execution']['summary']['cut_day']+=1
    elif target=='input_identity':saved['execution']['executed_input_sha256']='0'*64
    elif target=='clock_type':saved['execution']['native_timeout_ms']=True
    else:saved['execution']['publication_deadline_unix']+=3600.
    ref=db.CODEC.dump(saved)
    payload=db.CODEC.load(db.REPO.job(jid)['request']);payload['args'][0]['prior_publication']['binding']['result']=ref
    with db.REPO.engine.begin()as conn:
        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==sid).values(result=ref))
        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(request=db.CODEC.dump(payload)))
    row=execute(jid,'candidate-prior-tamper');assert row['status']!='done'and row['result']is None


@pytest.mark.parametrize('target',['source_result','source_attempt','source_partition','new_attempt','cancel','deadline','evidence','queued_input','executable','lease'])
def test_candidate_prior_final_sql_rechecks_source_and_current_authority(durable,monkeypatch,target):
    import sqlalchemy as sa
    from app.repository import jobs
    sid,raw,jid=queued();complete=db.REPO.complete
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
                if target=='cancel':worker.STOP.set()
                elif target=='lease':db.REPO.release(db.WORKER)
                elif target=='executable':
                    from portfolio_risk.core import product_strategy_native
                    monkeypatch.setattr(product_strategy_native,'binary_identity',lambda:'0'*64)
                elif target=='deadline':
                    from app import commercial_analysis_jobs
                    from types import SimpleNamespace
                    deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
                    monkeypatch.setattr(commercial_analysis_jobs,'time',SimpleNamespace(time=lambda:deadline+1.))
                elif target=='source_partition':
                    ref=next(iter(db.CODEC.tables(db.REPO.job(sid)['result']).values()));(db.CODEC.objects.root/ref['key']).write_bytes(b'late-source-bytes')
                else:
                    with db.REPO.engine.begin()as conn:
                        selected=jid if target in('new_attempt','queued_input')else sid;row=db.REPO.job(selected)
                        if target=='queued_input':
                            payload=db.CODEC.load(row['request']);payload['args'][0]['request']['revised_candidate']['allocation'][0]['face']+=1.
                            changes=dict(request=db.CODEC.dump(payload))
                        else:changes=dict(attempt=row['attempt']+1)if target.endswith('attempt')else dict(result=db.CODEC.dump({'replaced':True}))
                        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==selected).values(**changes))
            return complete(job,**kw)
        monkeypatch.setattr(db.REPO,'complete',commit)
    try:row=execute(jid,'candidate-prior-final')
    finally:worker.STOP.clear()
    assert row['status']!='done'and row['result']is None


@pytest.mark.parametrize('target',['duplicate','bool_cut','checkpoint','string_source','completed_source','authority','nonfinite','oversize'])
def test_candidate_prior_http_preserves_original_strict_authority_fields(durable,target):
    from fastapi.testclient import TestClient
    from app import main
    value=dict(expected_revision=0,source_job_id='missing-source',cut_ordinal=739900,revised_candidate={})
    if target=='bool_cut':value['cut_ordinal']=True
    elif target=='checkpoint':value['opening']={'cash':1000.}
    elif target=='string_source':value['source_job_id']=123
    elif target=='completed_source':value['completed_candidate']={}
    elif target=='authority':value['prior_publication_verified']=True
    elif target=='nonfinite':value['revised_candidate']={'money':float('nan')}
    elif target=='oversize':value['revised_candidate']={'padding':'x'*(2<<20)}
    body=json.dumps(value)
    if target=='duplicate':body=body.replace('"cut_ordinal": 739900','"cut_ordinal": 739900,"cut_ordinal": 739901')
    with TestClient(main.app)as client:response=client.post('/models/candidate-holding-journals',content=body,headers={'content-type':'application/json'})
    assert response.status_code==422,response.text


def test_candidate_prior_openapi_declares_only_server_bound_intake():
    from app.main import app
    spec=app.openapi();route=spec['paths']['/models/candidate-holding-journals']['post']
    schema=spec['components']['schemas']['PublishedCandidateHoldingRequest']
    assert route['responses']['200']['content']['application/json']['schema']['$ref'].endswith('/JobStatus')
    assert schema['additionalProperties']is False
    assert set(schema['properties'])=={'expected_revision','source_job_id','cut_ordinal','revised_candidate','timeout'}
    assert schema['properties']['cut_ordinal']['type']=='integer'
    assert set(schema['required'])=={'expected_revision','source_job_id','cut_ordinal','revised_candidate'}


def test_candidate_prior_materialization_preserves_raw_and_existing_input_bound():
    from app import commercial_candidate_prior_source as source
    raw=model_routes.example(source.SCHEMA)['request'];captured=dict(raw=raw)
    before=deepcopy(raw);revised=deepcopy(raw['revised_candidate'])
    result=source.materialize(captured,revised,raw['cut_ordinal'])
    assert raw==before and result['completed_candidate']==raw['revised_candidate']
    result['completed_candidate']['allocation'][0]['face']=99.
    assert raw==before
    revised['padding']='x'*(2<<20)
    with pytest.raises(ValueError,match='bounded'):source.materialize(captured,revised,raw['cut_ordinal'])
