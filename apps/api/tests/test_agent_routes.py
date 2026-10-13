"""Agent discovery and edits use the same durable revisions and scoped metadata."""
import pytest
from fastapi.testclient import TestClient

from app import main, store, persistence as db
from app.repository import Repository, sessions
from test_persistence import durable


def test_book_preview_patch_and_stale_revision(durable):
    client = TestClient(main.app)  # fixture owns lifespan and its isolated SQLite workspace
    page = client.get('/books/loans/rows', params={'limit': 1}).json()
    original = page['rows'][0]
    payload = {'expected_revision': 0, 'updates': {original['id']: {'price': original['price'] + 1}}}
    preview = client.post('/books/loans/preview', json=payload)
    assert preview.status_code == 200 and not preview.json()['committed']
    assert db.REPO.head() == 0
    changed = client.patch('/books/loans/rows', json=payload, headers={'If-Match': '0'})
    assert changed.status_code == 200 and changed.json()['revision'] == 1
    assert changed.json()['rebuild_required'] and not store.CACHE
    assert client.patch('/books/loans/rows', json=payload).status_code == 409
    selected = client.get('/books/loans/rows', params={'position_id': original['id'], 'columns': ['id', 'price']}).json()
    assert selected['rows'] == [{'id': original['id'], 'price': original['price'] + 1}]
    db.stop(); db.start(durable)
    assert store.STATE_META['revision'] == 1
    assert client.get('/books/loans/rows', params={'position_id': original['id']}).json()['rows'][0]['price'] == original['price'] + 1


def test_invalid_edits_and_pages_have_no_effect(durable):
    client = TestClient(main.app)
    row = client.get('/books/loans/rows', params={'limit': 1}).json()['rows'][0]
    for edits in [{row['id']: {'id': 'changed'}}, {'missing-id': {'price': 1}},
                  {row['id']: {'unknown': 1}}, {row['id']: {'price': -1}}]:
        response = client.patch('/books/loans/rows', json={'expected_revision': 0, 'updates': edits})
        assert response.status_code == 422, response.text
    assert db.REPO.head() == 0
    assert client.get('/books/loans/rows', params={'limit': 1001}).status_code == 422
    assert client.get('/books/loans/rows', params={'columns': 'missing'}).status_code == 422


def test_header_revision_fences_existing_routes_before_action(durable):
    client = TestClient(main.app)
    response = client.post('/run', json={'kind': 'pricing'}, headers={'If-Match': '9'})
    assert response.status_code == 409 and db.REPO.list_jobs() == []
    assert client.get('/jobs', headers={'If-Match': 'not-a-number'}).status_code == 400


def test_discovery_scoping_reconnect_and_cancel(durable):
    client = TestClient(main.app)
    jid = db.REPO.enqueue('pricing', 0, {'ref': 'fixture'}, {}, key='test')
    other = Repository(db.REPO.engine.url, 'different', durable.workspace)
    other.initialize({'fixture': True})
    other_jid = other.enqueue('pricing', 0, {'ref': 'other'}, {})
    try:
        rows = client.get('/jobs', params={'kind': 'pricing', 'status': 'queued'}).json()['rows']
        assert [r['id'] for r in rows] == [jid]
        assert 'request' not in rows[0] and 'result' not in rows[0]
        assert client.get('/jobs/' + other_jid).status_code == 404
        assert client.delete('/jobs/' + jid).json()['cancelled']
        assert client.get('/jobs', params={'status': 'queued'}).json()['rows'] == []
        db.stop(); db.start(durable)
        assert client.get('/jobs').json()['rows'][0]['id'] == jid
    finally:
        other.engine.dispose()


def test_session_metadata_never_recovers_or_loads_financial_state(durable, monkeypatch):
    client = TestClient(main.app)
    with db.REPO.engine.begin() as connection:
        connection.execute(sessions.insert().values(**db.REPO.identity, id='test-session', revision=0,
            version=3, closed=False, manifest={'must-not-be-loaded': True}))
    monkeypatch.setattr(db.CODEC, 'load', lambda _: (_ for _ in ()).throw(AssertionError('session hydrated')))
    row = client.get('/decision/sessions').json()['rows'][0]
    assert row['id'] == 'test-session' and not row['rebuild_required']
    assert client.get('/decision/sessions/test-session').json()['version'] == 3
    assert client.delete('/decision/sessions/test-session').json()['closed']
    assert client.get('/decision/sessions/test-session').json()['rebuild_required']
    assert client.get('/decision/sessions/missing').status_code == 404


def test_failed_commit_restores_book(durable, monkeypatch):
    client = TestClient(main.app, raise_server_exceptions=False)
    row = client.get('/books/loans/rows', params={'limit': 1}).json()['rows'][0]
    def failure(*a, **kw): raise OSError('storage unavailable')
    monkeypatch.setattr(db.REPO, 'save_revision', failure)
    response = client.patch('/books/loans/rows', json={'expected_revision': 0,
        'updates': {row['id']: {'price': row['price'] + 1}}})
    assert response.status_code == 500 and db.REPO.head() == 0
    assert client.get('/books/loans/rows', params={'position_id': row['id']}).json()['rows'][0]['price'] == row['price']


def test_exact_http_retry_retains_original_derived_deadline(durable, monkeypatch):
    state = store.snapshot()
    key_token = db.IDEMPOTENCY_KEY.set('retry-derived-input')
    hash_token = db.HTTP_REQUEST_SHA256.set('a' * 64)
    try:
        first = db.submit('model_analysis', 'store.run_model_analysis', ({'deadline': 1},), None, state)
        original_ref = db.REPO.job(first)['request']
        def forbidden(*args): raise AssertionError('Retry hydrated old books/results')
        monkeypatch.setattr(db.CODEC, 'load', forbidden)
        assert db.submit('model_analysis', 'store.run_model_analysis', ({'deadline': 2},), None, state) == first
        assert db.REPO.job(first)['request'] == original_ref
        assert len(db.REPO.list_jobs()) == 1
        changed = db.HTTP_REQUEST_SHA256.set('b' * 64)
        try:
            with pytest.raises(db.Conflict):
                db.submit('model_analysis', 'store.run_model_analysis', ({'deadline': 3},), None, state)
        finally:
            db.HTTP_REQUEST_SHA256.reset(changed)
    finally:
        db.IDEMPOTENCY_KEY.reset(key_token)
        db.HTTP_REQUEST_SHA256.reset(hash_token)


def test_submission_rechecks_revision_after_request_admission(durable):
    token = db.EXPECTED_REVISION.set(0)
    try:
        with pytest.raises(db.Conflict):
            db.submit('pricing', 'store.run_pricing', (), None, store.snapshot() | {'revision': 1})
        assert db.REPO.list_jobs() == []
    finally:
        db.EXPECTED_REVISION.reset(token)
