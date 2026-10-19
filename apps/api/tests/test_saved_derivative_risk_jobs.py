"""Durable saved-derivative-risk-1 jobs: native graph risk, independent receipt and tamper fences."""
import json
from pathlib import Path
import pytest
from app import model_routes,persistence as db,worker
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]


def queued():
    raw=json.loads((ROOT/'examples/models/saved-derivative-risk-1.json').read_text())
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0))
    db.WORKER='saved-derivative-risk-test-worker';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER)


def test_saved_derivative_risk_job_publishes_with_independent_paired_risk(durable):
    jid,job=queued();budget=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['publication_deadline_unix']==budget
    receipt=execution['source_verification']
    assert receipt['verified'] is True and receipt['paired_risk_recomputed'] is True
    assert receipt['accounting_designation_output'] is False and receipt['capital_ftp_eligible'] is False
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],execution['table_layouts'])
    assert [p['position_id'] for p in result['positions']]==['swap-fvh','swap-cfh']
    fvh=result['relationships'][0]
    assert fvh['relationship_id']=='fvh-1' and fvh['hedged_item']['dv01']>0>fvh['instrument']['dv01']


@pytest.mark.parametrize('target',['position','item','flag','receipt'])
def test_rehashed_saved_derivative_risk_tamper_cannot_publish(durable,monkeypatch,target):
    jid,job=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def changed(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='saved-derivative-risk-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='position':result['positions'][0]['dv01']+=1e-6
        elif target=='item':result['relationships'][0]['hedged_item']['key_rate_dv01'][0]+=1e-6
        elif target=='flag':result['observed_fixings_bumped']=True
        else:saved['execution']['source_verification']['paired_risk_recomputed']=False
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',changed);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None


def test_saved_derivative_risk_contract_is_listed_with_its_example(durable):
    assert 'saved-derivative-risk-1' in model_routes.contracts()['schemas']
    example=model_routes.example('saved-derivative-risk-1')
    assert example['synthetic'] is True and example['request']['schema']=='saved-derivative-risk-1'
