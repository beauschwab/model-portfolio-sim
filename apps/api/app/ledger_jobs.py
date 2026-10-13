"""Ledger jobs; the caller supplies the sole state owner.

Functions transport/format engine outputs. Financial calculations stay in Rust.
"""
from __future__ import annotations

import time

def run_balance_stress(owner, specification):
    from portfolio_risk.analytics.balance_stress import run_balance_stress as simulate
    result = simulate(specification, progress=lambda stage, pct: owner.report(stage=stage, pct=pct))
    result["revision"] = owner.current_state()["revision"]
    return result


def run_treasury(owner, specification):
    from portfolio_risk.analytics.treasury import evaluate
    owner.report('Calculating capital ratios and FTP reconciliation', 0.1)
    result = evaluate(specification)
    result['revision'] = owner.current_state()['revision']
    owner.report('Capital and FTP report complete', 1.)
    return result


def run_saved_balance_stress(owner, request):
    from portfolio_risk.analytics.balance_workflow import run_saved_book_stress
    from . import persistence as db
    state = owner.current_state()
    expected = None
    if db.ACTIVE_JOB.get() is not None and state['settings'].compute_backend == 'rust':
        from portfolio_risk.analytics.owned_workflow import binary_path
        from portfolio_risk.analytics.balance_stream import _hash
        expected = db.identity()['workflow_native_sha256']
        binary = binary_path()
        if expected is None or not binary.is_file() or _hash(binary) != expected:
            raise RuntimeError('MODEL_VERSION_CHANGED: Rust workflow binary differs from queued identity')
    result = run_saved_book_stress(owner.balance_sheet(state), state['market']['swap_rates'],
        state['market']['vol_pts'], state['dep_hist'], request, seed=state['settings'].seed,
        asof=state['asof'], progress=lambda stage, pct: owner.report(stage=stage, pct=pct))
    if expected is not None and result['execution']['journal_identity'] != expected:
        raise RuntimeError('MODEL_VERSION_CHANGED: executed workflow binary differs from queued identity')
    result['revision'] = state['revision']
    return result


def run_streamed_balance_stress(owner, request):
    """Worker-only engine adapter; persist partitions without materializing tables."""
    if request['backend'] != 'rust':
        raise ValueError('PYTHON_BACKEND_DEPRECATED: submit a new Rust simulation')
    import tempfile
    from portfolio_risk.analytics.balance_stream import run_streamed_balance_stress as simulate, binary_path, _hash
    from . import persistence as db, worker
    job = db.ACTIVE_JOB.get()
    if db.REPO is None or job is None:
        raise RuntimeError('Partitioned simulation requires a durable worker')
    expected = request['binary_sha256']
    if request['backend'] == 'rust' and (not binary_path().is_file() or _hash(binary_path()) != expected):
        raise RuntimeError('MODEL_VERSION_CHANGED: Rust state binary differs from queued identity')
    checked = [0., False]
    def cancelled():
        # Native watchdog and writer both call this; duplicate read-only probes
        # are harmless. SQL complete() remains the authoritative final fence.
        if worker.STOP.is_set():
            return True
        now = time.monotonic()
        if now - checked[0] >= .2:
            row = db.REPO.job(job['id'])
            checked[1] = (row['status'] != 'running' or row['token'] != job['token']
                          or row['owner'] != job['owner'] or not db.REPO.owns(job['owner']))
            checked[0] = now
        return checked[1]
    from .maintenance import scratch_root
    scratch = scratch_root(db.CONFIG, db.CODEC.objects)
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='balance-', dir=scratch) as directory:
        manifest = simulate(request['specification'], directory, backend=request['backend'],
                            large_book=request['large_book'], cancelled=cancelled,
                            progress=lambda stage: owner.report(stage=stage))
        result = db.CODEC.import_balance_partitions(manifest, cancelled)
        if request['backend'] == 'rust' and result['execution']['binary_sha256'] != expected:
            raise RuntimeError('MODEL_VERSION_CHANGED: executed binary differs from queued identity')
    # The immutable queued request already owns the complete input. Keeping it
    # inline here makes every table page re-read a portfolio-sized manifest.
    result['execution'].pop('specification')
    result['execution']['request_manifest'] = job['request']
    result['revision'] = owner.current_state()['revision']
    return result


def run_model_analysis(owner,request):
    from . import worker
    from .model_jobs import run
    return run(owner,request,worker.STOP.is_set)


def build_aged_unitlib(owner,request):
    from . import worker
    from .model_jobs import build_aged
    return build_aged(owner,request,worker.STOP.is_set)


def run_dated_balance_stress(owner, request):
    """Durable native dated execution, with queued binary and validator fencing."""
    from portfolio_risk.analytics.dated_ledger import run_dated_ledger
    return _run_source_balance_stress(owner,request,run_dated_ledger,'import_dated_partitions')


