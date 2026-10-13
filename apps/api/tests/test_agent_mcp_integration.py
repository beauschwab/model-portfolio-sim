"""Run with `uv run --with-editable ../mcp ...`; MCP -> real API -> durable worker."""
import asyncio
import base64

import httpx
import pytest

pytest.importorskip('workbench_mcp', reason='Run the dedicated test-mcp integration target')
from mcp.shared.memory import create_connected_server_and_client_session
from workbench_mcp.adapter import Adapter, Config, PERMISSIONS
from workbench_mcp.server import create_server

from app import main, store, worker, persistence as db
from test_persistence import durable


def test_mcp_import_worker_publication_reconnect_and_edit(durable):
    async def journey():
        adapter = Adapter(Config(profile='ops', permissions=PERMISSIONS), transport=httpx.ASGITransport(app=main.app))
        async with create_connected_server_and_client_session(create_server(adapter)) as client:
            async def call(name, args=None):
                result = await client.call_tool(name, args or {})
                assert not result.isError, result.structuredContent
                return result.structuredContent['data']
            example = await call('get_cohorts_example')
            args = {'expected_revision': 0, 'idempotency_key': 'mcp-tape-import', 'body': {
                'name': 'example.csv', 'format': 'csv', 'content_base64': base64.b64encode(example['csv'].encode()).decode()}}
            submitted = await call('post_cohorts_imports', args)
            jid = submitted['id']
            assert (await call('post_cohorts_imports', args))['id'] == jid
            assert (await call('get_jobs', {'query': {'idempotency_key': 'mcp-tape-import'}}))['rows'][0]['id'] == jid
            db.WORKER = 'mcp-test'; worker.STOP.clear(); assert db.REPO.acquire(db.WORKER)
            worker.execute(db.REPO.claim(db.WORKER))
            assert (await call('get_jobs_jid', {'path': {'jid': jid}}))['status'] == 'done'
            manifest = await call('get_jobs_jid_manifest', {'path': {'jid': jid}})
            assert manifest['result'] and manifest['tables']
            path = next(iter(manifest['tables']))
            page = await call('get_jobs_jid_table', {'path': {'jid': jid}, 'query': {'path': path, 'limit': 1}})
            assert len(page['rows']) == 1
            adopted = await call('put_cohorts_tapes_tape_id', {'path': {'tape_id': 'agent-tape'},
                'expected_revision': 0, 'body': {'job_id': jid, 'expected_revision': 0}})
            assert store.STATE_META['revision'] == 1
            presets = await call('get_cohorts_presets')
            build = await call('post_cohorts_builds', {'expected_revision': 1,
                'idempotency_key': 'ops-cohort-build', 'body': {'tape_id': 'agent-tape',
                    'config': presets['config'], 'expected_revision': 1}})
            worker.execute(db.REPO.claim(db.WORKER))
            summary = await call('get_cohorts_builds_jid_summary', {'path': {'jid': build['id']}})
            assert summary['summary']['loans'] == 20
            assert summary['source_sha256'] == adopted['sha256']
            audit = await call('post_cohorts_builds_jid_audit', {'path': {'jid': build['id']},
                'expected_revision': 1, 'idempotency_key': 'ops-cohort-audit',
                'body': {'products': ['mortgage', 'deposit'], 'shocks': [0., -200., 200.]}})
            worker.execute(db.REPO.claim(db.WORKER))
            assert (await call('get_jobs_jid', {'path': {'jid': audit['id']}}))['status'] == 'done'
            report = await call('get_cohorts_audits_jid_summary', {'path': {'jid': audit['id']}})
            assert report['summary']['loans'] == 8 and report['source_sha256'] == adopted['sha256']
            errors = await call('get_jobs_jid_table', {'path': {'jid': audit['id']},
                'query': {'path': '/errors', 'limit': 10}})
            assert errors['rows']  # Read the actual tolerance evidence before test-authorized publication.
            publication = {'path': {'jid': build['id']},
                'expected_revision': 1, 'body': {'expected_revision': 1,
                    'products': ['mortgage', 'deposit'], 'mode': 'replace_tape'}}
            unsupported = await client.call_tool('put_cohorts_builds_jid_publish', publication)
            assert unsupported.isError and 'book_yield' in unsupported.structuredContent['error']['message']
            assert store.STATE_META['revision'] == 1
            # Explicitly replace these temporary fixture books: historical yields cannot be inferred.
            published = await call('put_cohorts_builds_jid_publish',
                publication | {'body': publication['body'] | {'mode': 'replace_books'}})
            assert published['revision'] == 2 and published['books'] == {'mbs': 1, 'deposits': 1}
            rows = await call('get_books_name_rows', {'path': {'name': 'loans'}, 'query': {'limit': 1}})
            row = rows['rows'][0]
            body = {'expected_revision': 2, 'updates': {row['id']: {'price': row['price'] + 1}}}
            assert not (await call('post_books_name_preview', {'path': {'name': 'loans'}, 'body': body}))['committed']
            patched = await call('patch_books_name_rows', {'path': {'name': 'loans'}, 'expected_revision': 2, 'body': body})
            assert patched['revision'] == 3
            stale = await client.call_tool('patch_books_name_rows', {'path': {'name': 'loans'}, 'expected_revision': 2, 'body': body})
            assert stale.isError and stale.structuredContent['error']['code'] == 'CONFLICT'
        await adapter.close()
        db.stop(); db.start(durable)
        reconnect = Adapter(Config(profile='ops', permissions=PERMISSIONS), transport=httpx.ASGITransport(app=main.app))
        async with create_connected_server_and_client_session(create_server(reconnect)) as client:
            result = await client.call_tool('get_jobs', {'query': {'idempotency_key': 'mcp-tape-import'}})
            assert result.structuredContent['data']['rows'][0]['id'] == jid
            assert store.TAPES['agent-tape']
        await reconnect.close()
    asyncio.run(journey())


