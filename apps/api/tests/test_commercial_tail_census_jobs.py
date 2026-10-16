"""Selected tail census is independently rechecked through final SQL publish."""
import json,os,subprocess
from pathlib import Path
import pytest
from app import model_routes,persistence as db,worker
from app.model_tables import format_sources,restore_sources
from test_persistence import durable
ROOT=Path(__file__).resolve().parents[3]
@pytest.fixture(autouse=True)
def artifact(monkeypatch):
    from portfolio_risk.core import product_strategy_native
    directory=Path(os.environ.get('TAIL_NATIVE_DEBUG_DIR',ROOT/'artifacts/nd-decision/debug'))
    monkeypatch.setattr(product_strategy_native,'binary_path',lambda:directory/'portfolio-strategy.exe')

def queued(*,cut=False):
    path=ROOT/'packages/portfolio-risk/tests/fixtures/commercial-short-secondary-cut-refusal.json'
    code='const fs=require("fs");process.stdout.write(JSON.stringify(JSON.parse(fs.readFileSync(process.argv[1],"utf8"))))'
    raw=json.loads(subprocess.run(['node','-e',code,str(path)],capture_output=True,text=True,check=True).stdout)
    if not cut:raw=raw['completed_candidate']
    jid=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0)).id
    db.WORKER='tail-census-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return jid,db.REPO.claim(db.WORKER),raw

def projection(result):
    return next(s for s in result['preparation']['native_preparation']['source']['native_graphs'][0]['native_sources']if s['role']=='physical')['projection']

def test_selected_candidate_publishes_reopens_and_replays_complete_tail_census(durable):
    jid,job,raw=queued();worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
    assert result['preparation']['raw_request']==raw
    from portfolio_risk.analytics.commercial_candidate_reference import verify
    assert verify(raw,result)['persisted_partitions_replayed']
    assert projection(result)['carry']['tail_census']['observed']['scheduled_source_cash']==1
    assert saved['execution']['source_verification']['purchased_journal_replayed']

@pytest.mark.parametrize('target',['tail','cash','phase','journal'])
def test_rehashed_persisted_census_or_monetary_tamper_never_publishes(durable,monkeypatch,target):
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load;changed=[]
    def mutate(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='commercial-candidate-journal-1':return ref
        saved=load(ref);result=restore_sources(saved['model_result'],saved['execution']['table_layouts']);p=projection(result);changed.append(True)
        if target=='tail':p['carry']['tail_census']['maximum_recoveries']+=1
        elif target=='cash':p['carry']['tail_census']['observed']['scheduled_source_cash']+=1
        elif target=='phase':p['selected_accounting_owner']='economic_carry_only-1'
        else:result['tables']['journal'][0]['value']+=1.
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',mutate);worker.execute(job);row=db.REPO.job(jid)
    assert changed and row['status']!='done'and row['result']is None

@pytest.mark.parametrize('target',['source','artifact','cancel'])
def test_selected_tail_final_sql_fences_remain_after_replay(durable,monkeypatch,target):
    jid,job,_=queued();complete=db.REPO.complete
    def commit(bound,**kw):
        if kw.get('guard'):
            if target=='source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            elif target=='artifact':
                table=next(iter(db.CODEC.tables(kw['result']).values()));(db.CODEC.objects.root/table['key']).write_bytes(b'changed-tail-after-replay')
            else:worker.STOP.set()
        return complete(bound,**kw)
    monkeypatch.setattr(db.REPO,'complete',commit)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None

def test_short_cut_actual_worker_retains_joined_refusal_and_no_result(durable):
    jid,job,_=queued(cut=True);worker.execute(job);row=db.REPO.job(jid)
    assert row['status']=='error'and row['result']is None
    assert 'before dual graph generation'in row['detail']
