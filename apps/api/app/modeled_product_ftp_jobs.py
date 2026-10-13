"""Prospective raw-source FTP durable job. Financial execution and verification stay in engine.

Raw source input and explicitly selected completed Card2/performing-deposit jobs
share this operation. Selection has its own reviewed transport binding and never
infers source, funding policy or permission from a receipt flag.
"""
from copy import deepcopy
from dataclasses import dataclass
import hashlib,json,math,time
from pathlib import Path
import polars as pl
from . import persistence as db
from .artifacts import Codec,ObjectReference
from portfolio_risk.analytics.mortgage_mixed_identity import exact_json_identity

SCHEMA='modeled-product-ftp-preparation-1'
MAX_BODY=1048576
MAX_OUTPUT=32*1024*1024
_SEAL=object()
def encoded(value):return json.dumps(value,allow_nan=False,separators=(',',':'),sort_keys=True).encode()
def transport_value(value):
    if isinstance(value,ObjectReference):return dict(value)
    if type(value)is dict:return {k:transport_value(v)for k,v in value.items()}
    if type(value)is list:return [transport_value(v)for v in value]
    return value
def exact(a,b,message):
    if not exact_json_identity(transport_value(a),transport_value(b)):raise ValueError(message)
def probe(check):
    if check():raise InterruptedError('FTP cancelled')
def native_bundle(check=lambda:False):
    from portfolio_risk.core import modeled_product_ftp_native as runtime
    from portfolio_risk.analytics import modeled_product_ftp_reference as reference
    from portfolio_risk.analytics import modeled_product_ftp_source_reference as source
    from portfolio_risk.analytics import modeled_product_funding_reference as funding
    from portfolio_risk.analytics.balance_stream import source_identity,_hash
    current=db.current_identity();probe(check)
    for key in ('native_sha256','risk_native_sha256','balance_native_sha256','ledger_validation_native_sha256','workflow_native_sha256'):
        value=current.get(key)
        if type(value)is not str or len(value)!=64:raise RuntimeError('full native FTP bundle requires rebuild; no fallback')
    paths=dict(runtime=Path(runtime.__file__),reference=Path(reference.__file__),source_reference=Path(source.__file__),funding_reference=Path(funding.__file__),worker=Path(__file__).with_name('worker.py'),operations=Path(__file__).with_name('operations.py'),repository=Path(__file__).with_name('repository.py'),route=Path(__file__).with_name('treasury_routes.py'),operation=Path(__file__),transport=Path(__file__).with_name('modeled_product_ftp_transport.py'),source_binding=Path(__file__).with_name('modeled_product_ftp_source_binding.py'),source_selection=Path(__file__).with_name('modeled_product_ftp_selection.py'),source_identity_reference=Path(reference.__file__).with_name('modeled_product_ftp_identity.py'),source_dto_identity=Path(reference.__file__).with_name('_modeled_product_dto_identity.py'),source_portfolio_funding=Path(reference.__file__).with_name('_modeled_product_deposit_portfolio_reference.py'),source_portfolio_reference=Path(reference.__file__).with_name('deposit_portfolio_reference.py'),source_portfolio_principal=Path(reference.__file__).with_name('_deposit_portfolio_principal_reference.py'),source_portfolio_income=Path(reference.__file__).with_name('_deposit_portfolio_income_reference.py'))
    code={}
    for name,path in paths.items():probe(check);code[name]=_hash(path);probe(check)
    return dict(workspace=current,strategy_sha256=runtime.binary_identity(),financial_source_sha256=source_identity(),code_sha256=code)
