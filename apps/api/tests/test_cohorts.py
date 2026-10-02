"""Real durable worker, artifact lineage, loan analytics and snapshot restoration."""
import base64
import polars as pl
import pytest
from app import store, persistence as db, worker, cohort_store, cohort_routes
from test_persistence import durable, BACKENDS
from portfolio_risk.analytics.cohorts import example, presets


def execute(kind, fn, *args):
    jid=store.submit(kind,fn,*args)
    db.WORKER='cohort-test';worker.STOP.clear()
    assert db.REPO.acquire(db.WORKER)
    job=db.REPO.claim(db.WORKER)
    worker.execute(job)
    assert db.REPO.job(jid)['status']=='done',db.REPO.job(jid)['detail']
    return jid,db.CODEC.load(db.result_ref(jid))


def setup_tape():
    jid,_=execute('tape_import',cohort_store.import_tape,dict(name='demo.csv',format='csv',content_base64=base64.b64encode(example().encode()).decode()))
    cohort_store.adopt('demo',jid,store.STATE_META['revision'])
    return execute('cohort_build',cohort_store.build,'demo',presets())


def test_tape_lineage_publish_and_restart(durable):
    jid,out=setup_tape()
    row=cohort_routes.lineage(jid,loan_id='mortgage-000',offset=0,limit=10)
    assert row['total']==1 and row['rows'][0]['source_sha256']==out['source_sha256']
    summary=cohort_routes.summary(jid)
    assert summary['summary']['loans']==20
    assert store.job_result(jid).startswith(b'ARW1')
    revision=store.STATE_META['revision']
    with pytest.raises(ValueError,match='dedicated'):
        cohort_store.publish(jid,['credit_card'],revision)
    assert store.STATE_META['revision']==revision
    published=cohort_store.publish(jid,['mortgage','deposit'],revision)
    assert published['books']=={'mbs':1,'deposits':1}
    assert store.COHORT_PUBLICATIONS['mbs']['build_id']==out['build_id']
    db.stop();db.start(durable)
    assert store.TAPES['demo']['sha256']==out['source_sha256']
    assert store.TAPES['demo']['source_bytes']==example().encode()
    assert store.COHORT_PUBLICATIONS['mbs']['job_id']==jid
    with pytest.raises(db.Conflict):cohort_store.publish(jid,['mortgage'],store.STATE_META['revision'])


def test_original_loan_pricing_and_allocations(durable):
    store.SETTINGS=store.SETTINGS.model_copy(update={'n_paths':32,'n_paths_base':32,'horizon_months':3,'n_threads':2})
    jid,out=setup_tape()
    priced,cohort=execute('cohort_analytics',cohort_store.analytics,jid,['mortgage','deposit'])
    assert cohort['calculation_basis']=='representative_cohort_reprice'
    _,loan=execute('cohort_analytics',cohort_store.analytics,jid,['mortgage'],['mortgage-000'])
    assert loan['calculation_basis']=='individual_model_reprice'
    assert loan['positions']['mbs']['notional'].to_list()==[100000.]
    _,allocation=execute('cohort_attribution',cohort_store.attribution,jid,priced)
    assert allocation['positions']['loan_id'].n_unique()==8
    assert allocation['positions']['market_value'].sum()==pytest.approx(cohort['risk']['market_value'].sum())
    for name in ('principal','cash_interest','accrual_interest','book_amortization'):
        assert allocation['cashflows'][name].sum()==pytest.approx(cohort['cashflows'][name].sum(),abs=1e-8)


def test_financial_audit_persists_partitioned_loan_outputs(durable):
    from app.reporting import table
    store.SETTINGS=store.SETTINGS.model_copy(update={'n_paths':32,'n_paths_base':32,'horizon_months':3,'n_threads':2})
    jid,out=setup_tape()
    audited,report=execute('cohort_audit',cohort_store.audit,jid,['mortgage','deposit'],{'shocks':[0.,200.]})
    assert report['build_id']==out['build_id']
    assert report['summary']['loans']==8
    assert report['loan_cashflows']['format']=='partitioned-parquet'
    assert report['loan_cashflows']['rows']==24
    assert len(report['loan_risk']['parts'])==1  # Coalesce small pricing batches for object storage.
    page=table(audited,'/errors',limit=10)
    assert page['total']==2*(2*3+3*4)
    loans=table(audited,'/loan_risk',limit=100)
    assert loans['total']==16
    assert 'individual_model_reprice' in loans['rows'][0]


