"""Buyer secondary rights survive durable publication and final authority checks."""
from copy import deepcopy
import json
import os
from pathlib import Path
import subprocess
import pytest
from app import model_routes, persistence as db, worker
from app.model_tables import format_sources,restore_sources
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]

@pytest.fixture(autouse=True)
def selected_debug_artifact(monkeypatch):
    selected=os.environ.get('SECONDARY_NATIVE_DEBUG_DIR')
    if selected:
        from portfolio_risk.core import product_strategy_native
        monkeypatch.setattr(product_strategy_native,'binary_path',lambda:Path(selected)/'portfolio-strategy.exe')

def queued():
    js='const fs=require("fs");process.stdout.write(JSON.stringify(JSON.parse(fs.readFileSync(process.argv[1],"utf8"))))'
    raw=json.loads(subprocess.run(['node','-e',js,str(ROOT/'examples/models/commercial-secondary-candidate-journal-1.json')],check=True,capture_output=True,text=True).stdout)
    jid=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0)).id
    db.WORKER='secondary-candidate-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return jid,db.REPO.claim(db.WORKER),raw

def test_secondary_buyer_publishes_reopens_and_independently_replays(durable):
    jid,job,raw=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));assert saved['execution']['publication_deadline_unix']==deadline
    assert saved['execution']['source_verification']['purchased_journal_replayed']
    result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
    assert result['preparation']['raw_request']==raw and result['producer_completion']==dict(complete=True,protocol=7)
    from portfolio_risk.analytics.commercial_candidate_reference import verify
    assert verify(raw,result)['persisted_partitions_replayed']
    stock=next(r['native_fee_snapshot']for r in result['tables']['commercial_fee_daily']if r['day']==0)
    assert stock['fee_receivable']==6. and stock['acquired_fee_asset_valuation_adjustment']==-1.

@pytest.mark.parametrize('target',['origin','bought_coupon','fee_contra','fee_gross','journal','deadline'])
def test_rehashed_secondary_acquisition_and_persisted_posting_tamper_refuses(durable,monkeypatch,target):
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load;changed=[]
    def mutate(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='commercial-candidate-journal-1':return ref
        saved=load(ref);result=restore_sources(saved['model_result'],saved['execution']['table_layouts']);changed.append(True)
        graph=result['preparation']['native_preparation']['source']['native_graphs'][0]
        source=next(s for s in graph['native_sources']if s['role']=='physical')
        transfer=source['projection']['secondary_acquisition']
        stock=next(g['next']for g in source['fee_projection']['bound_path']['source']['groups']if g['event']['action']=='acquire')
        if target=='origin':transfer['contract_origin_ordinal']+=1
        elif target=='bought_coupon':transfer['purchased_legal_interest']+=1.
        elif target=='fee_contra':stock['acquired_fee_asset_valuation_adjustment']-=1.
        elif target=='fee_gross':stock['fee_receivable']+=1.
        elif target=='journal':next(r for r in result['tables']['journal']if r['gl_account']=='cash'and r['value']<0.)['value']-=1.
        else:saved['execution']['publication_deadline_unix']+=3600.
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',mutate);worker.execute(job)
    row=db.REPO.job(jid);assert changed and row['status']!='done'and row['result']is None

@pytest.mark.parametrize('target',['artifact','source','cancel'])
def test_secondary_final_sql_fences_refuse_after_persisted_replay(durable,monkeypatch,target):
    jid,job,_=queued();complete=db.REPO.complete
    def commit(bound,**kw):
        if kw.get('guard'):
            if target=='artifact':
                descriptor=next(iter(db.CODEC.tables(kw['result']).values()));(db.CODEC.objects.root/descriptor['key']).write_bytes(b'changed-secondary-after-verification')
            elif target=='source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            else:worker.STOP.set()
        return complete(bound,**kw)
    monkeypatch.setattr(db.REPO,'complete',commit)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None
