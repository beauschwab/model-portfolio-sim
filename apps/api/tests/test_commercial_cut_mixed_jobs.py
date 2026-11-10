"""Queued cut refusal for a distinct over-budget request and authority fences."""
from pathlib import Path
import json
import time
import pytest
from app import model_routes,persistence as db,worker
from app.commercial_execution_envelope import retained_command
from test_persistence import durable
ROOT=Path(__file__).resolve().parents[3]
def request():return json.loads((ROOT/'examples/models/commercial-cut-mixed-journal-1.json').read_text())

def queued(raw=None):
    raw=request() if raw is None else raw
    jid=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0)).id
    db.WORKER='cut-mixed-admission-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return jid,db.REPO.claim(db.WORKER),raw

def test_actual_overbudget_cut_refusal_publishes_no_result_or_manifest(durable):
    raw=request()
    # The unchanged original example must publish successfully. Negative
    # coverage now requests larger complete credit paths within their valid
    # jump-count domain, retaining every source and zero-allocation policy.
    for side in ('completed_candidate','revised_candidate'):
        for unit in raw[side]['commercial']['units']:
            position=unit['graph']['positions'][0]
            histories=list(position['original_pricing'])
            for scenario in position['histories']:
                for role in ('physical','pricing','pricing_down','pricing_up'):
                    histories.extend(scenario[role])
            for history in histories:
                history['source']['credit']['max_jumps_per_path']=4096
    jid,job,raw=queued(raw);payload=db.CODEC.load(job['request'])['args'][0]
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']=='error',row['detail']
    assert row['result']is None and 'exceeds128MiB'in row['detail']
    assert payload['request']==raw and type(payload['publication_deadline_unix'])is float
    assert db.CODEC.load(row['request'])['args'][0]==payload

@pytest.mark.parametrize('target',['source','executable','deadline','cancel'])
def test_queued_cut_uses_original_source_artifact_deadline_and_ownership(durable,monkeypatch,target):
    jid,job,raw=queued()
    if target=='source':
        from portfolio_risk.analytics import balance_stream
        monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
    elif target=='executable':
        from portfolio_risk.core import product_strategy_native
        monkeypatch.setattr(product_strategy_native,'binary_identity',lambda:'0'*64)
    elif target=='deadline':
        import sqlalchemy as sa
        from app.repository import jobs
        payload=db.CODEC.load(job['request']);payload['args'][0]['publication_deadline_unix']=time.time()-1.
        ref=db.CODEC.dump(payload)
        with db.REPO.engine.begin()as c:c.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(request=ref))
        job=db.REPO.job(jid)
    else:worker.STOP.set()
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None
    assert 'before dual graph generation'not in row['detail']

@pytest.mark.parametrize('milliseconds',[123,1001,1455])
def test_cut_retained_native_integer_clock_is_never_refloored(milliseconds):
    raw=request();value=retained_command(raw['schema'],raw,10.,milliseconds)
    assert value['op']=='analyze_commercial_cut_mixed_journal'
    assert value['execution_timeout_ms']==milliseconds and value['request']==raw

def test_cut_retained_clock_rejects_boolean_or_increased_budget():
    raw=request()
    for invalid in(True,0,10001):
        with pytest.raises(ValueError):retained_command(raw['schema'],raw,10.,invalid)
