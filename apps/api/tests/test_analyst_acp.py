"""Exercise the actual JSON-RPC stream with a deterministic subprocess, no model account."""
import asyncio
import json
import os
import sys
import time

import pytest

from app.analyst_acp import AcpClient, AcpError, process_environment


PEER = r'''
import sys,json
for line in sys.stdin:
    msg=json.loads(line)
    method=msg.get('method')
    if method=='initialize':
        print(json.dumps({'jsonrpc':'2.0','id':msg['id'],'result':{'protocolVersion':1}}),flush=True)
    elif method=='session/prompt':
        print(json.dumps({'jsonrpc':'2.0','id':'permission','method':'session/request_permission','params':{}}),flush=True)
        response=json.loads(sys.stdin.readline())
        assert response['result']['outcome']['outcome']=='cancelled'
        print(json.dumps({'jsonrpc':'2.0','id':'file','method':'fs/read_text_file','params':{}}),flush=True)
        response=json.loads(sys.stdin.readline())
        assert response['error']['code']==-32601
        for sid,text in [('other','CROSS-SESSION'),('owned','Evidence '),('owned','[E123].')]:
            print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':sid,'update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':text}}}}),flush=True)
        print(json.dumps({'jsonrpc':'2.0','id':msg['id'],'result':{'stopReason':'end_turn'}}),flush=True)
    elif method=='broken':
        print('not JSON',flush=True)
'''


def test_stream_denies_extra_capabilities_and_ignores_other_sessions():
    pytest.importorskip('psutil', reason='Install the analysts extra for subprocess supervision')
    async def check():
        process = await asyncio.create_subprocess_exec(sys.executable, '-u', '-c', PEER,
            stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.DEVNULL)
        chunks=[]; client=AcpClient(process, chunks.append)
        try:
            assert (await asyncio.wait_for(client.request('initialize', {}), 5))['protocolVersion']==1
            client.session_id='owned'
            assert (await asyncio.wait_for(client.request('session/prompt', {}), 5))['stopReason']=='end_turn'
            assert chunks[-1]=='Evidence [E123].'
            with pytest.raises(AcpError): await asyncio.wait_for(client.request('broken', {}), 5)
        finally:
            await client.close()
        assert process.returncode is not None
    asyncio.run(check())


def test_process_environment_does_not_inherit_application_credentials(tmp_path, monkeypatch):
    monkeypatch.setenv('WORKBENCH_API_TOKEN','never-forward')
    monkeypatch.setenv('DATABASE_URL','never-forward')
    monkeypatch.setenv('ANTHROPIC_API_KEY','approved-provider')
    monkeypatch.setenv('WORKBENCH_GOOSE_ENV_KEYS','ANTHROPIC_API_KEY')
    env=process_environment(tmp_path,dict(provider='anthropic',model='chosen'))
    assert 'WORKBENCH_API_TOKEN' not in env and 'DATABASE_URL' not in env
    assert env['ANTHROPIC_API_KEY']=='approved-provider'
    assert env['GOOSE_PATH_ROOT']==str(tmp_path) and env['CONTEXT_FILE_NAMES']=='[]'
    monkeypatch.setenv('WORKBENCH_GOOSE_ENV_KEYS','WORKBENCH_API_TOKEN')
    with pytest.raises(ValueError):process_environment(tmp_path,dict(provider='anthropic',model='chosen'))


def test_openrouter_only_inherits_its_key_by_default(tmp_path, monkeypatch):
    monkeypatch.setenv('OPENROUTER_API_KEY', 'provider-secret')
    monkeypatch.setenv('OPENROUTER_HOST', 'https://unapproved.invalid')
    monkeypatch.setenv('OPENROUTER_PARAMETERS', '{"plugins":[{"id":"web"}]}')
    monkeypatch.setenv('OPENAI_API_KEY', 'unrelated-secret')
    monkeypatch.setenv('WORKBENCH_GOOSE_ENV_KEYS', '')
    env = process_environment(tmp_path, dict(provider='openrouter', model='approved'))
    assert env['OPENROUTER_API_KEY'] == 'provider-secret'
    assert not {'OPENROUTER_HOST', 'OPENROUTER_PARAMETERS', 'OPENAI_API_KEY'} & env.keys()
    assert 'OPENROUTER_API_KEY' not in process_environment(tmp_path, dict(provider='openai', model='approved'))


def test_openrouter_missing_key_fails_preflight(tmp_path, monkeypatch):
    import hashlib
    from app.analyst_config import runtime_config
    executable = tmp_path / 'goose.exe'
    executable.write_bytes(b'fixture')
    for key, value in {
        'WORKBENCH_ANALYST_ENABLED': '1', 'WORKBENCH_ANALYST_SECRET': 's' * 48,
        'WORKBENCH_GOOSE_EXECUTABLE': str(executable),
        'WORKBENCH_GOOSE_SHA256': hashlib.sha256(b'fixture').hexdigest(),
        'WORKBENCH_GOOSE_PROVIDER': 'openrouter', 'WORKBENCH_GOOSE_MODEL': 'approved',
        'OPENROUTER_API_KEY': '  ',
    }.items():
        monkeypatch.setenv(key, value)
    with pytest.raises(ValueError, match='Set OPENROUTER_API_KEY'):
        runtime_config()
    monkeypatch.setenv('OPENROUTER_API_KEY', 'provider-secret')
    config = runtime_config()
    assert config['provider'] == 'openrouter'
    assert 'provider-secret' not in str(config)
