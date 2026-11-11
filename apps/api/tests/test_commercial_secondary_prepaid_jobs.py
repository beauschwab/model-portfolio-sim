"""Reviewed prepaid refund ownership survives raw and persisted publication gates."""
from copy import deepcopy
import json
import os
from pathlib import Path
import subprocess
import pytest
from app import model_routes,persistence as db,worker
from app.model_tables import format_sources,restore_sources
from test_persistence import durable
ROOT=Path(__file__).resolve().parents[3]
@pytest.fixture(autouse=True)
def selected_debug_artifact(monkeypatch):
    selected=os.environ.get('SECONDARY_NATIVE_DEBUG_DIR')
    if selected:
        from portfolio_risk.core import product_strategy_native
        monkeypatch.setattr(product_strategy_native,'binary_path',lambda:Path(selected)/'portfolio-strategy.exe')
def queued(default=False):
    js='const fs=require("fs");process.stdout.write(JSON.stringify(JSON.parse(fs.readFileSync(process.argv[1],"utf8"))))'
    raw=json.loads(subprocess.run(['node','-e',js,str(ROOT/'examples/models/commercial-secondary-prepaid-candidate-journal-1.json')],check=True,capture_output=True,text=True).stdout)
    if default:
        for hs in raw['commercial']['units'][0]['graph']['positions'][0]['histories']:
            for h in hs['physical']:h['source']['credit']['intervals'][0]['hazards_per_year'][0][3]=100.
    jid=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0)).id
    db.WORKER='secondary-prepaid-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return jid,db.REPO.claim(db.WORKER),raw
def physical(result):
    g=result['preparation']['native_preparation']['source']['native_graphs'][0]
    return next(s for s in g['native_sources']if s['role']=='physical')['fee_projection']['bound_path']
@pytest.mark.parametrize('default',[False,True])
def test_prepaid_buyer_publishes_reopens_and_replays_actual_refund(durable,default):
    jid,job,raw=queued(default);deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));assert saved['execution']['publication_deadline_unix']==deadline
    result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
    from portfolio_risk.analytics.commercial_candidate_reference import verify
    assert verify(raw,result)['persisted_partitions_replayed']
    bound=physical(result);stock=next(g['next']for g in bound['source']['groups']if g['event']['action']=='acquire')
    assert stock['deferred_fee_income']==-6. and stock['acquired_fee_liability_valuation_adjustment']==1.
    refunds=[g for g in bound['source']['groups']if g['event']['action']=='refund_prepaid']
    assert len(refunds)==1 and refunds[0]['event']['event_id'].startswith('native-fee:')==default
    assert not any(r['gl_account']=='pnl:commercial_nonintegral_fee_income'for r in result['tables']['journal'])
@pytest.mark.parametrize('target',['refund_suppression','exit_review','selected_convention','gross','contra','cash','deadline'])
def test_rehashed_prepaid_raw_owner_and_persisted_monetary_tamper_refuses(durable,monkeypatch,target):
    jid,job,_=queued(True);dump=db.CODEC.dump;load=db.CODEC.load;changed=[]
    def mutate(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='commercial-candidate-journal-1':return ref
        saved=load(ref);result=restore_sources(saved['model_result'],saved['execution']['table_layouts']);changed.append(True)
        bound=physical(result);refund=next(g for g in bound['source']['groups']if g['event']['action']=='refund_prepaid')
        stock=next(g['next']for g in bound['source']['groups']if g['event']['action']=='acquire')
        if target=='refund_suppression':refund['event']['event_id']='buyer-refunds-prepaid-consideration'
        elif target=='exit_review':refund['event']['review']['revision']='unreviewed-change'
        elif target=='selected_convention':
            result['preparation']['raw_request']['commercial']['units'][0]['graph']['positions'][0]['fees']['original_pricing'][0]['policy']['transfer']['purchase_cash_policy']['exit_refunds']='forgive_remaining'
        elif target=='gross':stock['deferred_fee_income']-=1.
        elif target=='contra':stock['acquired_fee_liability_valuation_adjustment']+=1.
        elif target=='cash':next(r for r in result['tables']['journal']if r['gl_account']=='cash'and r['value']<0.)['value']-=1.
        else:saved['execution']['publication_deadline_unix']+=3600.
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',mutate);worker.execute(job)
    row=db.REPO.job(jid);assert changed and row['status']!='done'and row['result']is None
@pytest.mark.parametrize('target',['artifact','source','cancel'])
def test_prepaid_final_sql_authority_fence_still_required(durable,monkeypatch,target):
    jid,job,_=queued();complete=db.REPO.complete
    def commit(bound,**kw):
        if kw.get('guard'):
            if target=='artifact':
                descriptor=next(iter(db.CODEC.tables(kw['result']).values()));(db.CODEC.objects.root/descriptor['key']).write_bytes(b'changed-prepaid-after-verification')
            elif target=='source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            else:worker.STOP.set()
        return complete(bound,**kw)
    monkeypatch.setattr(db.REPO,'complete',commit)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None
