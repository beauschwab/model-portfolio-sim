"""Proposed durable job transport and independent publication checks only."""
from copy import deepcopy
import hashlib
import json
import time
from . import persistence as db

SCHEMA='market-indexed-books-1'
MAX_PERSISTED_BYTES=64*1024*1024


def queued_identity():
    """Hash the product library without loading it in the API process."""
    from portfolio_risk.core.native import library_path
    path=library_path()
    if not path.is_file():raise RuntimeError('NATIVE_GRAPH_CONTROL_REQUIRED: rebuild product library and restart')
    digest=hashlib.sha256()
    with path.open('rb') as stream:
        while block:=stream.read(1024*1024):digest.update(block)
    return dict(artifact_kind='product_library',binary_sha256=digest.hexdigest())


def _canonical(value):
    return json.dumps(value,allow_nan=False,ensure_ascii=False,separators=(',',':'))


def _lease(job,stopping):
    def cancelled():
        if stopping():return True
        row=db.REPO.job(job['id'])
        # STOP may arrive inside either SQL read. A final publication probe
        # must observe it before returning its lease result to the guard.
        if stopping():return True
        if row['status']!='running' or row['token']!=job['token'] or row['owner']!=job['owner']:return True
        owned=db.REPO.owns(job['owner'])
        if stopping():return True
        return not owned
    return cancelled


def _budget(request,job,stopping,*,deadline=None):
    from portfolio_risk.core.indexed_book_job_native import JobBudget
    accepted=request.get('publication_deadline_unix')
    if type(accepted) not in (int,float) or not __import__('math').isfinite(accepted):
        raise ValueError('immutable queue-accepted deadline required')
    budget=JobBudget(request['timeout'],_lease(job,stopping),deadline_unix=accepted)
    if deadline is not None and budget.publication_deadline_unix!=deadline:
        raise ValueError('persisted indexed deadline differs from original worker entry')
    return budget


def final_identity(request,budget):
    from portfolio_risk.core.native import library_path
    from portfolio_risk.core.indexed_book_job_native import _library_hash
    from portfolio_risk.analytics.balance_stream import source_identity
    budget.check()
    if request.get('artifact_kind')!='product_library':raise ValueError('product DLL identity required')
    if _library_hash(library_path(),budget)!=request['binary_sha256'] or source_identity()!=request['source_sha256']:
        raise RuntimeError('MODEL_VERSION_CHANGED: indexed source/current DLL changed')
    budget.check()


def run(owner,request,stopping):
    from portfolio_risk.core.indexed_book_job_native import run as calculate
    from portfolio_risk.analytics.indexed_book_job_reference import verify
    from .model_tables import format_sources
    job=db.ACTIVE_JOB.get()
    if db.REPO is None or job is None:raise RuntimeError('durable indexed worker required')
    budget=_budget(request,job,stopping)
    queued=db.CODEC.load(job['request'])
    from portfolio_risk.analytics.market_indexed_reference import _source_equal
    if queued['operation']!='store.run_model_analysis':raise ValueError('immutable indexed operation required')
    _source_equal(queued['args'][0],request,budget.checkpoint,'private_original_queue_request')
    final_identity(request,budget)
    owner.report('Running fully-bound supplied Pricing source',.1)
    raw=request['request']
    result,execution=calculate(raw,expected_library_sha256=request['binary_sha256'],budget=budget,threads=request['threads'])
    # Full independent arithmetic runs after native calculation, never through
    # the native lease callback, and before formatting/persistence.
    receipt=verify(raw,result,check=budget.checkpoint)
    budget.check()
    formatted,layouts=format_sources(result)
    budget.check()
    envelope=dict(schema=SCHEMA,threads=request['threads'],request=raw)
    execution.update(input_sha256=hashlib.sha256(_canonical(envelope).encode()).hexdigest(),
        raw_input_sha256=receipt['input_sha256'],request_manifest=job['request'],source_sha256=request['source_sha256'],
        source_verification=receipt,table_layouts=layouts,empirical_validation=False,
        summary=dict(schema=SCHEMA,pricing_only=True,capital_ftp_eligible=False,physical_accounting=False,
                     accounting_adoption=False,joint_dynamics=False,production_adoption=False,saved_workspace_adoption=False),
        scope='Fully-bound supplied Pricing analytical source; no saved-workspace/Physical/accounting/capital adoption')
    final_identity(request,budget)
    return dict(model_result=formatted,execution=execution,revision=owner.current_state()['revision'])


