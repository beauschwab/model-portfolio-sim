"""One leased compute actor per workspace, in a process separate from the API.

Run uvicorn app.worker:app --host 127.0.0.1 --port 8002 --workers 1.
Scale independent workspaces with independent workers. Kernels own core usage.
"""
from __future__ import annotations

from contextlib import asynccontextmanager, contextmanager
import copy
import hmac
import logging
import os
import threading
import time
import uuid

from fastapi import Depends, FastAPI, HTTPException, Request, Response
from . import store, decision_store, persistence as db
from . import operations
from .contracts import WorkspaceSnapshot, DurableRequest, SavedLibrary
from .schemas import Allocation, DecisionEvalRequest

log = logging.getLogger(__name__)
READY = False
STOP = threading.Event()
BUSY = threading.Lock()
LEASE_SECONDS = max(3., float(os.getenv('WORKBENCH_LEASE_SECONDS', '60')))


@contextmanager
def computation(state: WorkspaceSnapshot):
    import numba
    from portfolio_risk.core.runtime import RunConfig, run_context
    settings = state['settings']
    settings.require_production_backend()
    numba.set_num_threads(settings.n_threads or numba.config.NUMBA_NUM_THREADS)
    config = RunConfig(settings.n_paths, settings.n_paths_base, settings.horizon_months,
        state['assumptions'].get('deposit_segments'), tuple(state['assumptions'].get('cd_ew_params', [])) or None, compute_backend=settings.compute_backend)
    token = store._RUN_STATE.set(state)
    try:
        with run_context(config) as context:
            yield context
    finally:
        store._RUN_STATE.reset(token)


def recover():
    """Recreate warm state outside request handlers. Never price during eval."""
    db.refresh()
    for sid in list(store.DECISIONS):
        decision_store.close(sid, durable=False)
    ref = db.REPO.library()
    if ref:
        saved: SavedLibrary = db.CODEC.load(ref)
        if saved['identity'] == db.identity():
            store.CACHE.update(saved['cache'])
    for row in db.REPO.saved_sessions():
        saved = db.CODEC.load(row['manifest'])
        if saved['identity'] != db.identity():
            continue  # Explicit rebuild is required after model/code changes.
        with computation(saved['state']):
            built = decision_store.build(copy.deepcopy(saved['options']))
            temporary = built['session_id']
            with store._LOCK:
                store.DECISIONS[row['id']] = store.DECISIONS.pop(temporary)
            try:
                for update in saved['updates']:
                    decision_store.update(row['id'], copy.deepcopy(update))
                if store.DECISIONS[row['id']]['session'].version != row['version']:
                    raise RuntimeError('recovered session version mismatch')
            except BaseException:
                decision_store.close(row['id'], durable=False)
                raise


