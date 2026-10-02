"""Stress submission validation, snapshot isolation and durable typed artifacts."""
import time
import json
import struct
from copy import deepcopy
from fastapi.testclient import TestClient
from app import main, store, persistence
from app.artifacts import Codec, Objects
from portfolio_risk.analytics.balance_stress import example_specification, run_balance_stress


def test_stress_http_validation_and_revision(monkeypatch):
    queued = []
    def submit(kind, fn, specification, *, state):
        queued.append((kind, fn, deepcopy(specification), state))
        return "test-job"
    with TestClient(main.app) as client:
        example = client.get('/balance-stress/example').json()
        body = dict(expected_revision=example['revision'], specification=example['specification'])
        bad = deepcopy(body)
        bad['specification']['accounts'][0]['equity'] += 1
        assert client.post('/balance-stress/run', json=bad).status_code == 422
        assert client.post('/balance-stress/run', json=body | {'expected_revision': body['expected_revision']+1}).status_code == 409
        monkeypatch.setattr(store, 'submit', submit)
        monkeypatch.setattr(store, 'job_status', lambda _: dict(id='test-job',kind='balance_stress',status='queued',revision=body['expected_revision']))
        assert client.post('/balance-stress/run', json=body).status_code == 200
        assert queued[0][0] == 'balance_stress'
        assert queued[0][2] == body['specification']
        assert queued[0][3]['revision'] == body['expected_revision']
        assert persistence.resolve(persistence.operation(queued[0][1])) is store.run_balance_stress


def test_stress_real_background_job_and_typed_parquet(tmp_path):
    with TestClient(main.app) as client:
        example = client.get('/balance-stress/example').json()
        spec = example['specification']
        spec.update(horizon_days=30, reverse_severities=[])
        submitted = client.post('/balance-stress/run', json=dict(specification=spec, expected_revision=example['revision']))
        assert submitted.status_code == 200, submitted.text
        jid = submitted.json()['id']
        end = time.monotonic()+30
        while time.monotonic() < end:
            job = client.get(f'/jobs/{jid}').json()
            if job['status'] in ('done', 'error'):
                break
            time.sleep(.05)
        assert job['status'] == 'done', job
        assert client.get(f'/jobs/{jid}/result').content.startswith(b'ARW1')
        encoded = store.job_result(jid)
        size = struct.unpack_from('<I', encoded, 4)[0]
        payload = json.loads(encoded[8:8+size])
        assert payload['specification'] == spec
        assert payload['revision'] == example['revision']
        result = run_balance_stress(spec)
        codec = Codec(Objects(str(tmp_path / 'artifacts'), 'test', 'stress'))
        ref = codec.dump(result)
        restored = codec.load(ref)
        for name in ('path', 'summary', 'actions', 'ledger', 'breaches', 'reverse_grid', 'attribution', 'exposures',
                     'journal', 'trial_balance', 'funding_claims', 'closing_statements', 'consolidated'):
            assert restored[name].equals(result[name])


def test_saved_book_inventory_mapping_and_durable_operation(monkeypatch):
    import polars as pl
    queued = []
    monkeypatch.setattr(store, 'balance_sheet', lambda state: {'loans': pl.DataFrame({'id': ['x']})})
    def submit(kind, fn, request, *, state):
        queued.append((kind, fn, deepcopy(request), state))
        return 'saved-job'
    with TestClient(main.app) as client:
        inv = client.get('/balance-stress/inventory').json()
        assert inv['instruments'][0]['source_id'] == 'loans:x'
        body = dict(expected_revision=inv['revision'], specification={}, amount_scale=1., position_mapping={})
        assert client.post('/balance-stress/saved-book', json=body).status_code == 422
        assert client.post('/balance-stress/saved-book', json=body | {'expected_revision': inv['revision']+1}).status_code == 409
        body['position_mapping'] = {'loans:x': dict(account='a', kind='loan', classification='ac', risk_weight=1.,
            asf_weight=0., rsf_weight=1., lcr_outflow_weight=0., hqla_weight=0.)}
        monkeypatch.setattr(store, 'submit', submit)
        monkeypatch.setattr(store, 'job_status', lambda _: dict(id='saved-job', kind='saved_balance_stress', status='queued', revision=inv['revision']))
        assert client.post('/balance-stress/saved-book', json=body).status_code == 200
        assert queued[0][2]['position_mapping'] == body['position_mapping']
        assert persistence.resolve(persistence.operation(queued[0][1])) is store.run_saved_balance_stress


def test_saved_book_real_engine_job_publishes_journal(monkeypatch):
    import datetime as dt
    import polars as pl
    from portfolio_risk.analytics.balance_stress import MODEL_VERSION
    def book(state):
        return {'loans': pl.DataFrame([dict(id='loan', face=100., maturity=state['asof']+dt.timedelta(days=365),
            freq_months=6, daycount='ACT/360', is_float=False, coupon_or_spread=.05,
            amort_type='bullet', price=100., book_yield=.05)])}
    monkeypatch.setattr(store, 'balance_sheet', book)
    with TestClient(main.app) as client:
        revision = client.get('/balance-stress/inventory').json()['revision']
        body = dict(expected_revision=revision, amount_scale=1.,
            specification=dict(version=MODEL_VERSION, horizon_days=30,
                accounts=[dict(id='a', entity='bank', currency='USD', cash=20., equity=120.)],
                scenarios=[dict(name='up', rate_shift=.01)]),
            position_mapping={'loans:loan': dict(account='a', kind='loan', classification='ac',
                risk_weight=1., asf_weight=0., rsf_weight=1., lcr_outflow_weight=0., hqla_weight=0.)})
        response = client.post('/balance-stress/saved-book', json=body)
        assert response.status_code == 200, response.text
        jid = response.json()['id']
        end = time.monotonic()+30
        while time.monotonic() < end:
            status = client.get(f'/jobs/{jid}').json()
            if status['status'] in ('done', 'error'):
                break
            time.sleep(.05)
        assert status['status'] == 'done', status
        encoded = store.job_result(jid)
        size = struct.unpack_from('<I', encoded, 4)[0]
        payload = json.loads(encoded[8:8+size])
        assert payload['revision'] == revision
        assert payload['source']['adapter'] == 'saved-book-monthly-v1'
        assert payload['validation']['dynamic_validated']
        assert 'journal' in payload and 'closing_statements' in payload
