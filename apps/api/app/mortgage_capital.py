"""Dedicated durable raw-native mortgage closing. No financial calculations."""
from copy import deepcopy
from dataclasses import dataclass
import math
import time
import json
from pathlib import Path
from . import persistence as db
from . import mortgage_capital_source as source
from .artifacts import Codec
from portfolio_risk.analytics.mortgage_mixed_identity import exact_json_identity

SCHEMA='mortgage-closing-capital-1'

def exact(a,b,message):
    if not exact_json_identity(a,b):raise ValueError(message)

def native_bundle(check=lambda:False):
    """Current code plus all existing native runtime identities; hashing only."""
    from portfolio_risk.core import model_native, product_strategy_native
    from portfolio_risk.analytics import model_ledger
    from portfolio_risk.analytics.balance_stream import _hash
    from portfolio_risk.analytics.balance_stream import source_identity
    result={'workspace':db.current_identity()}
    for name in ('native_sha256','risk_native_sha256','balance_native_sha256',
                 'ledger_validation_native_sha256','workflow_native_sha256'):
        value=result['workspace'].get(name)
        if type(value) is not str or len(value)!=64:
            raise RuntimeError('full current native bundle requires rebuild; no fallback')
    result['financial_source_sha256']=source_identity()
    from portfolio_risk.analytics import mortgage_closing_capital_reference as reference
    from portfolio_risk.core import mortgage_capital_native as runtime
    from . import mortgage_capital_fence
    paths={'independent_capital_reference':Path(reference.__file__),
        'capital_transport':Path(runtime.__file__),'source_binder':Path(source.__file__),
        'publication_fence':Path(mortgage_capital_fence.__file__),
        'capital_operation':Path(__file__),'worker':Path(__file__).with_name('worker.py'),
        'repository':Path(__file__).with_name('repository.py'),
        'operation_registry':Path(__file__).with_name('operations.py'),
        'route_schema':Path(__file__).with_name('treasury_routes.py')}
    result['capital_code_sha256']={name:_hash(path) for name,path in paths.items()}
    for name,path in [('strategy',product_strategy_native.binary_path()),
                      ('lifecycle',model_native.binary_path()),('model_ledger',model_ledger.binary_path())]:
        if check():raise InterruptedError('capital bundle check cancelled')
        if not path.is_file():raise RuntimeError('Rust capital requires full native rebuild; no fallback')
        result[name]=_hash(path)
        if check():raise InterruptedError('capital bundle check cancelled')
    return result

def queue_input(source_job,specification,closing_review,*,timeout=1800.):
    """Bounded metadata-only compatibility check, no journal/financial execution."""
    if type(timeout) not in (int,float) or not math.isfinite(timeout) or not .001<=timeout<=1800.:
        raise ValueError('finite bounded original capital job timeout required')
    if db.REPO is None:raise ValueError('mortgage closing capital requires durable storage')
    deadline=time.time()+timeout  # captured once before queue writes, never reset by retry
    def check():
        if time.time()>=deadline:raise TimeoutError('original capital queue deadline expired')
        return False
    binding=source.capture_binding(db.REPO,source_job,check)
    queued=db.CODEC.metadata(binding['request'],{'operation','args'})
    execution=db.CODEC.metadata(binding['result'],{'execution'}).get('execution')
    if (queued.get('operation')!='store.run_modeled_balance_stress' or
        type(queued.get('args')) not in (list,tuple) or len(queued['args'])!=1 or
        queued['args'][0].get('specification',{}).get('schema')!='mortgage-mixed-accounting-1' or
        type(execution) is not dict or execution.get('model_version')!='mortgage-mixed-accounting-1'):
        raise ValueError('completed native mortgage mixed source required')
    completion=execution.get('producer_completion')
    source._descriptor(completion,'bin',128)
    exact(source._json(db.CODEC.objects.get(completion)),{'complete':True,'protocol':3},'actual completed mortgage producer required')
    if type(specification) is not dict or specification.get('kind')!='ledger':
        raise ValueError('explicit mortgage closing ledger policies required; FTP unsupported')
    import json
    supplied={'specification':specification,'closing_review':closing_review}
    if len(json.dumps(supplied,allow_nan=False,separators=(',',':')).encode())>2*1024*1024:
        raise ValueError('mortgage closing supplied policy body exceeds2MiB')
    bundle=native_bundle(check)
    exact(bundle['workspace'],db.identity(),'MODEL_VERSION_CHANGED: worker/API startup bundle is stale; restart required')
    check()
    return dict(schema=SCHEMA,source_binding=binding,specification=deepcopy(specification),
        closing_review=deepcopy(closing_review),native_bundle=bundle,publication_deadline_unix=deadline)

