import io
import polars as pl
import pytest
from fastapi.testclient import TestClient
from app import store, persistence as db, worker, treasury_routes
from app.main import app
from portfolio_risk.analytics.treasury import example
from test_persistence import durable, BACKENDS


def test_durable_capital_ftp_snapshot_restart_and_paging(durable):
    spec=example()
    request=treasury_routes.TreasuryRequest(expected_revision=store.snapshot()['revision'],specification=spec)
    submitted=treasury_routes.submit(request)
    spec['capital'][0]['common_equity']=999.  # queued request must be immutable
    db.WORKER='treasury-test';worker.STOP.clear()
    assert db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER))
    assert db.REPO.job(submitted.id)['status']=='done',db.REPO.job(submitted.id)['detail']
    out=db.CODEC.load(db.result_ref(submitted.id))
    assert out['backend']=='rust' and out['capital_metrics'].height==26
    assert out['specification']['capital'][0]['common_equity']==100e6
    db.stop();db.start(durable)
    restored=db.CODEC.load(db.result_ref(submitted.id))
    assert restored['input_sha256']==out['input_sha256']
    # Persistence is owned by this fixture, not the application's default lifespan.
    client=TestClient(app)
    try:
        response=client.get(f'/jobs/{submitted.id}/table',params={'path':'/capital_metrics','offset':0,'limit':5})
        assert response.status_code==200 and response.json()['total']==26
        artifact=client.get(f'/jobs/{submitted.id}/parquet',params={'path':'/ftp_positions'})
        assert artifact.status_code==200
        assert pl.read_parquet(io.BytesIO(artifact.content)).height==2
    finally:
        client.close()


def test_revision_conflict_and_non_durable_fail_closed(durable):
    with pytest.raises(db.Conflict):
        treasury_routes.submit(treasury_routes.TreasuryRequest(expected_revision=999,specification=example()))
    from fastapi import HTTPException
    db.stop()
    with pytest.raises(HTTPException,match='durable'):
        treasury_routes.submit(treasury_routes.TreasuryRequest(expected_revision=0,specification=example()))


def test_treasury_iceberg_retry_and_null_columns(durable,tmp_path):
    load_catalog=pytest.importorskip('pyiceberg.catalog').load_catalog
    from app.iceberg import publish
    spec=example();spec['positions']=[]
    for c in spec['capital']:c['advanced_rwa']=None
    submitted=treasury_routes.submit(treasury_routes.TreasuryRequest(
        expected_revision=store.snapshot()['revision'],specification=spec))
    db.WORKER='treasury-iceberg';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER))
    assert db.REPO.job(submitted.id)['status']=='done'
    catalog=load_catalog('treasury-test',type='sql',uri=f'sqlite:///{(tmp_path/"cat.db").as_posix()}',
        warehouse=(tmp_path/'warehouse').as_uri(),**{'py-io-impl':'pyiceberg.io.fsspec.FsspecFileIO'})
    try:
        first=publish(db.REPO,db.CODEC,submitted.id,catalog)
        assert publish(db.REPO,db.CODEC,submitted.id,catalog)==first
        assert set(first['tables'])=={'/capital_metrics','/ftp_positions','/ftp_reconciliation'}
        for path,rows in [('/capital_metrics',26),('/ftp_positions',0),('/ftp_reconciliation',0)]:
            assert catalog.load_table(first['tables'][path]['table']).scan().to_arrow().num_rows==rows
    finally:catalog.engine.dispose()


def test_nonfinite_input_is_client_error(durable):
    from fastapi import HTTPException
    spec=example();spec['capital'][0]['common_equity']=float('nan')
    with pytest.raises(HTTPException) as caught:
        treasury_routes.submit(treasury_routes.TreasuryRequest(expected_revision=store.snapshot()['revision'],specification=spec))
    assert caught.value.status_code==422
