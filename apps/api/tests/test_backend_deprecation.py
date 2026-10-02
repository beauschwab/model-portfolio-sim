"""Historical Python snapshots can be inspected but never executed by the worker."""
from app import store, persistence as db, worker
from test_persistence import durable, BACKENDS


def test_worker_rejects_legacy_python_snapshot_without_publication(durable):
    state=store.snapshot()
    state['settings']=state['settings'].model_copy(update={'compute_backend':'python'})
    saved=db.CODEC.dump(state)
    assert db.CODEC.load(saved)['settings'].compute_backend=='python'
    # Bypass new-request admission to emulate an already queued historical job.
    jid=db.submit('pricing',store.run_pricing,(['loans'],state['market']['swap_rates'],state['market']['vol_pts']),None,state)
    db.WORKER='deprecated-backend-test';worker.STOP.clear()
    assert db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER))
    row=db.REPO.job(jid)
    assert row['status']=='error' and row['result'] is None
    assert 'PYTHON_BACKEND_DEPRECATED' in row['detail']
