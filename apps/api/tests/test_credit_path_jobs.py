"""Credit native execution through durable worker, source evidence and fences."""
import json
from pathlib import Path
import pytest

from app import main,store,worker,persistence as db,maintenance,reporting
from app.schemas import CreditPathBalanceStressRequest
from test_persistence import durable


def specification():
    return json.loads((Path(__file__).resolve().parents[3]/'examples/models/credit-path-ledger-1.json').read_text())


def queued():
    raw=specification();submitted=main.credit_path_balance_stress(CreditPathBalanceStressRequest(expected_revision=0,specification=raw))
    db.WORKER='credit-endpoint-test';worker.STOP.clear()
    assert db.REPO.acquire(db.WORKER)
    return submitted['id'],db.REPO.claim(db.WORKER)


def execute():
    jid,job=queued();worker.execute(job);return jid


def test_credit_job_publishes_verified_sources_and_survives_restart(durable,monkeypatch):
    from portfolio_risk.analytics import balance_stress,balance_stream
    def forbidden(*args,**kwargs):raise AssertionError('Python financial execution')
    monkeypatch.setattr(balance_stress,'validate',forbidden)
    monkeypatch.setattr(balance_stress,'_simulate',forbidden)
    monkeypatch.setattr(balance_stream,'_python',forbidden)
    assert main.credit_path_balance_stress_example()['specification']==specification()
    jid=execute();assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    result=db.CODEC.load(db.result_ref(jid));execution=result['execution']
    assert execution['validation']['credit_path_source_reconciled']
    assert execution['validation']['journal_replayed']
    assert execution['version']=='credit-path-balance-partitions-1'
    assert execution['input']['key'] in maintenance.reachable(db.REPO,db.CODEC.objects)
    assert json.loads(db.CODEC.objects.get(execution['input']))==specification()
    assert main.run_manifest(jid)['execution']['validation']['credit_path_source_reconciled']
    assert len(reporting.table(jid,'/credit_path_daily',offset=0,limit=2)['rows'])==2
    before=store.job_result(jid);db.stop();db.start(durable)
    assert store.job_result(jid)==before


@pytest.mark.parametrize('failure',['upload','cancel','lease'])
def test_credit_failed_publication_never_marks_done(durable,monkeypatch,failure):
    jid,job=queued()
    if failure=='upload':
        put=db.CODEC.objects.put
        def failed(data,suffix):
            if suffix=='parquet':raise OSError('injected credit upload failure')
            return put(data,suffix)
        monkeypatch.setattr(db.CODEC.objects,'put',failed)
    else:
        original=db.CODEC.import_credit_path_partitions
        def interrupted(path,cancelled):
            if failure=='cancel':db.REPO.cancel(db.ACTIVE_JOB.get()['id'])
            else:db.REPO.release(db.WORKER)
            return original(path,cancelled)
        monkeypatch.setattr(db.CODEC,'import_credit_path_partitions',interrupted)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
