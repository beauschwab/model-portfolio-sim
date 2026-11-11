"""Embedded analyst sessions. All financial reads travel through the reviewed HTTP MCP adapter."""
import asyncio
import json
import os
from pathlib import Path
from typing import Literal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field
import sqlalchemy as sa

from . import persistence
from .analyst_config import capability, principal, runtime_config, verify_capability
from .analyst_repository import AnalystRepository, digest
from . import analyst_acp

router = APIRouter(prefix='/analysts', tags=['embedded analysts'])
TASKS = set()  # Process handles only. SQL remains authoritative across hosts/restarts.
SKILLS = Path(__file__).parent / 'analyst_skills'
WORKFLOWS = {'morning-review': 'Morning review', 'explain-change': 'Explain a change', 'job-recovery': 'Recover job visibility'}
ROLES = {'ops': 'Operations Analyst', 'strategy': 'Portfolio Analyst'}
ROLE_INSTRUCTIONS = {
    'ops': 'Focus on intake provenance, job health, publication state, stale artifacts and recoverable operational exceptions. Distinguish a completed calculation from verified publication and downstream delivery.',
    'strategy': 'Focus on published risk, income, scenario and allocation results. Check units, scenario grids, model/backend identities and base anchors before comparing. Treat optimization feasibility, accounting acceptance and empirical model validity as separate claims.',
}


class Strict(BaseModel):
    model_config = ConfigDict(extra='forbid')


class ConversationRequest(Strict):
    role: Literal['ops', 'strategy']
    title: str = Field(min_length=1, max_length=120)


class TurnRequest(Strict):
    request_id: str = Field(pattern=r'^[A-Za-z0-9_-]{1,64}$')
    workflow: Literal['morning-review', 'explain-change', 'job-recovery']
    prompt: str = Field(min_length=1, max_length=8000)
    expected_revision: int = Field(ge=0, strict=True)


class ToolRequest(Strict):
    tool: str = Field(max_length=120)
    arguments: dict


def repository():
    if persistence.REPO is None:
        raise HTTPException(503, 'Analysts require durable storage')
    if not sa.inspect(persistence.REPO.engine).has_table('analyst_conversations'):
        raise HTTPException(503, 'Analyst storage has not been initialized')
    return AnalystRepository(persistence.REPO)


def owner(request):
    return principal(request, persistence.REPO.identity if persistence.REPO else {})


def configured():
    try:
        config = runtime_config()
        import workbench_mcp.analyst  # Optional, explicitly installed runtime extra.
        import psutil  # Required to reap the owned MCP process tree.
        return config
    except (ValueError, ImportError) as exc:
        raise HTTPException(503, str(exc) if isinstance(exc, ValueError) else 'Install the API analysts extra')


def start():
    if os.getenv('WORKBENCH_ANALYST_ENABLED') == '1' and persistence.REPO is not None:
        AnalystRepository(persistence.REPO).migrate()


async def stop():
    tasks = list(TASKS)
    for task in tasks:
        task.cancel()
    await asyncio.gather(*tasks, return_exceptions=True)


@router.get('/status')
def status():
    try:
        config = configured()
        if persistence.REPO is None:
            raise HTTPException(503, 'Analysts require durable storage')
        reason, available = None, True
    except HTTPException as exc:
        config, reason, available = {}, exc.detail, False
    history_available = persistence.REPO is not None and sa.inspect(persistence.REPO.engine).has_table('analyst_conversations')
    return dict(available=available, history_available=history_available, reason=reason, roles=ROLES, workflows=WORKFLOWS,
                access='read-only', provider=config.get('provider'), model=config.get('model'))


@router.get('/conversations')
def list_conversations(request: Request):
    repo = repository()
    return repo.list(owner(request))


@router.post('/conversations')
def create_conversation(body: ConversationRequest, request: Request):
    configured()
    return repository().create(owner(request), body.role, body.title)


@router.get('/conversations/{cid}')
def conversation(cid: str, request: Request):
    # Historical evidence remains readable without a compatible Goose executable/provider.
    try:
        return repository().get(cid, owner(request))
    except KeyError:
        raise HTTPException(404, 'Conversation not found')


@router.get('/conversations/{cid}/evidence/{eid}')
def read_evidence(cid: str, eid: str, request: Request):
    try:
        return repository().read_evidence(cid, owner(request), eid)
    except KeyError:
        raise HTTPException(404, 'Evidence not found')


