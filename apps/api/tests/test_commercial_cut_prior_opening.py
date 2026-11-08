"""Real native cut predecessor publication, replay and durable authority fences."""
from copy import deepcopy
import json
import pytest
from app import model_routes, persistence as db, worker
from app.model_tables import restore_sources, format_sources
from test_persistence import durable
from test_commercial_prior_opening import execute


def completed(zero=False):
    raw = model_routes.example('commercial-cut-mixed-journal-1')['request']
    successor = deepcopy(raw['revised_candidate'])
    raw['schema'] = 'commercial-cut-mixed-journal-1'
    raw['revised_candidate'] = deepcopy(raw['completed_candidate'])
    if zero:
        for candidate in (raw['completed_candidate'], raw['revised_candidate'], successor):
            for allocation in candidate['allocation']:
                allocation['face'] = 0.
    job = model_routes.submit(model_routes.ModelRunRequest(expected_revision=0,
        schema_name=raw['schema'], request=raw))
    row = execute(job.id, 'cut-prior-source')
    assert row['status'] == 'done', row['detail']
    return job.id, raw, successor


def selection(sid, raw, revised):
    return model_routes.PublishedCutMixedRequest(expected_revision=0,
        source_job_id=sid, cut_ordinal=raw['cut_ordinal'], revised_candidate=revised)


def queued(zero=False):
    sid, raw, revised = completed(zero)
    job = model_routes.published_cut_mixed(selection(sid, raw, revised))
    return sid, raw, job.id


@pytest.mark.parametrize('zero', [False, True])
def test_cut_bound_publication_replays_opening_gross_rights_and_restart(durable, zero):
    from app.commercial_cut_prior_source import _prefix
    sid, raw, jid = queued(zero)
    row = execute(jid, 'cut-prior-current')
    assert row['status'] == 'done', row['detail']
    saved = db.CODEC.load(row['result'])
    e = saved['execution']; receipt = e['prior_opening_verification']
    assert receipt['schema'] == 'commercial-cut-prior-opening-verification-1'
    assert receipt['modeled_prior_publication_verified'] is True
    assert receipt['cut_started_mixed_sim_adoption'] is True
    assert receipt['gross_unweighted_loan_cut_matched'] is True
    assert receipt['completed_candidate_bound_to_prior_revised'] is True
    assert receipt['observed_bank_reconciliation'] is False
    assert receipt['general_mixed_book_adoption'] is False
    assert receipt['provenance']['binding']['id'] == sid
    native = restore_sources(saved['model_result'], e['table_layouts'])
    prior = db.CODEC.load(db.REPO.job(sid)['result'])
    old = restore_sources(prior['model_result'], prior['execution']['table_layouts'])
    assert native['raw_source']['completed_candidate'] == raw['revised_candidate']
    assert native['producer_completion']['prior_publication_verified'] is False
    assert _prefix(native) == _prefix(old)
    if zero:
        rows = _prefix(native)['tables']['commercial_daily']
        assert any(r['unweighted_book']['principal'] > 0 for r in rows)
    if not zero:
        assert native['cut_tables']['journal'] != old['cut_tables']['journal']
    from app.maintenance import reachable
    assert {r['key'] for r in receipt['provenance']['objects']} <= reachable(db.REPO, db.CODEC.objects)
    db.stop(); db.start(durable)
    assert db.CODEC.load(db.result_ref(jid))['execution']['prior_opening_verification'] == receipt


@pytest.mark.parametrize('target', ['wrong_cut', 'foreign_source', 'wrong_schema'])
def test_cut_prior_rejects_invalid_source_authority(durable, target):
    from fastapi import HTTPException
    sid, raw, revised = completed()
    request = selection(sid, raw, revised)
    if target == 'wrong_cut': request.cut_ordinal -= 1
    elif target == 'foreign_source': request.source_job_id = 'not-in-this-workspace'
    elif target == 'wrong_schema':
        from test_commercial_candidate_prior_opening import completed as other
        request.source_job_id = other()[0]
    with pytest.raises(HTTPException): model_routes.published_cut_mixed(request)