def test_strategy_agent_native_session_update_evaluate_and_recover(durable, monkeypatch):
    """Real Rust/HiGHS and persistence; only HTTP sockets use in-process transports."""
    from fastapi.testclient import TestClient
    from app.schemas import OptimizeRequest
    from portfolio_risk.strategy.decision import NativeDecision
    NativeDecision()  # This acceptance journey requires the native artifact; no skip/fallback.
    internal = TestClient(worker.app)
    monkeypatch.setattr(worker, 'READY', True)
    monkeypatch.setattr(httpx, 'request', lambda method, url, **kwargs:
        internal.request(method, url, **{k: v for k, v in kwargs.items() if k != 'timeout'}))
    options = OptimizeRequest(lcr_min=.01, nsfr_min=.01, cet1_min=.001,
                             eve_limit=1., max_total_assets=1e7, cash_budget=1e7).model_dump()

    async def journey():
        adapter = Adapter(Config(profile='strategy', permissions=PERMISSIONS),
                          transport=httpx.ASGITransport(app=main.app))
        async with create_connected_server_and_client_session(create_server(adapter)) as client:
            async def call(name, args=None):
                result = await client.call_tool(name, args or {})
                assert not result.isError, result.structuredContent
                return result.structuredContent['data']

            submitted = await call('post_decision_sessions', {'expected_revision': 0,
                'idempotency_key': 'strategy-build', 'body': {'expected_revision': 0, 'options': options}})
            db.WORKER = 'strategy-agent-worker'; worker.STOP.clear()
            assert db.REPO.acquire(db.WORKER)
            worker.execute(db.REPO.claim(db.WORKER))
            assert (await call('get_jobs_jid', {'path': {'jid': submitted['id']}}))['status'] == 'done'
            built = await call('get_jobs_jid_result', {'path': {'jid': submitted['id']}})
            assert built['validated'] and built['version'] == 1
            sid = built['session_id']
            updated = await call('post_decision_sessions_sid_update', {'path': {'sid': sid},
                'expected_revision': 0, 'idempotency_key': 'strategy-constraints', 'body': {
                    'expected_revision': 0, 'version': 1,
                    'constraints': options | {'max_total_assets': 5e6}}})
            worker.execute(db.REPO.claim(db.WORKER))
            assert (await call('get_jobs_jid', {'path': {'jid': updated['id']}}))['status'] == 'done'
            result = await call('get_jobs_jid_result', {'path': {'jid': updated['id']}})
            assert result['validated'] and result['version'] == 2
            assert result['work']['positions_repriced'] == 0 and result['solver']['model_reused']
            args = {'path': {'sid': sid}, 'expected_revision': 0, 'body': {
                'expected_revision': 0, 'version': 2, 'allocation': result['allocation']}}
            before = await call('post_decision_sessions_sid_eval', args)
            stale = await client.call_tool('post_decision_sessions_sid_eval',
                args | {'body': args['body'] | {'version': 1}})
            assert stale.isError and stale.structuredContent['error']['code'] == 'INVALID_INPUT'
            assert 'version' in stale.structuredContent['error']['message']
            denied = await client.call_tool('patch_books_name_rows', {})
            assert denied.isError and denied.structuredContent['error']['code'] == 'FORBIDDEN'
            worker.recover()
            assert (await call('post_decision_sessions_sid_eval', args)) == before
            session = await call('get_decision_sessions_sid', {'path': {'sid': sid}})
            assert session['version'] == 2
        await adapter.close()
    try:
        asyncio.run(journey())
    finally:
        internal.close()
