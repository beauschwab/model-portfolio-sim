"""Exercise API -> durable worker -> artifacts -> paged/downloaded raw parity.

Without --base-url starts isolated local SQLite/API/worker processes. Remote runs
require an explicit endpoint and existing WORKBENCH_VALIDATION_TOKEN if needed.
No books/settings are edited. S3 is used only when explicitly passed for the local
stack; credentials come from the existing AWS provider chain, never arguments.
"""
import argparse
from contextlib import ExitStack
import io
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
import uuid

import httpx
import polars as pl
from polars.testing import assert_frame_equal
import psutil
from benchmark_mixed_state import mixed_fixture

ROOT = Path(__file__).resolve().parents[1]


def port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0)); return sock.getsockname()[1]


def stop(process):
    if process.poll() is not None: return
    parent = psutil.Process(process.pid)
    children = parent.children(recursive=True)
    for child in reversed(children):
        try: child.kill()
        except psutil.NoSuchProcess: pass
    process.kill(); process.wait(timeout=15)
    psutil.wait_procs(children, timeout=15)


def compare(client, manifests):
    left, right = manifests
    def frames(manifest, name):
        for i in range(len(manifest['tables'][name]['parts'])):
            response = client.get(f"/jobs/{manifest['id']}/parquet", params={'path':name, 'partition':i})
            response.raise_for_status()
            yield pl.read_parquet(io.BytesIO(response.content))
    counts = {}
    assert set(left['tables']) == set(right['tables'])
    for name in left['tables']:
        a, b = iter(frames(left,name)), iter(frames(right,name))
        af, bf, count = next(a,None), next(b,None), 0
        while af is not None and bf is not None:
            size = min(af.height, bf.height)
            assert_frame_equal(af.head(size), bf.head(size), check_exact=False, rel_tol=1e-12, abs_tol=1e-8)
            count += size
            af = af.slice(size) if size < af.height else next(a,None)
            bf = bf.slice(size) if size < bf.height else next(b,None)
        assert af is None and bf is None, name
        counts[name] = count
    return counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-url')
    parser.add_argument('--artifact-url')
    parser.add_argument('--local-s3', action='store_true', help='Use an isolated Moto S3 HTTP server; requires moto[server]')
    parser.add_argument('--positions', type=int, default=1000)
    parser.add_argument('--days', type=int, default=30)
    parser.add_argument('--memory-mib', type=int, default=4096)
    parser.add_argument('--output', default='docs/reviews/2026-09-29-deployment-capacity.json')
    args = parser.parse_args()
    if args.local_s3 and (args.base_url or args.artifact_url): parser.error('--local-s3 requires the isolated local stack')
    if not 24 <= args.positions <= 60000: parser.error('positions must be 24..60000')
    processes = []
    with ExitStack() as stack:
        if args.base_url:
            url = args.base_url.rstrip('/')
        else:
            directory = Path(stack.enter_context(tempfile.TemporaryDirectory(prefix='wbv-')))
            api_port, worker_port = port(), port()
            env = os.environ | {'DATABASE_URL':f'sqlite:///{(directory / "state.db").as_posix()}',
                'ARTIFACT_URL':args.artifact_url or str(directory/'obj'), 'WORKBENCH_ENV':'development',
                'WORKBENCH_TENANT_ID':'capacity-validation', 'WORKBENCH_WORKSPACE_ID':uuid.uuid4().hex,
                'WORKBENCH_EXECUTION':'external', 'WORKBENCH_WORKER_URL':f'http://127.0.0.1:{worker_port}',
                'WORKBENCH_API_TOKEN':'', 'WORKBENCH_WORKER_TOKEN':'', 'WORKBENCH_SEED_DEMO':'1',
                'WORKBENCH_BALANCE_LARGE_BOOK':'1', 'WORKBENCH_SCRATCH_DIR':str(directory/'scratch'),
                'NUMBA_NUM_THREADS':'4'}
            if args.local_s3:
                import boto3
                s3_port = port(); endpoint = f'http://127.0.0.1:{s3_port}'
                log = stack.enter_context((directory/'s3.log').open('wb'))
                process = subprocess.Popen([sys.executable,'-m','moto.server','--host','127.0.0.1','--port',str(s3_port)],
                    stdout=log,stderr=log,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
                processes.append(process); stack.callback(stop,process)
                deadline = time.monotonic()+30
                while time.monotonic()<deadline:
                    try:
                        if httpx.get(endpoint,timeout=1).status_code==200: break
                    except httpx.TransportError: pass
                    time.sleep(.2)
                else: raise TimeoutError('S3 emulator did not start')
                boto3.client('s3',endpoint_url=endpoint,region_name='us-east-1',aws_access_key_id='test',
                    aws_secret_access_key='test').create_bucket(Bucket='validation-bucket')
                env.update(ARTIFACT_URL='s3://validation-bucket/capacity', S3_ENDPOINT_URL=endpoint,
                    AWS_ACCESS_KEY_ID='test', AWS_SECRET_ACCESS_KEY='test', AWS_DEFAULT_REGION='us-east-1')
            for module, number, health in [('main',api_port,'health'), ('worker',worker_port,'state')]:
                log = stack.enter_context((directory/f'{module}.log').open('wb'))
                process = subprocess.Popen([sys.executable,'-m','uvicorn',f'app.{module}:app','--port',str(number)],
                    cwd=ROOT/'apps/api', env=env, stdout=log, stderr=log,
                    creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
                processes.append(process); stack.callback(stop,process)
                deadline = time.monotonic()+60
                while time.monotonic()<deadline:
                    if process.poll() is not None: raise RuntimeError((directory/f'{module}.log').read_text()[-4000:])
                    try:
                        if httpx.get(f'http://127.0.0.1:{number}/{health}',timeout=1).status_code==200: break
                    except httpx.TransportError: pass
                    time.sleep(.2)
                else: raise TimeoutError('server did not start')
            url = f'http://127.0.0.1:{api_port}'
        token = os.getenv('WORKBENCH_VALIDATION_TOKEN') if args.base_url else None
        client = stack.enter_context(httpx.Client(base_url=url, timeout=180,
            headers={'Authorization':f'Bearer {token}'} if token else {}))
        peak, exceeded, monitor_stop = [0.], [], threading.Event()
        def monitor():
            while not monitor_stop.wait(.025):
                rss = 0
                for process in processes:
                    try:
                        parent = psutil.Process(process.pid)
                        rss += sum(p.memory_info().rss for p in [parent]+parent.children(recursive=True))
                    except psutil.NoSuchProcess: pass
                peak[0] = max(peak[0], rss/1024**2)
                if peak[0] > args.memory_mib:
                    exceeded.append(True)
                    for process in reversed(processes): stop(process)
                    return
        thread = threading.Thread(target=monitor, daemon=True); thread.start()
        def stop_monitor(): monitor_stop.set(); thread.join(timeout=3)
        stack.callback(stop_monitor)
        manifests, samples = [], []
        specification = mixed_fixture(args.positions,args.days)
        for backend in ['python','rust']:
            peak[0] = 0.
            revision = client.get('/state').json()['revision']
            start = time.perf_counter()
            response = client.post('/balance-stress/stream', json=dict(expected_revision=revision,
                specification=specification, backend=backend, large_book=args.positions>2000))
            response.raise_for_status(); jid = response.json()['id']
            deadline = time.monotonic()+1800
            while time.monotonic()<deadline:
                response = client.get(f'/jobs/{jid}'); response.raise_for_status(); status=response.json()
                if status['status'] in ('done','error'): break
                time.sleep(.2)
            else: raise TimeoutError(f'job {jid} exceeded validation deadline')
            if status['status'] != 'done': raise RuntimeError(status.get('detail'))
            seconds = time.perf_counter()-start
            run_peak = peak[0]
            response = client.get(f'/jobs/{jid}/manifest'); response.raise_for_status(); manifest=response.json()
            manifests.append(manifest)
            page = client.get(f'/jobs/{jid}/table',params={'path':'/journal','offset':100,'limit':100})
            page.raise_for_status(); assert len(page.json()['rows'])==100
            samples.append(dict(backend=backend, seconds=seconds, job_id=jid,
                api_worker_native_peak_rss_mib=run_peak if processes else None,
                result_manifest_bytes=manifest['result']['bytes'],
                request_manifest_bytes=manifest['request']['bytes'],
                journal_rows=manifest['tables']['/journal']['rows'], identity=manifest['identity']))
            print(f'{backend}: {seconds:.3f}s, combined services {run_peak:.0f} MiB',flush=True)
        counts = compare(client,manifests)
        assert not exceeded
        report = dict(scope='submission through durable completion; one sample/backend in Python-then-Rust order on a persistent worker; includes HTTP, request persistence and object publication; excludes startup and post-run parity reads',
            environment='remote deployment' if args.base_url else 'isolated local SQLite/API/worker',
            storage='local S3 HTTP emulator (not live AWS)' if args.local_s3 else ('explicit configured object store' if args.artifact_url else ('deployment configured' if args.base_url else 'local Parquet')),
            positions=args.positions, days=args.days, scenarios=2, samples=samples, raw_parity_rows=counts,
            parity='all saved/downloaded financial rows in order at rtol=1e-12, atol=1e-8',
            memory='combined API+worker+native RSS, plus emulator when selected, sampled at 25ms; excludes harness/download comparison process; remote RSS unavailable')
        out=Path(args.output); out.parent.mkdir(parents=True,exist_ok=True)
        out.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
        print(out,flush=True)


if __name__ == '__main__': main()
