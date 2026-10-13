"""Local stdio MCP server using the official SDK; stdout is protocol-only."""
import asyncio
import argparse
import json
import logging
import os
from pathlib import Path
import time
from urllib.parse import quote, unquote

from mcp.server.lowlevel import Server
from mcp.server.lowlevel.helper_types import ReadResourceContents
from mcp.server.stdio import stdio_server
import mcp.types as types

from .adapter import Adapter, Config, ToolFailure
from .profiles import PROFILES

GUIDANCE = """Rates Workbench agent workflow
Read get_state and relevant schemas/examples before execution. Preserve the observed revision.
Use get_jobs and get_decision_sessions to recover previous work before submitting duplicates.
Inspect books with get_books_name_rows. Validate changes with post_books_name_preview,
then patch_books_name_rows using the same revision. Changes invalidate prepared libraries.
For pricing/risk/what-if/NII, submit post_run with an explicit expected_revision and a unique
idempotency_key. Keep the exact key and payload across uncertain retries. Poll get_jobs_jid;
done alone is not a substitute for inspecting result manifests, validation and limitations.
Read get_jobs_jid_manifest and get_jobs_jid_table for large results. Arrow tool outputs contain
bounded pages and explicit total/offset/limit metadata. Download tools return authenticated URLs.
Build unit libraries before strategy evaluation; never reprice in interactive evaluation.
For model analysis, read get_models_contracts, get_models_examples_schema and the model reference
resource. Inner model payloads retain native validation; examples are synthetic, not default policy.
For tapes: import, inspect job, adopt, build, audit, then explicitly publish with the reviewed revision.
For treasury: discover compatible completed sources, inspect their template, supply reviewed policy,
then submit a bridge or capital/FTP run. Never invent regulatory eligibility or fill unknown policy.
Respect RESTART_REQUIRED, REBUILD_REQUIRED and CAPITAL_REFRESH_REQUIRED. Never silently rebuild,
change revisions, relax limits or switch financial backend to make a request succeed.
Rust owns financial execution; fixed base OAS, CRN, independent verification and publication fences
remain authoritative. Available tools do not expand supported product/model adoption.
Cancelling an MCP request does not cancel a durable job. Use delete_jobs_jid explicitly.
Permissions are granted to this local stdio process by its operator, not supplied in tool arguments.
"""


