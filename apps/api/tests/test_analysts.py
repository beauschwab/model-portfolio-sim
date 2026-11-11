"""Analyst authorization, durable replay, evidence bounds and cancellation contracts."""
import asyncio
import hashlib
import hmac
import os
import time
from types import SimpleNamespace

import httpx
import pytest
import sqlalchemy as sa

from app import analyst_routes as routes, persistence as db, main
from app.analyst_config import capability, principal, verify_capability
from app.analyst_repository import AnalystRepository, turns, evidence
from app.repository import Conflict, Repository, workspaces


@pytest.fixture
def analyst_store(tmp_path, monkeypatch):
    repo = Repository(f'sqlite:///{tmp_path / "analysts.db"}', 'test-tenant', 'test-workspace')
    repo.migrate(); repo.initialize({'test': True})
    ar = AnalystRepository(repo); ar.migrate()
    monkeypatch.setattr(db, 'REPO', repo)
    monkeypatch.setattr(db, 'CONFIG', SimpleNamespace(api_token='api-secret'))
    monkeypatch.setattr(db, 'refresh', lambda: None)
    monkeypatch.setenv('WORKBENCH_ANALYST_SECRET', 's'*48)
    monkeypatch.setenv('WORKBENCH_ANALYST_LOCAL_USER', 'alice')
    monkeypatch.setenv('WORKBENCH_ENV', 'development')
    monkeypatch.setattr(routes, 'configured', lambda: dict(version='1.57.0', sha256='a'*64,
        provider='test', model='fixture', executable='not-executed'))
    yield ar
    repo.engine.dispose()


def request(key='first', prompt='Inspect the original job'):
    return dict(request_id=key, workflow='job-recovery', prompt=prompt, expected_revision=0)


def test_conversation_ownership_idempotency_and_restart(analyst_store):
    ar = analyst_store
    conv = ar.create('alice', 'ops', 'Recovery')
    with pytest.raises(KeyError): ar.get(conv['id'], 'bob')
    first, created = ar.start(conv['id'], 'alice', request(), 0, {'skill': 'pinned'})
    assert created
    assert ar.start(conv['id'], 'alice', request(), 0, {}) == (first | ar.repo.identity, False)
    with pytest.raises(Conflict): ar.start(conv['id'], 'alice', request(prompt='Different'), 0, {})
    with pytest.raises(Conflict): ar.start(conv['id'], 'alice', request('second'), 0, {})
    eid = ar.reserve_tool(first['id'], 'get_jobs', {'query': {'idempotency_key': 'original'}})
    ar.record_tool(eid, {'ok': True, 'data': {'id': 'original-job', 'status': 'running'}})
    reopened = AnalystRepository(Repository(ar.repo.engine.url, 'test-tenant', 'test-workspace'))
    try:
        assert reopened.get(conv['id'], 'alice')['turns'][0]['status'] == 'running'
        assert reopened.read_evidence(conv['id'], 'alice', eid)['result']['data']['id'] == 'original-job'
        assert 'result' not in reopened.get(conv['id'], 'alice')['turns'][0]['evidence'][0]
        other = reopened.create('bob', 'ops', 'Other')
        with pytest.raises(KeyError): reopened.read_evidence(other['id'], 'bob', eid)
        reopened.finish(first['id'], 'cancelled')
        assert not ar.active(first['id'])
        with pytest.raises(Conflict): ar.reserve_tool(first['id'], 'get_state', {})
        ar.finish(first['id'], 'done', answer='late output')
        assert ar.get(conv['id'], 'alice')['turns'][0]['status'] == 'cancelled'
    finally:
        reopened.repo.engine.dispose()
    assert ar.repo.head() == 0 and ar.repo.list_jobs() == []


def test_deadline_tool_budget_and_final_fence(analyst_store):
    ar = analyst_store; conv = ar.create('alice', 'ops', 'Bounded')
    turn, _ = ar.start(conv['id'], 'alice', request(), 0, {})
    for _ in range(24): ar.reserve_tool(turn['id'], 'get_state', {})
    with pytest.raises(Conflict): ar.reserve_tool(turn['id'], 'get_state', {})
    with ar.repo.engine.begin() as conn:
        conn.execute(turns.update().where(turns.c.id == turn['id']).values(deadline=time.time()-1))
    ar.finish(turn['id'], 'done', answer='late')
    assert ar.get(conv['id'], 'alice')['turns'][0]['status'] == 'interrupted'


