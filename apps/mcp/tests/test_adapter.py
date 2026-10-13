import json
import struct

import httpx
import pyarrow as pa
import pytest
from jsonschema import Draft202012Validator

from workbench_mcp.adapter import Adapter, Config, PERMISSIONS, ToolFailure, arrow_view, catalog


def make(handler, permissions=PERMISSIONS):
    return Adapter(Config(token='private-test-token', permissions=permissions), transport=httpx.MockTransport(handler))


def test_catalog_has_valid_schemas_and_unique_routes():
    operations = catalog()['operations']
    assert len(operations) >= 91
    assert len({(x['method'], x['path']) for x in operations.values()}) == len(operations)
    assert len(catalog()['model_contracts']) >= 50
    for name, op in operations.items():
        assert len(name) <= 64
        Draft202012Validator.check_schema(op['inputSchema'])
        assert op['permission'] in PERMISSIONS
        if op.get('job'): assert 'idempotency_key' in op['inputSchema']['required']


async def test_permissions_enforced_even_for_unlisted_tool():
    calls = []
    adapter = make(lambda r: calls.append(r), frozenset({'read'}))
    assert 'post_run' not in adapter.visible()
    with pytest.raises(ToolFailure, match='permission'):
        await adapter.call('post_run', {})
    assert not calls
    await adapter.close()


@pytest.mark.parametrize('arguments', [
    {'body': {'kind': 'pricing'}, 'expected_revision': 3},
    {'body': {'kind': 'pricing'}, 'expected_revision': 3, 'idempotency_key': 'key', 'url': 'https://evil.example'},
    {'body': {'kind': 'pricing', 'expected_revision': 2}, 'expected_revision': 3, 'idempotency_key': 'key'},
])
async def test_invalid_submission_never_reaches_api(arguments):
    adapter = make(lambda r: pytest.fail('Unexpected request'))
    with pytest.raises(ToolFailure): await adapter.call('post_run', arguments)
    await adapter.close()


async def test_revision_and_idempotency_forwarded_without_rewriting_body():
    calls = []
    def handler(request):
        calls.append(request)
        return httpx.Response(200, json={'id': 'durable-job', 'status': 'queued'})
    adapter = make(handler)
    args = {'body': {'kind': 'pricing', 'expected_revision': 3}, 'expected_revision': 3, 'idempotency_key': 'retry-key'}
    for _ in range(2): assert (await adapter.call('post_run', args))['id'] == 'durable-job'
    assert len(calls) == 2 and calls[0].content == calls[1].content
    assert calls[0].headers['if-match'] == '3'
    assert calls[0].headers['idempotency-key'] == 'retry-key'
    assert calls[0].headers['authorization'] == 'Bearer private-test-token'
    assert json.loads(calls[0].content) == args['body']
    await adapter.close()


@pytest.mark.parametrize('value', ['..', '../settings', '%2e%2e', 'x/y', 'x\\y', 'x?y', 'x#y'])
async def test_path_escape_rejected(value):
    adapter = make(lambda r: pytest.fail('Unexpected request'))
    with pytest.raises(ToolFailure): await adapter.call('get_books_name', {'path': {'name': value}})
    await adapter.close()


async def test_repeated_query_values_and_base_path():
    def handler(request):
        assert request.url.path == '/api/books/loans/rows'
        assert request.url.params.get_list('columns') == ['id', 'price']
        return httpx.Response(200, json={'rows': []})
    adapter = Adapter(Config(api_url='http://localhost:8000/api'), transport=httpx.MockTransport(handler))
    await adapter.call('get_books_name_rows', {'path': {'name': 'loans'}, 'query': {'columns': ['id', 'price']}})
    await adapter.close()


@pytest.mark.parametrize('status,detail,code', [(409, 'REBUILD_REQUIRED: stale', 'REBUILD_REQUIRED'),
    (429, 'queue full', 'QUEUE_FULL'), (401, 'private-test-token', 'UNAUTHENTICATED')])
