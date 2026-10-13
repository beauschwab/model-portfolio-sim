"""Pending actual durable worker/default-native closing-capital acceptance."""
from copy import deepcopy
from pathlib import Path
import json
import pytest
from app import worker,persistence as db,store,treasury_routes,mortgage_capital
from test_persistence import durable
from test_mortgage_mixed_jobs import queued as queued_source
ROOT=Path(__file__).resolve().parents[3]

def policies():
    r=json.loads((ROOT/'examples/models/mortgage-closing-capital-1.json').read_text())
    return {k:r[k] for k in ('treasury','policies','amount_multiplier','exposure_policy')}|{'kind':'ledger'},r['closing_review']

def complete_source():
    jid,job,_=queued_source();worker.execute(job)
    row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    return jid,deepcopy(row)

def queue_capital(source):
    spec,review=policies()
    response=treasury_routes.mortgage_closing_capital(treasury_routes.MortgageClosingRequest(
        expected_revision=0,source_job=source,specification=spec,closing_review=review,timeout=60.))
    jid=response.id;job=db.REPO.claim(db.WORKER);assert job['id']==jid
    return jid,job

def test_real_completed_source_new_capital_worker_and_reopen(durable):
    source,before=complete_source();jid,job=queue_capital(source);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    result=db.CODEC.load(db.result_ref(jid));receipt=result['execution']['validation']
    assert receipt['closing_capital_eligible'] is True and receipt['capital_ftp_eligible'] is False
    assert receipt['accounting_standard_approval'] is False and receipt['regulatory_credit_rwa_approval'] is False
    assert db.REPO.job(source)==before
    source_result=db.CODEC.load(before['result']);assert source_result['execution']['validation']['closing_capital_eligible'] is False
    assert result['execution']['source_binding']['id']==source
    saved=store.job_result(jid);db.stop();db.start(durable);assert store.job_result(jid)==saved

@pytest.mark.parametrize('target',['digest','receipt_alias','source_partition','cancel','lease','deadline'])
def test_actual_upload_replay_and_final_commit_fences(durable,monkeypatch,target):
    source,_=complete_source();jid,job=queue_capital(source);put=db.CODEC.objects.put;tripped=[]
    def changed(data,suffix):
        value=json.loads(data) if suffix=='json' else None
        decoded=db.CODEC.decode(value['value']) if type(value) is dict and 'value' in value else None
        if type(decoded) is dict and decoded.get('schema')==mortgage_capital.SCHEMA and not tripped:
            tripped.append(True)
            if target=='digest':decoded['native_result']['mortgage_closing_source']['raw_mixed_sha256']='a'*64
            elif target=='receipt_alias':decoded['execution']['validation']['closing_capital_eligible']=1
            elif target=='cancel':db.REPO.cancel(jid)
            elif target=='lease':db.REPO.release(db.WORKER)
            elif target=='deadline':decoded['execution']['publication_deadline_unix']=0.
            else:
                original=db.CODEC.load(db.REPO.job(source)['result']);part=original['journal']['parts'][0]['ref']
                (db.CODEC.objects.root/part['key']).write_bytes(b'late source partition corruption')
            data=json.dumps({'schema_version':1,'value':db.CODEC.encode(decoded)},allow_nan=False,separators=(',',':')).encode()
        return put(data,suffix)
    monkeypatch.setattr(db.CODEC.objects,'put',changed)
    worker.execute(job);row=db.REPO.job(jid)
    assert tripped and row['status']!='done' and row['result'] is None,row['detail']

def test_incompatible_source_rejected_before_queue(durable):
    spec,review=policies();jid=db.REPO.enqueue('treasury',0,db.CODEC.dump({'operation':'store.run_treasury','args':[]}),{})
    job=db.REPO.claim(db.WORKER) if db.WORKER else None
    # Compatibility is deliberately metadata-only before a new durable attempt.
    with pytest.raises((ValueError,KeyError)):
        mortgage_capital.queue_input(jid,spec,review,timeout=60.)
