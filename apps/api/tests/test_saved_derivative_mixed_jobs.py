"""Actual copied worker + pinned full dispatcher + real SQLite/Codec/Parquet.
Current deployment manifest is the actual startup/current code identity; native finance,
independent financial/source/journal replay and durable worker are never mocked.
"""
from copy import deepcopy
from pathlib import Path
import hashlib,threading,time
import pytest
import sqlalchemy as sa
from app import worker,store,persistence as db,saved_derivative_jobs as candidate
from app.artifacts import Codec,Objects
from app.repository import Repository,jobs,workspaces
from app.storage_config import StorageConfig
from portfolio_risk.analytics import saved_derivative_runtime as runtime
from portfolio_risk.analytics.saved_derivative_identity import loads,encoded
from app.saved_derivative_transport import verify_persisted,metadata,StoredFrames
ROOT=Path(__file__).resolve().parents[3]
FIXTURES=ROOT/'packages/portfolio-risk/tests/fixtures/saved_derivative_mixed'
def raw():return loads((FIXTURES/'base.json').read_bytes())

@pytest.fixture
def durable(tmp_path,monkeypatch):
    config=StorageConfig('sqlite:///'+str(tmp_path/'worker.db'),str(tmp_path/'objects'),'isolated','derivative')
    repo=Repository(config.database_url,config.tenant,config.workspace);repo.migrate()
    objects=Objects(config.artifact_url,config.tenant,config.workspace);codec=Codec(objects)
    identity=db.current_identity()
    monkeypatch.setattr(db,'_IDENTITY',deepcopy(identity))
    monkeypatch.setattr(db,'REPO',repo);monkeypatch.setattr(db,'CODEC',codec);monkeypatch.setattr(db,'CONFIG',config)
    worker.STOP.clear();store.SETTINGS=store.SETTINGS.model_copy(update={'n_threads':1,'compute_backend':'rust'})
    state=store.snapshot_local();state['revision']=0;store.STATE_META['revision']=0;repo.initialize(codec.dump(state))
    # Deny all production financial callbacks/default DLL loads. New refs are
    # explicit independent modules and the actual subprocess is the frozen owner.
    from portfolio_risk.core import native as product_native
    from portfolio_risk.core import quant_native as risk_native
    from portfolio_risk.analytics import ledger_native,derivative_verification
    monkeypatch.setattr(derivative_verification,'_derive',lambda *a,**kw: (_ for _ in ()).throw(AssertionError('old financial callback forbidden')))
    def forbidden(*a,**kw):raise AssertionError('Python financial/native DLL callback forbidden')
    for mod,attribute in [(risk_native,'_load'),(risk_native,'call'),(risk_native,'term_call'),(product_native,'discount_backend'),(product_native,'_numba_price'),(ledger_native,'validate_spec'),(ledger_native,'_spec_library')]:
        if hasattr(mod,attribute):monkeypatch.setattr(mod,attribute,forbidden)
    def start(r=None,timeout=60.,execute=True):
        q=candidate.queue_input(raw()if r is None else r,timeout=timeout)
        payload=dict(operation=candidate.OPERATION,args=[q],state=store.snapshot_local(),identity=deepcopy(identity))
        ref=codec.dump(payload);jid=repo.enqueue('saved_derivative_mixed',0,ref,{})
        assert repo.acquire('actual-copied-worker',300.)
        job=repo.claim('actual-copied-worker');assert job['id']==jid
        if execute:worker.execute(job)
        return q,job
    yield dict(repo=repo,objects=objects,codec=codec,start=start,identity=identity)
    worker.STOP.clear();repo.engine.dispose()

def test_actual_copied_worker_publishes_reopens_and_pages(durable):
    c=durable;q,job=c['start']();row=c['repo'].job(job['id']);assert row['status']=='done',row['detail']
    result=c['codec'].metadata(row['result'],{'execution','revision'});assert result['execution']['validation']['single_daily_gl_verified']is True
    facade,receipt=verify_persisted(c['codec'],row['result'],q['specification'])
    assert receipt['shared_journal_lines']==150
    tables=c['codec'].tables(row['result']);assert tables['/journal']['format']=='partitioned-parquet'
    from app.reporting import table
    page=table(job['id'],'/journal',offset=3,limit=4);assert page
    # A new Codec and Repository simulate reopening after process loss, without
    # retaining any private financial control or preloaded frame cache.
    reopened=Codec(Objects(str(c['objects'].root),'isolated','derivative'))
    new=Repository(c['repo'].engine.url,'isolated','derivative')
    try:
        retained=new.job(job['id']);_,again=verify_persisted(reopened,retained['result'],q['specification']);assert again==receipt
    finally:new.engine.dispose()

