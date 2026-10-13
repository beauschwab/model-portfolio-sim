"""Private bounded commercial analytical source and final SQL publication guard."""
from copy import deepcopy
import json
import math
import time
from . import persistence as db
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity as exact

SCHEMAS=frozenset({'commercial-product-graph-1','commercial-units-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-generated-exercise-1','commercial-holding-continuation-1','nonintegral-fee-accounting-1','commercial-holding-journal-1','commercial-fee-journal-1','graph-cohort-audit-1'})


def _library_bounded(ref,budget):
    """Capture all numerical library leaves before decoding their private bytes."""
    import io
    import numpy as np
    import polars as pl
    import pyarrow.parquet as pq
    from .artifacts import Codec
    from .cd_sequence_jobs import _descriptor,_body,_Captured,MAX_OUTPUT,MAX_OBJECTS
    from portfolio_risk.analytics._deposit_continuation_partition_bounds import parquet_metadata
    budget.check();_descriptor(ref,'json')
    body=db.CODEC.objects.get_bounded(ref,MAX_OUTPUT);budget.check()
    manifest=json.loads(body)
    if type(manifest)is not dict or set(manifest)!={'schema_version','value'} or type(manifest['schema_version'])is not int or manifest['schema_version']!=1:raise ValueError('typed commercial library manifest required')
    captured={_body(dict(ref)):body};compressed=len(body);decoded=logical=rows=0;array_bytes=0
    def scan(node):
        nonlocal compressed,decoded,logical,rows,array_bytes
        budget.check()
        if type(node)is not dict:
            if type(node)is list:raise ValueError('unexpected raw commercial library Codec list')
            return
        kind=node.get('type')
        if kind in ('dict','list','tuple'):
            if set(node)!={'type','items'} or type(node['items'])is not list:raise ValueError('commercial library container fields')
            for item in node['items']:
                if kind=='dict':
                    if type(item)is not list or len(item)!=2:raise ValueError('commercial library dictionary pair')
                    scan(item[0]);scan(item[1])
                else:scan(item)
        elif kind=='array':
            if set(node)!={'type','dtype','shape','value'} or type(node['dtype'])is not str or type(node['shape'])is not list:raise ValueError('commercial library array fields')
            dtype=np.dtype(node['dtype']);shape=node['shape']
            if dtype.hasobject or dtype.kind not in 'biuf' or len(shape)>8 or any(type(x)is not int or not 0<=x<=250000 for x in shape):raise ValueError('bounded numerical commercial library array required')
            count=math.prod(shape);array_bytes+=count*dtype.itemsize
            if count>250000 or array_bytes>MAX_OUTPUT:raise ValueError('commercial library logical array byte cap')
            child=node['value']
            if type(child)is not dict or child.get('type')!='frame':raise ValueError('commercial library array requires typed numerical frame')
            scan(child)
            footer=pq.read_metadata(io.BytesIO(captured[_body(child['ref'])]))
            if footer.num_rows!=count or footer.num_columns!=1:raise ValueError('commercial library array shape differs from admitted frame')
        elif kind=='frame':
            if set(node)!={'type','ref','objects'} or node['objects']!={}:raise ValueError('commercial library frame columns must be ordinary typed data')
            child=node['ref'];_descriptor(child,'parquet');key=_body(child)
            if key not in captured and len(captured)>=MAX_OBJECTS:raise ValueError('commercial library object cap')
            compressed+=child['bytes']
            if compressed>MAX_OUTPUT:raise ValueError('commercial library aggregate compressed byte cap')
            data=captured.get(key)
            if data is None:data=db.CODEC.objects.get_bounded(child,MAX_OUTPUT)
            budget.check();footer=pq.read_metadata(io.BytesIO(data));rows+=footer.num_rows
            if not 0<=footer.num_rows<=250000 or rows>250000:raise ValueError('commercial library aggregate row cap')
            schema={k:str(v) for k,v in pl.scan_parquet(io.BytesIO(data)).collect_schema().items()}
            decoded,logical=parquet_metadata(data,footer.num_rows,schema,decoded=decoded,logical=logical,check=budget.checkpoint)
            captured[key]=data
        else:raise ValueError('unsupported commercial library Codec node')
    scan(manifest['value']);budget.check()
    saved=Codec(_Captured(captured)).decode(manifest['value']);budget.check()
    return saved,captured


