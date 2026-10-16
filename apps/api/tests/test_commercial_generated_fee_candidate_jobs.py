"""Real joint generated fee source/Sim/partition replay and final durable authority gates."""
import io
import json

import polars as pl
import pytest

from app import main, worker, persistence as db, reporting, maintenance
from app.artifacts import ObjectReference
from app.schemas import ModeledBalanceStressRequest
from test_persistence import durable

SCHEMA = 'commercial-generated-candidate-fee-journal-1'


def queued(*, browser=False):
    raw = main.modeled_balance_stress_example(SCHEMA)['specification']
    if browser:
        import subprocess
        import shutil
        node=shutil.which('node')
        if node is None:pytest.skip('actual browser scalar normalization requires Node.js')
        script="let s='';process.stdin.on('data',x=>s+=x);process.stdin.on('end',()=>process.stdout.write(JSON.stringify(JSON.parse(s))))"
        raw=json.loads(subprocess.run([node,'-e',script],input=json.dumps(raw),text=True,capture_output=True,check=True).stdout)
    submitted = main.modeled_balance_stress(ModeledBalanceStressRequest(
        expected_revision=0, specification=raw))
    db.WORKER = 'generated-commercial-publication-test'
    worker.STOP.clear()
    # This fixture invokes execute synchronously without the service heartbeat.
    # Reserve enough test-only lease for complete independent persisted replay;
    # production lease renewal and all publication ownership checks are unchanged.
    assert db.REPO.acquire(db.WORKER, 300.)
    return submitted['id'], db.REPO.claim(db.WORKER), raw


def test_real_generated_worker_replays_partitions_and_reopens(durable, monkeypatch):
    from portfolio_risk.analytics import model_ledger, balance_stress
    def forbidden(*args, **kwargs): raise AssertionError('Python production financial callback')
    monkeypatch.setattr(model_ledger, 'validate_spec', forbidden)
    monkeypatch.setattr(model_ledger, '_finalize', forbidden)
    monkeypatch.setattr(balance_stress, '_simulate', forbidden)
    jid, job, raw = queued(browser=True)
    deadline = db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job)
    row = db.REPO.job(jid)
    assert row['status'] == 'done', row['detail']
    saved = db.CODEC.load(db.result_ref(jid)); execution = saved['execution']
    assert execution['publication_deadline_unix'] == deadline
    assert all(execution['validation'][key] is True
        for key in ('verified', 'journal_replayed', 'persisted_partitions_replayed'))
    assert json.loads(db.CODEC.objects.get(execution['input']))['request'] == raw
    trailer = json.loads(db.CODEC.objects.get(execution['producer_trailer']))
    assert trailer['physical_paths'] == 256 and trailer['maximum_live_full_physical_owners'] == 1
    assert trailer['mixed_ledger_adoption'] is False
    assert trailer['maximum_live_full_fee_owners']==1
    for name in ('journal', 'commercial_daily', 'commercial_fee_daily', 'commercial_fee_lineage', 'commercial_candidate_native_sources'):
        assert reporting.table(jid, '/'+name, offset=0, limit=2)['rows']
    assert execution['producer_trailer']['key'] in maintenance.reachable(db.REPO, db.CODEC.objects)
    db.stop(); db.start(durable)
    assert db.CODEC.load(db.result_ref(jid)) == saved


@pytest.mark.parametrize('target', ['source_weight', 'balanced_journal', 'missing_source',
    'completion_type', 'trailer_claim', 'extended_deadline', 'fee_endpoint', 'fee_source_cash', 'raw_source'])
