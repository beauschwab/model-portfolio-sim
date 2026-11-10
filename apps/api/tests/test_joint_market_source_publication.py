"""A coherent substituted market source cannot publish under the queued raw input."""
from copy import deepcopy
import pytest

from app import model_jobs, model_routes, persistence as db, worker
from app.model_tables import format_sources
from app.model_report_metadata import source_summary
from portfolio_risk.analytics.model_contracts import evaluate_joint_market_credit
from test_model_runs import queued
from test_persistence import durable


@pytest.mark.parametrize('change', ['covariance', 'seed'])
@pytest.mark.parametrize('phase', ['native', 'persisted'])
def test_coherent_source_substitution_never_publishes(durable, monkeypatch, change, phase):
    raw = model_routes.example('joint-market-credit-1')['request']
    alternative = deepcopy(raw)
    if change == 'covariance':
        alternative['market']['annual_covariance'] = [2 * x for x in raw['market']['annual_covariance']]
    else:
        alternative['market']['draws']['seed'] = [123]
    substituted = evaluate_joint_market_credit(alternative)
    substituted_receipt = model_jobs._verify_joint_market_credit(alternative, substituted)
    jid, job = queued('joint-market-credit-1')
    if phase == 'native':
        original = model_jobs.native_result
        def replace(*args, **kwargs):
            _, execution = original(*args, **kwargs)
            return deepcopy(substituted), execution
        monkeypatch.setattr(model_jobs, 'native_result', replace)
    else:
        original = db.CODEC.dump
        def replace(value):
            if isinstance(value, dict) and 'model_result' in value:
                formatted, layouts = format_sources(deepcopy(substituted))
                value['model_result'] = formatted
                value['execution']['table_layouts'] = layouts
                value['execution']['summary'] = source_summary(substituted)
                value['execution']['source_verification'] = substituted_receipt
            return original(value)
        monkeypatch.setattr(db.CODEC, 'dump', replace)
    worker.execute(job)
    row = db.REPO.job(jid)
    assert row['status'] == 'error' and row['result'] is None
    assert 'joint market tape' in row['detail']


def test_zero_variance_driver_is_valid_in_publication_oracle():
    raw = model_routes.example('joint-market-credit-1')['request']
    raw['market']['annual_covariance'][0] = 0.
    result = evaluate_joint_market_credit(raw)
    assert model_jobs._verify_joint_market_credit(raw, result)['raw_covariance_verified']
