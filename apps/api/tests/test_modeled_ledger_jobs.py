"""Native graph/driver source adoption through durable independent publication."""
import json
import io
import time
import pytest
from app import main,worker,persistence as db,maintenance,reporting,store
from app.schemas import ModeledBalanceStressRequest
from test_persistence import durable
from app.artifacts import ObjectReference

DATED_MODELS={'graph-dated-ledger-1','deposit-graph-dated-ledger-1','deposit-graph-income-ledger-1'}

def queued(schema):
    raw=main.modeled_balance_stress_example(schema)
    request=ModeledBalanceStressRequest(expected_revision=0,specification=raw['specification'])
    submitted=main.modeled_balance_stress(request)
    db.WORKER='modeled-ledger-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted['id'],db.REPO.claim(db.WORKER),raw['specification']

@pytest.mark.parametrize('schema',[*sorted(DATED_MODELS),'driven-credit-ledger-1'])
def test_native_modeled_sources_publish_and_survive_restart(durable,monkeypatch,schema):
    from portfolio_risk.analytics import model_contracts,balance_stream,balance_stress
    def forbidden(*args,**kwargs):raise AssertionError('Python financial execution callback')
    monkeypatch.setattr(model_contracts,'analyze_advanced_graph',forbidden)
    monkeypatch.setattr(model_contracts,'analyze_deposit_graph',forbidden)
    monkeypatch.setattr(model_contracts,'simulate_deposit_state',forbidden)
    monkeypatch.setattr(model_contracts,'project_credit_covariates',forbidden)
    monkeypatch.setattr(balance_stream,'_python',forbidden)
    monkeypatch.setattr(balance_stress,'_simulate',forbidden)
    jid,job,raw=queued(schema);worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    result=db.CODEC.load(db.result_ref(jid));execution=result['execution']
    gate={'graph-dated-ledger-1':'physical_consumer_source_recomputed','deposit-graph-dated-ledger-1':'physical_deposit_source_recomputed','deposit-graph-income-ledger-1':'original_deposit_income_recomputed','driven-credit-ledger-1':'driven_credit_source_recomputed'}[schema]
    assert execution['validation'][gate] and execution['validation']['journal_replayed']
    assert not execution['validation']['empirical_validation']
    assert execution['model_version']==schema
    assert execution['modeled_source']['key'] in maintenance.reachable(db.REPO,db.CODEC.objects)
    source=json.loads(db.CODEC.objects.get(execution['modeled_source']))
    assert source['model_schema']==schema
    assert json.loads(db.CODEC.objects.get(execution['input']))['request']==raw
    assert reporting.table(jid,'/journal',offset=0,limit=2)['rows']
    before=store.job_result(jid);db.stop();db.start(durable);assert store.job_result(jid)==before

@pytest.mark.parametrize('failure',['upload','cancel','lease','source','binary'])
def test_modeled_failed_publication_leaves_no_result(durable,monkeypatch,failure):
    from portfolio_risk.analytics import model_ledger
    jid,job,_=queued('graph-dated-ledger-1')
    if failure=='binary':
        payload=db.CODEC.load(job['request']);payload['args'][0]['binary_sha256']='0'*64;job['request']=db.CODEC.dump(payload)
    elif failure=='source':
        payload=db.CODEC.load(job['request']);payload['args'][0]['source_sha256']='0'*64;job['request']=db.CODEC.dump(payload)
    elif failure=='upload':
        original=db.CODEC.objects.put
        def fail(data,suffix):
            if suffix=='parquet':raise OSError('injected modeled upload failure')
            return original(data,suffix)
        monkeypatch.setattr(db.CODEC.objects,'put',fail)
    else:
        original=db.CODEC.import_dated_partitions
        def fail(path,cancelled):
            if failure=='cancel':db.REPO.cancel(jid)
            else:db.REPO.release(db.WORKER)
            return original(path,cancelled)
        monkeypatch.setattr(db.CODEC,'import_dated_partitions',fail)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('schema,kind',[(schema,'ledger') for schema in sorted(DATED_MODELS)]+[('driven-credit-ledger-1','credit_ledger')])