def test_evidence_integrity_citations_and_revision_fences(analyst_store):
    ar = analyst_store; conv = ar.create('alice', 'strategy', 'Evidence')
    turn, _ = ar.start(conv['id'], 'alice', request(), 0, {})
    eid = ar.reserve_tool(turn['id'], 'get_state', {})
    ar.record_tool(eid, {'ok': True, 'data': {'revision': 0}})
    with pytest.raises(Conflict): ar.record_tool(eid, {'ok': True, 'data': 'replacement'})
    ar.finish(turn['id'], 'done', answer=f'Workspace revision is 0 [{eid}].')
    assert ar.get(conv['id'], 'alice')['turns'][-1]['status'] == 'done'
    for key, answer in [('missing', 'Unsupported answer'), ('invented', '[Eunknown]')]:
        bad, _ = ar.start(conv['id'], 'alice', request(key), 0, {})
        ar.finish(bad['id'], 'done', answer=answer)
        assert ar.get(conv['id'], 'alice')['turns'][-1]['status'] == 'error'
    changed, _ = ar.start(conv['id'], 'alice', request('changed'), 0, {})
    other = ar.reserve_tool(changed['id'], 'get_state', {})
    ar.record_tool(other, {'ok': True, 'data': {'revision': 0}})
    with ar.repo.engine.begin() as conn:
        conn.execute(workspaces.update().where(ar.repo.scoped(workspaces)).values(revision=1))
        conn.execute(evidence.update().where(evidence.c.id == eid).values(result={'tampered': True}))
    with pytest.raises(Conflict): ar.read_evidence(conv['id'], 'alice', eid)
    with pytest.raises(Conflict): ar.reserve_tool(changed['id'], 'get_state', {})
    ar.finish(changed['id'], 'done', answer=f'Old revision [{other}].')
    assert ar.get(conv['id'], 'alice')['turns'][-1]['status'] == 'error'
    assert ar.start(conv['id'], 'alice', request('changed'), 0, {})[1] is False
    with pytest.raises(Conflict): ar.start(conv['id'], 'alice', request('new'), 0, {})


def test_workspace_admission_and_scope(analyst_store):
    ar = analyst_store
    for user in ('alice', 'bob'):
        conv = ar.create(user, 'ops', user)
        ar.start(conv['id'], user, request(), 0, {})
    third = ar.create('carol', 'ops', 'Third')
    with pytest.raises(Conflict): ar.start(third['id'], 'carol', request(), 0, {})
    other = AnalystRepository(Repository(ar.repo.engine.url, 'test-tenant', 'other-workspace'))
    try:
        assert other.list('alice') == []
        with pytest.raises(KeyError): other.get(conv['id'], 'bob')
    finally:
        other.repo.engine.dispose()


def test_concurrent_exact_retries_admit_one_turn(analyst_store):
    from concurrent.futures import ThreadPoolExecutor
    ar = analyst_store; conv = ar.create('alice', 'ops', 'Concurrent')
    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(lambda _: ar.start(conv['id'], 'alice', request(), 0, {}), range(2)))
    assert sum(created for _, created in results) == 1
    assert results[0][0]['id'] == results[1][0]['id']
    assert len(ar.get(conv['id'], 'alice')['turns']) == 1


def test_history_survives_disabled_inference_and_missing_tables_fail_cleanly(analyst_store, monkeypatch):
    from fastapi import HTTPException
    ar = analyst_store
    conv = ar.create('alice', 'ops', 'Retained')
    def disabled():
        raise HTTPException(503, 'Embedded analysts are disabled')
    monkeypatch.setattr(routes, 'configured', disabled)
    assert routes.status()['history_available'] is True
    assert routes.status()['available'] is False
    assert routes.repository().get(conv['id'], 'alice')['title'] == 'Retained'
    from app.analyst_repository import metadata
    metadata.drop_all(ar.repo.engine)
    assert routes.status()['history_available'] is False
    with pytest.raises(HTTPException) as error: routes.repository()
    assert error.value.status_code == 503


def test_scope_and_capability_expiry(analyst_store):
    scope = analyst_store.repo.identity
    token = capability({'id': 'turn', 'deadline': time.time()+30}, scope)
    assert verify_capability(token, scope) == 'turn'
    for value, other_scope in [(token+'x', scope), (token, scope | {'workspace': 'other'}),
            (capability({'id': 'turn', 'deadline': time.time()-1}, scope), scope)]:
        with pytest.raises(Exception) as exc: verify_capability(value, other_scope)
        assert exc.value.status_code == 401


