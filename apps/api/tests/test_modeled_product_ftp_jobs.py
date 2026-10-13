"""Actual default native source and real temporary Codec/SQLite publication.

Run only after root applies the successor and completes the official rebuild.
Financial callbacks and alternate native runtime selectors are absent.
"""
from copy import deepcopy
from contextvars import ContextVar
from pathlib import Path
import hashlib,importlib.util,json,subprocess,sys,threading,types
import pytest
import sqlalchemy as sa
from portfolio_risk.analytics import modeled_product_ftp_reference as ref
HERE=Path(__file__).resolve().parents[3]
FIXTURES=HERE/'packages/portfolio-risk/tests/fixtures/modeled_product_ftp'
def load(name):return json.loads((FIXTURES/f'{name}.json').read_text())
from app.artifacts import Codec,Objects
from app.repository import Repository,jobs,workspaces

from portfolio_risk.core import modeled_product_ftp_native as default_native
EXE_SHA=default_native.binary_identity()

def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path);out=importlib.util.module_from_spec(spec)
    sys.modules[name]=out;spec.loader.exec_module(out);return out

@pytest.fixture
def context(tmp_path,monkeypatch):
    from app import modeled_product_ftp_transport as transport,modeled_product_ftp_jobs as candidate
    repo=Repository('sqlite:///'+str(tmp_path/'jobs.db'),'test','ftp');repo.migrate();repo.initialize({})
    objects=Objects(str(tmp_path/'objects'),'test','ftp');codec=Codec(objects)
    from app import persistence
    identity=persistence.current_identity()
    bundle=dict(workspace=identity,strategy_sha256=EXE_SHA,code_sha256={'isolated':'controlled-operational-fixture'})
    active=ContextVar('isolated_ftp_active',default=None)
    database=types.SimpleNamespace(REPO=repo,CODEC=codec,ACTIVE_JOB=active,identity=lambda:deepcopy(identity),current_identity=lambda:deepcopy(identity))
    monkeypatch.setattr(candidate,'db',database)
    stop=threading.Event();worker=types.ModuleType('app.worker');worker.STOP=stop
    monkeypatch.setitem(sys.modules,'app.worker',worker)
    def start(name='deposit-continuation'):
        queued=candidate.queue_input(load(name),timeout=120.)
        request=codec.dump(dict(operation='modeled_product_ftp.run',args=[queued],identity=identity))
        jid=repo.enqueue('modeled_product_ftp',0,request,{})
        assert repo.acquire('owner',300.);job=repo.claim('owner');assert job['id']==jid
        active.set(job)
        return candidate.run(queued)
    yield types.SimpleNamespace(module=candidate,repo=repo,codec=codec,objects=objects,database=database,
        start=start,stop=stop,identity=identity,bundle=bundle,transport=transport)
    repo.engine.dispose()

def corrupt(objects,descriptor):
    path=objects.root/descriptor['key'];body=path.read_bytes();path.write_bytes(body+b'corruption')

@pytest.mark.parametrize('name',['deposit-continuation','consumer-initial-allowance','card-reviewed-gross-funding','mortgage'])
def test_actual_source_and_whole_journal_persist_reopen_and_guard(context,name):
    c=context;p=c.start(name);output=c.module.publish(p)
    row=c.repo.job(p.job['id']);assert row['status']=='done' and row['result']==output
    saved=c.codec.load(output);body=c.objects.get(saved['native_result']);native=json.loads(body)
    assert ref.verify(p.queued['request'],native)['verified']
    assert saved['tables']['source_journal'].height>0
    assert saved['tables']['source_completion']['complete'].to_list()==[True]*len(native['result']['source_preparation']['sources'])
    assert saved['summary']['capital_ftp_eligible']is False

@pytest.mark.parametrize('mutation',['job_token','job_owner','job_attempt','queued_request','native_reference','summary_bool','bundle','cancel','lease','sql_token','sql_attempt','revision'])
def test_private_authority_and_failed_publication(context,mutation):
    c=context;p=c.start();jid=p.job['id']
    if mutation=='job_token':p.job['token']='rebased'
    elif mutation=='job_owner':p.job['owner']='rebased'
    elif mutation=='job_attempt':p.job['attempt']+=1
    elif mutation=='queued_request':p.queued['request']['charges'][0]['option']['annual_rate']+=1e-16
    elif mutation=='native_reference':p.native_ref.update(c.objects.put(b'{}','bin'))
    elif mutation=='summary_bool':p.result['summary']['verified']=1
    elif mutation=='bundle':c.database.current_identity=lambda:deepcopy(c.identity)|{'native_sha256':'0'*64}
    elif mutation=='cancel':c.stop.set()
    elif mutation=='lease':c.repo.release('owner')
    elif mutation in ('sql_token','sql_attempt'):
        with c.repo.engine.begin()as conn:
            conn.execute(jobs.update().where(c.repo.scoped(jobs),jobs.c.id==jid).values(**({'token':'changed'}if mutation=='sql_token'else {'attempt':2})))
    else:
        with c.repo.engine.begin()as conn:conn.execute(workspaces.update().where(c.repo.scoped(workspaces)).values(revision=1))
    with pytest.raises((ValueError,RuntimeError,InterruptedError)):c.module.publish(p)
    assert c.repo.job(jid)['status']!='done'

