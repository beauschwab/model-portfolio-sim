"""Default installs admit only the v1 ALM scope; experimental work needs an explicit setting."""
import pytest
from fastapi.testclient import TestClient

from app import main, supported_scope
from app.model_routes import EXAMPLES


@pytest.fixture
def client():
    return TestClient(main.app)


@pytest.fixture
def v1_only(monkeypatch):
    monkeypatch.delenv(supported_scope.SETTING, raising=False)


def test_registry_names_only_known_contracts():
    assert supported_scope.V1_MODEL_SCHEMAS <= set(EXAMPLES)
    experimental = set(EXAMPLES) - supported_scope.V1_MODEL_SCHEMAS
    # The lanes #63 pauses stay outside the default surface.
    assert {s for s in EXAMPLES if s.startswith(('commercial-', 'card-', 'consumer-'))} <= experimental
    assert {'hedge-accounting-1', 'lifetime-credit-loss-1', 'product-mixed-product-credit-1'} <= experimental


def test_default_contract_listing_exposes_only_v1(client, v1_only):
    body = client.get('/models/contracts').json()
    assert body['experimental_enabled'] is False
    assert set(body['schemas']) == supported_scope.V1_MODEL_SCHEMAS == set(body['supported'])
    assert set(body['experimental']) == set(EXAMPLES) - supported_scope.V1_MODEL_SCHEMAS


def test_explicit_setting_exposes_every_contract(client):
    body = client.get('/models/contracts').json()
    # The enabled listing keeps its historical order.
    assert body['experimental_enabled'] is True and body['schemas'] == list(EXAMPLES)
    assert client.get('/models/examples/commercial-units-1').status_code == 200


@pytest.mark.parametrize('method,path,body', [
    ('get', '/models/examples/commercial-units-1', None),
    ('post', '/models/runs', dict(expected_revision=0, schema_name='card-credit-accounting-2', request={})),
    ('post', '/models/runs', dict(expected_revision=0, schema_name='graph-cohort-audit-1', request={'kind': 'commercial'})),
    ('post', '/models/aged-library', dict(expected_revision=0, schema_name='commercial-units-1', request={})),
    ('post', '/models/holding-journals', dict(expected_revision=0, source_job_id='j', cut_ordinal=1, revised_sources=[{}])),
    ('post', '/models/candidate-holding-journals', dict(expected_revision=0, source_job_id='j', cut_ordinal=1, revised_candidate={})),
    ('post', '/models/cut-mixed-journals', dict(expected_revision=0, source_job_id='j', cut_ordinal=1, revised_candidate={})),
    ('post', '/balance-stress/credit', dict(expected_revision=0, specification={})),
    ('post', '/balance-stress/credit-path', dict(expected_revision=0, specification={})),
    ('post', '/balance-stress/modeled', dict(expected_revision=0, specification={'schema': 'mortgage-mixed-accounting-1'})),
    ('post', '/treasury/mortgage-closing-capital', {}),
])
def test_experimental_admission_is_refused_by_default(client, v1_only, method, path, body):
    response = client.request(method, path, json=body)
    assert response.status_code == 403, response.text
    assert supported_scope.SETTING in response.json()['detail']


def test_v1_admission_is_not_refused_by_default(client, v1_only):
    assert client.get('/models/examples/deposit-state-1').status_code == 200
    # Memory mode has no durable queue: reaching that check proves the scope gate admitted the contract.
    response = client.post('/models/runs', json=dict(expected_revision=0, schema_name='deposit-state-1', request={}))
    assert response.status_code == 409 and 'durable' in response.json()['detail']
    response = client.post('/models/runs', json=dict(expected_revision=0, schema_name='graph-cohort-audit-1', request={'kind': 'mbs'}))
    assert response.status_code == 409