async def test_structured_errors_and_redaction(status, detail, code):
    adapter = make(lambda r: httpx.Response(status, json={'detail': detail}))
    with pytest.raises(ToolFailure) as error: await adapter.call('get_state', {})
    assert error.value.payload['code'] == code
    assert 'private-test-token' not in json.dumps(error.value.payload)
    await adapter.close()


async def test_redirect_not_followed_and_timeout_not_retried():
    seen = []
    def handler(request):
        seen.append(request)
        if len(seen) == 1: return httpx.Response(302, headers={'location': 'https://evil.example'})
        raise httpx.ReadTimeout('uncertain')
    adapter = make(handler)
    with pytest.raises(ToolFailure): await adapter.call('get_state', {})
    with pytest.raises(ToolFailure) as error: await adapter.call('get_state', {})
    assert error.value.payload['code'] == 'TRANSPORT_UNCERTAIN' and len(seen) == 2
    await adapter.close()


def envelope():
    sink = pa.BufferOutputStream()
    table = pa.table({'id': ['a', 'b', 'c'], 'pv': [1., 2., 3.]})
    with pa.ipc.new_file(sink, table.schema) as writer: writer.write_table(table)
    frame = sink.getvalue().to_pybytes()
    skeleton = json.dumps({'table': {'__arrow__': 0}, 'revision': 4}).encode()
    return b'ARW1' + struct.pack('<I', len(skeleton)) + skeleton + struct.pack('<II', 1, len(frame)) + frame


def test_arrow_paging_and_corrupt_envelopes():
    result = arrow_view(envelope(), offset=1, limit=1)
    assert result['revision'] == 4
    assert result['table']['rows'] == [{'id': 'b', 'pv': 2.}]
    assert result['table']['total'] == 3 and result['table']['truncated']
    for bad in [b'bad', envelope()[:-2], envelope() + b'x']:
        with pytest.raises(ValueError): arrow_view(bad)


async def test_binary_paging_and_download_reference():
    adapter = make(lambda r: httpx.Response(200, content=envelope(), headers={'content-type': 'application/vnd.arrow-envelope'}))
    result = await adapter.call('get_jobs_jid_result', {'path': {'jid': 'job'}, 'view': {'offset': 1, 'limit': 1}})
    assert result['table']['rows'][0]['id'] == 'b'
    result = await adapter.call('get_jobs_jid_parquet', {'path': {'jid': 'job'}, 'query': {'path': '/table'}})
    assert 'private-test-token' not in json.dumps(result)
    assert result['download_url'].endswith('parquet?path=%2Ftable') and not result['verified']
    await adapter.close()


async def test_transport_and_model_context_bounds(monkeypatch):
    import workbench_mcp.adapter as module
    adapter = make(lambda r: httpx.Response(200, json={'long': 'x' * 100}))
    monkeypatch.setattr(module, 'MAX_RESPONSE', 10)
    with pytest.raises(ToolFailure) as error: await adapter.call('get_state', {})
    assert error.value.payload['code'] == 'RESULT_TOO_LARGE'
    monkeypatch.setattr(module, 'MAX_RESPONSE', 1000)
    monkeypatch.setattr(module, 'MAX_OUTPUT', 10)
    with pytest.raises(ToolFailure) as error: await adapter.call('get_state', {})
    assert error.value.payload['code'] == 'RESULT_TOO_LARGE'
    await adapter.close()


async def test_finite_inputs_and_unknown_tools():
    adapter = make(lambda r: pytest.fail('Unexpected request'))
    with pytest.raises(ToolFailure): await adapter.call('shell', {})
    args = {'expected_revision': 0, 'idempotency_key': 'key', 'body': {
        'expected_revision': 0, 'schema_name': 'consumer-card-1', 'request': {'value': float('nan')}}}
    with pytest.raises(ToolFailure, match='finite JSON'): await adapter.call('post_models_runs', args)
    await adapter.close()


@pytest.mark.parametrize('url', ['http://remote.example', 'https://user:password@example.com', 'file:///tmp/api', 'https://example.com?url=evil'])
def test_remote_configuration_boundary(url):
    with pytest.raises(ValueError): Config(api_url=url)