class CheckedObjects:
    def __init__(self,objects,check):self.objects,self.check=objects,check
    def __getattr__(self,name):return getattr(self.objects,name)
    def put(self,*args,**kwargs):
        self.check();result=self.objects.put(*args,**kwargs);self.check();return result
    def get(self,*args,**kwargs):
        self.check();result=self.objects.get(*args,**kwargs);self.check();return result

@dataclass(frozen=True)
class Publication:
    result:dict
    queued:dict
    job:dict
    verified_source:object
    control:object
    native_request:dict
    basic:object
    check:object
    seal:object
    expected_result:bytes

_SEAL=object()

def run(queued):
    """Allowlisted worker operation. Private control survives through publication."""
    from .worker import STOP
    from portfolio_risk.core import mortgage_capital_native as runtime
    from portfolio_risk.analytics import mortgage_closing_capital_reference as reference
    if queued.get('schema')!=SCHEMA:raise ValueError('explicit mortgage closing queued contract required')
    active=db.ACTIVE_JOB.get()
    if db.REPO is None or active is None:raise RuntimeError('mortgage closing requires durable worker')
    # SQLAlchemy's quoted column-key strings are transport descriptors. Retain
    # ordinary JSON field names before binding the private durable row snapshot.
    job={str(k):deepcopy(v) for k,v in active.items()}
    payload=db.CODEC.metadata(job['request'],{'operation','args','identity'})
    if payload.get('operation')!='mortgage_capital.run' or type(payload.get('args')) not in (list,tuple) or len(payload['args'])!=1:
        raise ValueError('capital queued operation/input binding changed')
    exact(payload['args'][0],queued,'capital queued original inputs changed')
    exact(payload.get('identity'),queued['native_bundle']['workspace'],'capital queued manifest/startup bundle differs')
    exact(payload['identity'],db.identity(),'MODEL_VERSION_CHANGED: capital worker startup differs')
    deadline=queued.get('publication_deadline_unix')
    if type(deadline) not in (int,float) or not math.isfinite(deadline):raise ValueError('finite ORIGINAL queued deadline required')
    remaining=deadline-time.time()
    if remaining<=0:raise TimeoutError('original queued capital deadline expired')
    monotonic_deadline=time.monotonic()+remaining
    retained=deepcopy(queued)
    queued_bytes=json.dumps(queued,allow_nan=False,separators=(',',':')).encode()
    job_bytes=json.dumps(job,allow_nan=False,separators=(',',':')).encode()
    last=[0.]
    def basic(force=False):
        if STOP.is_set():raise InterruptedError('mortgage capital cancelled')
        if time.time()>=deadline or time.monotonic()>=monotonic_deadline:raise TimeoutError('original capital deadline expired')
        now=time.monotonic()
        if force or now-last[0]>=.1:
            exact(job,json.loads(job_bytes),'returned capital durable owner changed')
            exact(queued,json.loads(queued_bytes),'queued capital inputs changed')
            exact(retained,json.loads(queued_bytes),'returned capital inputs changed')
            row=db.REPO.job(job['id']);last[0]=now
            if row['status']!='running' or row['token']!=job['token'] or row['owner']!=job['owner'] or not db.REPO.owns(job['owner']):
                raise InterruptedError('capital token/cancel/worker lease lost')
            exact(row['request'],job['request'],'queued capital manifest changed')
            exact(row['revision'],job['revision'],'queued capital revision changed')
            exact(db.REPO.head(),job['revision'],'capital workspace revision changed')
            exact(queued,retained,'queued capital source/policy/deadline changed')
        if STOP.is_set():raise InterruptedError('mortgage capital cancelled during ownership reads')
        if time.time()>=deadline or time.monotonic()>=monotonic_deadline:
            raise TimeoutError('original capital deadline expired during ownership reads')
        return False
    basic()
    exact(native_bundle(basic),queued['native_bundle'],'MODEL_VERSION_CHANGED: capital bundle differs from queue')
    verified=source.reopen_verified(db.REPO,db.CODEC,queued['source_binding'],basic)
    header=dict(schema='model-ledger-preparation-1',model_schema='mortgage-mixed-accounting-1',
        prepared_request=verified.prepared,source_receipt=verified.source_receipt)
    # Independent private accounting control, never a caller receipt flag.
    control=reference.verify_source(verified.raw,header,verified.frames,basic)
    request=reference.native_request(queued['specification'],verified.raw,queued['closing_review'],control)
    def check():
        basic(True);verified.recheck()
        current_bundle=native_bundle(basic)
        exact(current_bundle,queued['native_bundle'],'MODEL_VERSION_CHANGED: capital bundle changed')
        exact(current_bundle['workspace'],payload['identity'],'MODEL_VERSION_CHANGED: capital startup/queued identity changed')
        basic(True)
        return False
    check()
    remaining=min(deadline-time.time(),monotonic_deadline-time.monotonic())
    result,execution=runtime.run(request,expected_binary=queued['native_bundle']['strategy'],cancelled=basic,timeout=remaining)
    check()
    validation=reference.verify_capital(request,result,control,basic)
    check()
    execution.update(publication_deadline_unix=deadline,source_binding=deepcopy(queued['source_binding']),
        source_provenance=deepcopy(verified.provenance),native_bundle=deepcopy(queued['native_bundle']),
        request_manifest=deepcopy(job['request']),validation=validation)
    result=dict(native_result=result,native_request=request,execution=execution,
        revision=job['revision'],schema=SCHEMA)
    encoded=json.dumps(result,allow_nan=False,separators=(',',':')).encode()
    return Publication(result,retained,job,verified,control,request,basic,check,_SEAL,encoded)

