"""Durable product coefficients retain original income and funding through recovery."""
from math import exp,log1p,expm1
import numpy as np
import pytest
from app import store,worker,model_routes,persistence as db
from test_persistence import durable


def queued():
    raw=model_routes.example('product-units-1')
    jid=model_routes.aged_library(model_routes.ModelRunRequest(schema_name=raw['schema_name'],request=raw['request'],expected_revision=0)).id
    db.WORKER='product-library-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return jid,db.REPO.claim(db.WORKER)


def test_original_yield_product_library_recovers_without_pricers(durable,monkeypatch):
    jid,job=queued();worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    lib=store.CACHE['library'];assert lib['preparation_schema']=='product-units-1'
    assert store.library_metadata()['library_schema']=='product-units-1'
    u=lib['units'][0];allocation=dict(template=u['template'],purchase_m=u['h'],notional=1000.)
    from portfolio_risk.core import quant_native
    monkeypatch.setattr(quant_native,'term_call',lambda *a,**k:pytest.fail('interactive product repricing'))
    result=store.eval_strategy_sync([allocation])
    quote=1002*exp(-log1p(.02)*2/365);daily=expm1(log1p(.02)/365)
    np.testing.assert_allclose(result['nii_incremental'],[-quote*daily,-quote*(1+daily)*daily],rtol=0,atol=1e-9)
    assert result['funding_gap'][-1]==pytest.approx(1002-quote,abs=1e-9)
    assert 'kpis' not in result and result['reporting_ordinals']==lib['reporting_ordinals']
    store.CACHE.clear();worker.recover()
    restored=store.eval_strategy_sync([allocation])
    np.testing.assert_array_equal(restored['nii_incremental'],result['nii_incremental'])
    np.testing.assert_array_equal(restored['funding_gap'],result['funding_gap'])
    with pytest.raises(ValueError,match='REBUILD_REQUIRED'):
        store.eval_strategy_sync([dict(allocation,purchase_m=999)])


@pytest.mark.parametrize('failure',['yield','cancel','futuredeadline'])
def test_failed_product_library_preserves_published_library(durable,monkeypatch,failure):
    prior=dict(revision=0,library={'prior':True},base=None,compute_backend='rust');store.CACHE.update(prior)
    jid,job=queued()
    if failure=='yield':
        payload=db.CODEC.load(job['request']);payload['args'][0]['request']['units'][0]['income_policy']['original_annual_effective_rate']=.03
        job['request']=db.CODEC.dump(payload)
    elif failure=='cancel':db.REPO.cancel(jid)
    else:
        original=db.CODEC.dump
        def tamper(result):
            if isinstance(result,dict) and 'execution' in result and 'units' in result:
                result['execution']['publication_deadline_unix']+=3600.
            return original(result)
        monkeypatch.setattr(db.CODEC,'dump',tamper)
    worker.execute(job)
    assert db.REPO.job(jid)['status']!='done' and db.REPO.job(jid)['result'] is None
    assert db.REPO.library() is None and store.CACHE==prior


@pytest.mark.parametrize('failure',['coefficient','funding','yield','source','receipt','artifact_coefficient'])
def test_product_library_persisted_sources_and_adapter_cells_are_independent_publication_gates(durable,monkeypatch,failure):
    jid,job=queued();original=db.CODEC.dump
    def corrupt(value):
        if isinstance(value,dict) and 'cache' in value and 'library' in value['cache']:
            value['cache']=dict(value['cache']);lib=dict(value['cache']['library']);value['cache']['library']=lib
            if failure=='coefficient':lib['nii']=lib['nii'].copy();lib['nii'][0,0]+=1.
            elif failure=='funding':
                lib['funding_vectors']={k:v.copy() for k,v in lib['funding_vectors'].items()}
                next(iter(lib['funding_vectors'].values()))[-1]+=1.
            elif failure=='yield':
                from copy import deepcopy
                lib['source_request']=deepcopy(lib['source_request']);lib['source_request']['units'][0]['income_policy']['original_annual_effective_rate']+=.01
            elif failure=='source':
                from copy import deepcopy
                lib['unit_sources']=deepcopy(lib['unit_sources']);lib['unit_sources'][0]['locked_purchase_oas']+=.01
        if isinstance(value,dict) and 'execution' in value and 'units' in value:
            if failure=='receipt':value['execution'].pop('source_verification')
            elif failure=='artifact_coefficient':value['model_result']['values'][0]+=1.
        return original(value)
    monkeypatch.setattr(db.CODEC,'dump',corrupt)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None and db.REPO.library() is None
    assert 'library' not in store.CACHE


@pytest.mark.parametrize('failure',['source','binary','bundle','cancel','deadline','ownership'])
def test_final_progress_cannot_publish_changed_product_library(durable,monkeypatch,failure):
    prior=dict(revision=0,library={'prior':True},base=None,compute_backend='rust')
    store.CACHE.update(prior)
    jid,job=queued();original=db.REPO.progress
    def change_at_final_progress(active,progress):
        result=original(active,progress)
        if progress.get('stage')=='done':
            if failure=='source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'changed-source')
            elif failure=='binary':
                from portfolio_risk.core import model_native
                monkeypatch.setattr(model_native,'binary_identity',lambda:'changed-binary')
            elif failure=='bundle':
                original_identity=db.current_identity
                monkeypatch.setattr(db,'current_identity',lambda:dict(original_identity(),risk_native_sha256='changed-risk-library'))
            elif failure=='cancel':worker.STOP.set()
            elif failure=='deadline':
                from app import model_jobs
                expired=model_jobs.time.time()+3600.
                monkeypatch.setattr(model_jobs.time,'time',lambda:expired)
            else:db.REPO.cancel(jid)
        return result
    monkeypatch.setattr(db.REPO,'progress',change_at_final_progress)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
    assert db.REPO.library() is None and store.CACHE==prior
