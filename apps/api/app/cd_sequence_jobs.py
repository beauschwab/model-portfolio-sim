"""Selected durable CD graph2 source, independent replay and private SQL fences.

No financial amounts or election behavior are calculated by this adapter.
"""
from copy import deepcopy
import hashlib,io,json,math,time
from pathlib import Path
from . import persistence as db
from .artifacts import Codec,ObjectReference
SCHEMAS=frozenset(('term-product-graph-2','term-product-reprice-2'))
MAX_RAW=2*1024*1024
MAX_OUTPUT=32*1024*1024
MAX_OBJECTS=1024

def _value(v):
    if isinstance(v,ObjectReference):return dict(v)
    if type(v)is dict:return {k:_value(x)for k,x in v.items()}
    if type(v)is list:return [_value(x)for x in v]
    return v

def _exact(a,b,check=lambda:False,name='immutable'):
    from portfolio_risk.analytics.cd_sequence_graph_reference import exact
    check();exact(_value(a),_value(b),name);check()

def _body(value):return json.dumps(value,allow_nan=False,separators=(',',':')).encode()

def queued_identity(check=lambda:False):
    from portfolio_risk.core import model_native,cd_sequence_native
    from portfolio_risk.analytics import cd_sequence_graph_reference
    from portfolio_risk.analytics.balance_stream import source_identity
    check();binary=model_native.binary_path().resolve()
    if not binary.is_file():raise RuntimeError('Rust CD graph2 runtime not built; rebuild; no fallback')
    paths=dict(runtime=Path(cd_sequence_native.__file__),reference=Path(cd_sequence_graph_reference.__file__),operation=Path(__file__),worker=Path(__file__).with_name('worker.py'),repository=Path(__file__).with_name('repository.py'),storage=Path(__file__).with_name('artifacts.py'),route=Path(__file__).with_name('model_routes.py'),dispatcher=Path(__file__).with_name('model_jobs.py'),tables=Path(__file__).with_name('model_tables.py'))
    code={name:cd_sequence_native._hash(path,check)for name,path in paths.items()}
    result=dict(artifact_kind='model_executable',binary_sha256=cd_sequence_native._hash(binary,check),binary_path=str(binary),source_sha256=source_identity(),code_sha256=code,workspace=db.current_identity())
    check();return result

def _lease(job,stopping):
    def cancelled():
        if stopping():return True
        row=db.REPO.job(job['id'])
        if stopping():return True
        if row['status']!='running' or row['token']!=job['token'] or row['owner']!=job['owner']:return True
        owned=db.REPO.owns(job['owner'])
        return stopping() or not owned
    return cancelled

def _budget(request,job,stopping,deadline=None):
    from portfolio_risk.core.indexed_book_job_native import JobBudget
    accepted=request.get('publication_deadline_unix')
    if type(accepted)not in(int,float)or not math.isfinite(accepted):raise ValueError('immutable accepted CD deadline required')
    if deadline is not None:_exact(accepted,deadline,name='original accepted deadline')
    return JobBudget(request['timeout'],_lease(job,stopping),deadline_unix=accepted)

def _fence(request,budget):
    budget.check();current=queued_identity(budget.checkpoint)
    _exact(current,request['native_bundle'],budget.checkpoint,'current CD source/executable/bundle')
    _exact(db.identity(),current['workspace'],budget.checkpoint,'worker startup bundle')
    budget.check()

def _descriptor(ref,kind):
    if not isinstance(ref,dict)or set(ref)!={'key','sha256','bytes','format'}or ref['format']!=kind or type(ref['bytes'])is not int or not 0<ref['bytes']<=MAX_OUTPUT or type(ref['sha256'])is not str or len(ref['sha256'])!=64:raise ValueError('bounded typed immutable CD object required')

class _Captured:
    def __init__(self,objects):self.objects=objects
    def get(self,ref):
        key=_body(dict(ref))
        if key not in self.objects:raise ValueError('unadmitted decoded object')
        return self.objects[key]

