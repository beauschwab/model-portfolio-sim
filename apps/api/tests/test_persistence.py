"""Durability, concurrency, publication and storage boundary integration gates.

Set TEST_POSTGRES_URL to run the same repository contracts on real PostgreSQL.
Each test creates a unique workspace; no existing data is modified.
"""
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
import datetime as dt
import io
import json
import os
from pathlib import Path
import time
import uuid

import numpy as np
import polars as pl
import pytest
import sqlalchemy as sa

from app import store, persistence as db
from app.artifacts import Codec, Objects
from app.repository import Conflict, Repository, workspaces
from app.storage_config import StorageConfig

BACKENDS = ['sqlite'] + (['postgres'] if os.getenv('TEST_POSTGRES_URL') else [])


@pytest.fixture(params=BACKENDS)
def repository(request, tmp_path):
    url = os.environ['TEST_POSTGRES_URL'] if request.param == 'postgres' else f'sqlite:///{tmp_path / "test.db"}'
    repo = Repository(url, 'tests', uuid.uuid4().hex)
    repo.migrate()
    repo.initialize({'snapshot': 'initial'})
    yield repo
    repo.engine.dispose()


def test_revision_cas_history_and_scope(repository):
    repo = repository
    assert repo.save_revision(0, {'snapshot': 'edited'}) == 1
    with pytest.raises(Conflict):
        repo.save_revision(0, {'snapshot': 'stale'})
    assert repo.revision()['manifest'] == {'snapshot': 'edited'}
    assert [r['revision'] for r in repo.history()] == [1, 0]
    other = Repository(repo.engine.url, 'another-tenant', repo.identity['workspace'])
    try:
        assert other.head() is None
        with pytest.raises(KeyError):
            other.revision(0)
    finally:
        other.engine.dispose()


def test_concurrent_edits_have_one_winner(repository):
    def edit(i):
        try:
            return repository.save_revision(0, {'value': i})
        except Conflict:
            return 'conflict'
    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(edit, [1, 2]))
    assert results.count(1) == 1 and results.count('conflict') == 1


def enqueue(repo, request=None, key=None):
    return repo.enqueue('pricing', 0, request or {'ref': 'input'}, {}, key=key)


def test_queue_idempotency_and_cancellation(repository):
    repo = repository
    jid = enqueue(repo, key='retry')
    assert enqueue(repo, key='retry') == jid
    with pytest.raises(Conflict):
        enqueue(repo, request={'different': True}, key='retry')
    assert repo.acquire('first')
    assert not repo.acquire('second')
    job = repo.claim('first')
    assert job['id'] == jid
    assert repo.cancel(jid)
    with pytest.raises(Conflict):
        repo.complete(job, result={'late': True})
    assert repo.job(jid)['status'] == 'error'


def test_queue_admission_and_attempt_limit(repository):
    from app.store import QueueFull
    repo = repository
    jid = repo.enqueue('pricing', 0, {'input': 1}, {}, max_queue=1)
    with pytest.raises(QueueFull):
        repo.enqueue('pricing', 0, {'input': 2}, {}, max_queue=1)
    for attempt in range(3):
        owner = f'worker-{attempt}'
        assert repo.acquire(owner)
        job = repo.claim(owner)
        assert job['attempt'] == attempt + 1
        repo.release(owner)
    assert repo.acquire('last-worker')
    assert repo.claim('last-worker') is None
    assert repo.job(jid)['status'] == 'error'
    assert repo.job(jid)['result'] is None


def test_crash_retry_fences_old_worker_and_survives_restart(repository):
    repo = repository
    jid = enqueue(repo)
    assert repo.acquire('old')
    first = repo.claim('old')
    with repo.engine.begin() as conn:
        conn.execute(workspaces.update().where(repo.scoped(workspaces)).values(lease_until=0.))
    assert not repo.renew('old')
    assert repo.acquire('new')
    second = repo.claim('new')
    assert second['id'] == jid and second['attempt'] == 2 and second['token'] != first['token']
    with pytest.raises(Conflict):
        repo.complete(first, result={'obsolete': True})
    repo.complete(second, result={'valid': True})
    reopened = Repository(repo.engine.url, **repo.identity)
    try:
        assert reopened.job(jid)['result'] == {'valid': True}
    finally:
        reopened.engine.dispose()


