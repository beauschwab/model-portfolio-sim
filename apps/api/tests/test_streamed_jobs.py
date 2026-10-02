"""Native durable jobs, bounded partition access and failure fencing."""
import hashlib
import json
from copy import deepcopy

import pytest
from fastapi import HTTPException
from app import store, persistence as db, worker, reporting, main
from app.artifacts import Codec, Objects
from app.schemas import StreamedBalanceStressRequest
from test_persistence import durable, BACKENDS
from portfolio_risk.analytics.balance_stress import example_specification
from portfolio_risk.analytics.balance_stream import run_streamed_balance_stress


def request():
    spec = example_specification()
    spec.update(horizon_days=30, reverse_severities=[])
    return dict(specification=spec, backend='rust', large_book=False,
                binary_sha256=db.identity()['balance_native_sha256'])


def queue(value):
    jid = store.submit('streamed_balance_stress', store.run_streamed_balance_stress,
                       value, state=store.snapshot())
    db.WORKER = 'stream-test'
    worker.STOP.clear()
    assert db.REPO.acquire(db.WORKER)
    return jid, db.REPO.claim(db.WORKER)


def test_durable_native_result_and_partition_pagination(durable, monkeypatch):
    from portfolio_risk.analytics import balance_stream
    simulate = balance_stream.run_streamed_balance_stress
    monkeypatch.setattr(balance_stream, 'run_streamed_balance_stress',
                        lambda *a, **kw: simulate(*a, **kw, partition_rows=37))
    value = request()
    original = deepcopy(value['specification'])
    jid, job = queue(value)
    value['specification']['accounts'][0]['cash'] = -1000
    worker.execute(job)
    assert db.REPO.job(jid)['status'] == 'done', db.REPO.job(jid)['detail']
    result = db.CODEC.load(db.result_ref(jid))
    public_manifest = main.run_manifest(jid)
    assert public_manifest['execution']['validation']['journal_replayed']
    assert 'specification' not in public_manifest['execution']
    assert 'specification' not in result['execution']
    request_ref = result['execution']['request_manifest']
    assert request_ref == job['request']
    assert db.CODEC.load(request_ref)['args'][0]['specification'] == original
    assert result['execution']['validation']['journal_replayed']
    tables = db.CODEC.tables(db.result_ref(jid))
    desc = tables['/journal']
    assert desc['format'] == 'partitioned-parquet'
    assert len(desc['parts']) > 1
    # A page crossing a partition boundary reads exactly those two objects.
    offset = desc['parts'][0]['rows'] - 1
    get = db.CODEC.objects.get
    reads = []
    def tracked(ref):
        assert ref != request_ref, 'Result paging must not fetch the full queued input'
        if ref['format'] == 'parquet':
            reads.append(ref)
        return get(ref)
    monkeypatch.setattr(db.CODEC.objects, 'get', tracked)
    page = reporting.table(jid, '/journal', offset=offset, limit=3)
    assert len(reads) == 2 and len(page['rows']) == 3 and page['total'] == desc['rows']
    assert main.result_parquet(jid, '/journal', partition=0).body[:4] == b'PAR1'
    with pytest.raises(HTTPException) as exc:
        main.result_parquet(jid, '/journal')
    assert exc.value.status_code == 422
    with pytest.raises(ValueError):
        reporting.table(jid, '/journal', columns=['untrusted'])
    before = store.job_result(jid)
    db.stop(); db.start(durable)
    assert store.job_result(jid) == before


@pytest.mark.parametrize('failure', ['binary', 'upload', 'cancel', 'lease'])
def test_failed_attempt_never_publishes(durable, monkeypatch, failure):
    value = request()
    if failure == 'binary':
        value['binary_sha256'] = '0' * 64
    jid, job = queue(value)
    if failure == 'upload':
        original = db.CODEC.objects.put
        uploads = []
        def failed(data, suffix):
            if suffix == 'parquet':
                uploads.append(suffix)
                if len(uploads) == 2:
                    raise OSError('injected partition upload failure')
            return original(data, suffix)
        monkeypatch.setattr(db.CODEC.objects, 'put', failed)
    elif failure in ('cancel', 'lease'):
        original = db.CODEC.import_balance_partitions
        def interrupted(path, cancelled):
            if failure == 'cancel':
                db.REPO.cancel(jid)
            else:
                db.REPO.release(db.WORKER)
            return original(path, cancelled)
        monkeypatch.setattr(db.CODEC, 'import_balance_partitions', interrupted)
    worker.execute(job)
    row = db.REPO.job(jid)
    assert row['result'] is None and row['status'] != 'done'
    if failure == 'lease':
        monkeypatch.setattr(db.CODEC, 'import_balance_partitions', original)
        db.WORKER = 'replacement'
        assert db.REPO.acquire(db.WORKER)
        replacement = db.REPO.claim(db.WORKER)
        assert replacement['attempt'] == 2
        worker.execute(replacement)
        assert db.REPO.job(jid)['status'] == 'done'


