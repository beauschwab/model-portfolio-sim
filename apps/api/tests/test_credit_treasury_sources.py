"""Durable verified-credit source adoption by native capital preparation."""
import polars as pl
import pytest
from app import store,persistence as db,treasury_store,worker
from test_persistence import durable
from test_cohorts import execute
from test_credit_jobs import execute as credit_source
from test_treasury_sources import fill_ledger


def policy(source):
    raw=fill_ledger(treasury_store.template(source,'credit_ledger')['specification'])
    for p in raw['policies']:
        p['gl_risk_weights'].update(credit_default_principal=1.,credit_default_accrued_interest=1.,credit_default_book_adjustment=1.)
    return raw


def test_native_credit_capital_source_reconciles_and_survives_restart(durable):
    source=credit_source();assert db.REPO.job(source)['status']=='done'
    raw=policy(source);db.REPO.release(db.WORKER)
    jid,out=execute('treasury',treasury_store.run,source,raw)
    assert out['source_verification']['journal_replayed']
    assert len(out['source_verification']['selected_runs'])==2
    assert out['source_receipt']['result_sha256']==db.result_ref(source)['sha256']
    assert out['capital_bridge'].height==2
    assert 'not an additional capital deduction' in out['credit_source_basis']
    before=store.job_result(jid);db.stop();db.start(durable)
    assert store.job_result(jid)==before


@pytest.mark.parametrize('failure',['journal','cancel','weights'])
def test_credit_source_capital_failure_never_publishes(durable,monkeypatch,failure):
    source=credit_source();raw=policy(source)
    if failure=='weights':
        for p in raw['policies']:p['gl_risk_weights'].pop('credit_default_principal')
    jid=store.submit('treasury',treasury_store.run,source,raw)
    original=treasury_store._frames
    def changed(ref):
        for frame in original(ref):
            if 'transaction_number' in frame.columns:
                if failure=='cancel':db.REPO.cancel(jid)
                elif failure=='journal':frame=frame.with_columns((pl.col('value')+1.).alias('value'))
            yield frame
    monkeypatch.setattr(treasury_store,'_frames',changed)
    worker.execute(db.REPO.claim(db.WORKER))
    row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