def load_bounded(ref,budget):
    """Admit compressed and logical tables before the existing Codec decoder.

    The decoder consumes privately captured verified bytes, preventing a second
    storage read from changing any object between prepass and source replay.
    """
    import pyarrow.parquet as pq
    import polars as pl
    from portfolio_risk.analytics._deposit_continuation_partition_bounds import parquet_metadata
    budget.check();_descriptor(ref,'json')
    body=db.CODEC.objects.get_bounded(ref,MAX_OUTPUT);budget.check()
    manifest=json.loads(body)
    if type(manifest)is not dict or set(manifest)!={'schema_version','value'}or type(manifest['schema_version'])is not int or manifest['schema_version']!=1:raise ValueError('typed CD persisted manifest')
    objects={_body(dict(ref)):body};compressed=len(body);decoded=0;logical=0;rows=0
    def scan(node):
        nonlocal compressed,decoded,logical,rows
        budget.checkpoint()
        if type(node)is not dict:
            if type(node)is list:raise ValueError('unexpected raw Codec list')
            return
        kind=node.get('type')
        if kind=='dict':
            if set(node)!={'type','items'}:raise ValueError('CD codec dictionary fields')
            for key,value in node['items']:scan(key);scan(value)
        elif kind in('list','tuple'):
            if set(node)!={'type','items'}:raise ValueError('CD codec sequence fields')
            for item in node['items']:scan(item)
        elif kind=='frame':
            if set(node)!={'type','ref','objects'}or node['objects']!={}:raise ValueError('CD source frames require ordinary typed columns')
            child=node['ref'];_descriptor(child,'parquet');key=_body(child)
            # Content-addressed Codec tables may legitimately share identical
            # object bytes. Charge every decoded appearance, including aliases.
            if key not in objects and len(objects)>=MAX_OBJECTS:raise ValueError('CD persisted object count cap')
            compressed+=child['bytes']
            if compressed>MAX_OUTPUT:raise ValueError('CD aggregate compressed object byte cap')
            data=objects[key] if key in objects else db.CODEC.objects.get_bounded(child,MAX_OUTPUT);budget.check()
            footer=pq.read_metadata(io.BytesIO(data))
            rows+=footer.num_rows
            if not 0<=footer.num_rows<=250000 or rows>250000:raise ValueError('CD source table aggregate row cap')
            schema={k:str(v)for k,v in pl.scan_parquet(io.BytesIO(data)).collect_schema().items()}
            decoded,logical=parquet_metadata(data,footer.num_rows,schema,decoded=decoded,logical=logical,check=budget.checkpoint)
            objects[key]=data
        else:raise ValueError('unsupported CD source Codec node')
    scan(manifest['value']);budget.check()
    value=Codec(_Captured(objects)).decode(manifest['value']);budget.check()
    return value,objects

def _queued(request,job,budget):
    # Metadata selects raw request/identity only; inherited workspace tables are
    # not loaded again by this analytical source guard.
    payload=db.CODEC.metadata(job['request'],{'operation','args','identity'})
    if payload['operation']!='store.run_model_analysis' or type(payload['args'])not in(list,tuple) or len(payload['args'])!=1:raise ValueError('one queued CD analytical operation required')
    _exact(payload['args'][0],request,budget.checkpoint,'immutable raw queued CD request')
    _exact(payload['identity'],request['native_bundle']['workspace'],budget.checkpoint,'queued bundle')
    budget.check();return payload

def run(owner,request,stopping):
    from portfolio_risk.core import cd_sequence_native
    from portfolio_risk.analytics.cd_sequence_graph_reference import verify_request
    from .model_tables import format_sources
    job=db.ACTIVE_JOB.get()
    if db.REPO is None or job is None or request['schema']not in SCHEMAS:raise RuntimeError('selected durable CD worker required')
    budget=_budget(request,job,stopping);_queued(request,job,budget);_fence(request,budget)
    owner.report('Running full supplied CD sequence source at original OAS',.1)
    result,execution=cd_sequence_native.run(request['schema'],request['request'],budget=budget,expected_binary=request['binary_sha256'],threads=request['threads'])
    receipt=verify_request(request['schema'],request['request'],result,check=budget.checkpoint)
    budget.check();formatted,layouts=format_sources(result);budget.check()
    execution.update(request_manifest=deepcopy(job['request']),source_sha256=request['source_sha256'],native_bundle=deepcopy(request['native_bundle']),publication_deadline_unix=request['publication_deadline_unix'],source_verification=receipt,table_layouts=layouts,empirical_validation=False,summary=dict(schema=request['schema'],complete_original_current_sources=True,original_oas_locked=True,full_tail_retained=True,accounting_adoption=False,capital_ftp_eligible=False,saved_library_cohort_adoption=False,empirical_validation=False),scope='Supplied CD source/risk analysis; no saved economics, stochastic behavior, effective-yield journal or capital/FTP adoption')
    _fence(request,budget)
    return dict(model_result=formatted,execution=execution,revision=job['revision'])

