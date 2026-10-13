"""Durable dated coefficients, recovery and coefficient-only interactive ownership."""
import numpy as np
import pytest
from app import store,worker,model_routes,persistence as db
from test_persistence import durable


def queued():
    raw=model_routes.example('aged-units-1')
    jid=model_routes.aged_library(model_routes.ModelRunRequest(schema_name=raw['schema_name'],request=raw['request'],expected_revision=0)).id
    db.WORKER='aged-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return jid,db.REPO.claim(db.WORKER)


def test_native_preparation_publishes_and_recovers_coefficients_only(durable,monkeypatch):
    jid,job=queued();worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    assert db.REPO.library() is not None
    lib=store.CACHE['library'];assert store.CACHE['base'] is None
    assert store.library_metadata()['library_reporting_ordinals']==lib['reporting_ordinals']
    allocation=dict(template=lib['units'][0]['template'],purchase_m=lib['units'][0]['h'],notional=100.)
    from app.schemas import Allocation
    assert Allocation(**allocation).template==allocation['template']
    from portfolio_risk.core import quant_native
    monkeypatch.setattr(quant_native,'term_call',lambda *a,**k:pytest.fail('interactive repricing'))
    output=store.eval_strategy_sync([allocation])
    np.testing.assert_allclose(output['fwd_dv01'],lib['vectors'][(allocation['template'],allocation['purchase_m'])][2]*100.,rtol=0,atol=1e-12)
    assert output['reporting_ordinals']==lib['reporting_ordinals'] and 'kpis' not in output
    store.CACHE.clear();worker.recover()
    recovered=store.eval_strategy_sync([allocation])
    np.testing.assert_array_equal(output['fwd_dv01'],recovered['fwd_dv01'])
    with pytest.raises(ValueError,match='REBUILD_REQUIRED'):
        store.eval_strategy_sync([dict(allocation,purchase_m=999)])
    with pytest.raises(ValueError,match='unknown template'):
        store.eval_strategy_sync([dict(allocation,template='not-published')])


def test_nonpar_quote_is_published_with_native_actual_cash_coefficients(durable):
    raw=model_routes.example('aged-units-1')
    terms=raw['request'];terms['purchases'][0]['quote_price_per_unit']=.9
    terms['purchases'][0]['original_annual_effective_yield']=(1/.9)**(365/366)-1
    jid=model_routes.aged_library(model_routes.ModelRunRequest(schema_name='aged-units-1',request=terms,expected_revision=0)).id
    db.WORKER='discount-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER));assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    result=store.eval_strategy_sync([dict(template='fixed-zero-coupon',purchase_m=0,notional=100.)])
    np.testing.assert_allclose(result['funding_gap'],[90.,90.,-10.],rtol=0,atol=1e-12)
    np.testing.assert_array_equal(result['principal_exposure_gap'],[100.,100.])


@pytest.mark.parametrize('failure',['cancel','lease','upload','invalidyield','stale','deadline','finalsource','finalbinary','stop'])
def test_failed_dated_library_never_replaces_prior_published_library(durable,monkeypatch,failure):
    prior=dict(revision=0,library={'prior':True},base=None,compute_backend='rust')
    store.CACHE.update(prior)
    jid,job=queued()
    if failure=='cancel':db.REPO.cancel(jid)
    elif failure=='lease':db.REPO.release(db.WORKER)
    elif failure=='upload':
        original=db.CODEC.objects.put
        def fail(data,suffix):
            if suffix=='parquet':raise OSError('aged upload failure')
            return original(data,suffix)
        monkeypatch.setattr(db.CODEC.objects,'put',fail)
    elif failure in {'deadline','finalsource','finalbinary','stop'}:
        from portfolio_risk.core import model_native
        from portfolio_risk.analytics import balance_stream
        original=db.CODEC.dump
        def change(result):
            if isinstance(result,dict) and 'execution' in result and 'units' in result:
                if failure=='deadline':result['execution']['publication_deadline_unix']=0.
                elif failure=='finalsource':monkeypatch.setattr(balance_stream,'source_identity',lambda:'changed')
                elif failure=='finalbinary':monkeypatch.setattr(model_native,'binary_identity',lambda:'changed')
                else:worker.STOP.set()
            return original(result)
        monkeypatch.setattr(db.CODEC,'dump',change)
    else:
        payload=db.CODEC.load(job['request'])
        if failure=='invalidyield':payload['args'][0]['request']['purchases'][0]['quote_price_per_unit']=.95
        else:payload['args'][0]['source_sha256']='f'*64
        job['request']=db.CODEC.dump(payload)
    worker.execute(job)
    assert db.REPO.job(jid)['status']!='done' and db.REPO.job(jid)['result'] is None
    assert db.REPO.library() is None and store.CACHE==prior