def execute(job):
    """Publish output, session journal and library reference in one transaction."""
    payload: DurableRequest = db.CODEC.load(job['request'])
    state = payload['state']
    with store._LOCK:
        store.JOBS[job['id']] = {'id': job['id'], 'status': 'running', 'progress': job['progress'],
                                '_t0': time.perf_counter()}
    store._CUR_JID = job['id']
    sid = None
    prior_library = store.CACHE.copy()
    deposit_guard=None
    deposit_job=(payload['operation']=='store.run_modeled_balance_stress' and
        payload['args'][0].get('specification',{}).get('schema') in {'deposit-performing-income-ledger-1','deposit-performing-portfolio-ledger-2'})
    accepted_deposit_deadline=payload['args'][0].get('publication_deadline_unix') if deposit_job else None
    commercial_job=(payload['operation']=='store.run_modeled_balance_stress' and payload['args'][0].get('specification',{}).get('schema')=='commercial-credit-ledger-1')
    commercial_guard=None
    accepted_commercial_deadline=payload['args'][0].get('publication_deadline_unix') if commercial_job else None
    commercial_library=payload['operation']=='store.build_aged_unitlib_job' and payload['args'][0].get('schema')=='commercial-units-1'
    from .commercial_selection import is_commercial_cohort
    from .commercial_analysis_jobs import SCHEMAS as commercial_analysis_schemas
    commercial_analysis=(payload['operation']=='store.run_model_analysis' and (is_commercial_cohort(payload['args'][0].get('schema'),payload['args'][0].get('request')) or payload['args'][0].get('schema') in commercial_analysis_schemas-{'graph-cohort-audit-1'})) or commercial_library
    commercial_analysis_guard=None
    accepted_commercial_analysis_deadline=payload['args'][0].get('publication_deadline_unix') if commercial_analysis else None
    cd_guard=None
    cd_job=payload['operation']=='store.run_model_analysis' and payload['args'][0].get('schema') in {'term-product-graph-2','term-product-reprice-2'}
    accepted_cd_deadline=payload['args'][0].get('publication_deadline_unix') if cd_job else None
    indexed_guard=None
    indexed_job=payload['operation']=='store.run_model_analysis' and payload['args'][0].get('schema')=='market-indexed-books-1'
    try:
        if payload['identity'] != db.identity():
            raise RuntimeError('MODEL_VERSION_CHANGED: submit a new run using the deployed engine')
        db.refresh()
        with computation(state) as context:
            token = db.ACTIVE_JOB.set(job)
            try:
                handler = operations.resolve(payload['operation'])
                if payload['operation']in ('saved_derivative.run','saved_derivative_deposit_portfolio.run'):
                    result = handler(*copy.deepcopy(payload['args']),stopping=STOP.is_set)
                else:
                    result = handler(*copy.deepcopy(payload['args']))
            finally:
                db.ACTIVE_JOB.reset(token)
            store.report(cache_hits=context.hits, cache_misses=context.misses)
        if payload['operation']=='saved_derivative_deposit_portfolio.run':
            from . import joint_deposit_derivative_jobs
            token=db.ACTIVE_JOB.set(job)
            try:
                joint_deposit_derivative_jobs.publish(result)
            finally:
                db.ACTIVE_JOB.reset(token)
            store.report(stage='done',pct=100.)
            return
        if payload['operation']=='saved_derivative.run':
            from . import saved_derivative_jobs
            token=db.ACTIVE_JOB.set(job)
            try:
                saved_derivative_jobs.publish(result)
            finally:
                db.ACTIVE_JOB.reset(token)
            store.report(stage='done',pct=100.)
            return
        if payload['operation']=='modeled_product_ftp.run':
            from . import modeled_product_ftp_jobs
            token=db.ACTIVE_JOB.set(job)
            try:
                modeled_product_ftp_jobs.publish(result)
            finally:
                db.ACTIVE_JOB.reset(token)
            store.report(stage='done',pct=100.)
            return
        if payload['operation']=='mortgage_capital.run':
            from . import mortgage_capital
            token=db.ACTIVE_JOB.set(job)
            try:
                mortgage_capital.publish(result)
            finally:
                db.ACTIVE_JOB.reset(token)
            store.report(stage='done',pct=100.)
            return
        model_publication = payload['operation'] in {'store.run_modeled_balance_stress',
            'store.run_model_analysis','store.build_aged_unitlib_job'}
        publication_deadline = accepted_commercial_analysis_deadline if commercial_analysis else accepted_commercial_deadline if commercial_job else accepted_cd_deadline if cd_job else accepted_deposit_deadline if deposit_job else result['execution']['publication_deadline_unix'] if model_publication else None
        if deposit_job or commercial_job:
            from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
            if not exact_json_identity(result['execution'].get('publication_deadline_unix'),publication_deadline):
                raise ValueError('deposit producer deadline changed from accepted budget')
        output = db.CODEC.dump(result)
        if payload['operation'] == 'store.run_modeled_balance_stress':
            from .model_jobs import verify_modeled_persisted
            token = db.ACTIVE_JOB.set(job)
            try:
                if commercial_job:
                    from .commercial_publication_jobs import prepare_publication
                    commercial_guard=prepare_publication(payload['args'][0],output,payload_identity=payload['identity'],
                        expected_deadline=publication_deadline,job=job,stopping=STOP.is_set)
                elif deposit_job:
                    from .deposit_continuation_jobs import prepare_publication
                    deposit_guard=prepare_publication(payload['args'][0],output,payload_identity=payload['identity'],
                        expected_deadline=publication_deadline,job=job,stopping=STOP.is_set)
                else:
                    verify_modeled_persisted(payload['args'][0], db.CODEC.load(output), job=job, stopping=STOP.is_set,
                        expected_deadline=publication_deadline)
            finally:
                db.ACTIVE_JOB.reset(token)
        if payload['operation'] in {'store.run_model_analysis','store.build_aged_unitlib_job'}:
            from .model_jobs import verify_persisted
            if commercial_analysis:
                from .commercial_analysis_jobs import prepare_publication
                if not commercial_library:
                    commercial_analysis_guard=prepare_publication(payload['args'][0],output,payload_identity=payload['identity'],expected_deadline=publication_deadline,job=job,stopping=STOP.is_set)
            elif cd_job:
                from .cd_sequence_jobs import prepare_publication
                cd_guard=prepare_publication(payload['args'][0],output,payload_identity=payload['identity'],expected_deadline=publication_deadline,job=job,stopping=STOP.is_set)
            elif indexed_job:
                from .indexed_book_jobs import prepare_publication
                indexed_guard=prepare_publication(payload['args'][0],output,payload_identity=payload['identity'],expected_deadline=publication_deadline,job=job,stopping=STOP.is_set)
            else:
                verify_persisted(payload['args'][0],db.CODEC.load(output),request_manifest=job['request'],
                    expected_deadline=publication_deadline,job=job,stopping=STOP.is_set)
            if STOP.is_set():raise InterruptedError('model source publication cancelled')

        session = library = None
        if payload['operation'] == 'decision.build':
            sid = result['session_id']
            journal = {'identity': payload['identity'], 'state': state,
                       'options': payload['args'][0], 'updates': []}
            session = (sid, result['version'], db.CODEC.dump(journal))
        elif payload['operation'] == 'decision.update':
            sid, request = payload['args']
            previous = db.REPO.session(sid)
            if previous is None or previous['closed']:
                raise RuntimeError('session is closed or unavailable')
            journal = db.CODEC.load(previous['manifest'])
            journal['updates'].append(request)
            session = (sid, result['version'], db.CODEC.dump(journal))
        elif payload['operation'] in ('store.build_unitlib_job','store.build_aged_unitlib_job'):
            library = db.CODEC.dump({'identity': payload['identity'], 'cache': store.CACHE})
            if payload['operation']=='store.build_aged_unitlib_job' and payload['args'][0]['schema'] in {'product-units-1','commercial-units-1'}:
                from .model_jobs import verify_product_library
                verify_product_library(payload['args'][0],db.CODEC.load(output),db.CODEC.load(library),
                    deadline=publication_deadline,job=job,stopping=STOP.is_set)
                if commercial_library:
                    from .commercial_analysis_jobs import prepare_publication
                    commercial_analysis_guard=prepare_publication(payload['args'][0],output,payload_identity=payload['identity'],expected_deadline=publication_deadline,job=job,stopping=STOP.is_set,library=library)
        store.report(stage='done', pct=100.)
        db.REPO.progress(job, copy.deepcopy(store.JOBS[job['id']]['progress']))
        if model_publication:
            from .model_jobs import verify_final_execution
            if db.current_identity()!=payload['identity']:
                raise RuntimeError('MODEL_VERSION_CHANGED: deployed engine bundle changed at final publication')
            verify_final_execution(payload['args'][0],deadline=publication_deadline,
                job=job,stopping=STOP.is_set)
        if cd_job:
            if cd_guard is None:raise RuntimeError('missing private CD source publication guard')
            cd_guard()
        elif indexed_job:
            if indexed_guard is None:raise RuntimeError('missing private indexed publication guard')
            indexed_guard()
        elif commercial_analysis:
            if commercial_analysis_guard is None:raise RuntimeError('missing private commercial analytical publication guard')
            db.REPO.complete(job,result=output,session=session,library=library,guard=commercial_analysis_guard)
        elif commercial_job:
            if commercial_guard is None:raise RuntimeError('missing private commercial publication guard')
            db.REPO.complete(job,result=output,session=session,library=library,guard=commercial_guard)
        elif deposit_job:
            if deposit_guard is None:raise RuntimeError('missing private deposit publication guard')
            db.REPO.complete(job, result=output, session=session, library=library,guard=deposit_guard)
        else:
            db.REPO.complete(job, result=output, session=session, library=library)
    except Exception as exc:
        if payload['operation'] in ('store.build_unitlib_job','store.build_aged_unitlib_job'):
            store.CACHE.clear()
            if prior_library.get('revision') == db.REPO.head():
                store.CACHE.update(prior_library)
        if payload['operation'] == 'decision.update':
            sid = payload['args'][0]
        if sid:
            decision_store.close(sid, durable=False)
        try:
            db.REPO.complete(job, error=f'{type(exc).__name__}: {exc}')
        except db.Conflict:
            pass  # Cancellation or a replacement owner already won publication.
        log.exception('Job %s failed', job['id'])
        # Restore the last committed session after a failed/aborted mutation.
        if sid and db.REPO.owns(db.WORKER):
            recover()
    finally:
        store._CUR_JID = None
        with store._LOCK:
            store.JOBS.pop(job['id'], None)