def test_stale_interactive_result_never_publishes(repository):
    repo = repository
    jid = enqueue(repo)
    repo.acquire('worker')
    job = repo.claim('worker')
    repo.save_revision(0, {'new': True})
    with pytest.raises(Conflict):
        repo.complete(job, result={'r': 1}, library={'cache': 1})
    assert repo.job(jid)['status'] == 'running' and repo.library() is None
    # Ordinary results are retained as historical results for their input revision.
    repo.complete(job, result={'historical': True})
    assert repo.job(jid)['revision'] == 0


def test_session_journal_commits_with_result_and_close_blocks_update(repository):
    repo = repository
    enqueue(repo)
    repo.acquire('worker')
    first = repo.claim('worker')
    repo.complete(first, result={'session_id': 'session'}, session=('session', 1, {'commands': []}))
    enqueue(repo, {'next': True})
    second = repo.claim('worker')
    repo.close_session('session')
    with pytest.raises(Conflict):
        repo.complete(second, result={'version': 2}, session=('session', 2, {'commands': [1]}))
    assert repo.session('session')['version'] == 1 and repo.job(second['id'])['status'] == 'running'


def test_typed_parquet_roundtrip_and_checksum(tmp_path):
    codec = Codec(Objects(str(tmp_path), 'tenant', 'workspace'))
    frame = pl.DataFrame({'id': ['x'], 'maturity': [dt.date(2030, 1, 1)]}).with_columns(
        pl.Series('call_schedule', [[(dt.date(2029, 1, 1), 100.)]], dtype=pl.Object))
    value = {'frame': frame, 'array': np.arange(12, dtype=np.float32).reshape(3, 4),
             'tuple': (dt.date(2026, 1, 1), {('a', 1): np.float64(4)}),
             '__arrow__': 3, 'nonfinite': float('nan')}
    ref = codec.dump(value)
    restored = codec.load(ref)
    assert restored['frame'].schema == frame.schema
    assert restored['frame'].to_dicts() == frame.to_dicts()
    np.testing.assert_array_equal(restored['array'], value['array'])
    assert restored['array'].dtype == np.float32 and restored['tuple'] == value['tuple']
    assert np.isnan(restored['nonfinite']) and restored['__arrow__'] == 3
    assert codec.tables(ref)['/frame']['format'] == 'parquet'
    with pytest.raises(ValueError, match='outside'):
        Codec(Objects(str(tmp_path), 'other-tenant', 'workspace')).load(ref)
    (tmp_path / ref['key']).write_bytes(b'corrupt')
    with pytest.raises(ValueError, match='checksum'):
        codec.load(ref)


def test_s3_objects_roundtrip_without_live_credentials(tmp_path):
    from botocore.stub import Stubber
    from botocore.response import StreamingBody
    import boto3
    client = boto3.client('s3', region_name='us-east-1', aws_access_key_id='test', aws_secret_access_key='test')
    objects = Objects('s3://example-bucket/outputs', 'tenant', 'workspace', s3_client=client)
    raw = b'parquet bytes'
    import hashlib
    digest = hashlib.sha256(raw).hexdigest()
    key = f'{objects.prefix}/{digest[:2]}/{digest}.parquet'
    with Stubber(client) as stub:
        stub.add_response('put_object', {}, {'Bucket': 'example-bucket', 'Key': key, 'Body': raw,
                                            'Metadata': {'sha256': digest}})
        ref = objects.put(raw, 'parquet')
        stub.add_response('get_object', {'Body': StreamingBody(io.BytesIO(raw), len(raw))},
                          {'Bucket': 'example-bucket', 'Key': key})
        assert objects.get(ref) == raw


@pytest.fixture(params=BACKENDS)
def durable(tmp_path, request):
    for mapping in (store.BOOKS, store.SCENARIOS, store.CACHE, store.PROGRAMS, store.JOBS, store.DECISIONS):
        mapping.clear()
    from app.schemas import RiskSettings
    store.SETTINGS = RiskSettings(n_paths=32, n_paths_base=32, n_threads=2, horizon_months=6)
    url = os.environ['TEST_POSTGRES_URL'] if request.param == 'postgres' else f'sqlite:///{tmp_path / "app.db"}'
    config = StorageConfig(url, str(tmp_path / 'objects'), tenant='tests', workspace=uuid.uuid4().hex)
    db.start(config)
    yield config
    from app import decision_store
    for sid in list(store.DECISIONS):
        decision_store.close(sid, durable=False)
    db.WORKER = None
    db.stop()


