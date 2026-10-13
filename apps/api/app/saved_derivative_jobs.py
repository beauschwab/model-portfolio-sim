"""Durable actual saved-derivative owner: transport and private SQL publication.
No Python financial production calculations, custom pricers or alternative runtime.
"""
from copy import deepcopy
from dataclasses import dataclass
import math,time,tempfile
from pathlib import Path
from . import persistence as db
from .artifacts import PartitionedTable,ObjectReference,Codec
from .saved_derivative_transport import import_manifest,verify_persisted,body,metadata,same,MAX_RETAINED
from portfolio_risk.analytics.saved_derivative_identity import encoded,loads,exact_identity,probe
from portfolio_risk.analytics.saved_derivative_transport import SCHEMA
_SEAL=object()
OPERATION='saved_derivative.run'

def plain(v):
    if isinstance(v,PartitionedTable):return dict(partitioned=v.descriptor())
    if isinstance(v,ObjectReference):return dict(v)
    if type(v)is dict:return {k:plain(x)for k,x in v.items()}
    if type(v)in(list,tuple):return [plain(x)for x in v]
    return v

def bundle(check=lambda:False):
    from portfolio_risk.analytics import saved_derivative_runtime as runtime
    from portfolio_risk.analytics.balance_stream import _hash,source_identity
    probe(check);workspace=db.current_identity();probe(check)
    if not exact_identity(workspace,db.identity()):raise RuntimeError('MODEL_VERSION_CHANGED: stale loaded worker/API')
    leaves=('saved_derivative_runtime','saved_derivative_transport','saved_derivative_identity','saved_derivative_financial_reference','saved_derivative_mapping_reference','saved_derivative_dated_reference','saved_derivative_mixed_reference','saved_derivative_work_bounds_reference','saved_derivative_dto_reference')
    paths={name:Path(runtime.__file__).with_name(name+'.py')for name in leaves}
    paths.update({name:Path(__file__).with_name(name+'.py')for name in ('saved_derivative_jobs','saved_derivative_transport','saved_derivative_routes','worker','operations','repository','artifacts','persistence','deposit_partition_bounds')})
    # Prefix names avoid replacing engine transport with API transport.
    paths['engine_saved_derivative_transport']=Path(runtime.__file__).with_name('saved_derivative_transport.py')
    hashes={}
    for name,path in paths.items():probe(check);hashes[name]=_hash(path);probe(check)
    binary=runtime.binary_path();probe(check);sha=_hash(binary);probe(check)
    return dict(workspace=workspace,binary_sha256=sha,independent_source_identity=source_identity(),code_sha256=hashes)

def queue_input(raw,*,timeout=1800.):
    if db.REPO is None:raise ValueError('saved derivatives require durable storage')
    if type(timeout)not in(int,float)or not math.isfinite(timeout)or not .001<=timeout<=1800.:raise ValueError('finite original derivative deadline required')
    deadline=time.time()+timeout
    def clock():
        if time.time()>=deadline:raise TimeoutError('derivative accepted queue deadline expired')
        return False
    probe(clock)
    if type(raw)is not dict or set(raw)!={'schema','dated','owners'}or raw['schema']!=SCHEMA or len(encoded(raw))>32*1024*1024:raise ValueError('bounded explicit raw saved derivative mixed source required')
    current=bundle(clock);probe(clock)
    return dict(schema=SCHEMA,specification=deepcopy(raw),native_bundle=current,publication_deadline_unix=deadline)

@dataclass(frozen=True)
class Publication:
    result:dict
    queued:dict
    job:dict
    clock:object
    basic:object
    fence:object
    private_job:bytes
    private_queue:bytes
    private_result:bytes
    seal:object

class CheckedObjects:
    def __init__(self,objects,check):self.objects,self.check=objects,check
    def __getattr__(self,k):return getattr(self.objects,k)
    def put(self,*a,**kw):probe(self.check);r=self.objects.put(*a,**kw);probe(self.check);return r
    def get(self,*a,**kw):probe(self.check);r=self.objects.get(*a,**kw);probe(self.check);return r
    def get_bounded(self,*a,**kw):probe(self.check);r=self.objects.get_bounded(*a,**kw);probe(self.check);return r

