"""Malformed financial instruction fields fail before source publication."""
import pytest
from app import model_routes,model_jobs,worker,persistence as db
from test_persistence import durable


@pytest.mark.parametrize('kind',['all_assets','all_liabilities','card1','card2'])
def test_discarded_financial_instruction_fields_publish_no_manifest(durable,monkeypatch,kind):
    schema='strategy-product-1' if kind.startswith('all_') else 'card-credit-accounting-1' if kind=='card1' else 'card-credit-accounting-2'
    raw=model_routes.example(schema)
    if kind.startswith('all_'):
        raw['request']['face_limits'][0]['selector']={'kind':kind,'unit_id':'loan-unit-1'}
    else:raw['request']['termination']={'kind':'source_default','amount':0.}
    submitted=model_routes.submit(model_routes.ModelRunRequest(
        schema_name=schema,request=raw['request'],expected_revision=0))
    db.WORKER='strict-model-input-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    job=db.REPO.claim(db.WORKER)
    def forbidden(*args,**kwargs):pytest.fail('malformed native input reached publication reference')
    monkeypatch.setattr(model_jobs,'verify_source',forbidden)
    worker.execute(job);row=db.REPO.job(submitted.id)
    assert row['status']!='done' and row['result'] is None
    assert 'unknown field' in row['detail']
