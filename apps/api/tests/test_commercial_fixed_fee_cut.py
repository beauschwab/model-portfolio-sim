"""Complete ordinary fee cut publication and predecessor authority."""
from copy import deepcopy
import pytest
from app import model_routes, persistence as db
from app.model_tables import restore_sources, format_sources
from test_persistence import durable
from test_commercial_prior_opening import execute
from test_commercial_cut_prior_chain import bound


def publish(zero=False):
    raw = model_routes.example('commercial-candidate-holding-journal-1')['request']
    raw['schema'] = 'commercial-cut-mixed-journal-1'
    raw['revised_candidate'] = deepcopy(raw['completed_candidate'])
    if zero:
        for side in ('completed_candidate', 'revised_candidate'):
            for allocation in raw[side]['allocation']:
                allocation['face'] = 0.
    job = model_routes.submit(model_routes.ModelRunRequest(expected_revision=0,
        schema_name=raw['schema'], request=raw))
    row = execute(job.id, 'fee-cut-source')
    assert row['status'] == 'done', row['detail']
    return job.id, raw, db.CODEC.load(row['result'])


@pytest.mark.parametrize('zero', [False, True])
def test_complete_fee_cut_publication_preserves_gross_rights_and_restart(durable, zero):
    jid, raw, saved = publish(zero)
    native = restore_sources(saved['model_result'], saved['execution']['table_layouts'])
    assert native['raw_source'] == raw
    packet = native['native_packet']
    assert packet['resource_admission']['maximum_work'] == 32_000_000
    assert packet['resource_admission']['maximum_bytes'] == 128 << 20
    assert packet['resource_admission']['source_phase_bytes_upper'] <= 128 << 20
    assert packet['component_receipt']['component_work_upper'] <= 8_000_000
    assert packet['component_receipt']['component_retained_bytes_upper'] <= 32 << 20
    assert packet['publication_eligible'] is False
    assert any(r['native_fee_snapshot']['fee_receivable'] > 0
        for r in native['cut_tables']['commercial_fee_daily'])
    if zero:
        assert all(r['allocation_factor'] == 0
            for r in native['cut_tables']['commercial_fee_daily'])
    assert max(r['day'] for r in native['cut_tables']['path']) == 45
    db.stop(); db.start(durable)
    reopened = db.CODEC.load(db.result_ref(jid))
    assert restore_sources(reopened['model_result'], reopened['execution']['table_layouts']) == native


@pytest.mark.parametrize('step,zero', [(0, False), (1, False), (1, True), (8, False), (14, True)])
def test_fee_predecessor_chain_matches_gross_fee_owners(durable, step, zero):
    sid, raw, _ = publish(zero)
    candidate = raw['revised_candidate']; cut = raw['cut_ordinal']
    mid, _ = bound(sid, candidate, cut + step, 'fee-middle')
    jid, saved = bound(mid, candidate, cut + 2 * step, 'fee-final')
    receipt = saved['execution']['prior_opening_verification']
    assert receipt['schema'] == 'commercial-cut-prior-opening-verification-3'
    assert receipt['separate_fee_adoption'] is True
    assert receipt['gross_unweighted_fee_cut_matched'] is True
    assert receipt['verified_publications'] == 2
    assert receipt['published_journal_rolled_forward'] is (step > 0)
    assert receipt['empirical_validation'] is False
    db.stop(); db.start(durable)
    assert db.CODEC.load(db.result_ref(jid))['execution']['prior_opening_verification'] == receipt


def test_fee_source_replaced_at_final_sql_boundary_prevents_publication(durable, monkeypatch):
    sid, raw, _ = publish()
    job = model_routes.published_cut_mixed(model_routes.PublishedCutMixedRequest(
        expected_revision=0, source_job_id=sid, cut_ordinal=raw['cut_ordinal'],
        revised_candidate=deepcopy(raw['revised_candidate'])))
    complete = db.REPO.complete
    entered = []
    def commit(current, **kw):
        if current['id'] == job.id and kw.get('result') is not None:
            entered.append(True)
            refs = db.CODEC.tables(db.REPO.job(sid)['result'])
            ref = next(iter(refs.values()))
            (db.CODEC.objects.root / ref['key']).write_bytes(b'changed prior fee source')
        return complete(current, **kw)
    monkeypatch.setattr(db.REPO, 'complete', commit)
    row = execute(job.id, 'fee-final-fence')
    assert entered and row['status'] != 'done' and row['result'] is None


@pytest.mark.parametrize('target', ['null_proof', 'gross_rights'])
def test_rehashed_fee_source_must_still_pass_independent_replay(durable, target):
    import sqlalchemy as sa
    from app.repository import jobs
    sid, raw, saved = publish(zero=True)
    job = model_routes.published_cut_mixed(model_routes.PublishedCutMixedRequest(
        expected_revision=0, source_job_id=sid, cut_ordinal=raw['cut_ordinal'],
        revised_candidate=deepcopy(raw['revised_candidate'])))
    native = restore_sources(saved['model_result'], saved['execution']['table_layouts'])
    if target == 'null_proof':
        native['native_packet']['completed_phase']['original_sources'][0]['proof']['fee_projection'] = None
    else:
        native['cut_tables']['commercial_fee_daily'][0]['native_fee_snapshot']['fee_receivable'] += 1.
    saved['model_result'], saved['execution']['table_layouts'] = format_sources(native)
    ref = db.CODEC.dump(saved)
    payload = db.CODEC.load(db.REPO.job(job.id)['request'])
    payload['args'][0]['prior_publication']['binding']['result'] = ref
    with db.REPO.engine.begin() as conn:
        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs), jobs.c.id == sid).values(result=ref))
        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs), jobs.c.id == job.id)
            .values(request=db.CODEC.dump(payload)))
    row = execute(job.id, 'fee-rehashed-source')
    assert row['status'] != 'done' and row['result'] is None