def run(queued,*,stopping):
    if not callable(stopping):raise ValueError("caller cancellation control required")
    from portfolio_risk.analytics import saved_derivative_runtime as runtime
    if db.REPO is None or db.ACTIVE_JOB.get()is None or queued.get('schema')!=SCHEMA:raise ValueError('selected durable derivative job required')
    job=deepcopy(db.ACTIVE_JOB.get());private_job=encoded(plain(job));private_queue=encoded(queued);expected_job=loads(private_job);expected_queue=loads(private_queue)
    deadline=queued.get('publication_deadline_unix')
    if type(deadline)is not float or not math.isfinite(deadline):raise ValueError('original finite accepted derivative deadline required')
    monotonic=time.monotonic()+max(0.,deadline-time.time());last=[-math.inf]
    codec,repo,objects=db.CODEC,db.REPO,db.CODEC.objects
    def snapshots():
        same(plain(job),expected_job,'returned derivative job rebased');same(queued,expected_queue,'returned derivative queue rebased')
        if db.CODEC is not codec or db.REPO is not repo or db.CODEC.objects is not objects:raise RuntimeError('derivative storage authority changed')
    def clock():
        if stopping():raise InterruptedError('derivative cancellation')
        if time.time()>=deadline or time.monotonic()>=monotonic:raise TimeoutError('original accepted derivative deadline expired')
        return False
    def basic(force=False):
        clock();now=time.monotonic()
        if force or now-last[0]>=.1:
            snapshots();row=repo.job(expected_job['id']);last[0]=now
            for key in ('id','request','revision','owner','token','attempt'):same(row[key],expected_job[key],'derivative durable authority changed')
            if row['status']!='running'or not repo.owns(expected_job['owner']):raise InterruptedError('derivative token/lease/cancellation lost')
            same(repo.head(),expected_job['revision'],'derivative input revision changed')
        clock();return False
    def fence(sql=False):
        clock();snapshots()
        if not sql:basic(True)
        same(bundle(clock),expected_queue['native_bundle'],'MODEL_VERSION_CHANGED: derivative bundle changed')
        payload=metadata(codec,expected_job['request'],{'operation','args','identity'},clock if sql else basic)
        same(payload.get('operation'),OPERATION,'queued derivative operation changed')
        if type(payload.get('args'))not in(list,tuple)or len(payload['args'])!=1:raise ValueError('queued derivative args changed')
        same(payload['args'][0],expected_queue,'queued derivative body changed')
        same(payload.get('identity'),expected_queue['native_bundle']['workspace'],'queued startup manifest changed')
        clock();return False
    basic(True);fence()
    from .maintenance import scratch_root
    root=scratch_root(db.CONFIG,objects);root.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='saved-derivative-',dir=root)as directory:
        remaining=min(deadline-time.time(),monotonic-time.monotonic())
        path=runtime.run(queued['specification'],directory,timeout=remaining,cancelled=basic,
            progress=lambda name: None,expected_binary=queued['native_bundle']['binary_sha256'])
        fence();result=import_manifest(Codec(CheckedObjects(objects,basic)),path,basic);fence()
    result['revision']=expected_job['revision'];e=result['execution']
    e.update(publication_deadline_unix=deadline,request_manifest=deepcopy(expected_job['request']),native_bundle=deepcopy(expected_queue['native_bundle']))
    same(e['binary_sha256'],expected_queue['native_bundle']['binary_sha256'],'executed derivative hash differs')
    private_result=encoded(plain(result));fence()
    return Publication(result,queued,job,clock,basic,fence,private_job,private_queue,private_result,_SEAL)

def publish(p):
    if type(p)is not Publication or p.seal is not _SEAL:raise ValueError('private derivative publication required')
    p.fence();same(plain(p.result),loads(p.private_result),'derivative returned result changed')
    codec=Codec(CheckedObjects(db.CODEC.objects,p.basic));output=codec.dump(p.result);p.fence()
    facade,receipt=verify_persisted(codec,output,p.queued['specification'],p.basic)
    same(plain(metadata(codec,output,{'execution','revision'},p.basic)),{k:v for k,v in loads(p.private_result).items()if k in {'execution','revision'}},'derivative persisted metadata changed')
    p.fence();facade.recheck();p.fence()
    # Never retain a permission flag as financial authority. The complete source
    # and ordered persisted GL were reconstructed above; final reads bind bytes.
    from .repository import jobs,workspaces
    import sqlalchemy as sa
    expected=loads(p.private_job);queued=loads(p.private_queue)
    output_ref=deepcopy(output);facade_snapshot=encoded(dict(parts=facade.parts,execution=facade.execution,revision=facade.revision))
    def current(conn):
        p.clock();same(plain(p.job),expected,'derivative returned job changed');same(p.queued,queued,'derivative queue changed')
        same(plain(p.result),loads(p.private_result),'derivative result rebased')
        row=conn.execute(sa.select(jobs).where(db.REPO.scoped(jobs),jobs.c.id==expected['id'])).mappings().one()
        for key in ('id','request','revision','owner','token','attempt'):same(row[key],expected[key],'final derivative SQL authority changed')
        workspace=conn.execute(sa.select(workspaces).where(db.REPO.scoped(workspaces))).mappings().one()
        if row['status']!='running'or workspace['owner']!=expected['owner']or workspace['lease_until']<=time.time():raise InterruptedError('final derivative owner/lease lost')
        same(workspace['revision'],expected['revision'],'final derivative revision changed');p.clock()
    def guard(conn):
        current(conn);p.fence(True)
        same(dict(output),dict(output_ref),'derivative output descriptor rebased')
        body(db.CODEC.objects,output_ref,p.clock)
        if encoded(dict(parts=facade.parts,execution=facade.execution,revision=facade.revision))!=facade_snapshot:raise ValueError('derivative facade rebased')
        # Final sweep uses the pure clock to avoid nested SQL connections while locked.
        facade.check=p.clock;facade.codec=Codec(CheckedObjects(db.CODEC.objects,p.clock));facade.recheck(p.clock)
        p.fence(True);current(conn);p.clock()
    db.REPO.complete(expected,result=output_ref,guard=guard)
    return output_ref
