"""Durable native contracts, immutable inputs, paging and failed publication."""
import json
import pytest
from fastapi import HTTPException
from app import model_routes,store,worker,persistence as db,reporting
from test_persistence import durable


@pytest.fixture(autouse=True)
def selected_model_test_binary(monkeypatch):
    import os
    from pathlib import Path
    override=os.environ.get('MORTGAGE_CREDIT_TEST_BINARY')
    if override:
        from portfolio_risk.core import model_native
        path=Path(override).resolve()
        assert path.name in {'portfolio-lifecycle','portfolio-lifecycle.exe'}
        monkeypatch.setattr(model_native,'binary_path',lambda:path)


def queued(schema='consumer-installment-1'):
    raw=model_routes.example(schema)
    submitted=model_routes.submit(model_routes.ModelRunRequest(**{k:raw[k] for k in ('schema_name','request')},expected_revision=0))
    db.WORKER='model-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted.id,db.REPO.claim(db.WORKER)


@pytest.mark.parametrize('schema',list(model_routes.EXAMPLES))
def test_explicit_models_run_on_worker_with_source_identity_and_restart(durable,monkeypatch,schema):
    from portfolio_risk.core import quant_native
    def forbidden(*args,**kwargs):raise AssertionError('in-process financial callback')
    if schema!='market-indexed-books-1':
        monkeypatch.setattr(quant_native,'term_call',forbidden)
    else:
        from portfolio_risk.core import pricing,curve,vol
        for module,name in [(pricing,'pv_from_A'),(pricing,'solve_oas_from_A'),(curve,'bootstrap_curve'),(vol,'calibrate_abcd')]:
            monkeypatch.setattr(module,name,forbidden)
    jid,job=queued(schema);worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    out=db.CODEC.load(db.result_ref(jid));execution=out['execution']
    assert execution['backend']=='rust' and execution['schema']==schema
    assert len(execution['input_sha256'])==64 and not execution['empirical_validation']
    assert execution['request_manifest']==db.REPO.job(jid)['request']
    tables=db.CODEC.tables(db.result_ref(jid));assert tables
    path=next(iter(tables));assert reporting.table(jid,path,offset=0,limit=2)['rows']
    before=store.job_result(jid);db.stop();db.start(durable)
    assert store.job_result(jid)==before


@pytest.mark.parametrize('failure',['binary','source','cancel','lease','upload','unknown'])
def test_model_failure_leaves_no_successful_manifest(durable,monkeypatch,failure):
    from portfolio_risk.core import model_native
    from portfolio_risk.analytics import balance_stream
    jid,job=queued()
    if failure=='binary':monkeypatch.setattr(model_native,'binary_identity',lambda:'f'*64)
    elif failure=='source':monkeypatch.setattr(balance_stream,'source_identity',lambda:'e'*64)
    elif failure=='cancel':db.REPO.cancel(jid)
    elif failure=='lease':db.REPO.release(db.WORKER)
    elif failure=='upload':
        original=db.CODEC.objects.put
        def fail(data,suffix):
            if suffix=='parquet':raise OSError('model upload failure')
            return original(data,suffix)
        monkeypatch.setattr(db.CODEC.objects,'put',fail)
    else:
        payload=db.CODEC.load(job['request']);payload['args'][0]['request']['python_backend']=True
        job['request']=db.CODEC.dump(payload)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None


def test_analysis_admission_rejects_unversioned_or_stale_request(durable):
    raw=model_routes.example('consumer-card-1')
    with pytest.raises(HTTPException) as exc:model_routes.submit(model_routes.ModelRunRequest(expected_revision=0,schema_name='python',request={}))
    assert exc.value.status_code==422
    with pytest.raises(db.Conflict):model_routes.submit(model_routes.ModelRunRequest(expected_revision=99,schema_name=raw['schema_name'],request=raw['request']))
    with pytest.raises(HTTPException):model_routes.example('../../AGENTS.md')


@pytest.mark.parametrize('schema',['conditional-exercise-1','derivative-accounting-1','credit-path-accounting-1','joint-state-1','joint-market-credit-1','market-quote-bootstrap-1','observed-deposit-calibration-1','consumer-credit-accounting-1','card-credit-accounting-1','card-credit-accounting-2','mortgage-credit-accounting-1','strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1'])
@pytest.mark.parametrize('failure',['amount','receipt','deadline','cancel','source'])
def test_persisted_independent_source_publication_fails_closed(durable,monkeypatch,schema,failure):
    import polars as pl
    from app import model_jobs
    from portfolio_risk.analytics import balance_stream
    jid,job=queued(schema);original=db.CODEC.dump
    def corrupted(result):
        if isinstance(result,dict) and 'model_result' in result:
            if failure=='receipt':result['execution'].pop('source_verification')
            elif failure=='deadline':result['execution']['publication_deadline_unix']=0.
            elif failure=='cancel':db.REPO.cancel(jid)
            elif failure=='source':monkeypatch.setattr(balance_stream,'source_identity',lambda:'different-source')
            else:
                # Change a persisted numerical source cell while retaining a valid Parquet object.
                def alter(value):
                    if isinstance(value,pl.DataFrame):
                        for name,dtype in value.schema.items():
                            if dtype==pl.Float64 and value.height:
                                return value.with_columns((pl.col(name)+1.).alias(name)),True
                    if isinstance(value,dict):
                        for key,child in value.items():
                            changed,done=alter(child)
                            if done:value[key]=changed;return value,True
                    return value,False
                _,changed=alter(result['model_result']);assert changed
        return original(result)
    monkeypatch.setattr(db.CODEC,'dump',corrupted)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('schema',['strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1'])
