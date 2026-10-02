import json
import os
from pathlib import Path
import tempfile
import time
from contextlib import ExitStack

import polars as pl
import pytest
import sqlalchemy as sa
from app import persistence as db, store, worker, maintenance
from app.artifacts import Codec, Objects, PartitionedTable
from app.iceberg import publish
from app.repository import schema
from test_persistence import durable
from test_streamed_jobs import request, queue


def test_offline_gc_keeps_every_reference_and_other_workspaces(durable, tmp_path):
    objects = db.CODEC.objects
    orphan = objects.put(b'orphan', 'bin')
    fresh = objects.put(b'fresh', 'bin')
    other = Objects(str(objects.root), 'other', 'workspace').put(b'other', 'bin')
    path = objects.root / orphan['key']
    os.utime(path, (time.time()-7200,)*2)
    scratch = tmp_path / 'scratch'; old = scratch / 'balance-old'; old.mkdir(parents=True)
    os.utime(old, (time.time()-7200,)*2)
    plan = maintenance.collect(db.REPO, objects, grace_seconds=3600, scratch=scratch)
    assert [x['key'] for x in plan['candidates']] == [orphan['key']]
    assert path.exists() and old.exists()
    with pytest.raises(ValueError, match='offline'):
        maintenance.collect(db.REPO, objects, apply=True)
    result = maintenance.collect(db.REPO, objects, grace_seconds=3600, apply=True, offline=True, scratch=scratch)
    assert result['removed'] == [orphan['key']] and not old.exists()
    assert objects.get(fresh) == b'fresh' and (objects.root / other['key']).exists()
    assert db.CODEC.load(db.REPO.revision()['manifest'])['revision'] == 0


def test_gc_fails_closed_on_live_corruption_or_active_worker(durable):
    assert db.REPO.acquire('active')
    with pytest.raises(RuntimeError, match='lease'):
        maintenance.collect(db.REPO, db.CODEC.objects, apply=True, offline=True)
    db.REPO.release('active')
    ref = db.REPO.revision()['manifest']
    (db.CODEC.objects.root / ref['key']).write_bytes(b'corrupt')
    with pytest.raises(ValueError, match='checksum'):
        maintenance.collect(db.REPO, db.CODEC.objects)


def test_catalog_recovery_and_no_duplicate_rows(durable):
    catalog_module = pytest.importorskip('pyiceberg.catalog')
    jid, job = queue(request()); worker.execute(job)
    assert db.REPO.job(jid)['status'] == 'done'
    with ExitStack() as stack:
        directory = stack.enter_context(tempfile.TemporaryDirectory(prefix='ice-'))
        folder = Path(directory)
        catalog = catalog_module.load_catalog('test', type='sql', uri=f'sqlite:///{(folder / "catalog.db").as_posix()}',
                                               warehouse=(folder / 'warehouse').as_uri(),
                                               **{'py-io-impl':'pyiceberg.io.fsspec.FsspecFileIO'})
        stack.callback(catalog.engine.dispose)
        failed = []
        def crash(path):
            failed.append(path)
            raise OSError('crash after catalog commit before SQL receipt')
        with pytest.raises(OSError, match='after catalog'):
            publish(db.REPO, db.CODEC, jid, catalog, after_commit=crash)
        assert not db.REPO.publication_receipts(jid)
        delivery = publish(db.REPO, db.CODEC, jid, catalog)
        again = publish(db.REPO, db.CODEC, jid, catalog)
        assert delivery == again and len(db.REPO.publication_receipts(jid)) == 1
        descriptor = db.CODEC.tables(db.result_ref(jid))
        for name, receipt in delivery['tables'].items():
            table = catalog.load_table(receipt['table'])
            frame = table.scan().to_arrow()
            assert frame.num_rows == descriptor[name]['rows']
            if frame.num_rows:
                assert set(frame['_wb_run_id'].to_pylist()) == {jid}
                # Independent full financial values, not just row counts.
                expected = pl.concat([pl.read_parquet(__import__('io').BytesIO(db.CODEC.objects.get(p['ref'])))
                                      for p in descriptor[name]['parts']])
                actual = pl.from_arrow(frame).select(expected.columns)
                assert actual.sort(actual.columns).equals(expected.sort(expected.columns))
            assert receipt['snapshot_id'] is not None
        # Catalog receipts themselves remain rooted for orphan collection.
        refs = maintenance.reachable(db.REPO, db.CODEC.objects)
        assert db.REPO.publication_receipts(jid)[0]['manifest']['key'] in refs
        another, next_job = queue(request()); worker.execute(next_job)
        second = publish(db.REPO, db.CODEC, another, catalog)
        assert second['tables']['/journal']['table'] == delivery['tables']['/journal']['table']
        assert publish(db.REPO, db.CODEC, jid, catalog) == delivery
        combined = catalog.load_table(delivery['tables']['/journal']['table']).scan().to_arrow()
        assert combined.num_rows == 2 * descriptor['/journal']['rows']
        assert set(combined['_wb_run_id'].to_pylist()) == {jid, another}