def verify_persisted(request,result,*,expected_deadline,request_manifest,job,stopping):
    from .model_tables import restore_sources,format_sources
    from portfolio_risk.analytics.cd_sequence_graph_reference import verify_request
    budget=_budget(request,job,stopping,expected_deadline)
    if set(result)!=set(('model_result','execution','revision')):raise ValueError('complete CD output fields')
    _exact(result['revision'],job['revision'],budget.checkpoint,'result original revision')
    execution=result['execution']
    if set(execution)!=set('schema backend binary_sha256 input_sha256 runtime_control caller_cooperative_native_control request_manifest source_sha256 native_bundle publication_deadline_unix source_verification table_layouts empirical_validation summary scope'.split()):raise ValueError('complete CD execution fields')
    expected=dict(schema=request['schema'],backend='rust',binary_sha256=request['binary_sha256'],source_sha256=request['source_sha256'],input_sha256=hashlib.sha256(_body(dict(schema=request['schema'],threads=request['threads'],request=request['request']))).hexdigest(),native_bundle=request['native_bundle'],request_manifest=request_manifest,publication_deadline_unix=expected_deadline,runtime_control='process-watchdog-1',caller_cooperative_native_control=False,empirical_validation=False)
    for field,value in expected.items():_exact(execution.get(field),value,budget.checkpoint,'execution.'+field)
    raw=restore_sources(result['model_result'],execution['table_layouts']);budget.check()
    receipt=verify_request(request['schema'],request['request'],raw,check=budget.checkpoint)
    _,layouts=format_sources(raw);budget.check()
    _exact(execution['table_layouts'],layouts,budget.checkpoint,'canonical complete source table layouts')
    _exact(execution['scope'],'Supplied CD source/risk analysis; no saved economics, stochastic behavior, effective-yield journal or capital/FTP adoption',budget.checkpoint,'analytical scope')
    _exact(execution.get('source_verification'),receipt,budget.checkpoint,'independent source receipt')
    _exact(execution['summary'],dict(schema=request['schema'],complete_original_current_sources=True,original_oas_locked=True,full_tail_retained=True,accounting_adoption=False,capital_ftp_eligible=False,saved_library_cohort_adoption=False,empirical_validation=False),budget.checkpoint,'analytical eligibility markers')
    _fence(request,budget);return receipt

def prepare_publication(request,output,*,payload_identity,expected_deadline,job,stopping):
    budget=_budget(request,job,stopping,expected_deadline);payload=_queued(request,job,budget)
    _exact(payload['identity'],payload_identity,budget.checkpoint,'private queued bundle')
    saved,objects=load_bounded(output,budget)
    verify_persisted(request,saved,expected_deadline=expected_deadline,request_manifest=job['request'],job=job,stopping=stopping)
    private_request,private_output,private_job,private_identity=map(deepcopy,(request,output,job,payload_identity))
    private_objects=deepcopy(objects)
    def recheck_objects():
        budget.check()
        for key,data in private_objects.items():
            ref=json.loads(key);_descriptor(ref,ref['format'])
            if db.CODEC.objects.get_bounded(ref,MAX_OUTPUT)!=data:raise ValueError('private persisted source object bytes changed')
            budget.check()
    def guard(connection):
        import sqlalchemy as sa
        from .repository import jobs,workspaces
        budget.check()
        def row_check():
            row=dict(connection.execute(sa.select(jobs).where(db.REPO.scoped(jobs),jobs.c.id==private_job['id'])).mappings().one())
            for key in('id','revision','request','owner','token','attempt'):_exact(row[key],private_job[key],budget.checkpoint,'private_job.'+key)
            if row['status']!='running':raise InterruptedError('CD SQL token/cancel lost')
            workspace=connection.execute(sa.select(workspaces).where(db.REPO.scoped(workspaces))).mappings().one()
            if workspace['owner']!=private_job['owner'] or workspace['lease_until']<=time.time():raise InterruptedError('CD SQL lease expired')
            _exact(workspace['revision'],private_job['revision'],budget.checkpoint,'SQL revision')
            return row
        row=row_check();_queued(private_request,row,budget)
        _exact(db.current_identity(),private_identity,budget.checkpoint,'SQL current bundle')
        _fence(private_request,budget);recheck_objects()
        _exact(private_request['publication_deadline_unix'],expected_deadline,budget.checkpoint,'SQL original deadline')
        row_check();budget.check()
    def publish():
        _exact(output,private_output,budget.checkpoint,'private output descriptor')
        db.REPO.complete(private_job,result=private_output,guard=guard)
    return publish
