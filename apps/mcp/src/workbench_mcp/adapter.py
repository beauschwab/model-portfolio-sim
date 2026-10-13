"""Validated, bounded HTTP transport. No pricing, storage or workspace ownership."""
from __future__ import annotations

import base64
import datetime
import json
import math
import struct
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import quote, urlsplit

import httpx
from jsonschema import Draft202012Validator
from .profiles import PROFILES, allows, validate_arguments

PERMISSIONS = frozenset({'read', 'compute', 'edit', 'publish', 'cancel'})
MAX_REQUEST = 46 * 1024 * 1024  # includes base64 expansion of the API's 32 MiB tape bound
MAX_RESPONSE = 16 * 1024 * 1024
MAX_OUTPUT = 2 * 1024 * 1024


def catalog():
    return json.loads(Path(__file__).with_name('catalog.json').read_text(encoding='utf-8'))


class ToolFailure(Exception):
    def __init__(self, code, message, *, status=None, retryable=False, recovery=None):
        self.payload = dict(code=code, message=message, status=status,
                            retryable=retryable, recovery=recovery)
        super().__init__(message)


@dataclass(frozen=True)
class Config:
    api_url: str = 'http://127.0.0.1:8000'
    token: str = ''
    permissions: frozenset[str] = frozenset({'read'})
    actor: str = 'local-agent'
    timeout: float = 30.0
    profile: str = 'workbench'

    def __post_init__(self):
        url = urlsplit(self.api_url)
        if url.scheme not in {'http', 'https'} or not url.hostname or url.username or url.password or url.query or url.fragment:
            raise ValueError('API URL must be a fixed HTTP(S) origin/base path without credentials or query')
        if url.scheme == 'http' and url.hostname not in {'127.0.0.1', 'localhost', '::1'}:
            raise ValueError('Remote API connections require HTTPS')
        if not self.permissions <= PERMISSIONS:
            raise ValueError('Unknown MCP permission')
        if self.profile not in PROFILES:
            raise ValueError('Unknown agent profile')
        if not 0 < self.timeout <= 120 or not self.actor or len(self.actor) > 200:
            raise ValueError('Invalid timeout or actor')


def safe_json(value):
    if isinstance(value, dict): return {str(k): safe_json(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)): return [safe_json(v) for v in value]
    if isinstance(value, (datetime.date, datetime.datetime)): return value.isoformat()
    if isinstance(value, bytes): return {'base64': base64.b64encode(value).decode()}
    if isinstance(value, float) and not math.isfinite(value): return None
    return value


def arrow_view(data, *, offset=0, limit=100):
    """Decode ARW1 as bounded table pages; never perform financial calculations."""
    import pyarrow as pa
    if data[:4] != b'ARW1' or len(data) < 12:
        raise ValueError('Invalid ARW1 envelope')
    size = struct.unpack_from('<I', data, 4)[0]
    pos = 8 + size
    if pos + 4 > len(data): raise ValueError('Truncated ARW1 skeleton')
    skeleton = json.loads(data[8:pos])
    count = struct.unpack_from('<I', data, pos)[0]; pos += 4
    if count > 10000 or pos + 4 * count > len(data): raise ValueError('Invalid ARW1 table count')
    sizes = struct.unpack_from(f'<{count}I', data, pos); pos += 4 * count
    frames = []
    for length in sizes:
        if pos + length > len(data): raise ValueError('Truncated ARW1 table')
        table = pa.ipc.open_file(pa.BufferReader(data[pos:pos + length])).read_all()
        frames.append(dict(columns=table.column_names, schema={f.name: str(f.type) for f in table.schema},
            rows=safe_json(table.slice(offset, limit).to_pylist()), total=table.num_rows,
            offset=offset, limit=limit, truncated=offset + limit < table.num_rows))
        pos += length
    if pos != len(data): raise ValueError('Trailing ARW1 bytes')
    def visit(node):
        if isinstance(node, dict):
            if set(node) == {'__arrow__'}:
                index = node['__arrow__']
                if type(index) is not int or not 0 <= index < len(frames): raise ValueError('Invalid ARW1 table index')
                return frames[index]
            return {k: visit(v) for k, v in node.items()}
        if isinstance(node, list): return [visit(v) for v in node]
        return node
    return visit(skeleton)