def test_verified_model_source_feeds_explicit_native_closing_capital(durable,schema,kind):
    from app import treasury_store
    from test_cohorts import execute
    from test_treasury_sources import fill_ledger
    jid,job,_=queued(schema);worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    policy=fill_ledger(treasury_store.template(jid,kind)['specification'])
    for p in policy['policies']:
        if kind=='ledger':p['gl_risk_weights'].update(dated_interest_receivable=1.,dated_prepaid_expense=1.)
        else:p['gl_risk_weights'].update(credit_default_principal=1.,credit_default_accrued_interest=1.,credit_default_book_adjustment=1.)
    db.REPO.release(db.WORKER)
    _,out=execute('treasury',treasury_store.run,jid,policy)
    assert out['source_verification']['journal_replayed']
    assert out['capital_bridge'].height==2
    assert out['source_receipt']['kind']=='modeled_balance_stress'
    with pytest.raises(ValueError):treasury_store.template(jid,'credit_ledger' if kind=='ledger' else 'ledger')
    with pytest.raises(ValueError):treasury_store.template(jid,'cashflows')


@pytest.mark.parametrize('failure',['raw','header','origin','balanced_money','date','drop_table','replace_table',
    'upload_deadline','extended_deadline','cancel_after_upload','lease_after_upload','binary_after_upload','library_after_upload','source_after_upload'])
@pytest.mark.parametrize('schema',[*sorted(DATED_MODELS),'driven-credit-ledger-1'])
def test_persisted_modeled_publication_rechecks_source_and_every_partition(durable,monkeypatch,failure,schema):
    from app import ledger_jobs
    from portfolio_risk.analytics import model_ledger,balance_stream,ledger_native
    import polars as pl
    jid,job,_=queued(schema)
    original_dump=db.CODEC.dump;original_load=db.CODEC.load
    def replace_result(value):
        ref=original_dump(value)
        if not isinstance(value,dict) or value.get('execution',{}).get('model_version')!=schema:return ref
        loaded=original_load(ref);execution=loaded['execution']
        if failure in {'raw','header','origin'}:
            key='input' if failure=='raw' else 'modeled_source'
            source=json.loads(db.CODEC.objects.get(execution[key]))
            if failure=='raw':source['request']['monetary_unit']='forged-unit'
            elif failure=='header':source['schema']='forged-header'
            elif schema in DATED_MODELS:source['source_receipt']['capture']['accrual_plans'][0]['segments'][0]['origin_id']='forged-origin'
            else:source['source_receipt']['projections'][0]['projection']['model_revision']['revision']='forged-driver-origin'
            execution[key]=ObjectReference(db.CODEC.objects.put(json.dumps(source).encode(),'bin'))
        elif failure in {'balanced_money','date','replace_table'}:
            name='journal' if failure!='replace_table' else ('event_lineage' if schema in DATED_MODELS else 'credit_path_sources');part=loaded[name]['parts'][0]
            frame=pl.read_parquet(io.BytesIO(db.CODEC.objects.get(part['ref'])))
            rows=frame.to_dicts()
            if failure=='balanced_money':
                # Opening cash/principal and equity remain a balanced transaction.
                if schema in DATED_MODELS:
                    for row in rows[:2]:
                        row['debit']+=1. if row['debit'] else 0.
                        row['credit']+=1. if row['credit'] else 0.
                else:
                    rows[0]['value']+=1.;rows[1]['value']-=1.
            elif failure=='date':
                for row in rows:row['day']+=1
            else:rows[0]['source_revision']='forged-source-revision'
            altered=pl.DataFrame(rows,schema=frame.schema);stream=io.BytesIO();altered.write_parquet(stream)
            part['ref']=db.CODEC.objects.put(stream.getvalue(),'parquet')
        elif failure=='drop_table':loaded.pop('event_lineage' if schema in DATED_MODELS else 'credit_path_sources')
        elif failure=='upload_deadline':
            execution['publication_deadline_unix']=time.time()+.001
            time.sleep(.01)
        elif failure=='extended_deadline':execution['publication_deadline_unix']+=3600.
        elif failure=='cancel_after_upload':db.REPO.cancel(jid)
        elif failure=='lease_after_upload':db.REPO.release(db.WORKER)
        elif failure in {'binary_after_upload','library_after_upload'}:
            changed=model_ledger.binary_path() if failure=='binary_after_upload' else ledger_native.library_path()
            hash_file=balance_stream._hash
            monkeypatch.setattr(balance_stream,'_hash',lambda path:'0'*64 if path==changed else hash_file(path))
        else:monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
        return original_dump(loaded)
    monkeypatch.setattr(db.CODEC,'dump',replace_result)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
    assert 'KeyError' not in str(row['detail'])
    if failure=='upload_deadline':assert 'deadline' in row['detail']