def test_generated_rehashed_persisted_tamper_refuses(durable, monkeypatch, target):
    jid, job, _ = queued(); dump, load = db.CODEC.dump, db.CODEC.load
    changed = []
    def mutate(value):
        ref = dump(value)
        if type(value) is not dict or value.get('execution', {}).get('model_version') != SCHEMA:
            return ref
        saved = load(ref); execution = saved['execution']; changed.append(True)
        if target == 'raw_source':
            envelope=json.loads(db.CODEC.objects.get(execution['input']))
            envelope['request']['allocation'][0]['face']+=1.
            execution['input']=ObjectReference(db.CODEC.objects.put(json.dumps(envelope).encode(),'bin'))
        elif target == 'extended_deadline': execution['publication_deadline_unix'] += 3600.
        elif target == 'completion_type':
            execution['producer_completion'] = ObjectReference(db.CODEC.objects.put(
                b'{"complete":true,"protocol":7.0}', 'bin'))
        elif target == 'trailer_claim':
            trailer = json.loads(db.CODEC.objects.get(execution['producer_trailer']))
            trailer['mixed_ledger_adoption'] = True
            execution['producer_trailer'] = ObjectReference(db.CODEC.objects.put(json.dumps(trailer).encode(), 'bin'))
        elif target == 'missing_source':
            table = saved['commercial_candidate_native_sources']
            table['rows'] -= table['parts'].pop()['rows']
        else:
            name = 'commercial_candidate_native_sources' if target in {'source_weight','fee_source_cash'} else 'commercial_fee_daily' if target=='fee_endpoint' else 'journal'
            part = saved[name]['parts'][0]
            frame = pl.read_parquet(io.BytesIO(db.CODEC.objects.get(part['ref'])))
            rows = frame.to_dicts()
            if target == 'fee_endpoint':
                snapshot=json.loads(rows[0]['native_fee_snapshot_json']);snapshot['fee_receivable']+=.01
                rows[0]['native_fee_snapshot_json']=json.dumps(snapshot,separators=(',',':'))
            elif target == 'fee_source_cash':
                native=json.loads(rows[0]['native_json']);native['generated_fee_projection']['cash'][0]['fees']+=.01
                rows[0]['native_json']=json.dumps(native,separators=(',',':'))
            elif target == 'source_weight':
                native = json.loads(rows[0]['native_json']); native['weight'] += .1
                rows[0]['native_json'] = json.dumps(native, separators=(',', ':'))
            else:
                for row in rows[:2]:
                    row['debit'] += 1. if row['debit'] else 0.
                    row['credit'] += 1. if row['credit'] else 0.
            stream = io.BytesIO(); pl.DataFrame(rows, schema=frame.schema).write_parquet(stream)
            part['ref'] = db.CODEC.objects.put(stream.getvalue(), 'parquet')
        return dump(saved)
    monkeypatch.setattr(db.CODEC, 'dump', mutate)
    worker.execute(job)
    row = db.REPO.job(jid)
    assert changed, row['detail']
    assert row['status'] != 'done' and row['result'] is None


@pytest.mark.parametrize('target', ['late_trailer', 'queued_deadline', 'attempt', 'stop', 'foreign_owner', 'native_artifact', 'source_identity'])
def test_generated_sql_fence_rechecks_objects_and_original_authority(durable, monkeypatch, target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid, job, _ = queued(); complete = db.REPO.complete; reached = []
    def commit(bound, **kwargs):
        if kwargs.get('guard'):
            reached.append(True)
            if target == 'source_identity':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'changed-after-independent-replay')
            elif target == 'native_artifact':
                from portfolio_risk.analytics import model_ledger
                from pathlib import Path
                monkeypatch.setattr(model_ledger,'binary_path',lambda:Path('nonexistent-post-replay-executable'))
            elif target == 'stop': worker.STOP.set()
            elif target == 'late_trailer':
                descriptor = db.CODEC.load(kwargs['result'])['execution']['producer_trailer']
                (db.CODEC.objects.root/descriptor['key']).write_bytes(b'changed-after-replay')
            else:
                if target == 'foreign_owner': changes=dict(owner='foreign-post-replay-worker')
                elif target == 'attempt': changes = dict(attempt=bound['attempt']+1)
                else:
                    payload = db.CODEC.load(bound['request'])
                    payload['args'][0]['publication_deadline_unix'] += 3600.
                    changes = dict(request=db.CODEC.dump(payload))
                with db.REPO.engine.begin() as connection:
                    connection.execute(sa.update(jobs).where(db.REPO.scoped(jobs),
                        jobs.c.id == jid).values(**changes))
        return complete(bound, **kwargs)
    monkeypatch.setattr(db.REPO, 'complete', commit)
    try: worker.execute(job)
    finally: worker.STOP.clear()
    row = db.REPO.job(jid)
    assert reached, row['detail']
    assert row['status'] != 'done' and row['result'] is None


@pytest.mark.parametrize('kind',['duplicate','boolean_money','old_protocol_schema'])
def test_http_selected_generated_fee_raw_contract_is_strict_before_queue(durable, kind):
    from fastapi.testclient import TestClient
    raw=main.modeled_balance_stress_example(SCHEMA)['specification']
    if kind=='boolean_money':
        raw['commercial']['units'][0]['graph']['positions'][0]['generated_policy']['fee_acquisition']['total_purchase_cost']=True
    elif kind=='old_protocol_schema':
        raw['schema']='commercial-generated-candidate-journal-1'
    body=json.dumps(dict(expected_revision=0,specification=raw))
    if kind=='duplicate':
        body=body.replace('"allocated_cost": 2.0','"allocated_cost": 2.0, "allocated_cost": 3.0',1)
        assert body.count('"allocated_cost": 3.0')==1
    client=TestClient(main.app)
    try:response=client.post('/balance-stress/modeled',content=body,headers={'content-type':'application/json'})
    finally:client.close()
    if kind=='duplicate':assert response.status_code==422
    else:
        # Native financial DTO/adoption validation is durable worker-owned.
        assert response.status_code in (200,422)
        if response.status_code==200:
            db.WORKER='generated-fee-http-test';worker.STOP.clear()
            assert db.REPO.acquire(db.WORKER,300.)
            job=db.REPO.claim(db.WORKER);worker.execute(job)
            row=db.REPO.job(response.json()['id'])
            assert row['status']!='done' and row['result'] is None