def queue_input(request,*,timeout=1800.):
    if db.REPO is None:raise ValueError('FTP requires durable storage')
    if type(timeout)not in(int,float)or not math.isfinite(timeout)or not .001<=timeout<=1800.:raise ValueError('finite original FTP deadline required')
    deadline=time.time()+timeout
    def check():
        if time.time()>=deadline:raise TimeoutError('original FTP queue deadline expired')
        return False
    if type(request)is not dict or request.get('schema')!=SCHEMA or len(encoded(request))>MAX_BODY:raise ValueError('bounded explicit raw FTP source contract required')
    bundle=native_bundle(check)
    exact(bundle['workspace'],db.identity(),'MODEL_VERSION_CHANGED: API startup is stale; restart required')
    probe(check)
    return dict(schema=SCHEMA,request=deepcopy(request),native_bundle=bundle,publication_deadline_unix=deadline)

class CheckedObjects:
    def __init__(self,objects,check):self.objects,self.check=objects,check
    def __getattr__(self,name):return getattr(self.objects,name)
    def put(self,*a,**kw):probe(self.check);v=self.objects.put(*a,**kw);probe(self.check);return v
    def get(self,*a,**kw):probe(self.check);v=self.objects.get(*a,**kw);probe(self.check);return v

def tables(result,check=lambda:False):
    """Strict typed transport only. No financial amounts are calculated here."""
    native=result['result'];treasury=native['treasury_result']
    schemas=dict(
        ftp_positions={**dict.fromkeys('id loan_id cohort_id entity currency scenario period curve_id'.split(),pl.String),**dict.fromkeys('reference_rate liquidity_spread reference_charge liquidity_charge option_charge contingent_liquidity_charge ftp_charge external_profit business_profit capital_cost economic_profit raroc'.split(),pl.Float64),'flat_tail_used':pl.Boolean},
        ftp_reconciliation={**dict.fromkeys('entity currency scenario period'.split(),pl.String),**dict.fromkeys('external_profit business_profit treasury_profit consolidated_profit capital_cost elimination_error'.split(),pl.Float64)})
    from .modeled_product_ftp_transport import source_tables
    frames={name:pl.DataFrame(treasury[name],schema=schema,strict=True) for name,schema in schemas.items()}
    frames.update(source_tables(result,check));return frames

@dataclass(frozen=True)
class Publication:
    result:dict
    queued:dict
    job:dict
    basic:object
    clock:object
    fence:object
    private_job:bytes
    private_queue:bytes
    private_result:bytes
    native_ref:dict
    seal:object

