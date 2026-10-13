"""Original generated quote fit publication, nested receipts and final authority."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from app import model_routes, persistence as db, worker
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]
def queued(options=False, timeout=1800.):
    name='commercial-generated-calibrated-exercise-graph-1' if options else 'commercial-generated-calibrated-graph-1'
    raw=json.loads((ROOT/'examples/models'/f'{name}.json').read_text())
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0,timeout=timeout))
    db.WORKER='commercial-oas-worker';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER),raw

@pytest.mark.parametrize('options',[False,True])
def test_commercial_original_generated_calibration_durably_replays_q_and_p(durable,options):
    jid,job,raw=queued(options);deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['source_verification']['verified'] is True
    assert execution['publication_deadline_unix']==deadline
    assert type(execution['native_timeout_ms']) is int and 0<execution['native_timeout_ms']<=1_800_000
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],execution['table_layouts'])
    assert result['generated_oas_calibration']['physical_outcomes_used_for_calibration'] is False
    assert result['original_oas'][0]['original_signed_cash_quote']==raw['positions'][0]['original_signed_cash_quote']
    assert result['empirical_validation'] is False

@pytest.mark.parametrize('target',['spread','trial','source','original_cost'])
def test_commercial_original_calibration_rehashed_persisted_tamper_rejects(durable,monkeypatch,target):
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def changed(value):
        ref=dump(value)
        if not isinstance(value,dict) or value.get('execution',{}).get('schema')!='commercial-product-graph-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='spread':result['generated_oas_calibration']['selected_base_oas']+=.001
        elif target=='trial':result['generated_oas_calibration']['trials'][2]['evaluation_value']+=1.
        elif target=='source':result['raw_source']['positions'][0]['generated_policy']['oas_calibration']['max_trials']+=1
        else:result['original_acquisition_proofs'][0]['acquisition']['cost']+=1.
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',changed);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done' and row['result'] is None

def test_commercial_original_calibration_keeps_queued_deadline_and_publishes_no_partial(durable,monkeypatch):
    from types import SimpleNamespace
    from app import model_jobs
    import time
    jid,job,_=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    monkeypatch.setattr(model_jobs,'time',SimpleNamespace(time=lambda:deadline+1.,monotonic=time.monotonic))
    worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done' and row['result'] is None
    assert 'deadline' in row['detail'].lower()
    assert db.CODEC.load(row['request'])['args'][0]['publication_deadline_unix']==deadline