@pytest.mark.parametrize('target', ['opening', 'gross_loan', 'raw_anchor', 'receipt', 'input_identity'])
def test_cut_prior_rehashed_source_is_independently_replayed(durable, target):
    import sqlalchemy as sa
    from app.repository import jobs
    sid, raw, jid = queued(zero=target == 'gross_loan')
    saved = db.CODEC.load(db.REPO.job(sid)['result'])
    if target in ('opening', 'gross_loan', 'raw_anchor'):
        native = restore_sources(saved['model_result'], saved['execution']['table_layouts'])
        if target == 'opening': native['cut_tables']['journal'][0]['value'] += 1.
        elif target == 'gross_loan':
            native['cut_tables']['commercial_daily'][0]['unweighted_book']['principal'] += 1.
        else: native['raw_source']['revised_candidate']['allocation'][0]['face'] += 1.
        saved['model_result'], saved['execution']['table_layouts'] = format_sources(native)
    elif target == 'receipt': saved['execution']['source_verification']['verified'] = 1
    else: saved['execution']['executed_input_sha256'] = '0' * 64
    ref = db.CODEC.dump(saved)
    payload = db.CODEC.load(db.REPO.job(jid)['request'])
    payload['args'][0]['prior_publication']['binding']['result'] = ref
    with db.REPO.engine.begin() as conn:
        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs), jobs.c.id == sid).values(result=ref))
        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs), jobs.c.id == jid).values(request=db.CODEC.dump(payload)))
    row = execute(jid, 'cut-source-rehashed')
    assert row['status'] != 'done' and row['result'] is None


@pytest.mark.parametrize('target', ['source_attempt', 'source_partition', 'new_attempt', 'cancel', 'deadline', 'evidence', 'queued_input', 'executable', 'lease'])
def test_cut_prior_final_sql_rechecks_source_and_current_authority(durable, monkeypatch, target):
    import sqlalchemy as sa
    from app.repository import jobs
    sid, raw, jid = queued(); complete = db.REPO.complete; entered = []
    if target == 'evidence':
        dump = db.CODEC.dump
        def changed(value):
            if isinstance(value, dict) and value.get('execution', {}).get('prior_opening_verification'):
                entered.append(True)
                value = deepcopy(value)
                value['execution']['prior_opening_verification']['observed_bank_reconciliation'] = True
            return dump(value)
        monkeypatch.setattr(db.CODEC, 'dump', changed)
    else:
        def commit(job, **kw):
            if job['id'] == jid and kw.get('result') is not None:
                entered.append(True)
                assert kw.get('guard') is not None
                if target == 'cancel': worker.STOP.set()
                elif target == 'lease': db.REPO.release(db.WORKER)
                elif target == 'executable':
                    from portfolio_risk.core import product_strategy_native
                    monkeypatch.setattr(product_strategy_native, 'binary_identity', lambda: '0' * 64)
                elif target == 'deadline':
                    from app import commercial_analysis_jobs
                    from types import SimpleNamespace
                    deadline = db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
                    monkeypatch.setattr(commercial_analysis_jobs, 'time', SimpleNamespace(time=lambda: deadline + 1.))
                elif target == 'source_partition':
                    ref = next(iter(db.CODEC.tables(db.REPO.job(sid)['result']).values()))
                    (db.CODEC.objects.root / ref['key']).write_bytes(b'late-source-bytes')
                else:
                    selected = jid if target in ('new_attempt', 'queued_input') else sid
                    row = db.REPO.job(selected)
                    if target == 'queued_input':
                        payload = db.CODEC.load(row['request'])
                        payload['args'][0]['request']['completed_candidate']['allocation'][0]['face'] += 1.
                        changes = dict(request=db.CODEC.dump(payload))
                    else: changes = dict(attempt=row['attempt'] + 1)
                    with db.REPO.engine.begin() as conn:
                        conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs), jobs.c.id == selected).values(**changes))
            return complete(job, **kw)
        monkeypatch.setattr(db.REPO, 'complete', commit)
    try: row = execute(jid, 'cut-prior-final')
    finally: worker.STOP.clear()
    assert entered and row['status'] != 'done' and row['result'] is None