def test_production_identity_requires_gateway_signature(analyst_store, monkeypatch):
    monkeypatch.setenv('WORKBENCH_ENV', 'production')
    scope = analyst_store.repo.identity
    timestamp = str(int(time.time()))
    req = SimpleNamespace(method='GET', url=SimpleNamespace(path='/analysts/conversations'), headers={
        'x-workbench-user': 'bob', 'x-workbench-identity-time': timestamp})
    with pytest.raises(Exception): principal(req, scope)
    signed = '\n'.join(['bob', timestamp, scope['tenant'], scope['workspace'], req.method, req.url.path])
    req.headers['x-workbench-identity-signature'] = hmac.new(b's'*48, signed.encode(), hashlib.sha256).hexdigest()
    assert principal(req, scope) == 'bob'
    req.headers['x-workbench-user'] = 'alice'
    with pytest.raises(Exception): principal(req, scope)


def test_tool_gateway_denies_writes_and_records_real_adapter_reads(analyst_store, monkeypatch):
    pytest.importorskip('workbench_mcp', reason='Install the analysts extra for gateway integration')
    from workbench_mcp.adapter import Adapter
    ar = analyst_store; conv = ar.create('alice', 'ops', 'Read')
    turn, _ = ar.start(conv['id'], 'alice', request(), 0, {})
    token = capability(turn, ar.repo.identity)
    calls = []
    async def read(self, name, args):
        calls.append((name, args))
        assert self.config.permissions == frozenset({'read'})
        return {'revision': 0, 'original_job': 'known'}
    monkeypatch.setattr(Adapter, 'call', read)
    async def check():
        async with httpx.AsyncClient(transport=httpx.ASGITransport(app=main.app), base_url='http://test') as client:
            url = '/analysts/tools/call'; headers = {'Authorization': 'Bearer '+token}
            assert (await client.post(url, json={'tool':'get_state','arguments':{}})).status_code == 401
            assert (await client.post(url, headers={'Authorization':'Bearer api-secret'}, json={'tool':'get_state','arguments':{}})).status_code == 401
            assert (await client.post(url, headers=headers, json={'tool':'post_run','arguments':{}})).status_code == 403
            assert (await client.post(url, headers=headers, json={'tool':'get_jobs','arguments':{'query':{'limit':101}}})).status_code == 422
            assert (await client.post(url, headers=headers, json={'tool':'get_jobs','arguments':{'query':[]}})).status_code == 422
            good = await client.post(url, headers=headers, json={'tool':'get_state','arguments':{}})
            assert good.status_code == 200, good.text
            saved = ar.read_evidence(conv['id'], 'alice', good.json()['evidence_id'])
            assert saved['sha256'] == good.json()['sha256'] and saved['result']['data']['original_job'] == 'known'
            ar.finish(turn['id'], 'cancelled')
            assert (await client.post(url, headers=headers, json={'tool':'get_state','arguments':{}})).status_code == 409
    asyncio.run(check())
    assert calls == [('get_state', {})]


def test_http_turn_reconnect_stop_and_private_history(analyst_store, monkeypatch):
    ar = analyst_store
    started = []
    async def execute(repo, turn, *args):
        started.append(turn['id'])
        try: await asyncio.Event().wait()
        except asyncio.CancelledError: pass
    monkeypatch.setattr(routes, 'execute', execute)
    async def check():
        async with httpx.AsyncClient(transport=httpx.ASGITransport(app=main.app), base_url='http://test',
                                    headers={'Authorization':'Bearer api-secret'}) as client:
            conv = (await client.post('/analysts/conversations', json={'role':'strategy','title':'Compare'})).json()
            url = f"/analysts/conversations/{conv['id']}"
            first = await client.post(url+'/turns', json=request())
            assert first.status_code == 200, first.text
            with ar.repo.engine.begin() as conn:
                conn.execute(workspaces.update().where(ar.repo.scoped(workspaces)).values(revision=1))
            assert (await client.post(url+'/turns', json=request())).json()['id'] == first.json()['id']
            assert (await client.post(url+'/turns', json=request('stale') | {'expected_revision':99})).status_code == 409
            stopped = await client.post(url+'/turns/'+first.json()['id']+'/stop')
            assert stopped.json()['financial_jobs_cancelled'] is False
            assert (await client.get(url)).json()['turns'][0]['status'] == 'cancelled'
            def disabled():
                raise AssertionError('Exact replay must not require an inference runtime')
            monkeypatch.setattr(routes, 'configured', disabled)
            assert (await client.post(url+'/turns', json=request())).json()['id'] == first.json()['id']
            monkeypatch.setenv('WORKBENCH_ANALYST_LOCAL_USER', 'bob')
            assert (await client.get(url)).status_code == 404
            assert (await client.get('/analysts/conversations')).json() == []
            await routes.stop()
    asyncio.run(check())
    assert len(started) == 1
