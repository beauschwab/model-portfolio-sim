"""Decision HTTP jobs, version guards, and unchanged strategy hot-path contract."""
import time
import pytest
from app import main, store, decision_store
from test_contracts import client, finished, skeleton


@pytest.fixture(autouse=True)
def native_available():
    from portfolio_risk.strategy.decision import NativeDecision
    try:
        NativeDecision()
    except RuntimeError:
        pytest.skip('optional native decision library not built')


def test_complete_decision_session_jobs(client):
    store.SETTINGS = main.RiskSettings(n_paths=32, n_paths_base=32, horizon_months=6, n_threads=2)
    revision = store.STATE_META['revision']
    options = dict(lcr_min=.01, nsfr_min=.01, cet1_min=.001, eve_limit=1., max_total_assets=1e7, cash_budget=1e7)
    assert client.post('/decision/sessions', json={'expected_revision': revision+1}).status_code == 409
    response = client.post('/decision/sessions', json={'expected_revision': revision, 'options': options})
    assert response.status_code == 200, response.text
    jid = response.json()['id']
    job = finished(jid)
    assert job['status'] == 'done', job
    result = skeleton(store.job_result(jid))
    sid = result['session_id']
    try:
        assert result['validated'] and result['version'] == 1
        payload = {'expected_revision': revision, 'version': 1, 'constraints': options | {'max_total_assets': 5e6}}
        update = client.post(f'/decision/sessions/{sid}/update', json=payload)
        jid = update.json()['id']
        assert finished(jid)['status'] == 'done', store.job_status(jid)
        newer = skeleton(store.job_result(jid))
        assert newer['work']['positions_repriced'] == 0 and newer['solver']['model_reused']
        assert newer['version'] == 2
        assert client.post(f'/decision/sessions/{sid}/update', json=payload).status_code == 409
        evaluated = client.post(f'/decision/sessions/{sid}/eval', json={'expected_revision': revision,
            'version': 2, 'allocation': newer['allocation']})
        assert evaluated.status_code == 200
        assert evaluated.json()['replay'] == newer['replay']
        with store._LOCK:
            store.changed()
        assert client.post(f'/decision/sessions/{sid}/update', json=payload | {'version': 2}).status_code == 409
    finally:
        assert client.delete(f'/decision/sessions/{sid}').status_code == 200
        assert sid not in store.DECISIONS


def test_snapshot_change_during_decision_publication_discards_candidate(client, monkeypatch):
    from portfolio_risk.strategy import decision
    store.SETTINGS = main.RiskSettings(n_paths=32, n_paths_base=32, horizon_months=6, n_threads=2)
    revision = store.STATE_META['revision']
    options = dict(lcr_min=.01, nsfr_min=.01, cet1_min=.001, eve_limit=1., max_total_assets=1e7, cash_budget=1e7)
    jid = client.post('/decision/sessions', json={'expected_revision': revision, 'options': options}).json()['id']
    assert finished(jid)['status'] == 'done', store.job_status(jid)
    sid = skeleton(store.job_result(jid))['session_id']
    session = store.DECISIONS[sid]['session']
    original = decision.validate_replay
    def edit_saved_input(*args):
        original(*args)
        with store._LOCK:
            store.changed()
    monkeypatch.setattr(decision, 'validate_replay', edit_saved_input)
    try:
        jid = client.post(f'/decision/sessions/{sid}/update', json={'expected_revision': revision, 'version': 1}).json()['id']
        job = finished(jid)
        assert job['status'] == 'error' and 'candidate discarded' in job['detail']
        assert session.version == 1
        assert not session._call('status')['pending']
    finally:
        decision_store.close(sid)
