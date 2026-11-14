"""Standard EVE/NII rate shock report: policy storage, admission and the durable job."""
import time

import pytest
from fastapi.testclient import TestClient

from app import main, store
from test_contracts import skeleton


@pytest.fixture
def client():
    with store._LOCK:
        store.BOOKS.clear()
        store.SCENARIOS.clear()
        store.JOBS.clear()
        store.CACHE.clear()
        store.ASSUMPTIONS.pop('rate_shock_policy', None)
        store.SETTINGS = main.RiskSettings(n_threads=2, n_paths=33, n_paths_base=35, horizon_months=24)
        store.STATE_META["revision"] = 0
        store._FINGERPRINTS.update(revision=None, inputs={})   # memoised by revision, which this fixture resets
    with TestClient(main.app) as client:
        yield client
    with store._LOCK:
        store.ASSUMPTIONS.pop('rate_shock_policy', None)


def finished(jid, seconds=180):
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        job = store.job_status(jid)
        if job["status"] in ("done", "error"):
            return job
        time.sleep(.02)
    raise AssertionError("job did not finish")


def test_policy_defaults_validation_and_revision(client):
    body = client.get('/rate-shocks/policy').json()
    assert body['configured'] is False and body['schema'] == 'rate-shock-1'
    assert body['policy']['parallel_bp'] == [100, 200, 300, 400] and body['policy']['limits'] == []
    names = [s['name'] for s in body['scenarios']]
    assert len(names) == 14 and names[:2] == ['parallel_up_100', 'parallel_down_100']
    assert {'steepener', 'flattener', 'short_up', 'short_down', 'ramp_up_200', 'ramp_down_200'} <= set(names)
    revision = body['revision']
    policy = body['policy'] | {'parallel_bp': [200], 'floor': 0.0, 'limits': [
        {'metric': 'eve', 'scenario': 'parallel_up_200', 'applicability': 'internal_limit', 'max_decline_pct': 15}]}

    def put(value, expected=revision):
        return client.put('/rate-shocks/policy', json={'policy': value, 'expected_revision': expected})

    assert put(policy, revision + 1).status_code == 409
    # A limit on a scenario the policy no longer produces is refused, not orphaned.
    assert put(policy | {'parallel_bp': [100]}).status_code == 422
    assert put(policy | {'limits': [{'metric': 'eve', 'scenario': 'parallel_up_200'}]}).status_code == 422
    assert put(policy | {'limits': policy['limits'] * 2}).status_code == 422
    assert put(policy | {'parallel_bp': [200, 100]}).status_code == 422
    assert put(policy | {'surprise': 1}).status_code == 422
    assert client.get('/rate-shocks/policy').json()['revision'] == revision
    before = store.input_fingerprints()['nodes']
    assert put(policy).status_code == 200
    after = store.input_fingerprints()['nodes']
    # Saving limits moves only its own input node, so KPIs, NII, risk and stress stay current.
    assert {k for k in after if after[k] != before.get(k)} == {'rate_shock_policy'}
    saved = client.get('/rate-shocks/policy').json()
    assert saved['configured'] is True and saved['revision'] == revision + 1
    assert saved['policy']['limits'][0]['max_decline_pct'] == 15
    assert len(saved['scenarios']) == 8


def test_report_runs_on_the_queued_policy_and_never_passes_without_a_limit(client):
    revision = client.get('/rate-shocks/policy').json()['revision']
    policy = client.get('/rate-shocks/policy').json()['policy'] | {'parallel_bp': [200], 'limits': [
        {'metric': 'eve', 'scenario': 'parallel_up_200', 'applicability': 'internal_limit', 'max_decline_pct': 0.0001},
        {'metric': 'nii_12m', 'scenario': 'parallel_up_200', 'applicability': 'internal_limit', 'max_decline_pct': 99},
        {'metric': 'eve', 'scenario': 'ramp_up_200', 'applicability': 'internal_limit', 'max_decline_pct': 5},
        {'metric': 'nii_24m', 'scenario': 'steepener', 'applicability': 'not_applicable'}]}
    assert client.put('/rate-shocks/policy', json={'policy': policy, 'expected_revision': revision}).status_code == 200
    assert client.post('/rate-shocks/run', json={'expected_revision': revision}).status_code == 409
    response = client.post('/rate-shocks/run', json={'expected_revision': revision + 1})
    assert response.status_code == 200, response.text
    assert response.json()['progress']['plan']['kind'] == 'rate_shocks'
    jid = response.json()['id']
    job = finished(jid)
    assert job['status'] == 'done', job
    result = skeleton(client.get(f'/jobs/{jid}/result').content)
    assert result['schema'] == 'rate-shock-1' and result['revision'] == revision + 1
    assert result['policy'] == client.get('/rate-shocks/policy').json()['policy'] and result['horizon_months'] == 24
    assert result['policy']['parallel_bp'] == [200] and len(result['policy']['limits']) == 4
    assert result['status'] == 'breach'
    assert result['counts'] == {'breach': 1, 'not_applicable': 1, 'unavailable': 1, 'unconfigured': 20, 'within_limit': 1}
    assert set(result['conventions']) >= {'floor', 'eve', 'nii', 'ramp', 'valuation', 'limits'}