def test_endpoint_admission_and_identity(durable, monkeypatch):
    capabilities = main.balance_stress_capabilities()
    assert capabilities['durable'] and capabilities['rust'] and not capabilities['large_book']
    assert capabilities['scope_id'] == db.CODEC.objects.prefix.split('/')[-1]
    body = StreamedBalanceStressRequest(expected_revision=0, **{k:v for k,v in request().items() if k != 'binary_sha256'})
    with pytest.raises(HTTPException) as exc:
        main.streamed_balance_stress(body.model_copy(update={'large_book': True}))
    assert exc.value.status_code == 422
    with pytest.raises(HTTPException) as exc:
        main.streamed_balance_stress(body.model_copy(update={'expected_revision': 1}))
    assert exc.value.status_code == 409
    submitted = main.streamed_balance_stress(body)
    queued = db.CODEC.load(db.REPO.job(submitted['id'])['request'])
    assert queued['args'][0]['binary_sha256'] == db.identity()['balance_native_sha256']
    monkeypatch.setitem(db._IDENTITY, 'balance_native_sha256', '0' * 64)
    with pytest.raises(HTTPException) as exc:
        main.streamed_balance_stress(body)
    assert exc.value.status_code == 409


@pytest.mark.parametrize('change', ['replacement', 'selection', 'restart', 'executed'])
def test_saved_workflow_binary_identity_fences_publication(durable, monkeypatch, tmp_path, change):
    from portfolio_risk.analytics import balance_workflow
    binary = tmp_path / 'workflow-test.exe'
    binary.write_bytes(b'queued native workflow')
    monkeypatch.setenv('PORTFOLIO_WORKFLOW_RUST_BIN', str(binary))
    monkeypatch.setattr(db, '_IDENTITY', None)
    state = store.snapshot()
    state['settings'] = state['settings'].model_copy(update={'compute_backend': 'rust'})
    expected = hashlib.sha256(binary.read_bytes()).hexdigest()
    jid = store.submit('saved_balance_stress', store.run_saved_balance_stress, {}, state=state)
    assert db.CODEC.load(db.REPO.job(jid)['request'])['identity']['workflow_native_sha256'] == expected
    db.WORKER = 'workflow-identity-test'
    assert db.REPO.acquire(db.WORKER)
    job = db.REPO.claim(db.WORKER)
    calls = []
    def simulate(*args, **kwargs):
        calls.append(True)
        return {'execution': {'journal_identity': '0' * 64}}
    monkeypatch.setattr(balance_workflow, 'run_saved_book_stress', simulate)
    if change in ('replacement', 'restart'):
        binary.write_bytes(b'rebuilt native workflow')
    if change == 'selection':
        selected = tmp_path / 'selected.exe'
        selected.write_bytes(b'other native workflow')
        monkeypatch.setenv('PORTFOLIO_WORKFLOW_RUST_BIN', str(selected))
    if change == 'restart':
        monkeypatch.setattr(db, '_IDENTITY', None)
    worker.execute(job)
    row = db.REPO.job(jid)
    assert row['status'] == 'error' and row['result'] is None
    assert 'MODEL_VERSION_CHANGED' in row['detail']
    assert bool(calls) == (change == 'executed')


def test_saved_native_workflow_publishes_queued_identity(durable, monkeypatch):
    import datetime as dt
    import polars as pl
    state = store.snapshot()
    state['settings'] = state['settings'].model_copy(update={
        'compute_backend': 'rust', 'n_paths': 32, 'n_paths_base': 32, 'horizon_months': 6, 'n_threads': 4})
    book = {'loans': pl.DataFrame([dict(id='loan', face=100.,
        maturity=state['asof'] + dt.timedelta(days=365), freq_months=6,
        daycount='ACT/360', is_float=False, coupon_or_spread=.05,
        amort_type='bullet', price=100., book_yield=.05)])}
    monkeypatch.setattr(store, 'balance_sheet', lambda _: book)
    request = dict(amount_scale=1., specification=dict(version='balance-stress-2', horizon_days=30,
        accounts=[dict(id='a', entity='bank', currency='USD', cash=20., equity=120.)],
        scenarios=[dict(name='up', rate_shift=.01)]),
        position_mapping={'loans:loan': dict(account='a', kind='loan', classification='ac',
            risk_weight=1., asf_weight=0., rsf_weight=1., lcr_outflow_weight=0., hqla_weight=0.)})
    jid = store.submit('saved_balance_stress', store.run_saved_balance_stress, request, state=state)
    db.WORKER = 'native-saved-test'
    assert db.REPO.acquire(db.WORKER)
    job = db.REPO.claim(db.WORKER)
    worker.execute(job)
    assert db.REPO.job(jid)['status'] == 'done', db.REPO.job(jid)['detail']
    result = db.CODEC.load(db.result_ref(jid))
    assert result['execution']['journal_identity'] == db.identity()['workflow_native_sha256']
    assert result['execution']['partition_validation']['journal_replayed']
    assert result['journal'].height > 0 and result['closing_statements'].height > 0


