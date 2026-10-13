import os
import sys

import httpx
import pytest
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from mcp.shared.memory import create_connected_server_and_client_session
from jsonschema import Draft202012Validator

from workbench_mcp.adapter import Adapter, Config, PERMISSIONS, ToolFailure, catalog
from workbench_mcp.profiles import PROFILES
from workbench_mcp.server import create_server


@pytest.mark.parametrize('profile,allowed,denied', [
    ('ops', 'post_cohorts_imports', 'post_decision_sessions'),
    ('strategy', 'post_decision_sessions', 'patch_books_name_rows'),
    ('strategy', 'post_strategy_eval', 'put_cohorts_builds_jid_publish'),
])
async def test_role_limits_survive_full_permission_grant(profile, allowed, denied):
    calls = []
    adapter = Adapter(Config(profile=profile, permissions=PERMISSIONS),
                      transport=httpx.MockTransport(lambda r: calls.append(r)))
    async with create_connected_server_and_client_session(create_server(adapter)) as client:
        names = {t.name for t in (await client.list_tools()).tools}
        assert allowed in names and denied not in names
        result = await client.call_tool(denied, {})
        assert result.isError and result.structuredContent['error']['code'] == 'FORBIDDEN'
        assert not calls
        instructions = (await client.read_resource('workbench://agent')).contents[0].text
        assert PROFILES[profile].instructions in instructions
        assert (await client.get_prompt('workbench-' + profile, {'objective': 'Review this workspace'})).messages
    await adapter.close()


@pytest.mark.parametrize('profile,kind,calibration', [
    ('ops', 'strategy', 'hold'), ('ops', 'whatif', 'hold'),
    ('strategy', 'whatif', 'recalibrate'),
])
async def test_multipurpose_routes_cannot_bypass_role(profile, kind, calibration):
    calls = []
    adapter = Adapter(Config(profile=profile, permissions=PERMISSIONS),
                      transport=httpx.MockTransport(lambda r: calls.append(r)))
    with pytest.raises(ToolFailure) as caught:
        await adapter.call('post_run', {'expected_revision': 0, 'idempotency_key': 'role-gate',
            'body': {'kind': kind, 'calibration_mode': calibration}})
    assert caught.value.payload['code'] == 'FORBIDDEN'
    assert not calls
    await adapter.close()


def test_reviewed_role_actions_exist_and_default_grants_are_bounded():
    operations = catalog()['operations']
    for name in ('ops', 'strategy'):
        assert PROFILES[name].actions <= operations.keys()
        assert PROFILES[name].default_permissions == {'read', 'compute'}
    with pytest.raises(ValueError, match='profile'):
        Config(profile='superuser')


@pytest.mark.parametrize('profile,allowed,denied', [
    ('ops', 'post_cohorts_imports', 'patch_books_name_rows'),
    ('strategy', 'post_decision_sessions', 'put_books_name'),
])
async def test_profile_stdio_defaults_and_role_instructions(profile, allowed, denied):
    env = {k: v for k, v in os.environ.items() if k != 'WORKBENCH_MCP_PERMISSIONS'}
    env['WORKBENCH_MCP_API_URL'] = 'http://127.0.0.1:1'
    params = StdioServerParameters(command=sys.executable,
        args=['-m', 'workbench_mcp.server', '--profile', profile], env=env)
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as client:
            initialized = await client.initialize()
            assert PROFILES[profile].instructions in initialized.instructions
            names = {t.name for t in (await client.list_tools()).tools}
            assert allowed in names and denied not in names


async def test_read_only_grant_cannot_be_broadened_by_role():
    adapter = Adapter(Config(profile='strategy'))
    assert all(op['permission'] == 'read' for op in adapter.visible().values())
    with pytest.raises(ToolFailure, match='permission'):
        await adapter.call('post_decision_sessions', {})
    await adapter.close()


async def test_discovery_advertises_role_run_restrictions_without_mutating_catalog():
    for profile, denied in [('ops', {'kind': 'strategy'}),
                            ('strategy', {'kind': 'whatif', 'calibration_mode': 'recalibrate'})]:
        adapter = Adapter(Config(profile=profile, permissions=PERMISSIONS))
        args = {'expected_revision': 0, 'idempotency_key': 'schema-check', 'body': denied}
        original = adapter.operations['post_run']['inputSchema']
        restricted = adapter.visible()['post_run']['inputSchema']
        assert Draft202012Validator(original).is_valid(args)
        assert not Draft202012Validator(restricted).is_valid(args)
        assert Draft202012Validator(restricted).is_valid(args | {'body': {'kind': 'pricing'}})
        await adapter.close()
