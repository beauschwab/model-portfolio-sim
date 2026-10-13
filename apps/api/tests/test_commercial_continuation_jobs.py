"""Complete-prefix durable source verification and final publication authority."""
import json
from pathlib import Path
import pytest
from app import model_routes, persistence as db, worker
from test_persistence import durable

ROOT = Path(__file__).resolve().parents[3]

def queued(optional=False):
    raw = json.loads((ROOT/'examples/models/commercial-holding-continuation-1.json').read_text())
    if optional:raw['revised_source']['performing_draw_tranches'] = []
    submitted = model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'], request=raw, expected_revision=0))
    db.WORKER='commercial-holding-worker';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id, db.REPO.claim(db.WORKER), raw

@pytest.mark.parametrize('optional', [False, True])
def test_commercial_holding_publishes_replayed_opening_with_original_deadline(durable, optional):
    jid, job, raw = queued(optional); deadline = db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job); row = db.REPO.job(jid); assert row['status']=='done', row['detail']
    saved = db.CODEC.load(db.result_ref(jid)); execution = saved['execution']
    assert execution['publication_deadline_unix'] == deadline and execution['native_timeout_ms'] > 0
    assert execution['source_verification']['completed_prefix_recomputed'] is True
    from app.model_tables import restore_sources
    result = restore_sources(saved['model_result'], execution['table_layouts'])
    assert result['raw_source']['cut_ordinal'] == raw['cut_ordinal'] and result['verified_prior_publication'] is False
    assert result['openings'][0]['book']['principal'] == 100.

@pytest.mark.parametrize('target', ['opening', 'prefix', 'suffix', 'yield', 'evidence'])
def test_commercial_holding_rehashed_persisted_mutations_reject(durable, monkeypatch, target):
    jid, job, _ = queued(); dump = db.CODEC.dump; load = db.CODEC.load
    def changed(value):
        ref = dump(value)
        if not isinstance(value, dict) or value.get('execution', {}).get('schema') != 'commercial-holding-continuation-1':return ref
        saved = load(ref)
        from app.model_tables import restore_sources, format_sources
        result = restore_sources(saved['model_result'], saved['execution']['table_layouts'])
        if target == 'opening':result['openings'][0]['book']['principal'] += 1.
        elif target == 'prefix':result['completed']['accounting']['groups'][1]['next']['gross_carry'] += 1.
        elif target == 'suffix':result['openings'][0]['subsequent_native_group_indices'].pop()
        elif target == 'yield':result['revised']['raw_request']['source']['acquisition']['original_annual_effective_rate'] += .01
        else:result['verified_prior_publication'] = True
        saved['model_result'], saved['execution']['table_layouts'] = format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC, 'dump', changed); worker.execute(job)
    row = db.REPO.job(jid); assert row['status']!='done' and row['result'] is None

def test_commercial_holding_queue_deadline_is_not_restarted(durable, monkeypatch):
    import time
    from types import SimpleNamespace
    from app import model_jobs
    jid, job, _ = queued(); deadline = db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    monkeypatch.setattr(model_jobs, 'time', SimpleNamespace(time=lambda:deadline+1., monotonic=time.monotonic))
    worker.execute(job); row = db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None and 'deadline' in row['detail'].lower()