def test_partition_import_s3_protocol_and_corruption(tmp_path):
    import boto3
    from botocore.stub import Stubber
    spec = example_specification()
    spec.update(horizon_days=30, reverse_severities=[])
    manifest_path = run_streamed_balance_stress(spec, tmp_path / 'run')
    manifest = json.loads(manifest_path.read_text())
    client = boto3.client('s3', region_name='us-east-1', aws_access_key_id='test', aws_secret_access_key='test')
    objects = Objects('s3://test-bucket/prefix', 'tenant', 'workspace', s3_client=client)
    codec = Codec(objects)
    with Stubber(client) as stub:
        for table in manifest['tables'].values():
            for part in table['parts']:
                raw = (manifest_path.parent / part['path']).read_bytes()
                digest = hashlib.sha256(raw).hexdigest()
                stub.add_response('put_object', {}, dict(Bucket='test-bucket',
                    Key=f'{objects.prefix}/{digest[:2]}/{digest}.parquet', Body=raw, Metadata={'sha256':digest}))
        imported = codec.import_balance_partitions(manifest_path, lambda: False)
        assert imported['journal'].descriptor()['rows'] > 0
        stub.assert_no_pending_responses()
    first = manifest['tables']['journal']['parts'][0]
    (manifest_path.parent / first['path']).write_bytes(b'corrupt')
    with pytest.raises(ValueError, match='checksum'):
        codec.import_balance_partitions(manifest_path, lambda: False)


@pytest.mark.parametrize('backend', BACKENDS)
def test_streamed_job_across_real_api_worker_restart(tmp_path, backend):
    import os
    import uuid
    import httpx
    from test_durable_processes import port, server, wait_for
    api_port, worker_port = port(), port()
    env = os.environ | {
        'WORKBENCH_ENV': 'development', 'WORKBENCH_EXECUTION': 'external',
        'DATABASE_URL': os.environ['TEST_POSTGRES_URL'] if backend == 'postgres' else f'sqlite:///{tmp_path / "stream.db"}',
        'ARTIFACT_URL': str(tmp_path / 'objects'),
        'WORKBENCH_WORKER_URL': f'http://127.0.0.1:{worker_port}',
        'WORKBENCH_API_TOKEN': 'api-test', 'WORKBENCH_WORKER_TOKEN': 'worker-test',
        'WORKBENCH_TENANT_ID': 'stream-test', 'WORKBENCH_WORKSPACE_ID': uuid.uuid4().hex,
        'NUMBA_NUM_THREADS': '2',
    }
    with httpx.Client(base_url=f'http://127.0.0.1:{api_port}', headers={'Authorization':'Bearer api-test'}, timeout=20) as client:
        with server('app.main:app', api_port, env, tmp_path / 'api.log'):
            wait_for(lambda: client.get('/health').status_code == 200)
            example = client.get('/balance-stress/example').json()
            example['specification'].update(horizon_days=30, reverse_severities=[])
            submitted = client.post('/balance-stress/stream', json={
                'expected_revision': example['revision'], 'specification':example['specification']})
            assert submitted.status_code == 200, submitted.text
            jid = submitted.json()['id']
        with server('app.main:app', api_port, env, tmp_path / 'api-restart.log'):
            wait_for(lambda: client.get('/health').status_code == 200)
            with server('app.worker:app', worker_port, env, tmp_path / 'worker.log'):
                done = wait_for(lambda: (r if (r := client.get(f'/jobs/{jid}').json())['status'] in ('done','error') else None))
                assert done['status'] == 'done', (done, (tmp_path/'worker.log').read_text())
                manifest = client.get(f'/jobs/{jid}/manifest').json()
                assert manifest['identity']['balance_native_sha256']
                assert manifest['tables']['/journal']['format'] == 'partitioned-parquet'
                page = client.get(f'/jobs/{jid}/table', params={'path':'/journal','limit':2}).json()
                assert len(page['rows']) == 2 and page['total'] > 2
                output = client.get(f'/jobs/{jid}/result').content
                assert output.startswith(b'ARW1')
                parquet = client.get(f'/jobs/{jid}/parquet', params={'path':'/journal','partition':0}).content
                assert parquet[:4] == parquet[-4:] == b'PAR1'
        with server('app.main:app', api_port, env, tmp_path / 'api-final.log'):
            wait_for(lambda: client.get('/health').status_code == 200)
            assert client.get(f'/jobs/{jid}/result').content == output
            assert client.get(f'/jobs/{jid}/table', params={'path':'/journal','limit':2}).json() == page