def test_demo_snapshot_survives_process_state_reload(durable):
    original = store.snapshot()
    store.replace_market(type('Market', (), {'swap_rates': [.05] * 10, 'vol_pts': original['market']['vol_pts'].tolist()})())
    assert db.REPO.head() == 1
    db.stop()
    store.BOOKS.clear()
    db.start(durable)
    assert store.STATE_META['revision'] == 1
    np.testing.assert_array_equal(store.MARKET['swap_rates'], [.05] * 10)
    for book, frame in original['books'].items():
        assert store.BOOKS[book].schema == frame.schema
        assert store.BOOKS[book].to_dicts() == frame.to_dicts()
    assert db.CODEC.load(db.REPO.revision(0)['manifest'])['asof'] == original['asof']


def test_failed_persistence_restores_memory(durable, monkeypatch):
    previous = store.SETTINGS
    def failure(*_args, **_kwargs):
        raise OSError('storage unavailable')
    monkeypatch.setattr(db.REPO, 'save_revision', failure)
    store.SETTINGS = previous.model_copy(update={'seed': 999})
    with pytest.raises(OSError):
        store.changed()
    assert store.SETTINGS == previous and store.STATE_META['revision'] == 0


def test_durable_pricing_job_and_duckdb_result(durable):
    from app import worker, reporting
    state = store.snapshot()
    jid = store.submit('pricing', store.run_pricing, ['loans'], state['market']['swap_rates'],
        state['market']['vol_pts'], 0., {}, {'include_analytics': False}, state=state)
    db.WORKER = 'test-worker'
    db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER))
    assert store.job_status(jid)['status'] == 'done', store.job_status(jid)
    before = store.job_result(jid)
    tables = db.CODEC.tables(db.result_ref(jid))
    assert tables
    result = reporting.table(jid, next(iter(tables)), limit=2)
    assert 0 < len(result['rows']) <= 2
    db.stop()
    db.start(durable)
    assert store.job_result(jid) == before


def test_http_revisions_idempotency_auth_and_conflicts(tmp_path, monkeypatch):
    from fastapi.testclient import TestClient
    from app.main import app
    monkeypatch.setenv('WORKBENCH_EXECUTION', 'external')
    monkeypatch.setenv('DATABASE_URL', f'sqlite:///{tmp_path / "http.db"}')
    monkeypatch.setenv('ARTIFACT_URL', str(tmp_path / 'objects'))
    monkeypatch.setenv('WORKBENCH_API_TOKEN', 'test-only')
    with TestClient(app) as client:
        assert client.get('/settings').status_code == 401
        client.headers['Authorization'] = 'Bearer test-only'
        settings = client.get('/settings').json()
        assert client.put('/settings', json=settings | {'seed': 123}, headers={'If-Match': '0'}).status_code == 200
        assert client.put('/settings', json=settings | {'seed': 321}, headers={'If-Match': '0'}).status_code == 409
        assert client.get('/settings').json()['seed'] == 123
        headers = {'Idempotency-Key': 'same-submission'}
        payload = {'kind': 'pricing', 'books': ['loans']}
        first = client.post('/run', json=payload, headers=headers)
        again = client.post('/run', json=payload, headers=headers)
        assert first.status_code == again.status_code == 200
        assert first.json()['id'] == again.json()['id']
        assert client.post('/run', json=payload | {'books': ['cds']}, headers=headers).status_code == 409
        assert len(client.get('/revisions').json()) == 2
        assert client.delete('/jobs/' + first.json()['id']).json()['cancelled']


