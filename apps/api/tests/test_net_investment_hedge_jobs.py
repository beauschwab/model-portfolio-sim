"""Durable net-investment-hedge-1 jobs: native accounting, independent receipt and tamper fences."""
import json
from pathlib import Path
import pytest
from app import model_routes,persistence as db,worker
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]


def queued():
    raw=json.loads((ROOT/'examples/models/net-investment-hedge-1.json').read_text())
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0))
    db.WORKER='net-investment-test-worker';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER)


def test_net_investment_job_publishes_with_independent_replay(durable):
    jid,job=queued();budget=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['publication_deadline_unix']==budget
    receipt=execution['source_verification']
    assert receipt['verified'] is True and receipt['hedge_cta_recomputed'] is True and receipt['accounting_standard_approval'] is False
    from app.model_tables import restore_sources
    result=restore_sources(saved['model_result'],execution['table_layouts'])
    assert {r['relationship_id'] for r in result['relationships']}=={'nih-fwd','nih-debt'}
    assert abs(sum(b['debit_balance'] for b in result['closing']))<1e-9


@pytest.mark.parametrize('target',['journal','period','flag','receipt'])
def test_rehashed_net_investment_tamper_cannot_publish(durable,monkeypatch,target):
    jid,job=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def changed(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='net-investment-hedge-1':return ref
        saved=load(ref)
        from app.model_tables import restore_sources,format_sources
        result=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target=='journal':result['journal'][3]['debit']+=1e-6
        elif target=='period':result['periods'][0]['cta']+=1e-6
        elif target=='flag':result['debt_interest_included']=True
        else:saved['execution']['source_verification']['hedge_cta_recomputed']=False
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',changed);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None


def test_net_investment_contract_is_listed_with_its_example(durable):
    assert 'net-investment-hedge-1' in model_routes.contracts()['schemas']
    example=model_routes.example('net-investment-hedge-1')
    assert example['synthetic'] is True and example['request']['schema']=='net-investment-hedge-1'
