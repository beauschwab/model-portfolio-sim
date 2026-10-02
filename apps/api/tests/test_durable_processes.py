"""Real separate API/worker processes; no shared module state or mocks."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import time
import uuid

import httpx
import pytest


def port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


@contextmanager
def server(module, number, environment, log):
    with log.open('wb') as stream:
        process = subprocess.Popen([sys.executable, '-m', 'uvicorn', module, '--host', '127.0.0.1', '--port', str(number)],
            cwd=Path(__file__).resolve().parents[1], env=environment, stdout=stream, stderr=stream)
        try:
            yield process
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)


def wait_for(fn, *, timeout=60):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        try:
            result = fn()
            if result:
                return result
        except httpx.TransportError:
            pass
        time.sleep(.2)
    raise AssertionError('server/job did not become ready')


@pytest.mark.parametrize('backend', ['sqlite'] + (['postgres'] if os.getenv('TEST_POSTGRES_URL') else []))
@pytest.mark.parametrize('compute_backend', ['rust'])
def test_api_worker_restart_and_queued_snapshot(tmp_path, backend, compute_backend):
    from portfolio_risk.core.native import library_path
    if compute_backend == 'rust' and not library_path().is_file():
        pytest.skip('build native product backend')
    api_port, worker_port = port(), port()
    env = os.environ | {
        'WORKBENCH_ENV': 'development', 'WORKBENCH_EXECUTION': 'external',
        'DATABASE_URL': os.environ['TEST_POSTGRES_URL'] if backend == 'postgres' else f'sqlite:///{tmp_path / "process.db"}',
        'ARTIFACT_URL': str(tmp_path / 'objects'),
        'WORKBENCH_WORKER_URL': f'http://127.0.0.1:{worker_port}',
        'WORKBENCH_API_TOKEN': 'api-test', 'WORKBENCH_WORKER_TOKEN': 'worker-test',
        'WORKBENCH_TENANT_ID': 'process-test', 'WORKBENCH_WORKSPACE_ID': uuid.uuid4().hex,
        'NUMBA_NUM_THREADS': '2', 'WORKBENCH_LEASE_SECONDS': '6',
    }
    with httpx.Client(base_url=f'http://127.0.0.1:{api_port}', headers={'Authorization': 'Bearer api-test'}, timeout=20) as client:
        with server('app.main:app', api_port, env, tmp_path / 'api.log'):
            wait_for(lambda: client.get('/health').status_code == 200)
            settings = client.get('/settings').json() | {'n_paths': 32, 'n_paths_base': 32, 'n_threads': 2, 'horizon_months': 6, 'compute_backend': compute_backend}
            assert client.put('/settings', json=settings).status_code == 200
            # Submit while the worker is absent. A later edit cannot change this job.
            submitted = client.post('/run', json={'kind': 'pricing', 'books': ['loans']}).json()
            jid = submitted['id']
            assert client.get(f'/jobs/{jid}').json()['status'] == 'queued'
            assert client.put('/settings', json=settings | {'compute_backend': 'python'}).status_code == 422
            assert client.put('/settings', json=settings | {'seed': 987}).status_code == 200
        with server('app.main:app', api_port, env, tmp_path / 'api-restarted.log'):
            wait_for(lambda: client.get('/health').status_code == 200)
            assert client.get('/settings').json()['seed'] == 987
            with server('app.worker:app', worker_port, env, tmp_path / 'worker.log'):
                done = wait_for(lambda: (r if (r := client.get(f'/jobs/{jid}').json())['status'] in ('done', 'error') else None))
                assert done['status'] == 'done', (done, (tmp_path / 'worker.log').read_text())
                manifest = client.get(f'/jobs/{jid}/manifest').json()
                assert manifest['settings']['seed'] == settings['seed']
                assert manifest['settings']['compute_backend'] == compute_backend
                assert manifest['revision'] == submitted['revision']
                result = client.get(f'/jobs/{jid}/result').content
                assert result.startswith(b'ARW1')
                table = next(iter(manifest['tables']))
                assert client.get(f'/jobs/{jid}/table', params={'path': table, 'limit': 2}).status_code == 200
                assert client.put('/settings', json=settings).status_code == 200
                library_job = client.post('/run', json={'kind': 'unitlib'}).json()['id']
                library = wait_for(lambda: (r if (r := client.get(f'/jobs/{library_job}').json())['status'] in ('done', 'error') else None), timeout=120)
                assert library['status'] == 'done', (library, (tmp_path / 'worker.log').read_text())
                evaluated = client.post('/strategy/eval', json=[])
                assert evaluated.status_code == 200, evaluated.text
                allocation_before = evaluated.content
            # A new actor must wait for the dead owner's lease, then restore its library.
            time.sleep(6.1)
            with server('app.worker:app', worker_port, env, tmp_path / 'worker-restarted.log'):
                wait_for(lambda: client.get('/state').json().get('library_ready'), timeout=30)
                assert client.post('/strategy/eval', json=[]).content == allocation_before
                assert client.get(f'/jobs/{jid}/result').content == result


@pytest.mark.parametrize('backend', ['sqlite'] + (['postgres'] if os.getenv('TEST_POSTGRES_URL') else []))
def test_balance_stress_survives_api_restart_and_exports_parquet(tmp_path, backend):
    api_port, worker_port = port(), port()
    env = os.environ | {
        'WORKBENCH_ENV': 'development', 'WORKBENCH_EXECUTION': 'external',
        'DATABASE_URL': os.environ['TEST_POSTGRES_URL'] if backend == 'postgres' else f'sqlite:///{tmp_path / "stress.db"}',
        'ARTIFACT_URL': str(tmp_path / 'objects'),
        'WORKBENCH_WORKER_URL': f'http://127.0.0.1:{worker_port}',
        'WORKBENCH_API_TOKEN': 'api-test', 'WORKBENCH_WORKER_TOKEN': 'worker-test',
        'WORKBENCH_TENANT_ID': 'stress-test', 'WORKBENCH_WORKSPACE_ID': uuid.uuid4().hex,
        'NUMBA_NUM_THREADS': '2',
    }
    with httpx.Client(base_url=f'http://127.0.0.1:{api_port}', headers={'Authorization': 'Bearer api-test'}, timeout=20) as client:
        with server('app.main:app', api_port, env, tmp_path / 'api.log'):
            wait_for(lambda: client.get('/health').status_code == 200)
            example = client.get('/balance-stress/example').json()
            spec = example['specification']
            spec.update(horizon_days=30, reverse_severities=[])
            submitted = client.post('/balance-stress/run', json={'specification': spec, 'expected_revision': example['revision']})
            assert submitted.status_code == 200, submitted.text
            jid = submitted.json()['id']
            assert client.get(f'/jobs/{jid}').json()['status'] == 'queued'
        with server('app.main:app', api_port, env, tmp_path / 'api-restarted.log'):
            wait_for(lambda: client.get('/health').status_code == 200)
            with server('app.worker:app', worker_port, env, tmp_path / 'worker.log'):
                done = wait_for(lambda: (r if (r := client.get(f'/jobs/{jid}').json())['status'] in ('done', 'error') else None))
                assert done['status'] == 'done', (done, (tmp_path / 'worker.log').read_text())
                manifest = client.get(f'/jobs/{jid}/manifest').json()
                assert any('summary' in p for p in manifest['tables'])
                table = next(p for p in manifest['tables'] if p.endswith('summary'))
                rows = client.get(f'/jobs/{jid}/table', params={'path': table}).json()
                assert rows['total'] == 10
                data = client.get(f'/jobs/{jid}/parquet', params={'path': table}).content
                assert data[:4] == data[-4:] == b'PAR1'
                result = client.get(f'/jobs/{jid}/result').content
                size = struct.unpack_from('<I', result, 4)[0]
                payload = json.loads(result[8:8+size])
                assert payload['specification'] == spec
            assert client.get(f'/jobs/{jid}/result').content == result
