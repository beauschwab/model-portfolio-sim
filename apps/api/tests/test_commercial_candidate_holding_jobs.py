"""Actual durable complete-origin fee holding publication and final fences."""
from pathlib import Path
import json

import pytest

from app import model_routes,persistence as db,worker
from app.model_tables import format_sources,restore_sources
from test_persistence import durable

ROOT=Path(__file__).resolve().parents[3]


def queued():
    raw=json.loads((ROOT/'examples/models/commercial-candidate-holding-journal-1.json').read_text())
    jid=model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'],request=raw,expected_revision=0)).id
    db.WORKER='candidate-holding-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return jid,db.REPO.claim(db.WORKER),raw


def test_candidate_holding_persists_and_reopens_both_complete_owners_and_verified_window(durable):
    jid,job,raw=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['publication_deadline_unix']==deadline
    assert execution['source_verification']['exact_ordered_monetary_prefix_retained']
    assert execution['source_verification']['loan_and_fee_original_anchors_retained']
    result=restore_sources(saved['model_result'],execution['table_layouts'])
    assert result['raw_source']==raw and result['component_output_limit_bytes']==8<<20
    assert result['prior_publication_verified']is False and result['cut_started_mixed_sim_adoption']is False
    assert any(r['gl_account']=='commercial_acquired_fee_asset'and r['balance']==1.5 for r in result['window_tables']['opening'])
    from portfolio_risk.analytics.commercial_candidate_holding_reference import verify
    assert verify(raw,result)['persisted_partitions_replayed']
    # Reopen immutable stored objects, retaining every source and nested variant.
    reopened=db.CODEC.load(db.REPO.job(jid)['result'])
    assert restore_sources(reopened['model_result'],reopened['execution']['table_layouts'])==result


@pytest.mark.parametrize('target',['cut_basis','prior_pnl','future_fee_cash','prefix_order','component_type'])
def test_rehashed_holding_nested_financial_or_policy_tamper_refuses(durable,monkeypatch,target):
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load;changed=[]
    def mutate(value):
        ref=dump(value)
        if type(value)is not dict or value.get('execution',{}).get('schema')!='commercial-candidate-holding-journal-1':return ref
        saved=load(ref);result=restore_sources(saved['model_result'],saved['execution']['table_layouts']);changed.append(True)
        if target=='cut_basis':next(r for r in result['window_tables']['opening']if r['gl_account']=='commercial_acquired_fee_asset')['balance']+=1.
        elif target=='prior_pnl':result['window_tables']['retained_prior_pnl'][0]['signed_prior_pnl']+=1.
        elif target=='future_fee_cash':next(r for r in result['window_tables']['journal']if r['event']=='commercial_nonintegral_fee'and r['gl_account']=='cash')['value']+=1.
        elif target=='prefix_order':
            rows=result['completed_candidate']['tables']['journal'];rows[0],rows[1]=rows[1],rows[0]
        else:result['component_output_limit_bytes']=True
        saved['model_result'],saved['execution']['table_layouts']=format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',mutate);worker.execute(job)
    row=db.REPO.job(jid);assert changed and row['status']!='done'and row['result']is None


@pytest.mark.parametrize('target',['bytes','source','cancel','lease','queued_deadline','attempt'])
def test_holding_sql_lock_rechecks_original_authority_and_verified_artifacts(durable,monkeypatch,target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid,job,_=queued();complete=db.REPO.complete
    def commit(bound,**kwargs):
        if kwargs.get('guard'):
            if target=='bytes':
                descriptor=next(iter(db.CODEC.tables(kwargs['result']).values()));(db.CODEC.objects.root/descriptor['key']).write_bytes(b'changed-after-holding-verification')
            elif target=='source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            elif target=='cancel':worker.STOP.set()
            elif target=='lease':db.REPO.release(db.WORKER)
            else:
                with db.REPO.engine.begin()as connection:
                    if target=='attempt':
                        current=connection.execute(sa.select(jobs.c.attempt).where(db.REPO.scoped(jobs),jobs.c.id==jid)).scalar_one()
                        connection.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(attempt=current+1))
                    else:
                        payload=db.CODEC.load(bound['request']);payload['args'][0]['publication_deadline_unix']+=3600.
                        connection.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(request=db.CODEC.dump(payload)))
        return complete(bound,**kwargs)
    monkeypatch.setattr(db.REPO,'complete',commit)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert row['status']!='done'and row['result']is None