@pytest.mark.parametrize('stage',['cancel','deadline','token','attempt','lease','queued_same_token','helper','partition_after_replay','source_after_replay','completion_alias','receipt_alias','input_signedzero'])
def test_actual_worker_late_failure_never_publishes_done(durable,monkeypatch,stage):
    c=durable;q,job=c['start'](execute=False);original=candidate.publish
    def mutate(p):
        if stage=='cancel':worker.STOP.set()
        elif stage=='deadline':
            real=candidate.time.time;monkeypatch.setattr(candidate.time,'time',lambda:q['publication_deadline_unix']+1.)
        elif stage in ('token','attempt','lease','queued_same_token'):
            if stage=='lease':c['repo'].release(job['owner'])
            else:
                if stage=='queued_same_token':
                    payload=c['codec'].load(job['request']);payload['args'][0]['specification']['dated']['specification']['accounts'][0]['cash']+=1e-12;replacement=c['codec'].dump(payload);changes={'request':replacement}
                else:changes={'token':'lost'}if stage=='token'else {'attempt':job['attempt']+1}
                with c['repo'].engine.begin()as conn:conn.execute(jobs.update().where(c['repo'].scoped(jobs),jobs.c.id==job['id']).values(**changes))
        elif stage=='helper':
            from portfolio_risk.analytics import balance_stream
            prior=balance_stream._hash
            def changed(path):return '0'*64 if Path(path).name=='saved_derivative_identity.py'else prior(path)
            monkeypatch.setattr(balance_stream,'_hash',changed)
        elif stage in ('partition_after_replay','source_after_replay'):
            import app.saved_derivative_jobs as m
            verify=m.verify_persisted
            def after(codec,ref,r,check):
                out=verify(codec,ref,r,check);facade=out[0]
                part=next(v[0]['ref']for v in facade.parts.values()if v)if stage=='partition_after_replay'else facade.execution['modeled_source']
                path=c['objects'].root/part['key'];path.write_bytes(path.read_bytes()+b'tampered');return out
            monkeypatch.setattr(m,'verify_persisted',after)
        else:
            key={'completion_alias':'producer_completion','input_signedzero':'input'}.get(stage)
            if key:
                ref=p.result['execution'][key];v=loads(c['objects'].get(ref))
                if stage=='completion_alias':v['complete']=1
                else:v['request']['dated']['specification']['positions'][0]['rate']=-0.
                # A valid rehashed replacement must still fail independently.
                p.result['execution'][key]=c['objects'].put(encoded(v),'bin')
            else:p.result['execution']['validation']['capital_ftp_eligible']=0
        return original(p)
    monkeypatch.setattr(candidate,'publish',mutate);worker.execute(job)
    row=c['repo'].job(job['id']);assert row['status']!='done';assert row['result']is None

@pytest.mark.parametrize('stage',['job_token','job_owner','job_revision','job_request','queue_deadline'])
def test_private_returned_authority_cannot_be_rebased(durable,stage):
    c=durable;q,job=c['start'](execute=False);token=db.ACTIVE_JOB.set(job)
    try:p=candidate.run(q,stopping=worker.STOP.is_set)
    finally:db.ACTIVE_JOB.reset(token)
    if stage=='queue_deadline':p.queued['publication_deadline_unix']+=10.
    else:p.job[stage.split('_')[1]]='forged'if stage not in ('job_revision','job_request')else 1 if stage=='job_revision'else dict(job['request'],bytes=1)
    with pytest.raises((ValueError,RuntimeError,InterruptedError)):candidate.publish(p)
    assert c['repo'].job(job['id'])['status']!='done'

def test_final_sql_uses_same_connection_and_rechecks_after_io(durable,monkeypatch):
    c=durable;complete=c['repo'].complete
    def publication(job,**kw):
        if 'guard'not in kw:return complete(job,**kw)
        read=c['repo'].job
        def no_new_connections(*a,**kw):raise AssertionError('repository read inside final SQL guard')
        monkeypatch.setattr(c['repo'],'job',no_new_connections)
        try:return complete(job,**kw)
        finally:monkeypatch.setattr(c['repo'],'job',read)
    monkeypatch.setattr(c['repo'],'complete',publication)
    _,job=c['start']();row=c['repo'].job(job['id']);assert row['status']=='done',row['detail']

def test_expired_original_deadline_survives_reclaim(durable,monkeypatch):
    c=durable;q,job=c['start'](execute=False);c['repo'].release(job['owner']);assert c['repo'].acquire('replacement',300.)
    retry=c['repo'].claim('replacement');assert retry['attempt']==2
    queued=c['codec'].load(retry['request'])['args'][0];assert queued['publication_deadline_unix']==q['publication_deadline_unix']
    monkeypatch.setattr(candidate.time,'time',lambda:q['publication_deadline_unix']+1.)
    worker.execute(retry);assert c['repo'].job(job['id'])['status']!='done'

