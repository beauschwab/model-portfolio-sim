"""Opt-in pinned Goose integration check against a loopback model and evidence fixture.

By default no real model credentials, user database or financial worker is used.
--live sends a synthetic prompt and evidence to the configured OpenRouter account.
Install the API analysts extra first.
Run: uv run --no-sync --project apps/api python scripts/check_goose_analyst.py /absolute/goose.exe
"""
import argparse
import asyncio
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'apps/api'))
from app.analyst_acp import run_turn
from analysts import DEFAULT_EXECUTABLE, load_local_config


class Fixture(BaseHTTPRequestHandler):
    evidence_calls = 0
    inference_calls = 0

    def log_message(self, *args):
        pass

    def reply(self, value):
        body = json.dumps(value).encode()
        self.send_response(200); self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)

    def do_GET(self):
        self.reply({'object': 'list', 'data': [{'id': 'fixture-model', 'name': 'Fixture',
            'object': 'model', 'owned_by': 'test', 'context_length': 128000,
            'supported_parameters': ['tools'], 'pricing': {'prompt': '0', 'completion': '0'}}]})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        if self.path == '/analysts/tools/call':
            assert self.headers['Authorization'] == 'Bearer fixture-capability'
            assert body == {'tool': 'get_state', 'arguments': {}}
            Fixture.evidence_calls += 1
            return self.reply({'evidence_id': 'E123abc', 'sha256': 'fixture', 'ok': True,
                               'data': {'revision': 7, 'fixture': True}})
        if self.path not in {'/v1/chat/completions', '/api/v1/chat/completions'}:
            raise RuntimeError('Unexpected inference route: '+self.path)
        assert self.headers['Authorization'] == 'Bearer loopback-fixture-only'
        Fixture.inference_calls += 1
        returned = any(x['role'] == 'tool' for x in body['messages'])
        if returned:
            delta = {'role': 'assistant', 'content': 'Fixture revision 7 is supported by [E123abc].'}
            finish = 'stop'
        else:
            names = [x['function']['name'] for x in body['tools']]
            name = next(x for x in names if x.endswith('__get_state'))
            delta = {'role': 'assistant', 'tool_calls': [{'index': 0, 'id': 'call_fixture', 'type': 'function',
                     'function': {'name': name, 'arguments': '{}'}}]}
            finish = 'tool_calls'
        if body.get('stream'):
            self.send_response(200); self.send_header('Content-Type', 'text/event-stream'); self.end_headers()
            for change, reason in [(delta, None), ({}, finish)]:
                chunk = {'id': 'fixture', 'object': 'chat.completion.chunk', 'created': int(time.time()),
                    'model': 'fixture-model', 'choices': [{'index': 0, 'delta': change, 'finish_reason': reason}]}
                self.wfile.write(('data: '+json.dumps(chunk)+'\n\n').encode())
            self.wfile.write(b'data: [DONE]\n\n'); self.wfile.flush()
        else:
            self.reply({'id': 'fixture', 'object': 'chat.completion', 'created': int(time.time()),
                'model': 'fixture-model', 'choices': [{'index': 0, 'message': delta, 'finish_reason': finish}],
                'usage': {'prompt_tokens': 20, 'completion_tokens': 10, 'total_tokens': 30}})


async def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', nargs='?', type=Path, default=DEFAULT_EXECUTABLE)
    parser.add_argument('--provider', choices=['openai', 'openrouter'], default='openai')
    parser.add_argument('--live', action='store_true')
    args = parser.parse_args()
    if args.live:
        if args.provider != 'openrouter':
            parser.error('--live requires --provider openrouter')
        load_local_config()
        from app.analyst_config import runtime_config
        config = runtime_config()
        if config['provider'] != 'openrouter':
            parser.error('The configured provider must be openrouter')
        # Live check always targets the official service, without optional request overrides.
        os.environ['WORKBENCH_GOOSE_ENV_KEYS'] = ''
    else:
        config = dict(executable=str(args.executable.resolve(strict=True)), provider=args.provider, model='fixture-model')
    server = ThreadingHTTPServer(('127.0.0.1', 0), Fixture)
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    url = f'http://127.0.0.1:{server.server_port}'
    if not args.live:
        # The subprocess receives only these fixture settings, never account credentials.
        if args.provider == 'openrouter':
            os.environ['WORKBENCH_GOOSE_ENV_KEYS'] = 'OPENROUTER_HOST'
            os.environ['OPENROUTER_HOST'] = url
            os.environ['OPENROUTER_API_KEY'] = 'loopback-fixture-only'
        else:
            os.environ['WORKBENCH_GOOSE_ENV_KEYS'] = 'OPENAI_HOST,OPENAI_API_KEY,OPENAI_BASE_PATH'
            os.environ['OPENAI_HOST'] = url
            os.environ['OPENAI_BASE_PATH'] = 'v1/chat/completions'
            os.environ['OPENAI_API_KEY'] = 'loopback-fixture-only'
    try:
        answer = await run_turn(config,
            url, 'fixture-capability', 'Call get_state exactly once with empty arguments. Report the returned revision '
            'in one sentence and cite its evidence_id in square brackets. Do not call any other tool.',
            'Inspect fixture workspace state.', time.time()+90, lambda _: None, lambda: True)
        assert Fixture.evidence_calls == 1, Fixture.evidence_calls
        if not args.live:
            assert Fixture.inference_calls == 2, Fixture.inference_calls
        assert '[E123abc]' in answer
        print(json.dumps({'goose': '1.57.0', 'provider': config['provider'],
            'model': config['model'] if args.live else 'loopback fixture',
            'fixture_inference_calls': Fixture.inference_calls,
            'mcp_evidence_calls': Fixture.evidence_calls, 'result': 'passed', 'live_provider_validated': args.live}))
    finally:
        server.shutdown(); server.server_close()


if __name__ == '__main__':
    try:
        asyncio.run(main())
    except (ValueError, FileNotFoundError) as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(1)