class Adapter:
    def __init__(self, config: Config, *, transport=None):
        self.config = config
        self.catalog = catalog()
        self.operations = self.catalog['operations']
        self.client = httpx.AsyncClient(base_url=config.api_url.rstrip('/') + '/',
            headers={'Authorization': f'Bearer {config.token}'} if config.token else {},
            timeout=config.timeout, follow_redirects=False, trust_env=False, transport=transport)

    async def close(self):
        await self.client.aclose()

    def visible(self):
        visible = {k: v for k, v in self.operations.items() if v['permission'] in self.config.permissions
                   and allows(self.config.profile, k, v)}
        role = PROFILES[self.config.profile]
        if 'post_run' in visible and role.run_kinds is not None:
            # Advertise the same restrictions enforced below; never mutate the shared catalog.
            op = visible['post_run']
            schema = op['inputSchema']
            restrictions = {'kind': {'enum': sorted(role.run_kinds)}}
            if self.config.profile == 'strategy':
                restrictions['calibration_mode'] = {'const': 'hold'}
            visible['post_run'] = op | {'inputSchema': schema | {'properties': schema['properties'] | {
                'body': {'allOf': [schema['properties']['body'], {'properties': restrictions}]}}}}
        return visible

    async def request(self, method, path, *, query=None, headers=None, body=None):
        try:
            async with self.client.stream(method, path.lstrip('/'), params=query, headers=headers,
                                          content=body) as response:
                chunks, size = [], 0
                async for chunk in response.aiter_bytes():
                    size += len(chunk)
                    if size > MAX_RESPONSE:
                        raise ToolFailure('RESULT_TOO_LARGE', 'Use book rows or job manifests and paged tables.',
                                          recovery='Read get_jobs_jid_manifest and get_jobs_jid_table; reduce page size.')
                    chunks.append(chunk)
                data = b''.join(chunks)
                if response.status_code >= 300:
                    try: detail = json.loads(data).get('detail', 'API request failed')
                    except (ValueError, AttributeError): detail = 'API request failed'
                    message = json.dumps(detail, ensure_ascii=False)[:8000]
                    if self.config.token: message = message.replace(self.config.token, '[REDACTED]')
                    status = response.status_code
                    code = {401: 'UNAUTHENTICATED', 403: 'FORBIDDEN', 404: 'NOT_FOUND',
                            409: 'CONFLICT', 422: 'INVALID_INPUT', 429: 'QUEUE_FULL'}.get(status, 'API_ERROR')
                    for known in ('RESTART_REQUIRED', 'REBUILD_REQUIRED', 'CAPITAL_REFRESH_REQUIRED',
                                  'PYTHON_BACKEND_DEPRECATED', 'MODEL_VERSION_CHANGED'):
                        if known in message: code = known; break
                    recovery = ('Reload state and inspect changes; do not silently replace the expected revision.'
                                if status == 409 else 'Correct inputs or prerequisites before retrying.')
                    if status == 429: recovery = 'Wait for queue capacity, then retry with the same idempotency key.'
                    raise ToolFailure(code, message, status=status, retryable=status == 429, recovery=recovery)
                return data, response.headers.get('content-type', '')
        except (httpx.TimeoutException, httpx.NetworkError) as exc:
            raise ToolFailure('TRANSPORT_UNCERTAIN', 'No confirmed API response; the action may have been accepted.',
                recovery='Discover jobs first. Retry job submissions only with the identical request and idempotency key.') from exc

    async def call(self, name, arguments):
        operation = self.operations.get(name)
        if operation is None:
            raise ToolFailure('UNKNOWN_TOOL', 'Unknown workbench tool')
        if operation['permission'] not in self.config.permissions or not allows(self.config.profile, name, operation):
            raise ToolFailure('FORBIDDEN', 'This server process is not granted the required permission')
        error = next(Draft202012Validator(operation['inputSchema']).iter_errors(arguments), None)
        if error:
            raise ToolFailure('INVALID_INPUT', f'{list(error.absolute_path)}: {error.message}'[:2000])
        denied = validate_arguments(self.config.profile, name, arguments)
        if denied:
            raise ToolFailure('FORBIDDEN', denied)
        try: encoded = json.dumps(arguments, allow_nan=False).encode()
        except (ValueError, TypeError) as exc: raise ToolFailure('INVALID_INPUT', 'Arguments must be finite JSON') from exc
        if len(encoded) > MAX_REQUEST: raise ToolFailure('INPUT_TOO_LARGE', 'Request exceeds MCP transport bound')
        path = operation['path']
        for key, value in arguments.get('path', {}).items():
            segment = str(value)
            if segment in {'.', '..'} or any(c in segment for c in '/\\%?#\x00'):
                raise ToolFailure('INVALID_INPUT', 'Invalid path segment')
            path = path.replace('{' + key + '}', quote(segment, safe=''))
        headers = {'Content-Type': 'application/json'}
        revision = arguments.get('expected_revision')
        body = arguments.get('body')
        if revision is not None:
            headers['If-Match'] = str(revision)
            if isinstance(body, dict) and body.get('expected_revision', revision) != revision:
                raise ToolFailure('INVALID_INPUT', 'Body and tool expected revisions must agree')
        if arguments.get('idempotency_key'):
            headers['Idempotency-Key'] = arguments['idempotency_key']
        query = [(k, str(x).lower() if isinstance(x, bool) else x)
                 for k, v in arguments.get('query', {}).items() if v is not None
                 for x in (v if isinstance(v, list) else [v])]
        if operation.get('transport') == 'artifact':
            url = self.client.build_request('GET', path.lstrip('/'), params=query).url
            return dict(download_url=str(url), authentication='Use the configured API authentication; credentials are never embedded.',
                        verified=False, description='Download reference; existence and permission are checked by the API when fetched.')
        data, content_type = await self.request(operation['method'], path, query=query, headers=headers,
            body=json.dumps(body, allow_nan=False).encode() if 'body' in arguments else None)
        try:
            if 'application/vnd.arrow-envelope' in content_type:
                result = arrow_view(data, **arguments.get('view', {}))
            else: result = json.loads(data)
        except (ValueError, IndexError, struct.error) as exc:
            raise ToolFailure('INVALID_RESPONSE', 'API returned an invalid response envelope') from exc
        result = safe_json(result)
        if len(json.dumps(result, allow_nan=False).encode()) > MAX_OUTPUT:
            raise ToolFailure('RESULT_TOO_LARGE', 'Response exceeds model-context bound; request a smaller page or manifest.')
        return result