@pytest.mark.parametrize('target',['source_clone','native_amount','completion_bool','completion_float','receipt_bool','schema','journal','path','summary','dated_rights','lineage'])
def test_rehashed_persisted_source_and_partitions_fail_independent_reopen(durable,target):
    import io,polars as pl
    from app.artifacts import PartitionedTable,ObjectReference
    c=durable;q,job=c['start']();row=c['repo'].job(job['id']);assert row['status']=='done',row['detail']
    facade=StoredFrames(c['codec'],row['result'],lambda:False)
    if target=='schema':
        original=deepcopy(facade.schemas);facade.schemas['journal']['day']=pl.Float64
        with pytest.raises(ValueError):list(facade.frames('journal'))
        from portfolio_risk.analytics.saved_derivative_transport import SCHEMAS
        assert SCHEMAS['journal']['day']==original['journal']['day'];return
    result={name:PartitionedTable({k:str(v)for k,v in facade.schemas[name].items()},deepcopy(parts))for name,parts in facade.parts.items()}
    result['execution']=deepcopy(facade.execution);result['revision']=facade.revision
    e=result['execution']
    if target in ('source_clone','native_amount'):
        h=loads(c['objects'].get(e['modeled_source']))
        if target=='source_clone':h['source_receipt']['original_request']['dated']['specification']['accounts'][0]['cash']+=1e-12
        else:h['source_receipt']['native_sources'][0]['source']['daily'][-1]['cash']+=.01
        e['modeled_source']=ObjectReference(c['objects'].put(encoded(h),'bin'))
    elif target.startswith('completion_'):
        value={'complete':1 if target=='completion_bool'else True,'protocol':3. if target=='completion_float'else 3}
        e['producer_completion']=ObjectReference(c['objects'].put(encoded(value),'bin'))
    elif target=='receipt_bool':e['validation']['complete_gross_rights_verified']=1
    else:
        table={'journal':'journal','path':'path','summary':'summary','dated_rights':'dated_rights','lineage':'derivative_journal_lineage'}[target]
        parts=deepcopy(facade.parts[table]);part=parts[0]
        frame=pl.read_parquet(io.BytesIO(c['objects'].get(part['ref'])))
        col={'journal':'debit','path':'assets','summary':'final_cash','dated_rights':'recognized_interest','lineage':'debit_amount'}[target]
        if col not in frame.columns:col=next(n for n,v in frame.schema.items()if v==pl.Float64)
        frame=frame.with_columns(pl.when(pl.int_range(pl.len())==0).then(pl.col(col)+.01).otherwise(pl.col(col)).alias(col))
        out=io.BytesIO();frame.write_parquet(out);part['ref']=c['objects'].put(out.getvalue(),'parquet')
        result[table]=PartitionedTable({k:str(v)for k,v in facade.schemas[table].items()},parts)
    replacement=c['codec'].dump(result)
    with pytest.raises((ValueError,ArithmeticError,KeyError)):verify_persisted(c['codec'],replacement,q['specification'])

@pytest.mark.parametrize('mutation',['cancel_during_dump','token_during_dump','deadline_inside_final_guard'])
def test_actual_upload_and_final_guard_failure_windows(durable,monkeypatch,mutation):
    c=durable;q,job=c['start'](execute=False);token=db.ACTIVE_JOB.set(job)
    try:p=candidate.run(q,stopping=worker.STOP.is_set)
    finally:db.ACTIVE_JOB.reset(token)
    if mutation=='deadline_inside_final_guard':
        complete=c['repo'].complete
        def delayed(job,**kwargs):
            if 'guard'in kwargs:
                guard=kwargs['guard']
                def expired(conn):
                    monkeypatch.setattr(candidate.time,'time',lambda:q['publication_deadline_unix']+1.)
                    return guard(conn)
                kwargs['guard']=expired
            return complete(job,**kwargs)
        monkeypatch.setattr(c['repo'],'complete',delayed)
    else:
        put=c['objects'].put
        def changed(*a,**kw):
            ref=put(*a,**kw)
            if mutation=='cancel_during_dump':worker.STOP.set()
            else:
                with c['repo'].engine.begin()as conn:conn.execute(jobs.update().where(c['repo'].scoped(jobs),jobs.c.id==job['id']).values(token='replacement'))
            return ref
        monkeypatch.setattr(c['objects'],'put',changed)
    with pytest.raises((ValueError,RuntimeError,InterruptedError,TimeoutError)):candidate.publish(p)
    row=c['repo'].job(job['id']);assert row['status']!='done'and row['result']is None

@pytest.mark.parametrize('kind',['ledger','credit_ledger'])
def test_verified_derivative_source_is_not_capital_or_ftp_permission(durable,kind):
    c=durable;_,job=c['start']();assert c['repo'].job(job['id'])['status']=='done'
    from app import treasury_store
    with pytest.raises(ValueError,match='compatible'):treasury_store.template(job['id'],kind)