@pytest.mark.parametrize('phase', ['cancel', 'deadline'])
def test_cut_prior_budget_covers_replay_before_native_execution(durable, monkeypatch, phase):
    from app import commercial_cut_prior_source as source, model_jobs
    from portfolio_risk.core import commercial_cut_mixed_native as runtime
    from types import SimpleNamespace
    import time
    sid, raw, jid = queued(); verify = source._verify; entered = []
    deadline = db.CODEC.load(db.REPO.job(jid)['request'])['args'][0]['publication_deadline_unix']
    def replay(raw, native, check):
        entered.append(True)
        if phase == 'cancel': worker.STOP.set()
        else: monkeypatch.setattr(model_jobs, 'time', SimpleNamespace(time=lambda: deadline + 1., monotonic=time.monotonic))
        return verify(raw, native, check)
    def forbidden(*args, **kwargs): raise AssertionError('native execution before prior replay')
    monkeypatch.setattr(source, '_verify', replay); monkeypatch.setattr(runtime, 'run', forbidden)
    try: row = execute(jid, 'cut-prior-budget')
    finally: worker.STOP.clear()
    assert entered and row['status'] != 'done' and row['result'] is None


@pytest.mark.parametrize('target', ['duplicate', 'bool_cut', 'checkpoint', 'string_source', 'completed_source', 'authority', 'nonfinite', 'oversize'])
def test_cut_prior_http_rejects_caller_authority_and_coercion(durable, target):
    from fastapi.testclient import TestClient
    from app.main import app
    value = dict(expected_revision=0, source_job_id='missing-source', cut_ordinal=739900, revised_candidate={})
    if target == 'bool_cut': value['cut_ordinal'] = True
    elif target == 'checkpoint': value['opening'] = {'cash': 1000.}
    elif target == 'string_source': value['source_job_id'] = 123
    elif target == 'completed_source': value['completed_candidate'] = {}
    elif target == 'authority': value['prior_publication_verified'] = True
    elif target == 'nonfinite': value['revised_candidate'] = {'money': float('nan')}
    elif target == 'oversize': value['revised_candidate'] = {'padding': 'x' * (2 << 20)}
    body = json.dumps(value)
    if target == 'duplicate': body = body.replace('"cut_ordinal": 739900', '"cut_ordinal": 739900,"cut_ordinal": 739901')
    with TestClient(app) as client:
        response = client.post('/models/cut-mixed-journals', content=body, headers={'content-type': 'application/json'})
    assert response.status_code == 422, response.text


def test_cut_prior_openapi_admits_only_source_id_and_future_request():
    from app.main import app
    spec = app.openapi()
    route = spec['paths']['/models/cut-mixed-journals']['post']
    assert route['responses']['200']['content']['application/json']['schema']['$ref'].endswith('/JobStatus')
    schema = spec['components']['schemas']['PublishedCutMixedRequest']
    assert schema['additionalProperties'] is False
    assert set(schema['properties']) == {'expected_revision', 'source_job_id', 'cut_ordinal', 'revised_candidate', 'timeout'}


def test_cut_prior_materialization_retains_raw_identity_and_bound():
    from app import commercial_cut_prior_source as source
    raw = model_routes.example(source.SCHEMA)['request']; before = deepcopy(raw)
    result = source.materialize(dict(raw=raw), raw['revised_candidate'], raw['cut_ordinal'])
    result['completed_candidate']['allocation'][0]['face'] += 1.
    assert raw == before
    revised = deepcopy(raw['revised_candidate']); revised['padding'] = 'x' * (2 << 20)
    with pytest.raises(ValueError, match='bounded'):
        source.materialize(dict(raw=raw), revised, raw['cut_ordinal'])


def test_fee_cut_retains_its_existing_admission_refusal(durable):
    raw = model_routes.example('commercial-candidate-holding-journal-1')['request']
    raw['schema'] = 'commercial-cut-mixed-journal-1'
    raw['revised_candidate'] = deepcopy(raw['completed_candidate'])
    job = model_routes.submit(model_routes.ModelRunRequest(expected_revision=0,
        schema_name=raw['schema'], request=raw))
    row = execute(job.id, 'advanced-cut-refusal')
    assert row['status'] == 'error' and row['result'] is None
    assert 'before dual graph generation' in row['detail']
