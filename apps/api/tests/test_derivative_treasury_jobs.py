"""Durable derivative closing-capital jobs: original deadline, independent receipt and tamper fences."""
import json
from pathlib import Path
import pytest
from app import model_routes,persistence as db,worker
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]


def queued():
    raw=json.loads((ROOT/'examples/models/derivative-treasury-1.json').read_text())
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0))
    db.WORKER='derivative-treasury-test-worker';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER)


def test_derivative_treasury_job_publishes_with_independent_capital_replay(durable):
    jid,job=queued();budget=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['publication_deadline_unix']==budget
    receipt=execution['source_verification']
    assert receipt['derivative_closing_capital_recomputed'] is True and receipt['hedge_accounting_applied'] is False
    assert receipt['accounting_standard_approval'] is False and receipt['interim_capital'] is False
    assert execution['summary']['closing_date']=='2024-04-01' and execution['summary']['ftp_selected'] is False
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],execution['table_layouts'])
    assert result['capital']['capital_bridge'][0]['eligible_aoci']==0. and len(result['capital']['capital_metrics'])==13


@pytest.mark.parametrize('target',['row','audit','flag','receipt'])
def test_rehashed_derivative_treasury_persistence_tamper_cannot_publish(durable,monkeypatch,target):
    jid,job=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def changed(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='derivative-treasury-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='row':result['rows'][0]['balance']+=1e-6
        elif target=='audit':result['capital']['capital_bridge'][0]['retained_earnings']+=1e-6
        elif target=='flag':result['hedge_accounting_applied']=True
        else:saved['execution']['source_verification']['derivative_closing_capital_recomputed']=False
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',changed);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None


def test_derivative_treasury_contract_is_listed_with_its_synthetic_example(durable):
    assert 'derivative-treasury-1' in model_routes.contracts()['schemas']
    example=model_routes.example('derivative-treasury-1')
    assert example['synthetic'] is True and example['request']['schema']=='derivative-treasury-1' and example['request']['interim_report'] is False
