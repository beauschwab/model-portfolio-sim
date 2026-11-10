"""Conveyed fee source4 funding and private closing capital retain SQL fences."""
import json
import os
from pathlib import Path
import pytest
from app import model_routes, persistence as db, worker
from app.model_tables import format_sources, restore_sources
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]
SCHEMA='commercial-candidate-fee-treasury-1'

@pytest.fixture(autouse=True)
def selected_native(monkeypatch):
    if selected:=os.environ.get('PORTFOLIO_FEE_TREASURY_TEST_BINARY'):
        from portfolio_risk.core import product_strategy_native
        monkeypatch.setattr(product_strategy_native,'binary_path',lambda:Path(selected))

def queued():
    raw=json.loads((ROOT/'examples/models/commercial-conveyed-fee-treasury-1.json').read_text())
    jid=model_routes.submit(model_routes.ModelRunRequest(schema_name=SCHEMA,request=raw,expected_revision=0)).id
    db.WORKER='conveyed-fee-treasury-test';worker.STOP.clear()
    # Direct execute has no worker heartbeat; reserve the complete replay window.
    assert db.REPO.acquire(db.WORKER,300.)
    return jid,db.REPO.claim(db.WORKER),raw

def test_conveyed_fee_treasury_publishes_reopens_and_retains_original_deadline(durable):
    jid,job,raw=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['publication_deadline_unix']==deadline
    assert execution['source_verification']['persisted_partitions_replayed']
    result=restore_sources(saved['model_result'],execution['table_layouts'])
    assert result['raw_request']==raw and result['conveyed_fee_source_adoption']is True
    assert result['capital']['conveyed_fee_asset_adjustment_grouped']is True
    assert all(p['request']['schema']=='modeled-product-ftp-source-channels-4'for p in result['funding_paths'])
    db.stop();db.start(durable)
    assert db.CODEC.load(db.result_ref(jid))['execution']==execution

@pytest.mark.parametrize('target',['contra','buyer_income','capital_group','raw_review'])
def test_rehashed_conveyed_earnings_stock_and_review_cannot_publish(durable,monkeypatch,target):
    jid,job,_=queued();dump,load=db.CODEC.dump,db.CODEC.load;reached=[]
    def mutate(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!=SCHEMA:return ref
        reached.append(True);saved=load(ref)
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='contra':
            next(a for a in result['funding_paths'][0]['request']['states'][0]['channels']if a['channel']=='acquired_fee_asset_valuation_adjustment')['amount']+=1.
        elif target=='buyer_income':
            next(a for d in result['funding_paths'][0]['preparation']['days']for a in d['earnings_by_channel']if a['channel']=='fee_cost_release')['amount']+=1.
        elif target=='capital_group':result['capital']['conveyed_fee_asset_adjustment_grouped']=False
        else:result['raw_request']['secondary_fee_review']['revision']='changed'
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',mutate);worker.execute(job)
    row=db.REPO.job(jid);assert reached and row['status']!='done'and row['result']is None

@pytest.mark.parametrize('target',['artifact','source','lease','attempt','queued_deadline','cancel'])
def test_conveyed_final_sql_commit_rechecks_immutable_objects_and_original_authority(durable,monkeypatch,target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid,job,_=queued();complete=db.REPO.complete;reached=[]
    def commit(bound,**kwargs):
        if kwargs.get('guard'):
            reached.append(True)
            if target=='artifact':
                descriptor=next(iter(db.CODEC.tables(kwargs['result']).values()))
                (db.CODEC.objects.root/descriptor['key']).write_bytes(b'changed-conveyed-after-replay')
            elif target=='source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            elif target=='lease':db.REPO.release(db.WORKER)
            elif target=='cancel':worker.STOP.set()
            else:
                if target=='attempt':changes=dict(attempt=bound['attempt']+1)
                else:
                    payload=db.CODEC.load(bound['request']);payload['args'][0]['publication_deadline_unix']+=3600.
                    changes=dict(request=db.CODEC.dump(payload))
                with db.REPO.engine.begin()as conn:
                    conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(**changes))
        return complete(bound,**kwargs)
    monkeypatch.setattr(db.REPO,'complete',commit)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert reached and row['status']!='done'and row['result']is None