def test_native_session_journal_rebuilds_committed_updates(durable):
    from app import worker, decision_store
    from app.schemas import DecisionEvalRequest, DecisionUpdateRequest, OptimizeRequest
    from portfolio_risk.strategy.decision import NativeDecision
    try:
        NativeDecision()
    except RuntimeError:
        pytest.skip('optional native library not built')
    options = OptimizeRequest(lcr_min=.01, nsfr_min=.01, cet1_min=.001, eve_limit=1.,
                              max_total_assets=1e7, cash_budget=1e7).model_dump()
    db.WORKER = 'session-worker'
    db.REPO.acquire(db.WORKER)
    jid = store.submit('decision_build', decision_store.build, options)
    worker.execute(db.REPO.claim(db.WORKER))
    assert store.job_status(jid)['status'] == 'done', store.job_status(jid)
    built = db.CODEC.load(db.result_ref(jid))
    sid = built['session_id']
    request = DecisionUpdateRequest(expected_revision=0, version=1,
        constraints=OptimizeRequest(**(options | {'max_total_assets': 5e6}))).model_dump()
    request['constraints'].pop('scenarios')
    updated = store.submit('decision_update', decision_store.update, sid, request)
    worker.execute(db.REPO.claim(db.WORKER))
    assert store.job_status(updated)['status'] == 'done', store.job_status(updated)
    output = db.CODEC.load(db.result_ref(updated))
    assert output['version'] == 2
    before = decision_store.evaluate(sid, DecisionEvalRequest(expected_revision=0, version=2, allocation=output['allocation']))
    worker.recover()
    after = decision_store.evaluate(sid, DecisionEvalRequest(expected_revision=0, version=2, allocation=output['allocation']))
    assert after == before
    assert db.REPO.session(sid)['version'] == 2
    decision_store.close(sid)
    worker.recover()
    assert sid not in store.DECISIONS


def test_research_snapshot_is_shared_through_repository(durable):
    from app import market_data
    payload = {'schema_version': market_data.VERSION, 'dataset': 'test', 'observations': [{'value': 3.}]}
    snap = market_data.save_snapshot(payload, [b'original source'])
    db.stop()
    db.start(durable)
    assert market_data.get_snapshot(snap['id']) == snap
    saved = db.CODEC.load(db.REPO.research(snap['id'])['manifest'])
    assert db.CODEC.objects.get(saved['raw_files'][0]) == b'original source'


def test_input_export_import_rewrites_workspace_scope(durable, tmp_path):
    import subprocess
    import sys
    env = os.environ | {
        'WORKBENCH_ENV': 'development', 'WORKBENCH_EXECUTION': 'external',
        'DATABASE_URL': durable.database_url, 'ARTIFACT_URL': durable.artifact_url,
        'WORKBENCH_TENANT_ID': durable.tenant, 'WORKBENCH_WORKSPACE_ID': durable.workspace,
    }
    folder = tmp_path / 'export'
    command = [sys.executable, '-m', 'app.storage_admin']
    subprocess.run(command + ['export', str(folder)], env=env, check=True, capture_output=True)
    destination = uuid.uuid4().hex
    env['WORKBENCH_WORKSPACE_ID'] = destination
    subprocess.run(command + ['import', str(folder)], env=env, check=True, capture_output=True)
    imported = Repository(durable.database_url, durable.tenant, destination)
    try:
        codec = Codec(Objects(durable.artifact_url, durable.tenant, destination))
        state = codec.load(imported.revision()['manifest'])
        assert state['books']['cds'].to_dicts() == store.BOOKS['cds'].to_dicts()
        assert state['revision'] == 0 and state['settings'] == store.SETTINGS
        result = subprocess.run(command + ['import', str(folder)], env=env, capture_output=True)
        assert result.returncode != 0 and imported.head() == 0
    finally:
        imported.engine.dispose()


def test_artifact_upload_failure_does_not_publish_job(durable, monkeypatch):
    from app import worker
    state = store.snapshot()
    jid = store.submit('pricing', store.run_pricing, ['loans'], state['market']['swap_rates'],
        state['market']['vol_pts'], 0., {}, {'include_analytics': False}, state=state)
    def unavailable(*args, **kwargs):
        raise OSError('object store unavailable')
    monkeypatch.setattr(db.CODEC.objects, 'put', unavailable)
    db.WORKER = 'upload-test'
    db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER))
    row = db.REPO.job(jid)
    assert row['status'] == 'error' and row['result'] is None
    assert 'object store unavailable' in row['detail']