def run_credit_balance_stress(owner, request):
    """Transport the explicit native credit route through the durable worker."""
    from portfolio_risk.analytics.credit_ledger import run_credit_ledger
    return _run_source_balance_stress(owner,request,run_credit_ledger,'import_credit_partitions')


def run_credit_path_balance_stress(owner, request):
    """Transport the explicit realized-path journal through the durable worker."""
    from portfolio_risk.analytics.credit_path_ledger import run_credit_path_ledger
    return _run_source_balance_stress(owner,request,run_credit_path_ledger,'import_credit_path_partitions')


def _run_source_balance_stress(owner, request, simulate, importer, *, executable=None):
    import tempfile
    from portfolio_risk.analytics.balance_stream import binary_path, _hash
    from portfolio_risk.analytics.ledger_native import library_path
    from . import persistence as db, worker
    from .maintenance import scratch_root
    job = db.ACTIVE_JOB.get()
    if db.REPO is None or job is None:
        raise RuntimeError('Dated simulation requires a durable worker')
    for path, expected in [(executable or binary_path(), request['binary_sha256']),
                           (library_path(), request['validation_library_sha256'])]:
        if not path.is_file() or _hash(path) != expected:
            raise RuntimeError('MODEL_VERSION_CHANGED: dated native artifacts differ from queued identity')
    if executable is not None:
        from portfolio_risk.analytics.balance_stream import source_identity
        if source_identity()!=request['source_sha256']:raise RuntimeError('MODEL_VERSION_CHANGED: modeled source differs from queued identity')
    # One budget covers native execution, uploads, persisted replay and publication.
    continuation = request.get('specification',{}).get('schema') in {'deposit-performing-income-ledger-1','deposit-performing-portfolio-ledger-2','commercial-credit-ledger-1'}
    if continuation:
        import math
        publication_deadline=request.get('publication_deadline_unix')
        if type(publication_deadline) is not float or not math.isfinite(publication_deadline):
            raise ValueError('trusted accepted deposit publication deadline required')
    else:
        publication_deadline = time.time() + 1800. if executable is not None else None
    checked = [0., False]
    def cancelled():
        if publication_deadline is not None and time.time() >= publication_deadline:
            raise TimeoutError('modeled ledger publication deadline exceeded')
        if worker.STOP.is_set(): return True
        now = time.monotonic()
        if now - checked[0] >= .2:
            row = db.REPO.job(job['id'])
            checked[1] = (row['status'] != 'running' or row['token'] != job['token'] or
                          row['owner'] != job['owner'] or not db.REPO.owns(job['owner']))
            checked[0] = now
        return checked[1]
    scratch = scratch_root(db.CONFIG, db.CODEC.objects)
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='dated-', dir=scratch) as directory:
        if cancelled():raise InterruptedError('dated publication ownership lost')
        budget = {'timeout':publication_deadline-time.time()} if continuation else {}
        manifest = simulate(request['specification'], directory,
            large_book=request['large_book'], cancelled=cancelled,
            progress=lambda stage: owner.report(stage=stage), **budget)
        result = getattr(db.CODEC,importer)(manifest, cancelled)
        execution = result['execution']
        if (execution['binary_sha256'] != request['binary_sha256'] or
                execution['validation_library_sha256'] != request['validation_library_sha256']):
            raise RuntimeError('MODEL_VERSION_CHANGED: executed dated artifacts differ from queued identity')
    if executable is not None and (execution['source_sha256']!=request['source_sha256'] or source_identity()!=request['source_sha256']):
        raise RuntimeError('MODEL_VERSION_CHANGED: modeled source changed before publication')
    if executable is not None:
        execution['publication_deadline_unix'] = publication_deadline
        for path, expected in [(executable, request['binary_sha256']),
                               (library_path(), request['validation_library_sha256'])]:
            if not path.is_file() or _hash(path) != expected:
                raise RuntimeError('MODEL_VERSION_CHANGED: modeled artifacts changed during import')
    result['execution'].pop('specification')
    result['execution']['request_manifest'] = job['request']
    result['revision'] = owner.current_state()['revision']
    return result



def run_modeled_balance_stress(owner,request):
    from portfolio_risk.analytics.model_ledger import run_model_ledger,binary_path
    importer='import_commercial_partitions' if request['specification']['schema']=='commercial-credit-ledger-1' else 'import_deposit_continuation_partitions' if request['specification']['schema'] in {'deposit-performing-income-ledger-1','deposit-performing-portfolio-ledger-2'} else 'import_credit_path_partitions' if request['specification']['schema']=='driven-credit-ledger-1' else 'import_dated_partitions'
    return _run_source_balance_stress(owner,request,run_model_ledger,importer,executable=binary_path())
