"""Bounded Goose 1.57 ACP transport. No financial execution or user-selected commands."""
import asyncio
import contextlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from .analyst_config import GOOSE_VERSION


class AcpError(RuntimeError):
    pass


class AcpClient:
    def __init__(self, process, on_text):
        self.process, self.on_text = process, on_text
        self.pending = {}
        self.counter = 0
        self.session_id = None
        self.answer = ''
        self.reader = asyncio.create_task(self._read())

    async def send(self, message):
        self.process.stdin.write(json.dumps(message).encode() + b'\n')
        await self.process.stdin.drain()

    async def request(self, method, params):
        self.counter += 1
        ident = self.counter
        future = asyncio.get_running_loop().create_future()
        self.pending[ident] = future
        try:
            await self.send(dict(jsonrpc='2.0', id=ident, method=method, params=params))
            return await future
        finally:
            self.pending.pop(ident, None)

    async def _read(self):
        try:
            while raw := await self.process.stdout.readline():
                msg = json.loads(raw)
                if 'method' not in msg:
                    future = self.pending.get(msg.get('id'))
                    if future and not future.done():
                        if 'error' in msg:
                            future.set_exception(AcpError('Goose rejected an ACP request; check the pinned runtime configuration.'))
                        else:
                            future.set_result(msg.get('result', {}))
                elif 'id' in msg:
                    # No filesystem, terminal, authentication or approval capability is delegated to Goose.
                    if msg['method'] in {'session/request_permission', 'session/requestPermission'}:
                        await self.send(dict(jsonrpc='2.0', id=msg['id'], result={'outcome': {'outcome': 'cancelled'}}))
                    else:
                        await self.send(dict(jsonrpc='2.0', id=msg['id'], error={'code': -32601, 'message': 'Client capability unavailable'}))
                elif msg['method'] == 'session/update':
                    params = msg.get('params', {})
                    if params.get('sessionId') != self.session_id:
                        continue
                    update = params.get('update', {})
                    if update.get('sessionUpdate') == 'agent_message_chunk':
                        content = update.get('content', {})
                        if content.get('type') == 'text':
                            self.answer += content.get('text', '')
                            if len(self.answer) > 64000:
                                raise AcpError('Analyst output limit reached')
                            self.on_text(self.answer)
            raise AcpError('Goose disconnected before completing the turn')
        except (Exception, asyncio.CancelledError) as exc:
            for future in self.pending.values():
                if not future.done():
                    future.set_exception(AcpError(str(exc) if isinstance(exc, AcpError) else 'Invalid or interrupted ACP stream'))

    async def close(self):
        if self.session_id and self.process.returncode is None:
            with contextlib.suppress(Exception):
                await asyncio.wait_for(self.send(dict(jsonrpc='2.0', method='session/cancel',
                    params={'sessionId': self.session_id})), 1)
        if self.process.returncode is None:
            # Goose owns its MCP child. Reap the whole owned process tree on Windows
            # as well as POSIX; do not leave a capability-holding transport behind.
            import psutil
            with contextlib.suppress(psutil.NoSuchProcess):
                children = psutil.Process(self.process.pid).children(recursive=True)
                for child in reversed(children):
                    with contextlib.suppress(psutil.NoSuchProcess):
                        child.terminate()
                _, remaining = await asyncio.to_thread(psutil.wait_procs, children, timeout=2)
                for child in remaining:
                    with contextlib.suppress(psutil.NoSuchProcess):
                        child.kill()
            with contextlib.suppress(ProcessLookupError):
                self.process.terminate()
            try:
                await asyncio.wait_for(self.process.wait(), 5)
            except asyncio.TimeoutError:
                self.process.kill()
                await self.process.wait()
        self.reader.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await self.reader