def heartbeat():
    while not STOP.wait(min(2., LEASE_SECONDS / 3)):
        try:
            if not db.REPO.renew(db.WORKER, LEASE_SECONDS):
                STOP.set()
                return
            jid = store._CUR_JID
            if jid:
                with store._LOCK:
                    progress = copy.deepcopy(store.JOBS.get(jid, {}).get('progress', {}))
                row = db.REPO.job(jid)
                if row['owner'] == db.WORKER:
                    db.REPO.progress(row, progress)
        except Exception:
            log.exception('Worker heartbeat failed')


def loop():
    global READY
    try:
        if not db.REPO.acquire(db.WORKER, LEASE_SECONDS):
            raise RuntimeError('Another worker owns this workspace; stop it or wait for its lease to expire')
        recover()
        READY = True
        while not STOP.is_set():
            if not db.REPO.owns(db.WORKER):
                READY = False
                break
            for sid in list(store.DECISIONS):
                saved = db.REPO.session(sid)
                if saved and saved['closed']:
                    decision_store.close(sid, durable=False)
            job = db.REPO.claim(db.WORKER)
            if job:
                with BUSY:
                    execute(job)
            else:
                STOP.wait(.2)
    except Exception:
        log.exception('Worker stopped')
    finally:
        READY = False
        STOP.set()