@pytest.mark.parametrize('failure',['binary','source'])
def test_final_progress_fences_strategy_process_source_and_executable(durable,monkeypatch,schema,failure):
    from portfolio_risk.core import product_strategy_native
    from portfolio_risk.analytics import balance_stream
    jid,job=queued(schema);original=db.REPO.progress
    def change(active,progress):
        result=original(active,progress)
        if progress.get('stage')=='done':
            if failure=='binary':monkeypatch.setattr(product_strategy_native,'binary_identity',lambda:'changed')
            else:monkeypatch.setattr(balance_stream,'source_identity',lambda:'changed')
        return result
    monkeypatch.setattr(db.REPO,'progress',change)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
    assert 'MODEL_VERSION_CHANGED' in row['detail']


@pytest.mark.parametrize('schema',['conditional-exercise-1','observed-deposit-calibration-1','market-quote-bootstrap-1','consumer-installment-1','consumer-credit-accounting-1','card-credit-accounting-1','card-credit-accounting-2','mortgage-credit-accounting-1','card-product-graph-1','aged-units-1','strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1'])
@pytest.mark.parametrize('failure',['deadline','source','binary','stop'])
def test_all_model_publications_keep_final_execution_fences(durable,monkeypatch,schema,failure):
    from portfolio_risk.core import model_native,product_strategy_native
    from portfolio_risk.analytics import balance_stream
    jid,job=queued(schema);original=db.CODEC.dump
    def changed(result):
        if isinstance(result,dict) and 'model_result' in result:
            if failure=='deadline':result['execution']['publication_deadline_unix']=0.
            elif failure=='source':monkeypatch.setattr(balance_stream,'source_identity',lambda:'changed')
            elif failure=='binary':monkeypatch.setattr(product_strategy_native if schema in {'strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1'} else model_native,'binary_identity',lambda:'changed')
            else:worker.STOP.set()
        return original(result)
    monkeypatch.setattr(db.CODEC,'dump',changed)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('kind',['model_analysis','aged_unitlib','product_strategy'])
@pytest.mark.parametrize('failure',['input_sha256','source_sha256','binary_sha256','schema','backend','request_manifest','nonfinite_deadline','extended_deadline'])
def test_persisted_model_execution_identity_matches_queued_raw_and_threads(durable,monkeypatch,kind,failure):
    if kind in {'model_analysis','product_strategy'}:
        jid,job=queued('strategy-product-1' if kind=='product_strategy' else 'consumer-installment-1')
    else:
        raw=model_routes.example('aged-units-1')
        submitted=model_routes.aged_library(model_routes.ModelRunRequest(schema_name='aged-units-1',request=raw['request'],expected_revision=0))
        jid=submitted.id;db.WORKER='model-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER);job=db.REPO.claim(db.WORKER)
    original=db.CODEC.dump
    def corrupt(value):
        if isinstance(value,dict) and 'execution' in value:
            if failure=='nonfinite_deadline':value['execution']['publication_deadline_unix']=float('nan')
            elif failure=='extended_deadline':value['execution']['publication_deadline_unix']+=3600.
            else:value['execution'][failure]='forged-persisted-identity'
        return original(value)
    monkeypatch.setattr(db.CODEC,'dump',corrupt)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
    assert 'persisted model' in row['detail'] or 'publication deadline' in row['detail']
    if kind=='aged_unitlib':assert 'library' not in store.CACHE


@pytest.mark.parametrize('phase',[1,2])
def test_product_independent_replay_observes_cancellation_during_source_work(durable,monkeypatch,phase):
    from portfolio_risk.analytics import product_strategy_reference as reference
    jid,job=queued('strategy-product-1');original=reference.verify;calls=[0]
    def interrupted(raw,result,check=lambda:False):
        calls[0]+=1;steps=[0]
        def checking():
            steps[0]+=1
            if calls[0]==phase and steps[0]==8:worker.STOP.set()
            return check()
        return original(raw,result,check=checking)
    monkeypatch.setattr(reference,'verify',interrupted)
    worker.execute(job);row=db.REPO.job(jid)
    assert calls[0]==phase and row['status']!='done' and row['result'] is None
    assert 'cancelled' in row['detail']


def test_strategy_executed_raw_identity_cannot_be_forged_after_persistence(durable,monkeypatch):
    jid,job=queued('strategy-product-1');original=db.CODEC.dump
    def corrupt(value):
        if isinstance(value,dict) and 'execution' in value:
            value['execution']['executed_input_sha256']='e'*64
        return original(value)
    monkeypatch.setattr(db.CODEC,'dump',corrupt)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
    assert 'execution identity' in row['detail']
