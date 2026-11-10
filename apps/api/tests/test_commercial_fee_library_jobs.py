"""Fee coefficient publication/recovery keeps service revenue outside NII."""
from copy import deepcopy
import json
from pathlib import Path

import numpy as np
import pytest

from app import model_routes, persistence as db, store, worker
from test_persistence import durable

ROOT = Path(__file__).resolve().parents[3]


def queued():
    raw = json.loads((ROOT/'examples/models/commercial-fee-units-1.json').read_text())
    request = model_routes.ModelRunRequest(schema_name=raw['schema'], request=raw, expected_revision=0)
    jid = model_routes.aged_library(request).id
    db.WORKER = 'commercial-fee-library-test'
    worker.STOP.clear(); assert db.REPO.acquire(db.WORKER)
    return jid, db.REPO.claim(db.WORKER)


def test_fee_library_reopens_and_evaluates_retained_income_without_repricing(durable, monkeypatch):
    jid, job = queued(); worker.execute(job)
    row = db.REPO.job(jid)
    assert row['status'] == 'done', row['detail']
    lib = store.CACHE['library']
    np.testing.assert_allclose(lib['nonintegral_fee_income'], [[.03]], atol=1e-12, rtol=0)
    from portfolio_risk.core import quant_native
    from portfolio_risk.analytics import model_contracts
    def forbidden(*args, **kwargs): raise AssertionError('interactive fee repricing')
    monkeypatch.setattr(quant_native, 'term_call', forbidden)
    monkeypatch.setattr(model_contracts, 'prepare_commercial_units', forbidden)
    unit = lib['units'][0]
    allocation = [dict(template=unit['template'], purchase_m=unit['h'], notional=1000.)]
    actual = store.eval_strategy_sync(allocation)
    np.testing.assert_allclose(actual['nii_incremental'], [0.], atol=1e-9, rtol=0)
    np.testing.assert_allclose(actual['nonintegral_fee_income_incremental'], [30.], atol=1e-9, rtol=0)
    assert actual['nonintegral_fee_income_total_$'] == pytest.approx(30., abs=1e-9)
    assert actual['nii_plus_fee_income_total_$'] == pytest.approx(30., abs=1e-9)
    assert 'kpis' not in actual
    store.CACHE.clear(); worker.recover()
    reopened = store.eval_strategy_sync(allocation)
    for key in ('nii_incremental', 'nonintegral_fee_income_incremental',
                'nii_plus_fee_income_incremental', 'funding_gap'):
        np.testing.assert_array_equal(reopened[key], actual[key])
    store.CACHE['library'].pop('fee_income_vectors')
    with pytest.raises(ValueError, match='REBUILD_REQUIRED'):
        store.eval_strategy_sync(allocation)


@pytest.mark.parametrize('target', ['income_array', 'income_vector', 'allocated_cost'])
def test_rehashed_fee_library_cells_cannot_substitute_source(durable, monkeypatch, target):
    jid, job = queued(); dump = db.CODEC.dump; changed = []
    def mutate(value):
        if type(value) is dict and 'cache' in value and 'library' in value['cache']:
            value = deepcopy(value); lib = value['cache']['library']; changed.append(True)
            if target == 'income_array': lib['nonintegral_fee_income'][0, 0] += 1.
            elif target == 'income_vector': next(iter(lib['fee_income_vectors'].values()))[0] += 1.
            else: lib['source_request']['units'][0]['graph']['positions'][0]['fees']['allocations'][0]['allocated_cost'] += 1.
        return dump(value)
    monkeypatch.setattr(db.CODEC, 'dump', mutate); worker.execute(job)
    row = db.REPO.job(jid)
    assert changed, row['detail']
    assert row['status'] != 'done' and row['result'] is None and db.REPO.library() is None