def prepare_publication(request,output,*,payload_identity,expected_deadline,job,stopping,library=None):
    from .cd_sequence_jobs import load_bounded
    from .model_jobs import verify_persisted,verify_final_execution,verify_product_library
    if request.get('schema') not in SCHEMAS or type(expected_deadline) is not float or not math.isfinite(expected_deadline) or not exact(expected_deadline,request.get('publication_deadline_unix')):raise ValueError('commercial original analytical deadline required')
    if request['schema']=='graph-cohort-audit-1':
        cohorts=request['request'].get('cohorts')
        if type(cohorts)is not list or not cohorts or any(c['representative']['graph']['kind']!='commercial' or any(i['graph']['kind']!='commercial' for i in c['members']) for c in cohorts):raise ValueError('private commercial cohort publication requires complete exclusively commercial sources')
    raw,ref,identity,bound=map(deepcopy,(request,output,payload_identity,job))
    codec,objects,repo=db.CODEC,db.CODEC.objects,db.REPO
    class Budget:
        def check(self):
            if stopping():raise InterruptedError('commercial analytical publication cancelled')
            if time.time()>=expected_deadline:raise TimeoutError('commercial original analytical deadline exceeded')
        def checkpoint(self):self.check();return False
    budget=Budget();saved,captured=load_bounded(ref,budget)
    verify_persisted(raw,saved,expected_deadline=expected_deadline,request_manifest=bound['request'],job=bound,stopping=stopping)
    prior=None
    if 'prior_publication'in raw:
        if raw['schema']!='commercial-holding-journal-1':raise ValueError('native holding prior binding required')
        from . import commercial_prior_source as source
        from .completed_source_reader import exact as source_exact
        from .model_tables import restore_sources
        source_exact(source.materialize(raw['prior_publication'],raw['request']['revised_sources'],raw['request']['cut_ordinal']),raw['request'],'server-bound prior input changed')
        prior=source.reopen(repo,codec,raw['prior_publication'],budget.checkpoint)
        prior.match(restore_sources(saved['model_result'],saved['execution']['table_layouts']),budget.checkpoint)
        source_exact(prior.receipt(),saved['execution'].get('prior_opening_verification'),'persisted prior-opening evidence changed')
    library_ref=deepcopy(library)
    if library is not None:
        if request['schema']!='commercial-units-1':raise ValueError('commercial units required for library publication')
        selected,library_objects=_library_bounded(library_ref,budget)
        verify_product_library(raw,saved,selected,deadline=expected_deadline,job=bound,stopping=stopping)
        captured.update(library_objects)
    private=deepcopy(captured)
    def queued_metadata():
        budget.check();manifest=json.loads(objects.get_bounded(bound['request'],32*1024*1024));budget.check()
        if type(manifest.get('schema_version')) is not int or manifest['schema_version']!=1 or manifest['value'].get('type')!='dict':raise ValueError('commercial bounded queued manifest required')
        selected={key:codec.decode(value) for key,value in manifest['value']['items'] if type(key) is str and key in {'operation','args','identity'}}
        budget.check();return selected
    def guard(connection):
        import sqlalchemy as sa
        from .repository import jobs,workspaces
        budget.check()
        if db.CODEC is not codec or db.REPO is not repo or codec.objects is not objects or not exact(request,raw) or not exact(output,ref) or not exact(library,library_ref):raise ValueError('private commercial analytical owner changed')
        def owner():
            budget.check();row=connection.execute(sa.select(jobs).where(repo.scoped(jobs),jobs.c.id==bound['id'])).mappings().one()
            if row['status']!='running' or any(not exact(row[key],bound[key]) for key in ('owner','token','request','revision','attempt')):raise InterruptedError('commercial analytical queued authority changed')
            workspace=connection.execute(sa.select(workspaces).where(repo.scoped(workspaces))).mappings().one()
            if workspace['owner']!=bound['owner'] or workspace['lease_until']<=time.time():raise InterruptedError('commercial analytical lease expired')
            if not exact(workspace['revision'],bound['revision']):raise ValueError('commercial analytical SQL revision changed')
            queued=queued_metadata()
            operation='store.build_aged_unitlib_job' if library_ref is not None else 'store.run_model_analysis'
            if queued['operation']!=operation or len(queued['args'])!=1 or not exact(queued['args'][0],raw) or not exact(queued['identity'],identity):raise ValueError('commercial analytical raw queued identity changed')
        def artifacts():
            budget.check()
            if not exact(db.current_identity(),identity):raise RuntimeError('MODEL_VERSION_CHANGED: commercial analytical bundle changed')
            verify_final_execution(raw,deadline=expected_deadline,job=bound,stopping=stopping);budget.check()
        owner();artifacts()
        if prior is not None:prior.recheck(budget.checkpoint,connection)
        for key,data in private.items():
            budget.check()
            if objects.get_bounded(json.loads(key),32*1024*1024)!=data:raise ValueError('commercial verified analytical object changed')
            budget.check()
        if prior is not None:prior.recheck(budget.checkpoint,connection)
        artifacts();owner();budget.check()
    return guard