@pytest.mark.parametrize('target',['native_after_replay','partition_after_replay','queued_after_replay','cancel_during_dump','token_during_dump'])
def test_late_output_and_queued_body_rechecks(context,monkeypatch,target):
    c=context;p=c.start();verify=ref.verify;count=[0]
    def checked(*a,**kw):
        result=verify(*a,**kw);count[0]+=1
        if target=='native_after_replay':corrupt(c.objects,p.native_ref)
        elif target=='queued_after_replay':corrupt(c.objects,p.job['request'])
        elif target=='partition_after_replay':
            files=list(c.objects.root.rglob('*.parquet'));assert files
            files[0].write_bytes(files[0].read_bytes()+b'corruption')
        return result
    if target.endswith('after_replay'):monkeypatch.setattr(ref,'verify',checked)
    else:
        put=c.objects.put
        def upload(*a,**kw):
            result=put(*a,**kw)
            if target=='cancel_during_dump':c.stop.set()
            else:
                with c.repo.engine.begin()as conn:conn.execute(jobs.update().where(c.repo.scoped(jobs),jobs.c.id==p.job['id']).values(token='changed'))
            return result
        monkeypatch.setattr(c.objects,'put',upload)
    with pytest.raises((ValueError,RuntimeError,InterruptedError)):c.module.publish(p)
    assert c.repo.job(p.job['id'])['status']!='done'

def test_startup_stale_refused_before_native(context):
    c=context;c.database.identity=lambda:{'model':'startup-stale'}
    with pytest.raises(ValueError,match='startup'):c.module.queue_input(load('deposit-continuation'))

@pytest.mark.parametrize('value',[True,float('nan'),float('inf'),0.])
def test_invalid_original_deadline(context,value):
    with pytest.raises(ValueError,match='finite'):context.module.queue_input(load('deposit-continuation'),timeout=value)

@pytest.mark.parametrize('target',['completion_bool','protocol_float','after_completion','journal_dictionary'])
def test_transport_does_not_synthesize_or_weaken_completion(context,target):
    raw=load('deposit-continuation');out,_=default_native.run(raw,timeout=120.);messages=out['result']['source_preparation']['sources'][0]['raw_native_result']['ledger_messages']
    if target=='completion_bool':messages[-1]['complete']=1
    elif target=='protocol_float':messages[-1]['protocol']=3.
    elif target=='after_completion':messages.append({'table':'journal','rows':[]})
    else:next(m for m in messages if m.get('table')=='journal')['columns']['account'][0]=True
    with pytest.raises(ValueError):context.transport.source_tables(out)

@pytest.mark.parametrize('target',['expire_after_sql','cancel_after_sql','expire_in_final_guard','cancel_in_final_guard','partition_in_final_guard'])
def test_original_deadline_and_cancel_after_io_and_final_sql(context,monkeypatch,target):
    import time
    c=context;p=c.start();offset=[0.]
    monkeypatch.setattr(c.module,'time',types.SimpleNamespace(time=lambda:time.time()+offset[0],monotonic=lambda:time.monotonic()+offset[0]))
    if target.endswith('after_sql'):
        owns=c.repo.owns
        def delayed(owner):
            result=owns(owner)
            if target.startswith('expire'):offset[0]=1000.
            else:c.stop.set()
            return result
        monkeypatch.setattr(c.repo,'owns',delayed)
    else:
        complete=c.repo.complete
        def at_commit(job,**kw):
            original=kw['guard']
            def guard(conn):
                if target.startswith('expire'):offset[0]=1000.
                elif target.startswith('cancel'):c.stop.set()
                else:
                    files=list(c.objects.root.rglob('*.parquet'));assert files
                    files[-1].write_bytes(files[-1].read_bytes()+b'corruption')
                original(conn)
            return complete(job,**dict(kw,guard=guard))
        monkeypatch.setattr(c.repo,'complete',at_commit)
    with pytest.raises((TimeoutError,InterruptedError,ValueError)):c.module.publish(p)
    assert c.repo.job(p.job['id'])['status']!='done'

def test_final_guard_uses_no_new_sql_connections(context,monkeypatch):
    c=context;p=c.start();complete=c.repo.complete
    def at_commit(job,**kw):
        original=kw['guard']
        def guard(conn):
            def forbidden(*a,**k):raise AssertionError('fresh SQL ownership read inside final transaction')
            with monkeypatch.context()as m:
                for name in ('job','owns','head'):m.setattr(c.repo,name,forbidden)
                original(conn)
        return complete(job,**dict(kw,guard=guard))
    monkeypatch.setattr(c.repo,'complete',at_commit)
    c.module.publish(p);assert c.repo.job(p.job['id'])['status']=='done'

def test_added_unverified_table_cannot_publish(context):
    import polars as pl
    c=context;p=c.start();p.result['tables']['invented']=pl.DataFrame({'value':[100.]})
    with pytest.raises(ValueError,match='coverage'):c.module.publish(p)
    assert c.repo.job(p.job['id'])['status']!='done'
