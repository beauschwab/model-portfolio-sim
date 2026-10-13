"""Durable capital adoption of verified physical path GL; no realized-path FTP."""
from copy import deepcopy
import polars as pl
import pytest
from app import store,persistence as db,treasury_store,worker
from test_persistence import durable
from test_cohorts import execute
import test_credit_path_jobs as paths
from test_credit_treasury_sources import policy


@pytest.mark.parametrize('signed',[False,True])
def test_path_source_capital_retained_verification_signed_closing_and_restart(durable,monkeypatch,signed):
    if signed:
        raw=deepcopy(paths.specification())
        raw['specification']['positions'][0]['opening_accrued']=-2.
        raw['specification']['accounts'][0]['equity']=64.
        for run in raw['runs']:
            q=run['batches'][0]['request'];q['bindings'][0]['chargeoffs']=[]
            q['path']['paths'][0]['accrued_interest']=-2.
            q['path']['intervals'][0]['accrual_rates'][0]=-.365
            for tail in q['path']['intervals'][0]['complete_recovery_tails']:tail[0]['lag_years']=20.
        monkeypatch.setattr(paths,'specification',lambda:raw)
    source=paths.execute();assert db.REPO.job(source)['status']=='done',db.REPO.job(source)['detail']
    raw=policy(source);db.REPO.release(db.WORKER)
    jid,out=execute('treasury',treasury_store.run,source,raw)
    assert out['source_verification']['credit_protocol']==5
    assert out['source_receipt']['kind']=='credit_path_balance_stress'
    assert out['source_receipt']['result_sha256']==db.result_ref(source)['sha256']
    assert out['capital_bridge'].height==2
    for row in out['capital_bridge'].iter_rows(named=True):
        assert row['assets']==pytest.approx(66. if signed else 66.05,abs=1e-9)
        assert row['liabilities']==pytest.approx(2.05 if signed else 0.,abs=1e-9)
        assert row['book_equity']==pytest.approx(63.95 if signed else 66.05,abs=1e-9)
    assert 'not an additional capital deduction' in out['credit_source_basis']
    before=store.job_result(jid);db.stop();db.start(durable)
    assert store.job_result(jid)==before


def test_realized_path_source_is_not_ftp_source(durable):
    source=paths.execute()
    with pytest.raises(ValueError,match='completed compatible'):
        treasury_store.template(source,'cashflows')


@pytest.mark.parametrize('failure',['journal','cancel','verification','marker'])
def test_path_capital_failed_source_checks_never_publish(durable,monkeypatch,failure):
    source=paths.execute();raw=policy(source)
    if failure=='marker':raw['credit_protocol']=5
    jid=store.submit('treasury',treasury_store.run,source,raw)
    if failure=='verification':
        original=treasury_store._metadata
        def unverified(ref,keys):
            result=deepcopy(original(ref,keys))
            if 'execution' in result:result['execution']['validation']['physical_paths_recomputed']=False
            return result
        monkeypatch.setattr(treasury_store,'_metadata',unverified)
    else:
        original=treasury_store._frames
        def changed(ref):
            for frame in original(ref):
                if 'transaction_key' in frame.columns:
                    if failure=='cancel':db.REPO.cancel(jid)
                    elif failure=='journal':frame=frame.with_columns((pl.col('value')+1.).alias('value'))
                yield frame
        monkeypatch.setattr(treasury_store,'_frames',changed)
    worker.execute(db.REPO.claim(db.WORKER));row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
