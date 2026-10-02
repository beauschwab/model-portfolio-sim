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
