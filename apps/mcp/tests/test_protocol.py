import os
import sys

import httpx
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from mcp.shared.memory import create_connected_server_and_client_session

from workbench_mcp.adapter import Adapter, Config
from workbench_mcp.server import create_server


async def test_sdk_discovery_resources_prompt_and_calls():
    adapter = Adapter(Config(), transport=httpx.MockTransport(lambda r: httpx.Response(200, json={'revision': 9})))
    async with create_connected_server_and_client_session(create_server(adapter)) as client:
        listed = await client.list_tools()
        assert any(t.name == 'get_state' for t in listed.tools)
        assert all(t.annotations.readOnlyHint for t in listed.tools)
        result = await client.call_tool('get_state', {})
        assert not result.isError and result.structuredContent['data']['revision'] == 9
        denied = await client.call_tool('post_run', {})
        assert denied.isError and denied.structuredContent['error']['code'] == 'FORBIDDEN'
        resources = await client.list_resources()
        assert any(str(r.uri) == 'workbench://guide' for r in resources.resources)
        assert (await client.read_resource('workbench://schema/get_state')).contents
        assert (await client.get_prompt('workbench-workflow', {'objective': 'Price the saved book'})).messages
    await adapter.close()


async def test_real_stdio_handshake_without_api_or_workspace_startup():
    params = StdioServerParameters(command=sys.executable, args=['-m', 'workbench_mcp.server'],
        env={**os.environ, 'WORKBENCH_MCP_PERMISSIONS': 'read', 'WORKBENCH_MCP_API_URL': 'http://127.0.0.1:1'})
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as client:
            initialized = await client.initialize()
            assert initialized.serverInfo.name == 'rates-workbench'
            assert len((await client.list_tools()).tools) > 30
            assert (await client.read_resource('workbench://models')).contents


async def test_artifact_resource_fetches_with_auth_and_rechecks_scope():
    calls = []
    def handler(request):
        calls.append(request)
        assert request.headers['authorization'] == 'Bearer resource-secret'
        return httpx.Response(200, content=b'PAR1', headers={'content-type': 'application/vnd.apache.parquet'})
    adapter = Adapter(Config(token='resource-secret'), transport=httpx.MockTransport(handler))
    async with create_connected_server_and_client_session(create_server(adapter)) as client:
        result = await client.call_tool('get_jobs_jid_parquet', {'path': {'jid': 'one'}, 'query': {'path': '/table'}})
        uri = result.structuredContent['data']['resource_uri']
        assert not calls and 'resource-secret' not in uri
        artifact = await client.read_resource(uri)
        assert artifact.contents[0].blob == 'UEFSMQ==' and len(calls) == 1
        adapter.config = Config(permissions=frozenset())
        import pytest
        with pytest.raises(Exception): await client.read_resource(uri)
        assert len(calls) == 1
    await adapter.close()
