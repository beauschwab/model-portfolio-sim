"""Authored default native source -> worker -> Parquet/SQLite; pending rebuild."""
from pathlib import Path
from copy import deepcopy
from contextlib import ExitStack
import json,time,math
import pytest
from app import store,worker,persistence as db,model_routes,reporting,treasury_store,cd_sequence_jobs
from app.model_tables import restore_sources,format_sources
from portfolio_risk.core import model_native,cd_sequence_native
from test_persistence import durable
ROOT=Path(__file__).resolve().parents[3]
CASES=json.loads((ROOT/'packages/portfolio-risk/tests/fixtures/cd-sequence-graph2/cases.json').read_bytes())

@pytest.fixture(autouse=True)
def actual_default_binary():
    path=model_native.binary_path().resolve()
    assert path.parent.name=='release' and path.name in ('portfolio-lifecycle','portfolio-lifecycle.exe')

def queued(case=26,timeout=60.):
    c=CASES[case]
    submission=model_routes.submit(model_routes.ModelRunRequest(schema_name=c['schema'],request=c['raw'],expected_revision=0,timeout=timeout))
    db.WORKER='contained-cd';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submission.id,db.REPO.claim(db.WORKER)

def assert_failed(jid):
    row=db.REPO.job(jid)
    assert row['status']!='done' and row['result']is None,row
    return row

@pytest.mark.parametrize('case',[12,26,27,29,30,31,32])
def test_actual_cd_durable_full_source_parquet_reopen_and_false_eligibility(durable,monkeypatch,case):
    from portfolio_risk.core import pricing,curve,vol,quant_native
    from portfolio_risk.analytics import cd_sequence_graph_reference
    original=cd_sequence_native.run
    def forbidden(*a,**k):raise AssertionError('Python financial callback during native calculation')
    def native(*a,**k):
        from unittest.mock import patch
        with ExitStack()as stack:
            for module,name in [(pricing,'pv_from_A'),(pricing,'solve_oas_from_A'),(curve,'bootstrap_curve'),(vol,'calibrate_abcd'),(quant_native,'term_call'),(cd_sequence_graph_reference,'verify_request')]:
                stack.enter_context(patch.object(module,name,forbidden))
            return original(*a,**k)
    monkeypatch.setattr(cd_sequence_native,'run',native)
    jid,job=queued(case);worker.execute(job)
    row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    saved=db.CODEC.load(db.result_ref(jid));execution=saved['execution']
    assert execution['summary']['original_oas_locked']is True
    assert execution['summary']['capital_ftp_eligible']is False
    assert execution['runtime_control']=='process-watchdog-1'
    tables=db.CODEC.tables(db.result_ref(jid));assert '/model_result/metrics'in tables
    page=reporting.table(jid,'/model_result/metrics',offset=0,limit=1)
    assert len(page['rows'])==1
    before=store.job_result(jid);db.stop();db.start(durable)
    assert store.job_result(jid)==before
    for kind in ['ledger','credit_ledger','cashflows']:
        with pytest.raises(ValueError):treasury_store._source(jid,kind)

@pytest.mark.parametrize('failure',['pv','preview','original_cash','original_oas','date_type','raw_identity','marker','receipt','deadline','source','binary','lease','cancel','upload','scope','layout','revision','completion'])
def test_actual_worker_refuses_raw_persisted_and_publication_drift(durable,monkeypatch,failure):
    jid,job=queued(12);dump=db.CODEC.dump
    def corrupt(value):
        if isinstance(value,dict)and'model_result'in value:
            if failure in ['pv','preview','original_cash','original_oas','date_type','raw_identity']:
                raw=restore_sources(value['model_result'],value['execution']['table_layouts'])
                if failure=='pv':raw['metrics'][0]['pv']+=1
                elif failure=='preview':raw['native_sources'][0]['projection']['result']['action_receipts'][0]['native']['remaining_schedule']['events'][0]['components']['principal']+=1
                elif failure=='original_cash':raw['original_native_sources'][0]['projection']['result']['schedule']['events'][0]['components']['cash_interest']+=1
                elif failure=='original_oas':raw['base_anchor']['oas'][0]['base_oas']=math.nextafter(raw['base_anchor']['oas'][0]['base_oas'],math.inf)
                elif failure=='date_type':raw['native_sources'][0]['projection']['result']['schedule']['events'][0]['payment_ordinal']=float(raw['native_sources'][0]['projection']['result']['schedule']['events'][0]['payment_ordinal'])
                else:raw['native_sources'][0]['product']['request']['terminal_projection_source']['revision']='changed'
                value['model_result'],value['execution']['table_layouts']=format_sources(raw)
            elif failure=='marker':value['execution']['summary']['capital_ftp_eligible']=True
            elif failure=='receipt':value['execution']['source_verification']['complete_preview_tails_verified']=False
            elif failure=='deadline':value['execution']['publication_deadline_unix']+=60.
            elif failure=='scope':value['execution']['scope']='Accounting approved'
            elif failure=='layout':value['execution']['table_layouts']['/unclaimed']=dict(row_json=False,json_columns=[])
            elif failure=='revision':value['revision']=float(value['revision'])
            elif failure=='completion':value['execution']['caller_cooperative_native_control']=0
            elif failure=='source':monkeypatch.setattr(cd_sequence_jobs,'queued_identity',lambda *a,**k:(_ for _ in()).throw(RuntimeError('SOURCE_CHANGED')))
            elif failure=='binary':
                missing=model_native.binary_path().with_suffix('.missing');monkeypatch.setattr(model_native,'binary_path',lambda:missing)
            elif failure=='lease':db.REPO.release(db.WORKER)
            elif failure=='cancel':db.REPO.cancel(jid)
            else:raise OSError('immutable upload failed')
        return dump(value)
    monkeypatch.setattr(db.CODEC,'dump',corrupt);worker.execute(job);assert_failed(jid)

