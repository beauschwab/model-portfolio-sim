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
from .schemas import Allocation, DecisionEvalRequest

log = logging.getLogger(__name__)
READY = False
STOP = threading.Event()
BUSY = threading.Lock()
LEASE_SECONDS = max(3., float(os.getenv('WORKBENCH_LEASE_SECONDS', '60')))


@contextmanager
def computation(state):
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
        saved = db.CODEC.load(ref)
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
    payload = db.CODEC.load(job['request'])
    state = payload['state']
    with store._LOCK:
        store.JOBS[job['id']] = {'id': job['id'], 'status': 'running', 'progress': job['progress'],
                                '_t0': time.perf_counter()}
    store._CUR_JID = job['id']
    sid = None
    prior_library = store.CACHE.copy()
    try:
        if payload['identity'] != db.identity():
            raise RuntimeError('MODEL_VERSION_CHANGED: submit a new run using the deployed engine')
        db.refresh()
        with computation(state) as context:
            token = db.ACTIVE_JOB.set(job)
            try:
                result = db.resolve(payload['operation'])(*copy.deepcopy(payload['args']))
            finally:
                db.ACTIVE_JOB.reset(token)
            store.report(cache_hits=context.hits, cache_misses=context.misses)
        output = db.CODEC.dump(result)
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
        elif payload['operation'] == 'store.build_unitlib_job':
            library = db.CODEC.dump({'identity': payload['identity'], 'cache': store.CACHE})
        store.report(stage='done', pct=100.)
        db.REPO.progress(job, copy.deepcopy(store.JOBS[job['id']]['progress']))
        db.REPO.complete(job, result=output, session=session, library=library)
    except Exception as exc:
        if payload['operation'] == 'store.build_unitlib_job':
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
    inputs = store.input_fingerprints()
    with store._LOCK:
        return {'revision': store.STATE_META['revision'], 'worker_ready': True, 'inputs': inputs,
                'library_ready': bool(store.CACHE), 'library_horizon': store.CACHE.get('library', {}).get('horizon')}


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