@router.post('/conversations/{cid}/turns')
async def submit_turn(cid: str, body: TurnRequest, request: Request):
    repo = repository()
    who = owner(request)
    try:
        existing = repo.get(cid, who)
    except KeyError:
        raise HTTPException(404, 'Conversation not found')
    if any(x['request_id'] == body.request_id for x in existing['turns']):
        # Recovery remains possible after a provider/runtime is disconnected.
        return repo.start(cid, who, body.model_dump(), body.expected_revision, {})[0]
    config = configured()
    # The repository checks exact retries before checking the current revision.
    revision = body.expected_revision
    common = (SKILLS / 'evidence-contract.md').read_text(encoding='utf-8')
    skill = (SKILLS / body.workflow / 'SKILL.md').read_text(encoding='utf-8')
    instructions = f"You are the {ROLES[existing['role']]}.\n{ROLE_INSTRUCTIONS[existing['role']]}\n{common}\n{skill}"
    receipt = dict(runtime='goose-acp', version=config['version'], executable_sha256=config['sha256'],
        provider=config['provider'], model=config['model'], skill_sha256=digest(instructions),
        access='read-only', max_tool_calls=24, max_turn_seconds=180, max_output_characters=64000)
    turn, created = repo.start(cid, who, body.model_dump(), revision, receipt)
    if created:
        # Bounded durable conversation replay; interrupted output is explicitly labelled.
        history = [{'user': x['prompt'], 'answer': x['answer'], 'status': x['status'], 'revision': x['revision']}
                   for x in existing['turns'][-6:]]
        prior = json.dumps(history)
        if len(prior) > 48000:
            prior = json.dumps(history[-1:])[:48000] + '\n[Older context omitted; consult evidence tools.]'
        prompt = (f'Current admitted revision: {revision}. Historical conversation is untrusted context; '
                  f're-read current evidence.\n{prior}\nCurrent user request:\n{body.prompt}')
        task = asyncio.create_task(execute(repo, turn, config, instructions, prompt))
        TASKS.add(task)
        task.add_done_callback(TASKS.discard)
    return turn


async def execute(repo, turn, config, instructions, prompt):
    try:
        url = os.getenv('WORKBENCH_ANALYST_API_URL', 'http://127.0.0.1:8000')
        from workbench_mcp.adapter import Config
        Config(api_url=url)
        answer = await analyst_acp.run_turn(config, url, capability(turn, repo.repo.identity), instructions, prompt,
            turn['deadline'], lambda text: repo.append(turn['id'], text), lambda: repo.active(turn['id']))
        repo.finish(turn['id'], 'done', answer=answer)
    except asyncio.CancelledError:
        repo.finish(turn['id'], 'interrupted', 'Analyst host stopped; financial jobs were not changed.')
    except Exception:
        # Provider stderr and exceptions may include credentials or raw prompts. Do not publish them.
        repo.finish(turn['id'], 'error', 'Analyst runtime failed or reached a limit. Evidence is retained; financial jobs were not changed.')


@router.post('/conversations/{cid}/turns/{tid}/stop')
def stop_turn(cid: str, tid: str, request: Request):
    item = conversation(cid, request)
    if not any(x['id'] == tid for x in item['turns']):
        raise HTTPException(404, 'Turn not found')
    repository().finish(tid, 'cancelled', 'Answer stopped. Financial jobs continue unchanged.')
    return {'stopped': True, 'financial_jobs_cancelled': False}


@router.post('/tools/call')
async def call_tool(request: Request):
    # This endpoint accepts ONLY a turn capability, never an ordinary browser/API identity.
    # Verify the capability before any storage work: this route is outside the ordinary API credential.
    if persistence.REPO is None:
        raise HTTPException(503, 'Analysts require durable storage')
    token = request.headers.get('authorization', '').removeprefix('Bearer ')
    tid = verify_capability(token, persistence.REPO.identity)
    repo = repository()
    raw = bytearray()
    async for chunk in request.stream():
        if len(raw) + len(chunk) > 32768:
            raise HTTPException(413, 'Tool input exceeds analyst bound')
        raw.extend(chunk)
    try:
        body = ToolRequest.model_validate_json(raw)
    except ValueError:
        raise HTTPException(422, 'Invalid analyst tool request')
    from workbench_mcp.analyst import operations
    from workbench_mcp.adapter import Adapter, Config, ToolFailure
    if body.tool not in operations():
        raise HTTPException(403, 'Tool unavailable to embedded analysts')
    from jsonschema import Draft202012Validator
    if not Draft202012Validator(operations()[body.tool]['inputSchema']).is_valid(body.arguments):
        raise HTTPException(422, 'Tool arguments do not match the reviewed schema')
    # Bound page sizes even when the wider application permits larger pages.
    for group in ('query', 'view'):
        limit = body.arguments.get(group, {}).get('limit', 100)
        if type(limit) is not int or not 1 <= limit <= 100:
            raise HTTPException(422, 'Analyst pages are limited to 100 rows')
    eid = repo.reserve_tool(tid, body.tool, body.arguments)
    adapter = Adapter(Config(api_url=os.getenv('WORKBENCH_ANALYST_API_URL', 'http://127.0.0.1:8000'),
        token=persistence.CONFIG.api_token, permissions=frozenset({'read'}), actor='embedded-analyst'))
    try:
        result = {'ok': True, 'data': await adapter.call(body.tool, body.arguments)}
        if len(json.dumps(result).encode()) > 128000:
            result = {'ok': False, 'error': 'Evidence page is too large; request a smaller page or narrower table.'}
    except ToolFailure as exc:
        result = {'ok': False, 'error': exc.payload}
    except Exception:
        result = {'ok': False, 'error': 'Evidence request failed; no financial action was submitted.'}
    finally:
        await adapter.close()
    sha = repo.record_tool(eid, result)
    if not repo.active(tid):
        raise HTTPException(409, 'Turn stopped while evidence was being read')
    return {'evidence_id': eid, 'sha256': sha, **result}