def test_catalog_concurrent_retry_has_one_marker(durable):
    pytest.importorskip('pyiceberg')
    import pyarrow as pa
    from pyiceberg.catalog import load_catalog
    from pyiceberg.exceptions import CommitFailedException
    # Two catalog transactions from the same base snapshot cannot both commit.
    with ExitStack() as stack:
        directory = stack.enter_context(tempfile.TemporaryDirectory(prefix='ice-'))
        folder = Path(directory)
        catalog = load_catalog('test', type='sql', uri=f'sqlite:///{(folder / "cat.db").as_posix()}', warehouse=(folder/'w').as_uri(),
                               **{'py-io-impl':'pyiceberg.io.fsspec.FsspecFileIO'})
        stack.callback(catalog.engine.dispose)
        catalog.create_namespace('ns')
        from pyiceberg.schema import Schema
        from pyiceberg.types import NestedField, LongType
        catalog.create_table('ns.test', schema=Schema(NestedField(1,'x',LongType(),required=False)))
        a, b = catalog.load_table('ns.test').transaction(), catalog.load_table('ns.test').transaction()
        for transaction in (a,b):
            transaction.append(pa.table({'x':[1]}))
            transaction.set_properties({'workbench.run.test':'committed'})
        a.commit_transaction()
        with pytest.raises(CommitFailedException): b.commit_transaction()
        assert catalog.load_table('ns.test').scan().to_arrow().num_rows == 1


def test_additive_schema_upgrade_preserves_revision(durable):
    with db.REPO.engine.begin() as conn: conn.execute(schema.update().values(version=1))
    before = db.REPO.revision()['manifest']
    db.REPO.migrate()
    assert db.REPO.revision()['manifest'] == before
    with db.REPO.engine.connect() as conn:
        assert conn.execute(sa.select(schema.c.version)).scalars().all() == [2]


def test_s3_orphan_collection_protocol(durable, monkeypatch):
    import boto3
    import datetime as dt
    import io
    from botocore.stub import Stubber
    from botocore.response import StreamingBody
    live = db.CODEC.dump({'keep':True})
    raw = db.CODEC.objects.get(live)
    orphan = db.CODEC.objects.put(b'orphan', 'bin')
    client = boto3.client('s3', region_name='us-east-1', aws_access_key_id='test', aws_secret_access_key='test')
    objects = Objects('s3://test-bucket', **db.REPO.identity, s3_client=client)
    monkeypatch.setattr(maintenance, 'roots', lambda repo: [live])
    old = dt.datetime.now(dt.timezone.utc)-dt.timedelta(days=2)
    with Stubber(client) as stub:
        stub.add_response('get_object', {'Body':StreamingBody(io.BytesIO(raw),len(raw))},
                          {'Bucket':'test-bucket','Key':live['key']})
        stub.add_response('list_objects_v2', {'Contents':[
            dict(Key=ref['key'],Size=ref['bytes'],LastModified=old) for ref in [live,orphan]]},
            {'Bucket':'test-bucket','Prefix':objects.prefix+'/'})
        stub.add_response('head_object', {'LastModified':old}, {'Bucket':'test-bucket','Key':orphan['key']})
        stub.add_response('delete_object', {}, {'Bucket':'test-bucket','Key':orphan['key']})
        result = maintenance.collect(db.REPO, objects, grace_seconds=3600, apply=True, offline=True)
        assert result['removed'] == [orphan['key']]
        stub.assert_no_pending_responses()