@pytest.mark.parametrize('failure',['missing_source','missing_all_markers','wrong_category','raw_header','missing_table'])
def test_modeled_import_rejects_incomplete_or_mismatched_source_manifest(durable,monkeypatch,failure):
    from pathlib import Path
    import hashlib
    jid,job,_=queued('graph-dated-ledger-1');original=db.CODEC.import_dated_partitions
    def tamper(path,cancelled):
        path=Path(path);manifest=json.loads(path.read_text())
        if failure=='missing_source':manifest.pop('modeled_source')
        elif failure=='missing_all_markers':manifest.pop('modeled_source');manifest.pop('model_version')
        elif failure=='wrong_category':manifest['model_version']='driven-credit-ledger-1';manifest['validation']['driven_credit_source_recomputed']=True
        elif failure=='missing_table':manifest['tables'].pop('dated_rights')
        else:
            source_path=path.parent/manifest['modeled_source']['path'];source=json.loads(source_path.read_text())
            source['source_receipt']['raw_request']['graph']['currency']='EUR'
            data=json.dumps(source).encode();source_path.write_bytes(data)
            manifest['modeled_source']['sha256']=hashlib.sha256(data).hexdigest()
        path.write_text(json.dumps(manifest));return original(path,cancelled)
    monkeypatch.setattr(db.CODEC,'import_dated_partitions',tamper)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('schema',[*sorted(DATED_MODELS),'driven-credit-ledger-1'])
def test_modeled_persisted_replay_detects_corrupt_uploaded_object(durable,monkeypatch,schema):
    jid,job,_=queued(schema);original=db.CODEC.objects.put;damaged=[]
    def corrupt(data,suffix):
        ref=original(data,suffix)
        if suffix=='parquet' and not damaged:
            damaged.append(ref)
            (db.CODEC.objects.root/ref['key']).write_bytes(b'corrupt immutable upload')
        return ref
    monkeypatch.setattr(db.CODEC.objects,'put',corrupt)
    worker.execute(job);row=db.REPO.job(jid)
    assert damaged and row['status']!='done' and row['result'] is None
    assert 'checksum' in row['detail']


@pytest.mark.parametrize('marker',['physical_deposit_source_recomputed','original_deposit_income_recomputed','original_yield_anchors_retained'])
def test_original_deposit_income_requires_each_distinct_source_marker(durable,monkeypatch,marker):
    from pathlib import Path
    jid,job,_=queued('deposit-graph-income-ledger-1');original=db.CODEC.import_dated_partitions
    def tamper(path,cancelled):
        path=Path(path);manifest=json.loads(path.read_text())
        manifest['validation'].pop(marker)
        path.write_text(json.dumps(manifest));return original(path,cancelled)
    monkeypatch.setattr(db.CODEC,'import_dated_partitions',tamper)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
    assert 'unverified' in row['detail']


@pytest.mark.parametrize('failure',['source','binary','library','cancel'])
def test_final_progress_fences_modeled_artifact_changes(durable,monkeypatch,failure):
    from portfolio_risk.analytics import balance_stream,model_ledger,ledger_native
    jid,job,_=queued('deposit-graph-income-ledger-1');original=db.REPO.progress
    def change(active,progress):
        result=original(active,progress)
        if progress.get('stage')=='done':
            if failure=='source':monkeypatch.setattr(balance_stream,'source_identity',lambda:'changed-source')
            elif failure in {'binary','library'}:
                changed=model_ledger.binary_path() if failure=='binary' else ledger_native.library_path()
                real_hash=balance_stream._hash
                monkeypatch.setattr(balance_stream,'_hash',lambda path:'changed-artifact' if path==changed else real_hash(path))
            else:worker.STOP.set()
        return result
    monkeypatch.setattr(db.REPO,'progress',change)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
    assert ('MODEL_VERSION_CHANGED' if failure!='cancel' else 'cancelled') in row['detail']


def test_original_yield_anchor_cannot_be_changed_in_a_persisted_source(durable,monkeypatch):
    jid,job,_=queued('deposit-graph-income-ledger-1');original=db.CODEC.dump
    def tamper(value):
        if isinstance(value,dict) and value.get('execution',{}).get('model_version')=='deposit-graph-income-ledger-1':
            execution=value['execution'];header=json.loads(db.CODEC.objects.get(execution['modeled_source']))
            header['source_receipt']['raw_request']['income_anchors'][0]['original_annual_effective_rate']+=.01
            execution['modeled_source']=ObjectReference(db.CODEC.objects.put(json.dumps(header).encode(),'bin'))
        return original(value)
    monkeypatch.setattr(db.CODEC,'dump',tamper)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
    assert 'source' in row['detail'] or 'anchor' in row['detail']
