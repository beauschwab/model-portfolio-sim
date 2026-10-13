"""Actual durable worker, uploaded source replay and SQL publication for source2."""
import json,time
import pytest
import sqlalchemy as sa
from app import main,worker,persistence as db,reporting,maintenance
from app.artifacts import ObjectReference
from app.schemas import ModeledBalanceStressRequest
from app.repository import jobs
from test_persistence import durable

SCHEMA='deposit-performing-portfolio-ledger-2'
TRUE=('physical_deposit_portfolio_sources_recomputed','original_yield_continuations_recomputed',
      'full_unweighted_gross_origins_verified','ordered_pooled_same_sim_journal_replayed','persisted_partitions_replayed')

def queued():
    raw=main.modeled_balance_stress_example(SCHEMA)['specification']
    submitted=main.modeled_balance_stress(ModeledBalanceStressRequest(expected_revision=0,specification=raw))
    db.WORKER='portfolio-default-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted['id'],db.REPO.claim(db.WORKER),raw

def test_actual_worker_multiple_sources_reopen_complete_verified_pooled_journal(durable,monkeypatch):
    from portfolio_risk.analytics import model_ledger
    def forbidden(*a,**k):raise AssertionError('unowned Python production finance')
    monkeypatch.setattr(model_ledger,'validate_spec',forbidden);monkeypatch.setattr(model_ledger,'_finalize',forbidden)
    jid,job,raw=queued();accepted=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    result=db.CODEC.load(db.result_ref(jid));execution=result['execution']
    assert execution['publication_deadline_unix']==accepted
    for key in TRUE:assert execution['validation'][key] is True
    for key in ('accounting_standard_approval','empirical_validation','installed_adoption','saved_book_adoption','capital_ftp_eligible'):assert execution['validation'][key] is False
    header=json.loads(db.CODEC.objects.get(execution['modeled_source']))
    assert model_ledger.exact_json_identity(header['source_receipt']['raw_request'],raw)
    assert len(header['source_receipt']['owners'])==2
    assert {p['run_id'] for o in header['source_receipt']['owners'] for p in o['paths']}=={r['run_id'] for r in raw['runs']}
    assert json.loads(db.CODEC.objects.get(execution['producer_completion']))=={'complete':True,'protocol':3}
    for name in ('journal','deposit_revision','trial_balance','dated_rights'):
        assert reporting.table(jid,'/'+name,offset=0,limit=2)['rows']
    reachable=maintenance.reachable(db.REPO,db.CODEC.objects)
    assert execution['modeled_source']['key'] in reachable and execution['producer_completion']['key'] in reachable
    assert execution['validation'].get('journal_replayed') is None

@pytest.mark.parametrize('target',['raw_signzero','raw_bool','named_source','completion_bool','protocol_float',
    'receipt_true','receipt_false','extended_deadline','dropped_table','cancel','lease','queued_request','attempt','late_stop','late_partition','late_source','late_binary'])
def test_actual_worker_refuses_uploaded_or_commit_authority_tamper(durable,monkeypatch,target):
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load;complete=db.REPO.complete
    def tamper(value):
        ref=dump(value)
        if not isinstance(value,dict) or value.get('execution',{}).get('model_version')!=SCHEMA:return ref
        changed=load(ref);e=changed['execution']
        if target in ('raw_signzero','raw_bool','named_source','completion_bool','protocol_float'):
            key='input' if target.startswith('raw_') else 'modeled_source' if target=='named_source' else 'producer_completion'
            data=json.loads(db.CODEC.objects.get(e[key]))
            if target=='raw_signzero':data['request']['specification']['positions'][0]['rate']=-0.
            elif target=='raw_bool':data['request']['positions'][0]['paths'][0]['source']['continuation']['anchor']['original_principal']=True
            elif target=='named_source':data['source_receipt']['owners'][1]['paths'][1]['native']['native']['raw_request']['continuation']['anchor']['original_annual_effective_rate']+=1e-12
            elif target=='completion_bool':data['complete']=1
            else:data['protocol']=3.
            e[key]=ObjectReference(db.CODEC.objects.put(json.dumps(data,allow_nan=False).encode(),'bin'))
        elif target=='receipt_true':e['validation'][TRUE[0]]=1
        elif target=='receipt_false':e['validation']['accounting_standard_approval']=0
        elif target=='extended_deadline':e['publication_deadline_unix']+=3600.
        elif target=='dropped_table':changed.pop('deposit_revision')
        elif target=='cancel':db.REPO.cancel(jid)
        elif target=='lease':db.REPO.release(db.WORKER)
        return dump(changed)
    def commit(bound,**kw):
        if kw.get('guard'):
            if target=='late_stop':worker.STOP.set()
            elif target=='late_source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            elif target=='late_binary':
                from portfolio_risk.analytics import model_ledger
                bad=tmp_binary=db.CONFIG.artifact_url+'/bad-executable'
                from pathlib import Path
                Path(bad).write_bytes(b'changed-selected-artifact')
                monkeypatch.setattr(model_ledger,'binary_path',lambda:Path(tmp_binary))
            elif target=='late_partition':
                result=load(kw['result']);part=result['journal']['parts'][0]['ref']
                (db.CODEC.objects.root/part['key']).write_bytes(b'corrupted-after-replay')
            elif target in ('queued_request','attempt'):
                with db.REPO.engine.begin() as conn:
                    if target=='attempt':changes={'attempt':bound['attempt']+1}
                    else:
                        payload=load(bound['request']);payload['args'][0]['publication_deadline_unix']+=3600.
                        changes={'request':dump(payload)}
                    conn.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(**changes))
        return complete(bound,**kw)
    monkeypatch.setattr(db.CODEC,'dump',tamper);monkeypatch.setattr(db.REPO,'complete',commit)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert row['status']!='done' and row['result'] is None,row['detail']
    assert 'KeyError' not in str(row['detail'])

def test_reclaimed_attempt_retains_original_accepted_deadline(durable):
    jid,first,_=queued();payload=db.CODEC.load(first['request']);deadline=payload['args'][0]['publication_deadline_unix']
    db.REPO.release(db.WORKER);db.WORKER='replacement-worker';assert db.REPO.acquire(db.WORKER)
    second=db.REPO.claim(db.WORKER);assert second['id']==jid and second['attempt']==2
    assert db.CODEC.load(second['request'])['args'][0]['publication_deadline_unix']==deadline
    worker.execute(second);row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    assert db.CODEC.load(db.result_ref(jid))['execution']['publication_deadline_unix']==deadline
