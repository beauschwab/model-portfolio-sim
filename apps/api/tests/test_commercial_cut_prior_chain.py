"""Advancing native cuts and bounded replay of every published predecessor."""
from copy import deepcopy
import pytest
from app import model_routes, persistence as db
from app.model_tables import restore_sources
from test_persistence import durable
from test_commercial_prior_opening import execute
from test_commercial_cut_prior_opening import completed


def bound(sid, candidate, cut, owner):
    request = model_routes.PublishedCutMixedRequest(expected_revision=0,
        source_job_id=sid, cut_ordinal=cut, revised_candidate=deepcopy(candidate))
    job = model_routes.published_cut_mixed(request)
    row = execute(job.id, owner)
    assert row['status'] == 'done', row['detail']
    return job.id, db.CODEC.load(row['result'])


@pytest.mark.parametrize('step,zero', [(0, False), (1, False), (1, True), (8, False), (8, True)])
def test_chain_replays_all_ancestors_and_advances_actual_native_cut(durable, step, zero):
    sid, raw, _ = completed(zero)
    candidate = raw['revised_candidate']; old_cut = raw['cut_ordinal']
    middle, saved = bound(sid, candidate, old_cut + step, 'chain-middle')
    last, result = bound(middle, candidate, old_cut + 2 * step, 'chain-last')
    receipt = result['execution']['prior_opening_verification']
    assert receipt['schema'] == 'commercial-cut-prior-opening-verification-2'
    assert receipt['verified_publications'] == 2
    assert receipt['published_journal_rolled_forward'] is (step > 0)
    assert receipt['published_cut_opening_matched'] is (step == 0)
    assert receipt['provenance']['binding']['id'] == middle
    assert receipt['provenance']['ancestor_verification']['provenance']['binding']['id'] == sid
    native = restore_sources(result['model_result'], result['execution']['table_layouts'])
    assert native['raw_source']['cut_ordinal'] == old_cut + 2 * step
    assert native['producer_completion']['prior_publication_verified'] is False
    assert all(r['day'] == old_cut + 2 * step - candidate['as_of_ordinal']
        for r in native['cut_tables']['journal'] if r['event'] == 'native_verified_cut_opening')
    db.stop(); db.start(durable)
    assert db.CODEC.load(db.result_ref(last))['execution']['prior_opening_verification'] == receipt


@pytest.mark.parametrize('target', ['ancestor_attempt', 'ancestor_partition', 'ancestor_receipt', 'cycle', 'depth', 'bytes'])
def test_chain_authority_and_aggregate_bounds_fail_closed(durable, monkeypatch, target):
    import sqlalchemy as sa
    from app.repository import jobs
    from app import commercial_cut_prior_chain as limits
    sid, raw, _ = completed()
    candidate = raw['revised_candidate']; cut = raw['cut_ordinal']
    middle, saved = bound(sid, candidate, cut, 'chain-control-middle')
    request = model_routes.PublishedCutMixedRequest(expected_revision=0,
        source_job_id=middle, cut_ordinal=cut + 1, revised_candidate=deepcopy(candidate))
    job = model_routes.published_cut_mixed(request)
    complete = db.REPO.complete; entered = []
    if target in ('ancestor_attempt', 'ancestor_partition'):
        def commit(current, **kw):
            if current['id'] == job.id and kw.get('result') is not None:
                entered.append(True)
                if target == 'ancestor_partition':
                    ref = next(iter(db.CODEC.tables(db.REPO.job(sid)['result']).values()))
                    (db.CODEC.objects.root / ref['key']).write_bytes(b'ancestor changed at final SQL boundary')
                else:
                    with db.REPO.engine.begin() as conn:
                        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs), jobs.c.id == sid)
                            .values(attempt=db.REPO.job(sid)['attempt'] + 1))
            return complete(current, **kw)
        monkeypatch.setattr(db.REPO, 'complete', commit)
    elif target == 'depth': monkeypatch.setattr(limits, 'MAX_LINKS', 1)
    elif target == 'bytes': monkeypatch.setattr(limits, 'MAX_BYTES', 1)
    elif target == 'cycle':
        # A repeated publication is rejected before its next raw/result read.
        enter = limits.ChainBudget.enter
        def repeat(self, job_id):
            if self.jobs: job_id = next(iter(self.jobs))
            return enter(self, job_id)
        monkeypatch.setattr(limits.ChainBudget, 'enter', repeat)
    else:
        saved['execution']['prior_opening_verification']['modeled_prior_publication_verified'] = 1
        ref = db.CODEC.dump(saved)
        payload = db.CODEC.load(db.REPO.job(job.id)['request'])
        payload['args'][0]['prior_publication']['binding']['result'] = ref
        with db.REPO.engine.begin() as conn:
            conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs), jobs.c.id == middle).values(result=ref))
            conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs), jobs.c.id == job.id)
                .values(request=db.CODEC.dump(payload)))
    row = execute(job.id, 'chain-control-last')
    assert row['status'] != 'done' and row['result'] is None
    if target.startswith('ancestor_') and target != 'ancestor_receipt': assert entered


def test_advanced_cut_beyond_published_tail_rejects_before_new_native_run(durable, monkeypatch):
    from portfolio_risk.core import commercial_cut_mixed_native as runtime
    sid, raw, _ = completed()
    request = model_routes.PublishedCutMixedRequest(expected_revision=0, source_job_id=sid,
        cut_ordinal=raw['revised_candidate']['reporting_end_ordinal'] + 1000,
        revised_candidate=deepcopy(raw['revised_candidate']))
    job = model_routes.published_cut_mixed(request)
    def forbidden(*a, **kw): raise AssertionError('new native execution before predecessor date admission')
    monkeypatch.setattr(runtime, 'run', forbidden)
    row = execute(job.id, 'chain-tail-bound')
    assert row['status'] == 'error' and row['result'] is None
    assert 'complete published tail' in row['detail']


@pytest.mark.parametrize('kind', ['read', 'decoded', 'logical', 'prefix', 'objects', 'cycle', 'depth'])
def test_chain_budget_rejects_before_crossing_cumulative_limits(kind):
    from app.commercial_cut_prior_chain import ChainBudget, MAX_BYTES, MAX_PREFIX_BYTES, MAX_LINKS, MAX_OBJECTS
    budget = ChainBudget()
    ref = dict(key='x', bytes=MAX_BYTES, format='json', sha256='0' * 64)
    if kind == 'read':
        budget.read(ref)
        action = lambda: budget.read(dict(ref, bytes=1))
    elif kind == 'decoded':
        budget.expand(MAX_BYTES); action = lambda: budget.expand(1)
    elif kind == 'logical':
        budget.materialize(MAX_BYTES); action = lambda: budget.materialize(1)
    elif kind == 'prefix':
        budget.retain(MAX_PREFIX_BYTES); action = lambda: budget.retain(1)
    elif kind == 'objects':
        for i in range(MAX_OBJECTS): budget.read(dict(ref, key=str(i), bytes=1))
        action = lambda: budget.read(dict(ref, key='new', bytes=1))
    elif kind == 'cycle':
        budget.enter('same'); action = lambda: budget.enter('same')
    else:
        for i in range(MAX_LINKS): budget.enter(str(i))
        action = lambda: budget.enter('next')
    with pytest.raises(ValueError): action()