def run(queued):
    from .worker import STOP
    from portfolio_risk.core import modeled_product_ftp_native as runtime
    from portfolio_risk.analytics import modeled_product_ftp_reference as reference
    if queued.get('schema')!=SCHEMA or db.REPO is None or db.ACTIVE_JOB.get()is None:raise ValueError('durable raw FTP job required')
    job={str(k):deepcopy(v)for k,v in db.ACTIVE_JOB.get().items()};private_job=encoded(job);private_queue=encoded(queued)
    payload=db.CODEC.metadata(job['request'],{'operation','args','identity'})
    exact(payload.get('operation'),'modeled_product_ftp.run','FTP durable operation changed')
    if type(payload.get('args'))not in(list,tuple)or len(payload['args'])!=1:raise ValueError('one immutable FTP queued input required')
    exact(payload['args'][0],queued,'FTP immutable queued inputs changed')
    exact(payload.get('identity'),queued['native_bundle']['workspace'],'queued startup bundle differs')
    exact(payload['identity'],db.identity(),'MODEL_VERSION_CHANGED: worker startup differs')
    deadline=queued.get('publication_deadline_unix')
    if type(deadline)not in(int,float)or not math.isfinite(deadline):raise ValueError('finite original queued FTP deadline required')
    monotonic=time.monotonic()+max(0.,deadline-time.time());last=[-math.inf]
    expected_job=json.loads(private_job);expected_queue=json.loads(private_queue)
    def snapshots():
        exact(job,expected_job,'returned FTP job rebased')
        exact(queued,expected_queue,'returned FTP input rebased')
    def clock():
        if STOP.is_set():raise InterruptedError('FTP cancelled')
        if time.time()>=deadline or time.monotonic()>=monotonic:raise TimeoutError('original queued FTP deadline expired')
        return False
    def basic(force=False):
        clock();now=time.monotonic()
        if force or now-last[0]>=.1:
            snapshots();row=db.REPO.job(expected_job['id']);last[0]=now
            for key in ('id','request','revision','owner','token','attempt'):exact(row[key],expected_job[key],'FTP durable identity changed')
            if row['status']!='running' or not db.REPO.owns(expected_job['owner']):raise InterruptedError('FTP cancel/token/lease lost')
            exact(db.REPO.head(),expected_job['revision'],'FTP input revision changed')
        clock();return False
    reopened=[]
    def fence(sql=False,conn=None):
        clock();snapshots()
        if not sql:basic(True)
        current=native_bundle(clock)
        exact(current,queued['native_bundle'],'MODEL_VERSION_CHANGED: FTP bundle changed')
        exact(current['workspace'],payload['identity'],'MODEL_VERSION_CHANGED: stale loaded worker')
        retained=db.CODEC.metadata(json.loads(private_job)['request'],{'operation','args','identity'})
        exact(retained.get('operation'),'modeled_product_ftp.run','FTP queued operation body changed')
        if type(retained.get('args'))not in(list,tuple)or len(retained['args'])!=1:raise ValueError('FTP queued argument coverage changed')
        exact(retained['args'][0],json.loads(private_queue),'FTP queued body changed')
        exact(retained.get('identity'),payload['identity'],'FTP queued identity body changed')
        for _,source in reopened:source.recheck(clock if sql else basic,conn)
        clock();return False
    basic(True);fence()
    if 'completed_sources'in queued:
        from .modeled_product_ftp_source_binding import reopen
        for captured in queued['completed_sources']:
            source=reopen(db.REPO,db.CODEC,captured,basic)
            records=[s for s in queued['request']['source']['sources']if s['source_id']==captured['source_id']]
            if len(records)!=1:raise ValueError('selected source raw coverage changed')
            exact(records[0]['input']['request'],source.raw,'selected source raw substitution')
            reopened.append((captured['source_id'],source))
        fence()
    timeout=min(deadline-time.time(),monotonic-time.monotonic())
    result,execution=runtime.run(queued['request'],expected_binary=queued['native_bundle']['strategy_sha256'],cancelled=basic,timeout=timeout)
    fence();validation=reference.verify(queued['request'],result,basic);fence()
    for source_id,source in reopened:source.match(result,source_id,basic)
    fence()
    body=encoded(result)
    if len(body)>MAX_OUTPUT:raise ValueError('complete retained native FTP body exceeds32MiB')
    native_ref=db.CODEC.objects.put(body,'bin');basic(True)
    if db.CODEC.objects.get(native_ref)!=body:raise ValueError('native source body changed on upload')
    if reopened:execution['completed_source_bindings']=[dict(source_id=sid,provenance=deepcopy(source.provenance),verification=deepcopy(source.verification))for sid,source in reopened]
    execution.update(validation=validation,native_bundle=deepcopy(queued['native_bundle']),publication_deadline_unix=deadline,request_manifest=deepcopy(job['request']),complete_native_ref=deepcopy(native_ref))
    value=dict(schema=SCHEMA,revision=job['revision'],execution=execution,
        native_result=ObjectReference(native_ref),tables=tables(result,basic),
        summary=dict(verified=True,source_count=len(result['result']['source_preparation']['sources']),position_count=len(result['result']['treasury_request']['positions']),regulatory_approval=False,empirical_validation=False,capital_ftp_eligible=False))
    # Private transport summary seal includes full frame bytes through Codec below;
    # only immutable scalar identity is exposed before publication.
    private_result=encoded({k:v for k,v in value.items()if k!='tables'})
    return Publication(value,queued,job,basic,clock,fence,private_job,private_queue,private_result,deepcopy(native_ref),_SEAL)

