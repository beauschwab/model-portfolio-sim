"""Proposed durable source/publication gates; require rebuilt installed binaries."""
import pytest
from app import model_routes,store,worker,persistence as db,treasury_store
from test_persistence import durable
from test_model_runs import queued
SCHEMA='market-indexed-adapter-1'
def test_verified_supplied_source_persists_reopens_and_has_no_ledger_eligibility(durable,monkeypatch):
    from portfolio_risk.core import quant_native
    def forbidden(*args,**kwargs):raise AssertionError('in-process financial execution')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    jid,job=queued(SCHEMA);worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    value=db.CODEC.load(db.result_ref(jid));receipt=value['execution']['source_verification']
    assert receipt['verified'] and receipt['analytical_market_indexed_only']
    assert value['execution']['summary']['capital_ftp_eligible'] is False
    before=store.job_result(jid);db.stop();db.start(durable);assert store.job_result(jid)==before
    for kind in ['ledger','credit_ledger','cashflows']:
        with pytest.raises(ValueError):treasury_store._source(jid,kind)
@pytest.mark.parametrize('failure',['amount','receipt','source','binary','deadline','cancel'])
def test_persisted_supplied_source_is_reverified_before_any_manifest(durable,monkeypatch,failure):
    import polars as pl
    from portfolio_risk.core import model_native
    from portfolio_risk.analytics import balance_stream
    jid,job=queued(SCHEMA);dump=db.CODEC.dump
    def corrupt(value):
        if isinstance(value,dict) and 'model_result' in value:
            if failure=='receipt':value['execution'].pop('source_verification')
            elif failure=='source':monkeypatch.setattr(balance_stream,'source_identity',lambda:'changed')
            elif failure=='binary':monkeypatch.setattr(model_native,'binary_identity',lambda:'changed')
            elif failure=='deadline':value['execution']['publication_deadline_unix']=0.
            elif failure=='cancel':db.REPO.cancel(jid)
            else:
                def alter(v):
                    if isinstance(v,pl.DataFrame):
                        for key,dtype in v.schema.items():
                            if dtype==pl.Float64 and v.height:return v.with_columns((pl.col(key)+1).alias(key)),True
                    if isinstance(v,dict):
                        for key,child in v.items():
                            changed,done=alter(child)
                            if done:v[key]=changed;return v,True
                    return v,False
                _,changed=alter(value['model_result']);assert changed
        return dump(value)
    monkeypatch.setattr(db.CODEC,'dump',corrupt);worker.execute(job)
    state=db.REPO.job(jid);assert state['status']!='done' and state['result'] is None
def test_native_raw_failure_never_reaches_independent_publication(durable,monkeypatch):
    from portfolio_risk.analytics import market_indexed_reference
    def forbidden(*args,**kwargs):raise AssertionError('verifier cannot rescue invalid native input')
    monkeypatch.setattr(market_indexed_reference,'verify',forbidden)
    r=model_routes.example(SCHEMA);r['request']['bridge']['current_market']['curves'][0]['ignored_money']=0
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=SCHEMA,request=r['request'],expected_revision=0))
    db.WORKER='market-invalid';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER));state=db.REPO.job(submitted.id)
    assert state['status']!='done' and state['result'] is None and 'unknown field' in state['detail']