@pytest.mark.parametrize('side',['current','original'])
def test_native_invalid_never_calls_publication_reference(durable,monkeypatch,side):
    from portfolio_risk.analytics import cd_sequence_graph_reference
    case=deepcopy(CASES[26]);raw=case['raw']
    q=raw['positions'][0]['histories'][0]['pricing'][0]['product']['request'] if side=='current'else raw['original_base']['histories'][0]['histories']['pricing'][0]['product']['request']
    q['opening']['unknown_money']=0
    def forbidden(*a,**k):raise AssertionError('invalid raw source reached reference')
    monkeypatch.setattr(cd_sequence_graph_reference,'verify_request',forbidden)
    submitted=model_routes.submit(model_routes.ModelRunRequest(schema_name=case['schema'],request=raw,expected_revision=0))
    db.WORKER='invalid-cd';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER));row=assert_failed(submitted.id)
    assert 'unknown field'in row['detail']

@pytest.mark.parametrize('failure',['deadline','cancel','token','revision','attempt','object'])
def test_private_sql_guard_rechecks_after_verified_parquet(durable,monkeypatch,failure):
    import sqlalchemy as sa
    from app.repository import jobs,workspaces
    jid,job=queued();original=db.REPO.complete;armed=[False];shift=[0.]
    make_budget=cd_sequence_jobs._budget
    def budget(*a,**k):
        value=make_budget(*a,**k);wall=value._wall;value._wall=lambda:wall()+shift[0];return value
    monkeypatch.setattr(cd_sequence_jobs,'_budget',budget)
    def complete(captured,**kw):
        if 'guard'not in kw:return original(captured,**kw)
        guard=kw['guard']
        def mutated(connection):
            armed[0]=True
            if failure=='deadline':shift[0]=120.
            elif failure=='cancel':worker.STOP.set()
            elif failure=='token':connection.execute(jobs.update().where(jobs.c.id==jid).values(token='wrong'))
            elif failure=='attempt':connection.execute(jobs.update().where(jobs.c.id==jid).values(attempt=captured['attempt']+1))
            elif failure=='revision':connection.execute(workspaces.update().where(db.REPO.scoped(workspaces)).values(revision=captured['revision']+1))
            elif failure=='object':monkeypatch.setattr(db.CODEC.objects,'get_bounded',lambda *a,**k:b'changed bytes')
            guard(connection)
        kw['guard']=mutated
        return original(captured,**kw)
    time_origin=time.time();monkeypatch.setattr(db.REPO,'complete',complete)
    try:
        worker.execute(job);row=assert_failed(jid);assert armed[0]
        if failure=='deadline':assert 'original job deadline'in row['detail']
    finally:worker.STOP.clear()

def test_expired_original_queue_budget_not_renewed_on_first_or_retry(durable,monkeypatch):
    jid,job=queued(timeout=.5);time.sleep(.55)
    original=cd_sequence_native.run
    monkeypatch.setattr(cd_sequence_native,'run',lambda *a,**k:(_ for _ in()).throw(AssertionError('expired raw request reached native')))
    worker.execute(job);row=assert_failed(jid)
    assert 'deadline'in row['detail'].lower()
