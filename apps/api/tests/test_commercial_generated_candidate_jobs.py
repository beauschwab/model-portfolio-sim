"""Real generated source/Sim/partition replay and final durable authority gates."""
import io
import json

import polars as pl
import pytest

from app import main, worker, persistence as db, reporting, maintenance
from app.artifacts import ObjectReference
from app.schemas import ModeledBalanceStressRequest
from test_persistence import durable

SCHEMA = 'commercial-generated-candidate-journal-1'


def queued():
    raw = main.modeled_balance_stress_example(SCHEMA)['specification']
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
    jid, job, raw = queued()
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
    for name in ('journal', 'commercial_daily', 'commercial_candidate_native_sources'):
        assert reporting.table(jid, '/'+name, offset=0, limit=2)['rows']
    assert execution['producer_trailer']['key'] in maintenance.reachable(db.REPO, db.CODEC.objects)
    db.stop(); db.start(durable)
    assert db.CODEC.load(db.result_ref(jid)) == saved


@pytest.mark.parametrize('target', ['source_weight', 'balanced_journal', 'missing_source',
    'completion_type', 'trailer_claim', 'extended_deadline'])
def test_generated_rehashed_persisted_tamper_refuses(durable, monkeypatch, target):
    jid, job, _ = queued(); dump, load = db.CODEC.dump, db.CODEC.load
    changed = []
    def mutate(value):
        ref = dump(value)
        if type(value) is not dict or value.get('execution', {}).get('model_version') != SCHEMA:
            return ref
        saved = load(ref); execution = saved['execution']; changed.append(True)
        if target == 'extended_deadline': execution['publication_deadline_unix'] += 3600.
        elif target == 'completion_type':
            execution['producer_completion'] = ObjectReference(db.CODEC.objects.put(
                b'{"complete":true,"protocol":6.0}', 'bin'))
        elif target == 'trailer_claim':
            trailer = json.loads(db.CODEC.objects.get(execution['producer_trailer']))
            trailer['mixed_ledger_adoption'] = True
            execution['producer_trailer'] = ObjectReference(db.CODEC.objects.put(json.dumps(trailer).encode(), 'bin'))
        elif target == 'missing_source':
            table = saved['commercial_candidate_native_sources']
            table['rows'] -= table['parts'].pop()['rows']
        else:
            name = 'commercial_candidate_native_sources' if target == 'source_weight' else 'journal'
            part = saved[name]['parts'][0]
            frame = pl.read_parquet(io.BytesIO(db.CODEC.objects.get(part['ref'])))
            rows = frame.to_dicts()
            if target == 'source_weight':
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


@pytest.mark.parametrize('target', ['late_trailer', 'queued_deadline', 'attempt', 'stop'])
def test_generated_sql_fence_rechecks_objects_and_original_authority(durable, monkeypatch, target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid, job, _ = queued(); complete = db.REPO.complete; reached = []
    def commit(bound, **kwargs):
        if kwargs.get('guard'):
            reached.append(True)
            if target == 'stop': worker.STOP.set()
            elif target == 'late_trailer':
                descriptor = db.CODEC.load(kwargs['result'])['execution']['producer_trailer']
                (db.CODEC.objects.root/descriptor['key']).write_bytes(b'changed-after-replay')
            else:
                if target == 'attempt': changes = dict(attempt=bound['attempt']+1)
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
