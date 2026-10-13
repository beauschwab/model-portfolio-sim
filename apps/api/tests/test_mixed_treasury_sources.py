"""Verified expected mixed journals feed native closing capital with date identity."""
from datetime import date
import pytest
from app import store,persistence as db,treasury_store,worker
from test_persistence import durable
from test_cohorts import execute
from test_model_runs import queued
from test_treasury_sources import fill_ledger


SCHEMAS=('product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1')


def source(schema='product-mixed-dated-1'):
    jid,job=queued(schema);worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    return jid


def policy(jid):
    template=treasury_store.template(jid,'ledger')['specification']
    report_date=template['treasury']['as_of']
    raw=fill_ledger(template);raw['treasury']['as_of']=report_date
    for p in raw['policies']:
        p['gl_risk_weights'].update(dated_interest_receivable=1.,dated_prepaid_expense=1.,
            credit_default_principal=1.,credit_default_book_adjustment=1.,credit_default_interest_receivable=1.)
    return raw


@pytest.mark.parametrize('schema',SCHEMAS)
def test_mixed_expected_closing_capital_replays_journal_and_retains_date(durable,schema):
    jid=source(schema);raw=policy(jid)
    db.REPO.release(db.WORKER)
    report,out=execute('treasury',treasury_store.run,jid,raw)
    assert out['source_verification']['journal_replayed']
    assert out['source_verification']['ledger_journal_protocol']==3
    assert out['source_receipt']['model_schema']==schema
    assert out['source_receipt']['basis']=='complete_tail_expected_closing-1'
    assert date.fromordinal(out['source_receipt']['closing_ordinal']).isoformat()==raw['treasury']['as_of']
    assert out['capital_bridge'].height==2
    before=store.job_result(report);db.stop();db.start(durable)
    assert store.job_result(report)==before


@pytest.mark.parametrize('failure',['date','journal','cancel'])
@pytest.mark.parametrize('schema',SCHEMAS)
def test_mixed_capital_rejects_changed_date_posting_or_cancel(durable,monkeypatch,failure,schema):
    import polars as pl
    jid=source(schema);raw=policy(jid)
    if failure=='date':raw['treasury']['as_of']='2000-01-01'
    report=store.submit('treasury',treasury_store.run,jid,raw)
    original=treasury_store._frames
    def changed(ref):
        for frame in original(ref):
            if 'transaction_number' in frame.columns:
                if failure=='cancel':db.REPO.cancel(report)
                elif failure=='journal':frame=frame.with_columns((pl.col('value')+1.).alias('value'))
            yield frame
    monkeypatch.setattr(treasury_store,'_frames',changed)
    worker.execute(db.REPO.claim(db.WORKER));row=db.REPO.job(report)
    assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('failure',['receipt','protocol_type'])
@pytest.mark.parametrize('schema',SCHEMAS)
def test_mixed_source_rejects_credit_ftp_and_missing_independent_receipt(durable,monkeypatch,failure,schema):
    jid=source(schema)
    for kind in ('credit_ledger','cashflows'):
        with pytest.raises(ValueError):treasury_store.template(jid,kind)
    original=treasury_store._metadata
    def missing(ref,keys):
        out=original(ref,keys)
        if 'execution' in out:
            if failure=='receipt':out['execution']['source_verification'].pop('ordered_mixed_journal_recomputed')
            else:out['execution']['summary']['protocol']=3.
        return out
    monkeypatch.setattr(treasury_store,'_metadata',missing)
    with pytest.raises(ValueError,match='independent persisted source gate'):
        treasury_store.template(jid,'ledger')