def test_source_allowlists_and_stale_adoption(durable,tmp_path,monkeypatch):
    monkeypatch.setenv('WORKBENCH_TAPE_ROOT',str(tmp_path/'allowed'))
    with pytest.raises(ValueError,match='outside'):
        cohort_store.import_tape({'uri':str(tmp_path/'private.csv'),'format':'csv'})
    monkeypatch.setenv('WORKBENCH_TAPE_S3_PREFIX','s3://bank/tapes')
    with pytest.raises(ValueError,match='outside'):
        cohort_store.import_tape({'uri':'s3://bank/tapes-other/file.csv','format':'csv'})
    jid,_=execute('tape_import',cohort_store.import_tape,dict(name='demo.csv',format='csv',content_base64=base64.b64encode(example().encode()).decode()))
    with pytest.raises(db.Conflict):cohort_store.adopt('bad',jid,store.STATE_META['revision']+1)
    assert 'bad' not in store.TAPES


def test_tape_snapshot_export_moves_original_bytes_and_rules(durable,tmp_path):
    import os,sys,subprocess,uuid
    from app.repository import Repository
    from app.artifacts import Codec,Objects
    jid,out=setup_tape()
    cohort_store.publish(jid,['mortgage'],store.STATE_META['revision'])
    env=os.environ | {'WORKBENCH_ENV':'development','WORKBENCH_EXECUTION':'external',
        'DATABASE_URL':durable.database_url,'ARTIFACT_URL':durable.artifact_url,
        'WORKBENCH_TENANT_ID':durable.tenant,'WORKBENCH_WORKSPACE_ID':durable.workspace}
    folder=tmp_path/'export';command=[sys.executable,'-m','app.storage_admin']
    subprocess.run(command+['export',str(folder)],env=env,check=True,capture_output=True)
    destination=uuid.uuid4().hex;env['WORKBENCH_WORKSPACE_ID']=destination
    subprocess.run(command+['import',str(folder)],env=env,check=True,capture_output=True)
    repo=Repository(durable.database_url,durable.tenant,destination)
    try:
        state=Codec(Objects(durable.artifact_url,durable.tenant,destination)).load(repo.revision()['manifest'])
        assert state['tapes']['demo']['source_bytes']==example().encode()
        assert state['cohort_publications']['mbs']['config']==out['config']
        assert 'job_id' not in state['cohort_publications']['mbs']
        assert state['cohort_publications']['mbs']['imported_input_snapshot']
    finally:repo.engine.dispose()


@pytest.mark.parametrize('product,book',[('deposit','deposits'),('mortgage','mbs')])
def test_selective_publication_preserves_unrelated_positions(durable,product,book):
    jid,out=setup_tape()
    config=presets()
    if product=='mortgage':
        config['defaults']['book_yield']=.055
        config['rules']['mortgage']['averages'].append('book_yield')
        jid,out=execute('cohort_build',cohort_store.build,'demo',config)
    original=store.BOOKS[book].clone()
    cohort_store.publish(jid,[product],store.STATE_META['revision'],mode='replace_tape')
    assert len(store.BOOKS[book])==len(original)+1
    assert store.BOOKS[book].filter(pl.col('source_tape_id').is_null()).select(original.columns).equals(original)
    next_job,_=execute('cohort_build',cohort_store.build,'demo',config,None,jid)
    refreshed=cohort_store.metadata(next_job)
    assert refreshed['refresh_summary']['counts']['unchanged']==20
    cohort_store.publish(next_job,[product],store.STATE_META['revision'],mode='replace_tape')
    assert len(store.BOOKS[book])==len(original)+1


def test_cohort_iceberg_delivery_is_retry_safe(durable,tmp_path):
    load_catalog=pytest.importorskip('pyiceberg.catalog').load_catalog
    from app.iceberg import publish
    jid,out=setup_tape()
    catalog=load_catalog('cohort-test',type='sql',uri=f'sqlite:///{(tmp_path/"cat.db").as_posix()}',
        warehouse=(tmp_path/'warehouse').as_uri(),**{'py-io-impl':'pyiceberg.io.fsspec.FsspecFileIO'})
    try:
        first=publish(db.REPO,db.CODEC,jid,catalog)
        second=publish(db.REPO,db.CODEC,jid,catalog)
        assert first==second
        assert '/source' not in first['tables']
        lineage=first['tables']['/lineage']
        assert lineage['rows']==20
        assert catalog.load_table(lineage['table']).scan().to_arrow().num_rows==20
    finally:catalog.engine.dispose()


def test_equal_tapes_keep_distinct_positions_when_one_is_refreshed(durable):
    jid,out=setup_tape()
    original=len(store.BOOKS['deposits'])
    cohort_store.publish(jid,['deposit'],store.STATE_META['revision'],mode='replace_tape')
    store.TAPES['second']=store.TAPES['demo']
    store.changed()
    second,_=execute('cohort_build',cohort_store.build,'second',presets())
    cohort_store.publish(second,['deposit'],store.STATE_META['revision'],mode='replace_tape')
    for tape in ('demo','demo','second','second'):
        next_job,_=execute('cohort_build',cohort_store.build,tape,presets())
        cohort_store.publish(next_job,['deposit'],store.STATE_META['revision'],mode='replace_tape')
        assert len(store.BOOKS['deposits'])==original+2
        assert store.BOOKS['deposits']['id'].n_unique()==original+2