def verify_persisted(request,result,*,expected_deadline,request_manifest,job,stopping):
    from portfolio_risk.core.indexed_book_job_native import CACHE_BYTES,CACHE_ENTRIES,MAX_OUTPUT,_encode
    from portfolio_risk.analytics.indexed_book_job_reference import verify
    from portfolio_risk.analytics.market_indexed_reference import _source_equal
    from .model_tables import restore_sources
    budget=_budget(request,job,stopping,deadline=expected_deadline)
    execution=result['execution'];raw=request['request']
    budget.check()
    envelope=dict(schema=SCHEMA,threads=request['threads'],request=raw)
    command=dict(schema='graph-1',threads=request['threads'],request=dict(op='create_market_indexed_books',max_bytes=CACHE_BYTES,max_entries=CACHE_ENTRIES,request=raw))
    expected=dict(schema=SCHEMA,backend='rust',artifact_kind='product_library',binary_sha256=request['binary_sha256'],
        source_sha256=request['source_sha256'],threads=request['threads'],
        input_sha256=hashlib.sha256(_canonical(envelope).encode()).hexdigest(),
        # Actual term_call uses ensure_ascii=True for its unchanged JSON encoder.
        executed_input_sha256=hashlib.sha256(json.dumps(command,allow_nan=False,separators=(',',':')).encode()).hexdigest(),
        request_manifest=request_manifest,publication_deadline_unix=expected_deadline,
        runtime_control='graph-call-control-1',job_local_actor=True,fully_bound=True,
        saved_workspace_adoption=False,source_verification_required=True,empirical_validation=False)
    for field,value in expected.items():_source_equal(execution.get(field),value,budget.checkpoint,'execution.'+field)
    restored=restore_sources(result['model_result'],execution['table_layouts'])
    _encode(restored,budget,MAX_OUTPUT)
    receipt=verify(raw,restored,check=budget.checkpoint)
    _source_equal(execution.get('source_verification'),receipt,budget.checkpoint,'source_verification')
    _source_equal(execution.get('raw_input_sha256'),receipt['input_sha256'],budget.checkpoint,'raw_input_sha256')
    _source_equal(execution['summary'],dict(schema=SCHEMA,pricing_only=True,capital_ftp_eligible=False,physical_accounting=False,
        accounting_adoption=False,joint_dynamics=False,production_adoption=False,saved_workspace_adoption=False),budget.checkpoint,'summary')
    final_identity(request,budget)
    return receipt