def process_environment(root, config):
    # No inherited API/database/cloud credentials, arbitrary plugins, hints or proxy settings.
    env = {key: os.environ[key] for key in ('PATH', 'SYSTEMROOT', 'WINDIR', 'TEMP', 'TMP') if key in os.environ}
    if config['provider'] == 'openrouter':
        # OpenRouter credentials are an explicit part of this provider's contract.
        # Host/parameter overrides still require the deployment's explicit allowlist.
        env['OPENROUTER_API_KEY'] = os.getenv('OPENROUTER_API_KEY', '')
    for key in os.getenv('WORKBENCH_GOOSE_ENV_KEYS', '').split(','):
        key = key.strip()
        if key:
            if not key.replace('_', '').isalnum() or key.startswith(('WORKBENCH_', 'GOOSE_')):
                raise ValueError('Only explicit provider environment keys may be forwarded')
            if key in os.environ:
                env[key] = os.environ[key]
    env.update(GOOSE_PATH_ROOT=str(root), HOME=str(root), USERPROFILE=str(root),
        APPDATA=str(root / 'appdata'), LOCALAPPDATA=str(root / 'local'), XDG_CONFIG_HOME=str(root / 'xdg'),
        GOOSE_PROVIDER=config['provider'], GOOSE_MODEL=config['model'], GOOSE_MODE='auto',
        GOOSE_MAX_TURNS='12', GOOSE_MAX_TOKENS='4096', GOOSE_DISABLE_SESSION_NAMING='true',
        GOOSE_DISABLE_KEYRING='true', GOOSE_TELEMETRY_ENABLED='false', CONTEXT_FILE_NAMES='[]')
    return env


async def run_turn(config, url, token, instructions, prompt, deadline, on_text, active):
    from workbench_mcp.analyst import operations
    allowed = set(operations())
    with tempfile.TemporaryDirectory(prefix='workbench-analyst-') as folder:
        root = Path(folder)
        (root / 'config').mkdir()
        (root / 'config/config.yaml').write_text('extensions: {}\n', encoding='utf-8')
        env = process_environment(root, config)
        flags = {'creationflags': subprocess.CREATE_NO_WINDOW} if os.name == 'nt' else {}
        # A separate version check is bounded and does not invoke inference.
        check = await asyncio.create_subprocess_exec(config['executable'], '--version', env=env, cwd=root,
            stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.DEVNULL, **flags)
        try:
            version, _ = await asyncio.wait_for(check.communicate(), 10)
        except BaseException:
            if check.returncode is None:
                check.kill()
                await check.wait()
            raise
        if version.decode().strip() not in {GOOSE_VERSION, 'goose ' + GOOSE_VERSION}:
            raise AcpError('Goose version does not match the reviewed ACP contract')
        process = await asyncio.create_subprocess_exec(config['executable'], 'acp', env=env, cwd=root,
            stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.DEVNULL, limit=1024*1024, **flags)
        client = AcpClient(process, on_text)
        async def execute():
            initialized = await client.request('initialize', {'protocolVersion': 1, 'clientCapabilities': {}})
            if initialized.get('protocolVersion') != 1:
                raise AcpError('Unsupported ACP protocol')
            session = await client.request('session/new', {'cwd': str(root), 'mcpServers': [{
                'name': 'workbench', 'command': sys.executable, 'args': ['-m', 'workbench_mcp.analyst'],
                'env': [{'name': 'WORKBENCH_ANALYST_API_URL', 'value': url},
                        {'name': 'WORKBENCH_ANALYST_CAPABILITY', 'value': token}]}]})
            client.session_id = session['sessionId']
            params = {'sessionId': client.session_id}
            extensions = await client.request('_goose/unstable/session/extensions/list', params)
            for entry in extensions['extensions']:
                if entry['extensionKey'] != 'workbench':
                    await client.request('_goose/unstable/session/extensions/remove', params | {'extensionKey': entry['extensionKey']})
            discovered = await client.request('_goose/unstable/tools/list', params)
            names = {item['name'] for item in discovered['tools']}
            if names != {'workbench__' + name for name in allowed}:
                raise AcpError('Goose tools do not match the exact read-only analyst allowlist')
            await client.request('_goose/unstable/session/system-prompt/set', params | {
                'mode': 'set', 'key': 'workbench-analyst', 'text': instructions})
            return await client.request('session/prompt', params | {'prompt': [{'type': 'text', 'text': prompt}]})
        task = asyncio.create_task(execute())
        try:
            while not task.done():
                if time.time() >= deadline:
                    raise AcpError('Analyst turn deadline reached')
                if not active():
                    raise AcpError('Analyst turn stopped')
                await asyncio.wait({task}, timeout=0.25)
            result = await task
            if result.get('stopReason') != 'end_turn':
                raise AcpError('Goose stopped before completing the answer')
            return client.answer
        finally:
            task.cancel()
            with contextlib.suppress(Exception, asyncio.CancelledError):
                await task
            await client.close()