@asynccontextmanager
async def lifespan(_app):
    global READY
    db.start()
    if db.REPO is None:
        raise RuntimeError('Worker requires durable execution mode')
    db.WORKER = uuid.uuid4().hex
    STOP.clear()
    worker = threading.Thread(target=loop, name='quant-worker', daemon=True)
    pulse = threading.Thread(target=heartbeat, name='quant-heartbeat', daemon=True)
    worker.start()
    pulse.start()
    try:
        yield
    finally:
        READY = False
        STOP.set()
        # Native kernels cannot be safely interrupted. Process termination lets
        # the lease expire and the next worker retry, with publication fenced.
        worker.join(timeout=1)
        pulse.join(timeout=1)
        if not worker.is_alive():
            db.REPO.release(db.WORKER)
            for sid in list(store.DECISIONS):
                decision_store.close(sid, durable=False)
            db.stop()
            db.WORKER = None


def authorized(request: Request):
    if db.CONFIG.worker_token and not hmac.compare_digest(
            request.headers.get('authorization', ''), f'Bearer {db.CONFIG.worker_token}'):
        raise HTTPException(401, 'Worker authentication required')
    if not READY or not db.REPO.owns(db.WORKER):
        raise HTTPException(503, 'Worker is recovering or unavailable')
    db.refresh()


app = FastAPI(title='Rates calculation worker', lifespan=lifespan, dependencies=[Depends(authorized)])


@app.exception_handler(RuntimeError)
async def conflict(_request, exc):
    from fastapi.responses import JSONResponse
    return JSONResponse(status_code=409, content={'detail': str(exc)})


@app.exception_handler(KeyError)
async def missing(_request, exc):
    from fastapi.responses import JSONResponse
    return JSONResponse(status_code=404, content={'detail': str(exc)})


@app.exception_handler(ValueError)
async def invalid(_request, exc):
    from fastapi.responses import JSONResponse
    return JSONResponse(status_code=422, content={'detail': str(exc)})


@app.get('/state')
def state():
    with store._LOCK:
        return {'revision': store.STATE_META['revision'], 'worker_ready': True,
                'library_ready': bool(store.CACHE), 'library_horizon': store.CACHE.get('library', {}).get('horizon'),
                **store.library_metadata()}


@app.post('/strategy/eval')
def strategy(allocations: list[Allocation]):
    if not BUSY.acquire(blocking=False):
        raise HTTPException(409, 'Worker is calculating; retry evaluation after completion')
    try:
        revision = store.STATE_META['revision']
        result = store.eval_strategy_sync([a.model_dump() for a in allocations])
        db.assert_current(revision)
        return Response(store.to_arrow_envelope(result), media_type=store.ARROW_ENVELOPE_MIME)
    finally:
        BUSY.release()


@app.post('/decision/{sid}/check')
def check(sid: str, request: dict):
    if BUSY.locked():
        raise HTTPException(409, 'Worker is calculating; retry after completion')
    item = decision_store.entry(sid, request['expected_revision'])
    if item['session'].version != request['version']:
        raise RuntimeError('stale session version')
    return {'ok': True}


@app.post('/decision/{sid}/eval')
def evaluate(sid: str, request: DecisionEvalRequest):
    # Never expose a candidate before its durable journal commits.
    if not BUSY.acquire(blocking=False):
        raise HTTPException(409, 'Worker is calculating; retry evaluation after completion')
    try:
        return decision_store.evaluate(sid, request)
    finally:
        BUSY.release()
