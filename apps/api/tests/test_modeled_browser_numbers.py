"""Durable default-native publication of requests serialized by a browser."""
from copy import deepcopy
import json
import pytest
from app import main, worker, persistence as db, reporting
from app.schemas import ModeledBalanceStressRequest
from test_persistence import durable


def browser_numbers(value):
    if type(value) is float and value.is_integer():
        return int(value)
    if type(value) is dict:
        return {k: browser_numbers(v) for k, v in value.items()}
    if type(value) is list:
        return [browser_numbers(v) for v in value]
    return value


@pytest.mark.parametrize('schema', ['mortgage-mixed-accounting-1', 'deposit-revised-income-ledger-1'])
def test_browser_numbers_publish_and_reopen_exact_raw_and_typed_source(durable, monkeypatch, schema):
    from portfolio_risk.analytics import model_ledger
    from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
    def forbidden(*args, **kwargs):
        raise AssertionError('Python financial execution callback')
    monkeypatch.setattr(model_ledger, 'validate_spec', forbidden)
    monkeypatch.setattr(model_ledger, '_finalize', forbidden)
    raw=browser_numbers(main.modeled_balance_stress_example(schema)['specification'])
    submitted=main.modeled_balance_stress(ModeledBalanceStressRequest(expected_revision=0, specification=raw))
    db.WORKER='modeled-browser-number-test'; worker.STOP.clear()
    assert db.REPO.acquire(db.WORKER)
    job=db.REPO.claim(db.WORKER); worker.execute(job)
    row=db.REPO.job(submitted['id'])
    assert row['status']=='done', row['detail']
    result=db.CODEC.load(db.result_ref(row['id']))
    execution=result['execution']
    envelope=json.loads(db.CODEC.objects.get(execution['input']))
    assert exact_json_identity(envelope['request'], raw)
    header=json.loads(db.CODEC.objects.get(execution['modeled_source']))
    retained=header['source_receipt']['raw_request']
    if schema=='mortgage-mixed-accounting-1':
        from portfolio_risk.analytics.mortgage_mixed_reference import verify_retained_request_identity
        verify_retained_request_identity(raw, retained)
        assert type(retained['owners'][0]['unit_face']) is float
        assert type(raw['owners'][0]['unit_face']) is int
        assert exact_json_identity(retained['dated']['specification'], raw['dated']['specification'])
    else:
        assert exact_json_identity(retained, raw)
        assert execution['validation']['original_yield_revision_recomputed'] is True
    assert reporting.table(row['id'], '/journal', offset=0, limit=2)['rows']
    saved=deepcopy(result)
    db.stop(); db.start(durable)
    assert db.CODEC.load(db.result_ref(row['id']))==saved


def test_browser_mortgage_source_and_capital_policies_publish_and_reopen(durable, monkeypatch):
    from app import treasury_routes
    from test_mortgage_closing_capital_jobs import policies
    from portfolio_risk.analytics import model_ledger

    def forbidden(*args, **kwargs):
        raise AssertionError('Python financial execution callback')

    monkeypatch.setattr(model_ledger, 'validate_spec', forbidden)
    monkeypatch.setattr(model_ledger, '_finalize', forbidden)
    raw = browser_numbers(main.modeled_balance_stress_example('mortgage-mixed-accounting-1')['specification'])
    submitted = main.modeled_balance_stress(ModeledBalanceStressRequest(expected_revision=0, specification=raw))
    db.WORKER = 'modeled-browser-capital-test'; worker.STOP.clear()
    assert db.REPO.acquire(db.WORKER)
    job = db.REPO.claim(db.WORKER); worker.execute(job)
    source = db.REPO.job(submitted['id'])
    assert source['status'] == 'done', source['detail']
    specification, review = policies()
    submitted_capital = treasury_routes.mortgage_closing_capital(treasury_routes.MortgageClosingRequest(
        expected_revision=0, source_job=source['id'], specification=browser_numbers(specification),
        closing_review=review, timeout=60.))
    job = db.REPO.claim(db.WORKER)
    assert job['id'] == submitted_capital.id
    worker.execute(job)
    row = db.REPO.job(submitted_capital.id)
    assert row['status'] == 'done', row['detail']
    result = db.CODEC.load(db.result_ref(row['id']))
    assert result['execution']['validation']['closing_capital_eligible'] is True
    assert result['execution']['validation']['regulatory_credit_rwa_approval'] is False
    assert db.REPO.job(source['id']) == source
    saved = deepcopy(result)
    db.stop(); db.start(durable)
    assert db.CODEC.load(db.result_ref(row['id'])) == saved