def create_server(adapter):
    role = PROFILES[adapter.config.profile]
    instructions = role.instructions + ('\n' + GUIDANCE if adapter.config.profile == 'workbench' else
        '\nRead the granted tool schemas and workbench reference resources before submitting work.')
    server = Server('rates-workbench', version='0.2.0', instructions=instructions)
    references = json.loads(Path(__file__).with_name('references.json').read_text(encoding='utf-8'))

    @server.list_tools()
    async def list_tools():
        return [types.Tool(name=name, description=op['description'], inputSchema=op['inputSchema'],
            outputSchema={'type': 'object', 'properties': {'ok': {'type': 'boolean'}, 'data': {},
                'error': {'type': 'object', 'properties': {'code': {'type': 'string'}, 'message': {'type': 'string'}},
                          'required': ['code', 'message']}}, 'required': ['ok']},
            annotations=types.ToolAnnotations(readOnlyHint=op['permission'] == 'read',
                destructiveHint=op['permission'] in {'edit', 'publish', 'cancel'},
                idempotentHint=op['method'] == 'GET', openWorldHint=True))
                for name, op in adapter.visible().items()]

    @server.call_tool(validate_input=False)
    async def call_tool(name, arguments):
        started = time.monotonic()
        code = 'ok'
        try:
            value = await adapter.call(name, arguments or {})
            if adapter.operations[name].get('transport') == 'artifact':
                value['resource_uri'] = 'workbench://artifact/' + name + '/' + quote(
                    json.dumps(arguments or {}, separators=(',', ':')), safe='')
            output = {'ok': True, 'data': value}
        except ToolFailure as exc:
            code = exc.payload['code']; output = {'ok': False, 'error': exc.payload}
        except asyncio.CancelledError:
            code = 'REQUEST_CANCELLED'
            raise
        except Exception:
            code = 'INTERNAL_ERROR'
            output = {'ok': False, 'error': {'code': code, 'message': 'Unexpected adapter failure; inspect local diagnostics.'}}
            # Do not emit tracebacks with request bodies or credentials to a client.
        finally:
            logging.getLogger('workbench_mcp.audit').info(json.dumps(dict(
                actor=adapter.config.actor, tool=name, outcome=code,
                elapsed_ms=round((time.monotonic() - started) * 1000))))
        content = [types.TextContent(type='text', text=json.dumps(output))]
        if output['ok'] and isinstance(output['data'], dict) and 'resource_uri' in output['data']:
            content.append(types.ResourceLink(type='resource_link', uri=output['data']['resource_uri'],
                name=name + ' artifact', description='Authenticated API artifact; bounded resource read.'))
        return types.CallToolResult(content=content,
                                    structuredContent=output, isError=not output['ok'])

    @server.list_resources()
    async def list_resources():
        resources = [types.Resource(uri='workbench://guide', name='Agent workflow guide', mimeType='text/plain'),
                     types.Resource(uri='workbench://agent', name=role.title + ' agent instructions', mimeType='text/plain'),
                     types.Resource(uri='workbench://coverage', name='Granted API/tool coverage', mimeType='application/json'),
                     types.Resource(uri='workbench://models', name='Supported analytical contract names', mimeType='application/json')]
        resources.extend(types.Resource(uri='workbench://schema/' + name, name=name + ' input schema',
                                        mimeType='application/json') for name in adapter.visible())
        resources.extend(types.Resource(uri='workbench://reference/' + name, name=name,
                                        mimeType='text/markdown') for name in references)
        return resources

    @server.read_resource()
    async def read_resource(uri):
        value = str(uri)
        if value.startswith('workbench://artifact/'):
            suffix = value.removeprefix('workbench://artifact/')
            name, encoded = suffix.split('/', 1)
            if len(encoded) > 16000 or adapter.operations.get(name, {}).get('transport') != 'artifact':
                raise ValueError('Unknown artifact resource')
            # Revalidate arguments and permissions on every resource read; no URL is trusted from the URI.
            descriptor = await adapter.call(name, json.loads(unquote(encoded)))
            data, mime = await adapter.request('GET', descriptor['download_url'])
            return [ReadResourceContents(content=data, mime_type=mime.split(';')[0] or 'application/octet-stream')]
        if value == 'workbench://guide': return GUIDANCE
        if value == 'workbench://agent': return instructions
        if value == 'workbench://coverage':
            return json.dumps({name: {k: v for k, v in op.items() if k != 'inputSchema'}
                               for name, op in adapter.visible().items()})
        if value == 'workbench://models': return json.dumps(adapter.catalog['model_contracts'])
        if value.startswith('workbench://schema/'):
            name = value.removeprefix('workbench://schema/')
            if name in adapter.visible(): return json.dumps(adapter.operations[name]['inputSchema'])
        if value.startswith('workbench://reference/'):
            name = value.removeprefix('workbench://reference/')
            if name in references: return references[name]
        raise ValueError('Unknown or unavailable workbench resource')

    @server.list_prompts()
    async def list_prompts():
        names = ['workbench-workflow'] if adapter.config.profile == 'workbench' else ['workbench-' + adapter.config.profile]
        return [types.Prompt(name=name, description=role.purpose,
            arguments=[types.PromptArgument(name='objective', description='The requested analysis or change', required=True)]) for name in names]

    @server.get_prompt()
    async def get_prompt(name, arguments):
        selected = 'workbench-workflow' if adapter.config.profile == 'workbench' else 'workbench-' + adapter.config.profile
        if name != selected or not arguments or not arguments.get('objective'):
            raise ValueError('A workflow objective is required')
        return types.GetPromptResult(messages=[types.PromptMessage(role='user',
            content=types.TextContent(type='text', text=instructions + '\nRequested objective: ' + arguments['objective']))])

    return server


async def run(profile='workbench'):
    config = Config(api_url=os.getenv('WORKBENCH_MCP_API_URL', 'http://127.0.0.1:8000'),
        token=os.getenv('WORKBENCH_API_TOKEN', ''),
        permissions=frozenset(x.strip() for x in os.getenv('WORKBENCH_MCP_PERMISSIONS', ','.join(sorted(PROFILES[profile].default_permissions))).split(',') if x.strip()),
        actor=os.getenv('WORKBENCH_MCP_ACTOR', profile + '-agent'), profile=profile)
    adapter = Adapter(config)
    try:
        server = create_server(adapter)
        async with stdio_server() as (read, write):
            await server.run(read, write, server.create_initialization_options())
    finally:
        await adapter.close()


def main(default_profile='workbench'):
    parser = argparse.ArgumentParser(description='Rates Workbench agent MCP server')
    parser.add_argument('--profile', choices=sorted(PROFILES), default=default_profile)
    args = parser.parse_args()
    logging.basicConfig(level=logging.WARNING)
    logging.getLogger('workbench_mcp.audit').setLevel(logging.INFO)
    asyncio.run(run(args.profile))


def ops_main():
    main('ops')


def strategy_main():
    main('strategy')


if __name__ == '__main__':
    main()
