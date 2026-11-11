"""No economic loss result publishes from self-certified paths or stale authority."""
from copy import deepcopy
import json

import pytest

from app import model_jobs, model_routes, persistence as db, worker
from app.model_tables import format_sources, restore_sources
from app.model_report_metadata import source_summary
from portfolio_risk.analytics.model_contracts import evaluate_lifetime_credit_loss
from test_model_runs import queued
from test_persistence import durable

SCHEMA = 'lifetime-credit-loss-1'


@pytest.mark.parametrize('phase', ['native', 'persisted'])
@pytest.mark.parametrize('defect', ['alternative_seed', 'raw_type', 'path', 'band', 'outcome'])
def test_coherent_result_or_nested_source_tamper_never_publishes(durable, monkeypatch, phase, defect):
    raw = model_routes.example(SCHEMA)['request']
    changed_raw = deepcopy(raw)
    if defect == 'alternative_seed': changed_raw['cohorts'][0]['source']['draw_stream']['seed'] += 1
    changed = evaluate_lifetime_credit_loss(changed_raw)
    if defect == 'raw_type': changed['raw_source']['drivers'][0]['offset'] = False
    elif defect == 'path': changed['cells'][0]['full_paths'][0]['cash_received'] += 1.
    elif defect == 'band': changed['sensitivity'][0]['spread'] += 1.
    elif defect == 'outcome': changed['holdout_outcomes'][0]['reported_minus_projected'] += 1.
    jid, job = queued(SCHEMA)
    if phase == 'native':
        original = model_jobs.native_result
        def replace(*args, **kwargs):
            _, execution = original(*args, **kwargs)
            return deepcopy(changed), execution
        monkeypatch.setattr(model_jobs, 'native_result', replace)
    else:
        original = db.CODEC.dump
        def replace(value):
            if isinstance(value, dict) and 'model_result' in value:
                value['model_result'], value['execution']['table_layouts'] = format_sources(deepcopy(changed))
                value['execution']['summary'] = source_summary(changed)
            return original(value)
        monkeypatch.setattr(db.CODEC, 'dump', replace)
    worker.execute(job)
    row = db.REPO.job(jid)
    assert row['status'] == 'error' and row['result'] is None
    assert 'lifetime loss' in row['detail']


def test_lifetime_loss_restarts_with_exact_typed_sources_and_original_deadline(durable):
    jid, job = queued(SCHEMA)
    queued_request = db.CODEC.load(job['request'])['args'][0]
    worker.execute(job)
    row = db.REPO.job(jid); assert row['status'] == 'done', row['detail']
    accepted = db.CODEC.load(db.result_ref(jid)); execution = accepted['execution']
    assert execution['publication_deadline_unix'] == queued_request['publication_deadline_unix']
    assert execution['source_verification']['verified']
    raw = restore_sources(accepted['model_result'], execution['table_layouts'])
    assert len(raw['cells']) == 9 and not raw['allowance_or_accounting_approval']
    assert db.CODEC.load(job['request'])['args'][0]['request'] == queued_request['request']
    db.stop(); db.start(durable)
    assert db.CODEC.load(db.result_ref(jid))['execution'] == execution


def test_original_nested_http_duplicates_reject_before_enqueue(durable):
    from fastapi.testclient import TestClient
    from app import main
    raw = model_routes.example(SCHEMA)['request']
    body = json.dumps(dict(expected_revision=0, schema_name=SCHEMA, request=raw), separators=(',', ':'))
    body = body.replace('"monetary_multiplier":1.0', '"monetary_multiplier":1.0,"monetary_multiplier":0.0')
    with TestClient(main.app) as client:
        response = client.post('/models/runs', content=body, headers={'content-type':'application/json'})
    assert response.status_code == 422 and 'duplicate' in response.text


def test_expired_original_deadline_refuses_before_enqueue(durable):
    from app.model_routes import ModelRunRequest
    from app import store
    from fastapi import HTTPException
    import time
    raw = model_routes.example(SCHEMA)['request']
    with pytest.raises(HTTPException) as error:
        model_routes.enqueue(ModelRunRequest(expected_revision=0, schema_name=SCHEMA,
            request=raw), 'model_analysis', store.run_model_analysis, accepted_start=time.time()-1900.)
    assert error.value.status_code == 408
