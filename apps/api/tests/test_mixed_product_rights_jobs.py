"""Durable Card2 atomic rights share the existing journal and persist their gate."""
import json
import os
from pathlib import Path
import pytest
from app import model_routes, store, worker, persistence as db, treasury_store
from test_persistence import durable


@pytest.fixture(autouse=True)
def explicit_test_runtime(monkeypatch):
    selected=os.environ.get('PRODUCT_RIGHTS_TEST_BINARY')
    if selected:
        from portfolio_risk.core import product_strategy_native
        path=Path(selected).resolve(strict=True)
        monkeypatch.setattr(product_strategy_native,'binary_path',lambda:path)


def queued():
    raw=json.loads((Path(__file__).resolve().parents[3]/'examples/models/product-mixed-card-credit-2.json').read_text())
    submitted=model_routes.submit(model_routes.ModelRunRequest(
        schema_name='product-mixed-product-credit-1',request=raw,expected_revision=0))
    db.WORKER='rights-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER)


def test_card2_mixed_atomic_rights_publish_and_reopen(durable,monkeypatch):
    from portfolio_risk.core import quant_native
    def forbidden(*args,**kwargs):raise AssertionError('in-process financial callback')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    jid,job=queued();worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    out=db.CODEC.load(db.result_ref(jid));receipt=out['execution']['source_verification']
    assert receipt['verified'] and receipt['card_v2_rights_owned']
    assert receipt['capital_ftp_eligible'] is False
    assert out['execution']['summary']['capital_ftp_eligible'] is False
    assert receipt['closing_capital_eligible'] is True
    assert out['execution']['summary']['closing_capital_eligible'] is True
    assert any('product_rights_daily' in name for name in db.CODEC.tables(db.result_ref(jid)))
    before=store.job_result(jid);db.stop();db.start(durable)
    assert store.job_result(jid)==before


@pytest.mark.parametrize('failure',['basis','gross','receipt','capital','actions','receipt_type','summary_type'])
def test_persisted_card2_atomic_rights_corruption_blocks_publication(durable,monkeypatch,failure):
    import polars as pl
    jid,job=queued();original=db.CODEC.dump
    def corrupt(result):
        if isinstance(result,dict) and 'model_result' in result:
            if failure=='receipt':result['execution']['source_verification']['card_v2_rights_owned']=False
            elif failure=='capital':result['execution']['summary']['capital_ftp_eligible']=True
            elif failure=='receipt_type':result['execution']['source_verification']['capital_ftp_eligible']=0
            elif failure=='summary_type':result['execution']['summary']['capital_ftp_eligible']=0
            else:
                table='product_rights_daily' if failure in {'basis','gross'} else 'product_rights_lineage'
                frame=result['model_result'][table];rows=frame.to_dicts()
                key='unweighted_rights' if failure!='actions' else 'native_action'
                value=json.loads(rows[0][key])
                if failure=='basis':value['live_interest_basis']+=1
                elif failure=='gross':value['core']['live'][0]['accrued_interest']+=1
                else:value['atomic_action']+=1
                rows[0][key]=json.dumps(value,separators=(',',':'))
                result['model_result'][table]=pl.DataFrame(rows,schema=frame.schema)
        return original(result)
    monkeypatch.setattr(db.CODEC,'dump',corrupt)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('failure',['deadline','source','binary','stop'])
def test_card2_mixed_final_execution_fences(durable,monkeypatch,failure):
    from portfolio_risk.core import product_strategy_native
    from portfolio_risk.analytics import balance_stream
    jid,job=queued();original=db.CODEC.dump
    def change(result):
        if isinstance(result,dict) and 'model_result' in result:
            if failure=='deadline':result['execution']['publication_deadline_unix']=0.
            elif failure=='source':monkeypatch.setattr(balance_stream,'source_identity',lambda:'changed')
            elif failure=='binary':monkeypatch.setattr(product_strategy_native,'binary_identity',lambda:'changed')
            else:worker.STOP.set()
        return original(result)
    monkeypatch.setattr(db.CODEC,'dump',change)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('kind',['credit_ledger','cashflows'])
def test_card2_mixed_unreviewed_treasury_mapping_is_rejected(durable,kind):
    jid,job=queued();worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    with pytest.raises(ValueError):treasury_store.template(jid,kind)


def capital_policy(jid):
    from test_mixed_treasury_sources import policy
    raw=policy(jid)
    for p in raw['policies']:
        p['gl_risk_weights'].update(live_interest_basis_adjustment=1.,default_interest_basis_adjustment=1.)
    return raw


def test_card2_verified_exact_tail_closing_capital_publishes_and_reopens(durable):
    from test_cohorts import execute
    jid,job=queued();worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    raw=capital_policy(jid);db.REPO.release(db.WORKER)
    report,out=execute('treasury',treasury_store.run,jid,raw)
    assert out['source_verification']['journal_replayed']
    assert out['source_receipt']['basis']=='complete_tail_expected_closing-1'
    assert out['capital_bridge']['assets'].to_list()==pytest.approx([1102.2506993647357]*2,abs=1e-9)
    before=store.job_result(report);db.stop();db.start(durable)
    assert store.job_result(report)==before


@pytest.mark.parametrize('failure',['date','posting','cancel'])
def test_card2_closing_capital_keeps_date_journal_and_cancellation_gates(durable,monkeypatch,failure):
    import polars as pl
    jid,job=queued();worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    raw=capital_policy(jid)
    if failure=='date':raw['treasury']['as_of']='2000-01-01'
    report=store.submit('treasury',treasury_store.run,jid,raw);original=treasury_store._frames
    def changed(ref):
        for frame in original(ref):
            if 'transaction_number' in frame.columns:
                if failure=='cancel':db.REPO.cancel(report)
                elif failure=='posting':frame=frame.with_columns((pl.col('value')+1.).alias('value'))
            yield frame
    monkeypatch.setattr(treasury_store,'_frames',changed)
    worker.execute(db.REPO.claim(db.WORKER));row=db.REPO.job(report)
    assert row['status']!='done' and row['result'] is None
