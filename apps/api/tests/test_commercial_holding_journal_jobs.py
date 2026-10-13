"""Holding journal publication replays raw sources and persisted window tables."""
import json
from pathlib import Path
import pytest
from app import model_routes,persistence as db,worker
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]
def queued():
    raw=json.loads((ROOT/'examples/models/commercial-holding-journal-1.json').read_text())
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0))
    db.WORKER='holding-journal-worker';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER),raw

def test_commercial_holding_journal_durably_replays_cut_and_complete_source(durable):
    jid,job,raw=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['publication_deadline_unix']==deadline and execution['native_timeout_ms']>0
    assert execution['source_verification']['window_trial_balance_replayed']is True
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],execution['table_layouts'])
    assert result['raw_source']==raw and result['prior_publication_verified']is False
    assert all(r['day']>=15 for r in result['window_tables']['journal'])

@pytest.mark.parametrize('target',['opening','prefix','closing','prior_pnl','future_order','evidence'])
def test_commercial_holding_journal_rehashed_persisted_tamper_rejects(durable,monkeypatch,target):
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def changed(value):
        ref=dump(value)
        if not isinstance(value,dict)or value.get('execution',{}).get('schema')!='commercial-holding-journal-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts']);window=result['window_tables']
        if target=='opening':window['opening'][0]['balance']+=1.
        elif target=='prefix':result['prefix_proofs'][0]['openings'][0]['book']['principal']+=1.
        elif target=='closing':window['trial_balance'][0]['balance']+=1.
        elif target=='prior_pnl':window['retained_prior_pnl'][0]['signed_prior_pnl']+=1.
        elif target=='future_order':window['journal'].reverse()
        else:result['prior_publication_verified']=True
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result);return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',changed);worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result']is None

def test_commercial_holding_journal_original_queue_deadline_survives(durable,monkeypatch):
    from types import SimpleNamespace
    import time
    from app import model_jobs
    jid,job,_=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    monkeypatch.setattr(model_jobs,'time',SimpleNamespace(time=lambda:deadline+1.,monotonic=time.monotonic));worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done' and row['result']is None and 'deadline'in row['detail'].lower()
