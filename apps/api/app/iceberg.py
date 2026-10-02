"""Idempotent delivery of committed partitioned results to an Iceberg catalog.

Install the iceberg extra. Catalogs use standard PyIceberg configuration; REST
and SQL catalogs share this adapter. Each table commits data and its run marker
atomically. A SQL receipt marks completion of the entire multi-table delivery.
"""
import argparse
import hashlib
import io
import json
import re

from . import persistence as db


def publish(repo, codec, jid, catalog, namespace='workbench', after_commit=None):
    import pyarrow as pa
    import pyarrow.parquet as pq
    from pyiceberg.exceptions import CommitFailedException, TableAlreadyExistsError
    from pyiceberg.schema import Schema
    from pyiceberg.types import NestedField, StringType, DoubleType, LongType, BooleanType

    if not re.fullmatch(r'[a-zA-Z][a-zA-Z0-9_]{0,63}', namespace):
        raise ValueError('namespace must be one simple identifier')
    job = repo.job(jid)
    cohort_job = job['kind'] in {'cohort_build', 'cohort_audit', 'cohort_attribution', 'cohort_analytics', 'treasury'}
    if job['status'] != 'done' or (job['kind'] != 'streamed_balance_stress' and not cohort_job):
        raise ValueError('publish requires a completed stress, cohort or treasury job')
    result = job['result']
    tables = codec.tables(result)
    if cohort_job:
        # Only explicit analytical tables, never retained raw tape fields or
        # nested metadata. The same run marker and retry contract applies.
        allowed = {'/cohorts','/lineage','/dispersion','/migration','/risk','/cashflows',
                   '/errors','/suggestions','/loan_risk','/loan_cashflows',
                   '/capital_metrics','/ftp_positions','/ftp_reconciliation',
                   '/capital_bridge','/rwa_contributions','/ftp_preparation'}
        tables = {path:ref for path,ref in tables.items() if path in allowed}
        if not tables: raise ValueError('cohort job has no publishable tables')
        import polars as pl
        for path, ref in list(tables.items()):
            if ref.get('format') == 'partitioned-parquet': continue
            frame = pl.read_parquet(io.BytesIO(codec.objects.get(ref)))
            tables[path] = {'format':'partitioned-parquet', 'schema':{k:str(v) for k,v in frame.schema.items()},
                            'rows':len(frame), 'parts':[{'rows':len(frame),'ref':ref}]}
    scope = codec.objects.prefix.split('/')[-1]
    destination = hashlib.sha256(json.dumps({'name': catalog.name, 'namespace': namespace,
        'configuration': {k: catalog.properties.get(k) for k in ['type', 'uri', 'warehouse']}}, sort_keys=True).encode()).hexdigest()
    catalog.create_namespace_if_not_exists(namespace)
    metadata = {'_wb_run_id': jid, '_wb_revision': job['revision'],
                '_wb_tenant': repo.identity['tenant'], '_wb_workspace': repo.identity['workspace'],
                '_wb_result_sha256': result['sha256']}
    types = {'String': (StringType(), pa.string()), 'Float64': (DoubleType(), pa.float64()),
             'Int64': (LongType(), pa.int64()), 'Boolean': (BooleanType(), pa.bool_())}
    receipts = {}
    for path, descriptor in tables.items():
        if descriptor.get('format') != 'partitioned-parquet':
            raise ValueError('catalog publisher accepts partitioned tables only')
        name = path.lstrip('/')
        if not re.fullmatch(r'[a-z_]+', name): raise ValueError('invalid result table name')
        fields = list(descriptor['schema'].items()) + [(k, 'Int64' if k == '_wb_revision' else 'String') for k in metadata]
        fingerprint = hashlib.sha256(json.dumps(fields).encode()).hexdigest()[:8]
        identifier = (namespace, f'w_{scope[:16]}_{name}_{fingerprint}')
        schema = Schema(*(NestedField(i+1, k, types[t][0], required=False) for i,(k,t) in enumerate(fields)))
        arrow_schema = pa.schema([(k, types[t][1]) for k,t in fields])
        try:
            catalog.create_table(identifier, schema=schema, properties={'format-version':'2', 'workbench.scope':scope})
        except TableAlreadyExistsError:
            pass
        marker = 'workbench.run.' + hashlib.sha256(jid.encode()).hexdigest()
        for attempt in range(3):
            table = catalog.load_table(identifier)
            if table.properties.get('workbench.scope') != scope or table.schema() != schema:
                raise ValueError('catalog table scope or schema mismatch')
            if marker in table.properties:
                receipt = json.loads(table.properties[marker])
                if receipt['result_sha256'] != result['sha256'] or receipt['rows'] != descriptor['rows']:
                    raise ValueError('catalog run marker conflicts with committed result')
                break
            transaction = table.transaction()
            total = 0
            for part in descriptor['parts']:
                # Objects.get checks workspace identity and checksum before decoding.
                frame = pq.read_table(io.BytesIO(codec.objects.get(part['ref'])))
                expected_arrow = pa.schema([(k, types[t][1]) for k,t in descriptor['schema'].items()])
                if frame.num_rows != part['rows'] or set(frame.column_names) != set(expected_arrow.names):
                    raise ValueError('partition rows or columns mismatch')
                # Parquet may encode Polars strings as large_string; use declared
                # equivalent Arrow logical types, rejecting unsafe casts.
                frame = frame.select(expected_arrow.names).cast(expected_arrow, safe=True)
                for key,value in metadata.items():
                    frame = frame.append_column(key, pa.array([value]*frame.num_rows, type=arrow_schema.field(key).type))
                transaction.append(frame.cast(arrow_schema), snapshot_properties={'workbench.run_id':jid})
                total += frame.num_rows
            if total != descriptor['rows']: raise ValueError('partition total mismatch')
            if not total:
                transaction.append(pa.Table.from_batches([], schema=arrow_schema), snapshot_properties={'workbench.run_id':jid})
            receipt = dict(table='.'.join(identifier), table_uuid=str(table.metadata.table_uuid),
                snapshot_id=transaction.table_metadata.current_snapshot_id, rows=total, result_sha256=result['sha256'])
            transaction.set_properties({marker: json.dumps(receipt, sort_keys=True)})
            try:
                transaction.commit_transaction()
                if after_commit: after_commit(path)  # Fault-injection seam for commit/receipt recovery.
                break
            except CommitFailedException:
                if attempt == 2: raise
                # Reload marker and recreate the partition iterator on retry.
        receipts[path] = receipt
    delivery = dict(version='iceberg-delivery-1', job_id=jid, revision=job['revision'],
                    result_sha256=result['sha256'], destination=destination, tables=receipts)
    delivery = json.loads(json.dumps(delivery, sort_keys=True))
    ref = codec.dump(delivery)
    repo.record_publication(jid, destination, result, ref)
    return delivery


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('job_id')
    parser.add_argument('--catalog', required=True, help='Configured PyIceberg catalog name; no secrets on command line')
    parser.add_argument('--namespace', default='workbench')
    args = parser.parse_args()
    from pyiceberg.catalog import load_catalog
    db.start()
    try:
        print(json.dumps(publish(db.REPO, db.CODEC, args.job_id, load_catalog(args.catalog), args.namespace), indent=2))
    finally: db.stop()


if __name__ == '__main__': main()
