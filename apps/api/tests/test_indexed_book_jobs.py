"""Authored default DLL durable acceptance; execution pending install/rebuild."""
from contextlib import ExitStack
import pytest
from app import store,worker,persistence as db,model_routes,treasury_store
from test_persistence import durable
from test_model_runs import queued

SCHEMA='market-indexed-books-1'


def test_default_fully_bound_book_persists_reopens_and_rejects_capital(durable,monkeypatch):
    from portfolio_risk.core import graph_native,pricing,curve,vol,native
    from portfolio_risk.analytics import market_indexed_reference
    assert native.library_path().resolve()==native._DEFAULT_LIBRARY_PATH.resolve()
    old=graph_native.NativeMarketIndexedBooks.__init__
    def forbidden(*a,**k):raise AssertionError('Python financial callback during native graph')
    def create(self,*a,**kw):
        with ExitStack() as stack:
            from unittest.mock import patch
            for module,name in [(pricing,'pv_from_A'),(pricing,'solve_oas_from_A'),(curve,'bootstrap_curve'),(vol,'calibrate_abcd'),(market_indexed_reference,'reconstruct')]:
                stack.enter_context(patch.object(module,name,forbidden))
            old(self,*a,**kw)
    monkeypatch.setattr(graph_native.NativeMarketIndexedBooks,'__init__',create)
    jid,job=queued(SCHEMA);worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    result=db.CODEC.load(db.result_ref(jid));execution=result['execution']
    assert execution['artifact_kind']=='product_library' and execution['source_verification']['fully_bound_rows']==2
    assert execution['runtime_control']=='graph-call-control-1'
    before=store.job_result(jid);db.stop();db.start(durable);assert store.job_result(jid)==before
    for kind in ['ledger','credit_ledger','cashflows']:
        with pytest.raises(ValueError):treasury_store._source(jid,kind)


@pytest.mark.parametrize('failure',['amount','source','binary','deadline','lease','cancel','upload','receipt','marker','original_oas'])
def test_persisted_or_publication_failure_never_completes(durable,monkeypatch,failure):
    from portfolio_risk.analytics import balance_stream
    from portfolio_risk.core import native
    from app import indexed_book_jobs
    from app.model_tables import restore_sources,format_sources
    jid,job=queued(SCHEMA);dump=db.CODEC.dump
    def corrupt(value):
        if isinstance(value,dict) and 'model_result' in value:
            if failure in ['amount','original_oas']:
                raw=restore_sources(value['model_result'],value['execution']['table_layouts'])
                if failure=='amount':raw['positions']['loans'][0]['market_value']+=1
                else:raw['market_indexed']['original_request']['bindings'][0]['graph']['source']['bridge']['original']['locked_oas']+=1e-15
                value['model_result'],value['execution']['table_layouts']=format_sources(raw)
            elif failure=='source':monkeypatch.setattr(balance_stream,'source_identity',lambda:'drift')
            elif failure=='binary':monkeypatch.setattr(indexed_book_jobs,'final_identity',lambda *a:(_ for _ in ()).throw(RuntimeError('MODEL_VERSION_CHANGED')))
            elif failure=='deadline':value['execution']['publication_deadline_unix']=0.
            elif failure=='lease':db.REPO.release(db.WORKER)
            elif failure=='cancel':db.REPO.cancel(jid)
            elif failure=='upload':raise OSError('immutable upload failed')
            elif failure=='receipt':value['execution'].pop('source_verification')
            elif failure=='marker':value['execution']['summary']['capital_ftp_eligible']=True
        return dump(value)
    monkeypatch.setattr(db.CODEC,'dump',corrupt);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']!='done' and row['result'] is None


def test_invalid_native_source_cannot_reach_reference(durable,monkeypatch):
    from portfolio_risk.analytics import indexed_book_job_reference
    def forbidden(*a,**k):raise AssertionError('invalid native source reached publication oracle')
    monkeypatch.setattr(indexed_book_job_reference,'verify',forbidden)
    example=model_routes.example(SCHEMA)
    example['request']['bindings'][0]['graph']['source']['bridge']['current_market']['curves'][0]['unknown_money']=0
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=SCHEMA,request=example['request'],expected_revision=0))
    db.WORKER='indexed-invalid';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER));row=db.REPO.job(submitted.id)
    assert row['status']!='done' and row['result'] is None and 'unknown field' in row['detail']


def test_stop_during_final_publication_probe_never_completes(durable,monkeypatch):
    from portfolio_risk.analytics import market_indexed_reference
    jid,job=queued(SCHEMA)
    armed=[False]
    original_equal=market_indexed_reference._source_equal
    def equal(actual,expected,check=lambda:False,name='source'):
        result=original_equal(actual,expected,check,name)
        if name=='final_revision':armed[0]=True
        return result
    monkeypatch.setattr(market_indexed_reference,'_source_equal',equal)
    original_job=db.REPO.job
    def read(ident):
        row=original_job(ident)
        if armed[0]:worker.STOP.set()
        return row
    monkeypatch.setattr(db.REPO,'job',read)
    try:
        worker.execute(job)
        row=original_job(jid)
        assert armed[0] and worker.STOP.is_set()
        assert row['status']!='done' and row['result'] is None
    finally:worker.STOP.clear()