def prepare_publication(request,output,*,payload_identity,expected_deadline,job,stopping):
    """Privately bind independently verified immutable inputs/output for SQL.

    The caller obtains this only after dump; no producer flag or returned
    deadline is used as authority. Each attempt reads the same sealed request.
    """
    from portfolio_risk.analytics.market_indexed_reference import _source_equal
    budget=_budget(request,job,stopping,deadline=expected_deadline)
    budget.check()
    queued_payload=db.CODEC.load(job['request'])
    if queued_payload['operation']!='store.run_model_analysis':raise ValueError('indexed immutable operation binding')
    _source_equal(queued_payload['args'][0],request,budget.checkpoint,'private_queued_request')
    _source_equal(queued_payload['identity'],payload_identity,budget.checkpoint,'private_bundle')
    saved=db.CODEC.load(output)
    verify_persisted(request,saved,expected_deadline=expected_deadline,request_manifest=job['request'],job=job,stopping=stopping)
    retained_request=deepcopy(request);retained_job=deepcopy(job)
    retained_identity=deepcopy(payload_identity);retained_output=deepcopy(output)
    retained_tables=deepcopy(db.CODEC.tables(output))

    def descriptor(ref,kind):
        if type(ref) is not dict and not isinstance(ref,dict):raise ValueError('typed object descriptor')
        if set(ref)!={'key','sha256','bytes','format'} or ref['format']!=kind or type(ref['bytes']) is not int or not 0<=ref['bytes']<=MAX_PERSISTED_BYTES:
            raise ValueError('bounded checksummed indexed object descriptor')

    def output_recheck():
        budget.check();descriptor(retained_output,'json')
        db.CODEC.objects.get(retained_output)
        tables=db.CODEC.tables(retained_output)
        _source_equal(tables,retained_tables,budget.checkpoint,'private_output_tables')
        total=retained_output['bytes']
        for table in retained_tables.values():
            parts=table.get('parts',[]) if table.get('format')=='partitioned-parquet' else [dict(ref=table)]
            for part in parts:
                budget.check();ref=part['ref'];descriptor(ref,'parquet')
                total+=ref['bytes']
                if total>MAX_PERSISTED_BYTES:raise ValueError('indexed persisted output aggregate bytes exceeded')
                db.CODEC.objects.get(ref);budget.check()
        budget.check()
    output_recheck()

    def guard(connection):
        import sqlalchemy as sa
        from .repository import jobs,workspaces
        budget.check()
        current=dict(connection.execute(sa.select(jobs).where(db.REPO.scoped(jobs),jobs.c.id==retained_job['id'])).mappings().one())
        for field in ['id','revision','request','owner','token','attempt']:
            _source_equal(current[field],retained_job[field],budget.checkpoint,'private_job.'+field)
        if current['status']!='running':raise InterruptedError('indexed job cancelled at SQL publication')
        fresh_payload=db.CODEC.load(current['request'])
        if fresh_payload['operation']!='store.run_model_analysis':raise ValueError('private operation drift')
        _source_equal(fresh_payload['args'][0],retained_request,budget.checkpoint,'private_request')
        _source_equal(fresh_payload['identity'],retained_identity,budget.checkpoint,'private_queued_bundle')
        _source_equal(db.current_identity(),retained_identity,budget.checkpoint,'private_current_bundle')
        final_identity(retained_request,budget)
        output_recheck()
        # Re-read durable authorities AFTER expensive code/object checks, using
        # the same transaction/connection. Repository.complete updates this row
        # only if the token still owns a running job.
        final=dict(connection.execute(sa.select(jobs).where(db.REPO.scoped(jobs),jobs.c.id==retained_job['id'])).mappings().one())
        for field in ['id','revision','request','owner','token','attempt']:
            _source_equal(final[field],retained_job[field],budget.checkpoint,'final_job.'+field)
        if final['status']!='running':raise InterruptedError('indexed final token lost')
        workspace=connection.execute(sa.select(workspaces).where(db.REPO.scoped(workspaces))).mappings().one()
        if workspace['owner']!=retained_job['owner'] or workspace['lease_until']<=time.time():raise InterruptedError('indexed final lease expired')
        _source_equal(workspace['revision'],retained_job['revision'],budget.checkpoint,'final_revision')
        # No fresh clock or result metadata can renew this queued authority.
        if retained_request['publication_deadline_unix']!=expected_deadline:raise ValueError('immutable queued deadline drift')
        budget.check()
    def publish():
        _source_equal(output,retained_output,budget.checkpoint,'private_returned_output_reference')
        # The committed result reference comes from this private verified
        # capture, not a replaceable caller/result metadata argument.
        db.REPO.complete(retained_job,result=retained_output,guard=guard)
    return publish
