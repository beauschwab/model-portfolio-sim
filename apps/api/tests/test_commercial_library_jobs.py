"""Durable commercial coefficient library and exact final publication ownership."""
from copy import deepcopy
import json
import numpy as np
import pytest
from app import store,worker,model_routes,persistence as db
from test_persistence import durable

def queued():
    raw=model_routes.example('commercial-units-1')['request']
    jid=model_routes.aged_library(model_routes.ModelRunRequest(schema_name='commercial-units-1',request=raw,expected_revision=0)).id
    db.WORKER='commercial-library-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return jid,db.REPO.claim(db.WORKER)

def test_commercial_library_recovers_and_evaluates_coefficients_without_financial_generators(durable,monkeypatch):
    jid,job=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix'];worker.execute(job)
    row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    result=db.CODEC.load(db.result_ref(jid));assert result['execution']['publication_deadline_unix']==deadline
    lib=store.CACHE['library'];assert lib['preparation_schema']=='commercial-units-1'
    assert store.library_metadata()['library_schema']=='commercial-units-1'
    from portfolio_risk.core import quant_native
    monkeypatch.setattr(quant_native,'term_call',lambda *a,**k:pytest.fail('commercial interactive repricing'))
    u=lib['units'][0];allocation=dict(template=u['template'],purchase_m=u['h'],notional=1000.)
    expected=store.eval_strategy_sync([allocation]);assert 'kpis'not in expected
    np.testing.assert_allclose(expected['nii_incremental'],[0.],atol=1e-9,rtol=0)
    assert expected['reporting_ordinals']==lib['reporting_ordinals']
    store.CACHE.clear();worker.recover();actual=store.eval_strategy_sync([allocation])
    np.testing.assert_array_equal(actual['nii_incremental'],expected['nii_incremental'])
    np.testing.assert_array_equal(actual['funding_gap'],expected['funding_gap'])
    with pytest.raises(ValueError,match='REBUILD_REQUIRED'):store.eval_strategy_sync([dict(allocation,purchase_m=999)])

@pytest.mark.parametrize('target',['coefficient','funding','raw_anchor','receipt'])
def test_rehashed_library_cannot_substitute_verified_original_source_coefficients(durable,monkeypatch,target):
    jid,job=queued();dump=db.CODEC.dump
    def changed(value):
        if isinstance(value,dict)and'cache'in value and'library'in value['cache']:
            value=deepcopy(value);lib=value['cache']['library']
            if target=='coefficient':lib['nii'][0,0]+=1.
            elif target=='funding':next(iter(lib['funding_vectors'].values()))[-1]+=1.
            elif target=='raw_anchor':lib['source_request']['units'][0]['graph']['positions'][0]['original_signed_cash_quote']+=1.
        if target=='receipt'and isinstance(value,dict)and'preparation_schema'in value and'execution'in value:
            value=deepcopy(value);value['execution']['source_verification']['verified']=1
        return dump(value)
    monkeypatch.setattr(db.CODEC,'dump',changed);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None and db.REPO.library()is None

@pytest.mark.parametrize('target',['source','deadline','cancel','library_partition','attempt'])
def test_commercial_library_sql_guard_checks_final_objects_and_original_authority(durable,monkeypatch,target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid,job=queued();complete=db.REPO.complete
    def changed(bound,**kw):
        if kw.get('guard'):
            if target=='cancel':worker.STOP.set()
            elif target=='source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            elif target=='library_partition':
                descriptor=next(iter(db.CODEC.tables(kw['library']).values()));(db.CODEC.objects.root/descriptor['key']).write_bytes(b'changed')
            else:
                with db.REPO.engine.begin()as connection:
                    if target=='attempt':change=dict(attempt=bound['attempt']+1)
                    else:
                        payload=db.CODEC.load(bound['request']);payload['args'][0]['publication_deadline_unix']+=3600.;change=dict(request=db.CODEC.dump(payload))
                    connection.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(**change))
        return complete(bound,**kw)
    monkeypatch.setattr(db.REPO,'complete',changed)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None and db.REPO.library()is None

def test_commercial_aged_library_preserves_original_http_duplicate_detection(durable):
    from fastapi.testclient import TestClient
    from app import main
    raw=model_routes.example('commercial-units-1')['request'];body=json.dumps(dict(expected_revision=0,schema_name='commercial-units-1',request=raw),separators=(',',':'))
    body=body.replace('"unit_face":100.0','"unit_face":100.0,"unit_face":101.0',1)
    with TestClient(main.app)as client:response=client.post('/models/aged-library',content=body,headers={'content-type':'application/json'})
    assert response.status_code==422 and 'duplicate'in response.text