def publish(publication):
    """Object writes, persisted replay and final SQL fence under the same deadline."""
    from portfolio_risk.analytics import mortgage_closing_capital_reference as reference
    if type(publication) is not Publication or publication.seal is not _SEAL:
        raise ValueError('private native capital publication required')
    p=publication;p.check()
    exact(p.result,json.loads(p.expected_result),'native capital returned result changed before publication')
    codec=Codec(CheckedObjects(db.CODEC.objects,p.check))
    output=codec.dump(p.result)  # each upload and its completion are checked
    p.check()
    saved=codec.load(output)
    exact(saved,json.loads(p.expected_result),'capital object changed after dump')
    receipt=reference.verify_capital(p.native_request,saved['native_result'],p.control,p.basic)
    exact(receipt,saved['execution']['validation'],'persisted capital independent receipt changed')
    p.check()
    retained_tables=deepcopy(codec.tables(output))
    def output_recheck():
        p.basic(True)
        source._descriptor(output,'json',32*1024*1024)
        # Checksummed immutable body reread, not a repeated finance calculation.
        db.CODEC.objects.get(output)
        exact(db.CODEC.tables(output),retained_tables,'capital output table registry changed')
        total=0
        for table in retained_tables.values():
            parts=table.get('parts',[]) if table.get('format')=='partitioned-parquet' else [dict(ref=table)]
            for part in parts:
                p.basic(True)
                ref=part['ref'];source._descriptor(ref,'parquet',32*1024*1024)
                total+=ref['bytes']
                if total>32*1024*1024:raise ValueError('bounded capital persisted table bytes exceeded')
                db.CODEC.objects.get(ref)
                p.basic(True)
        p.basic(True)
    output_recheck()
    def guard(conn):
        # Repository holds workspace lock. Source publication and capital token
        # are checked in this very transaction, preventing a metadata TOCTOU.
        import sqlalchemy as sa
        from .repository import jobs,workspaces
        source_row=conn.execute(sa.select(jobs).where(db.REPO.scoped(jobs),jobs.c.id==p.queued['source_binding']['id'])).mappings().one()
        exact(source._row_binding(dict(source_row)),p.queued['source_binding'],'completed source changed at SQL publication')
        p.check()
        output_recheck()
        # Fresh same-connection state AFTER expensive object/code verification.
        current=conn.execute(sa.select(jobs).where(db.REPO.scoped(jobs),jobs.c.id==p.job['id'])).mappings().one()
        for name in ('id','revision','request','token','owner'):
            exact(current[name],p.job[name],'capital durable owner changed at SQL publication')
        if current['status']!='running':raise InterruptedError('capital cancelled at SQL publication')
        lease=conn.execute(sa.select(workspaces.c.owner).where(db.REPO.scoped(workspaces),
            workspaces.c.owner==p.job['owner'],workspaces.c.lease_until>time.time())).scalar_one_or_none()
        if lease is None:raise InterruptedError('capital lease expired at SQL publication')
        revision=conn.execute(sa.select(workspaces.c.revision).where(db.REPO.scoped(workspaces))).scalar_one()
        exact(revision,p.job['revision'],'capital workspace revision changed at SQL publication')
        p.basic(True)
    db.REPO.complete(p.job,result=output,guard=guard)
    return output
