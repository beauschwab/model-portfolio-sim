"""Explicit deployment configuration and signed identities for the analyst pilot."""
import base64
import hashlib
import hmac
import json
import os
from pathlib import Path
import re
import time

from fastapi import HTTPException

GOOSE_VERSION = '1.57.0'


def secret():
    value = os.getenv('WORKBENCH_ANALYST_SECRET', '')
    if len(value) < 32:
        raise ValueError('Set WORKBENCH_ANALYST_SECRET to at least 32 random characters')
    return value.encode()


def runtime_config():
    if os.getenv('WORKBENCH_ANALYST_ENABLED') != '1':
        raise ValueError('Embedded analysts are disabled')
    secret()
    executable = Path(os.getenv('WORKBENCH_GOOSE_EXECUTABLE', ''))
    expected = os.getenv('WORKBENCH_GOOSE_SHA256', '')
    if not executable.is_absolute() or not executable.is_file() or not re.fullmatch('[a-f0-9]{64}', expected):
        raise ValueError('Configure an absolute Goose executable and its SHA-256')
    actual = hashlib.sha256(executable.read_bytes()).hexdigest()
    if not hmac.compare_digest(actual, expected):
        raise ValueError('Configured Goose executable hash does not match')
    provider, model = os.getenv('WORKBENCH_GOOSE_PROVIDER', ''), os.getenv('WORKBENCH_GOOSE_MODEL', '')
    if not provider or not model:
        raise ValueError('Configure the approved Goose provider and model')
    # ACP-backed coding providers can introduce another tool runtime. The pilot uses direct inference only.
    if provider not in {'openai', 'anthropic', 'ollama', 'google', 'openrouter', 'azure', 'bedrock', 'databricks'}:
        raise ValueError('Provider is not reviewed for this direct-inference pilot')
    if provider == 'openrouter' and not os.getenv('OPENROUTER_API_KEY', '').strip():
        raise ValueError('Set OPENROUTER_API_KEY in the API environment or apps/api/.env.analysts')
    return dict(executable=str(executable), sha256=actual, provider=provider, model=model, version=GOOSE_VERSION)


def principal(request, scope):
    local = os.getenv('WORKBENCH_ANALYST_LOCAL_USER', '')
    if local and os.getenv('WORKBENCH_ENV', 'development') == 'development':
        return local
    user = request.headers.get('x-workbench-user', '')
    timestamp = request.headers.get('x-workbench-identity-time', '')
    signature = request.headers.get('x-workbench-identity-signature', '')
    if not re.fullmatch(r'[A-Za-z0-9@._:-]{1,200}', user):
        raise HTTPException(401, 'Authenticated analyst identity required')
    try:
        if abs(time.time()-int(timestamp)) > 60:
            raise ValueError()
        message = '\n'.join([user, timestamp, scope['tenant'], scope['workspace'], request.method, request.url.path])
        expected = hmac.new(secret(), message.encode(), hashlib.sha256).hexdigest()
        if not hmac.compare_digest(signature, expected):
            raise ValueError()
    except ValueError:
        raise HTTPException(401, 'Invalid analyst gateway identity')
    return user


def capability_signature(payload):
    # Domain-separated from gateway identity signatures, which share the deployment secret.
    return hmac.new(secret(), b'analyst-capability-1\n' + payload.encode(), hashlib.sha256).hexdigest()


def capability(turn, scope):
    raw = json.dumps(dict(turn=turn['id'], exp=turn['deadline'], **scope), sort_keys=True).encode()
    payload = base64.urlsafe_b64encode(raw).decode().rstrip('=')
    signature = capability_signature(payload)
    return 'analyst1.' + payload + '.' + signature


def verify_capability(token, scope):
    try:
        if len(token) > 2048:
            raise ValueError()
        prefix, payload, signature = token.split('.')
        expected = capability_signature(payload)
        if prefix != 'analyst1' or not hmac.compare_digest(signature, expected):
            raise ValueError()
        value = json.loads(base64.urlsafe_b64decode(payload + '=' * (-len(payload) % 4)))
        if value['exp'] <= time.time() or any(value[k] != v for k, v in scope.items()):
            raise ValueError()
        return value['turn']
    except (ValueError, KeyError, TypeError):
        raise HTTPException(401, 'Invalid or expired analyst capability')