def publish(p):
    from portfolio_risk.analytics import modeled_product_ftp_reference as reference
    if type(p)is not Publication or p.seal is not _SEAL:raise ValueError('private native FTP publication required')
    p.fence();exact({k:v for k,v in p.result.items()if k!='tables'},json.loads(p.private_result),'FTP summary changed')
    exact(p.native_ref,json.loads(p.private_result)['native_result'],'native output reference rebased')
    codec=Codec(CheckedObjects(db.CODEC.objects,p.basic));output=codec.dump(p.result);p.fence()
    saved=codec.load(output);native_body=db.CODEC.objects.get(p.native_ref)
    if len(native_body)>MAX_OUTPUT:raise ValueError('retained native FTP body cap')
    result=json.loads(native_body);receipt=reference.verify(p.queued['request'],result,p.basic)
    exact(receipt,saved['execution']['validation'],'persisted FTP independent receipt changed')
    expected=tables(result,p.basic)
    if set(saved['tables'])!=set(expected):raise ValueError('persisted FTP table coverage changed')
    for name,want in expected.items():
        actual=saved['tables'][name]
        if actual.schema!=want.schema or not actual.equals(want,null_equal=True):raise ValueError('persisted FTP typed table changed')
    exact({k:v for k,v in saved.items()if k!='tables'},json.loads(p.private_result),'persisted FTP metadata changed')
    descriptors=deepcopy(codec.tables(output))
    def output_recheck(sql=False,conn=None):
        # Actual object-body rereads after oracle and inside final transaction.
        p.fence(sql,conn)
        exact(p.native_ref,json.loads(p.private_result)['native_result'],'native output reference rebased')
        db.CODEC.objects.get(p.native_ref);db.CODEC.objects.get(output)
        # The authoritative SQL checks use the caller's transaction. Object I/O
        # inside it must not initiate numerical ownership polls on fresh SQL connections.
        reader=Codec(CheckedObjects(db.CODEC.objects,p.clock))if sql else codec
        exact(reader.tables(output),descriptors,'FTP partition registry changed')
        count=p.native_ref['bytes']+output['bytes']
        for descriptor in descriptors.values():
            parts=descriptor.get('parts',[])if descriptor.get('format')=='partitioned-parquet'else [dict(ref=descriptor)]
            for part in parts:
                ref=part['ref'];count+=ref['bytes']
                if count>MAX_OUTPUT:raise ValueError('joined persisted FTP output exceeds32MiB')
                db.CODEC.objects.get(ref)
        p.fence(sql,conn)
    output_recheck()
    def guard(conn):
        import sqlalchemy as sa
        from .repository import jobs,workspaces
        def row_check():
            expected=json.loads(p.private_job)
            exact(p.job,expected,'returned FTP owner rebased')
            exact(p.queued,json.loads(p.private_queue),'returned FTP queued input rebased')
            retained=db.CODEC.metadata(expected['request'],{'operation','args','identity'})
            exact(retained.get('operation'),'modeled_product_ftp.run','FTP final queued operation changed')
            if type(retained.get('args'))not in(list,tuple)or len(retained['args'])!=1:raise ValueError('FTP final argument coverage changed')
            exact(retained['args'][0],json.loads(p.private_queue),'FTP final queued request changed')
            exact(retained.get('identity'),p.queued['native_bundle']['workspace'],'FTP final queued runtime changed')
            p.clock()
            row=conn.execute(sa.select(jobs).where(db.REPO.scoped(jobs),jobs.c.id==expected['id'])).mappings().one()
            for key in ('id','request','revision','owner','token','attempt'):exact(row[key],expected[key],'FTP final durable identity changed')
            if row['status']!='running':raise InterruptedError('FTP cancelled during publication')
            workspace=conn.execute(sa.select(workspaces).where(db.REPO.scoped(workspaces))).mappings().one()
            exact(workspace['revision'],expected['revision'],'FTP final revision changed')
            if workspace['owner']!=expected['owner']or workspace['lease_until']<=time.time():raise InterruptedError('FTP final lease lost')
        row_check();output_recheck(True,conn);row_check();p.clock()
    db.REPO.complete(json.loads(p.private_job),result=output,guard=guard)
    return output
