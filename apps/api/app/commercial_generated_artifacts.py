"""Storage-only importer for the explicit generated commercial partition tier."""
import hashlib
import io

import polars as pl

from portfolio_risk.analytics.bounded_native_records import loads
from portfolio_risk.analytics.commercial_generated_transport import (SCHEMA, WORKFLOW, SCHEMAS,
    RAW_BYTES, RECORD_BYTES, MANIFEST_BYTES, SERIALIZED_BYTES, DECODED_BYTES, MAX_OBJECTS,
    TRAILER_BYTES, validate_trailer, selected)
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity as exact
from portfolio_risk.analytics._deposit_continuation_partition_bounds import parquet_metadata

SOURCE_FILES = {'input': ('dated-input.json', RAW_BYTES),
    'modeled_source': ('prepared-source.json', RECORD_BYTES),
    'producer_completion': ('producer-completion.json', 128),
    'producer_trailer': ('producer-trailer.json', TRAILER_BYTES)}


def validate_sources(envelope, header, completion, trailer):
    model=envelope.get('schema')
    workflow,protocol,_,_,operation=selected(model)
    if (set(envelope) != {'op', 'schema', 'request', 'execution_timeout_ms'}
            or envelope['op'] != operation
            or envelope['request'].get('schema') != model
            or type(envelope['execution_timeout_ms']) is not int
            or not 1 <= envelope['execution_timeout_ms'] <= 1800000):
        raise ValueError('generated immutable input envelope differs')
    if (set(header) != {'schema', 'model_schema', 'prepared_request', 'source_receipt'}
            or header['schema'] != workflow or header['model_schema'] != model
            or not exact(header['source_receipt'].get('original_request'), envelope['request'])):
        raise ValueError('generated immutable preparation differs')
    if not exact(completion, {'complete': True, 'protocol': protocol}):
        raise ValueError('generated native Sim completion differs')
    validate_trailer(header, trailer)


def import_partitions(codec, manifest_path, cancelled, *, object_reference, table_factory, read_file):
    from pathlib import Path
    path = Path(manifest_path).resolve()
    manifest_bytes = read_file(path, MANIFEST_BYTES, cancelled)
    manifest = loads(manifest_bytes)
    model=manifest.get('model_version')
    _,_,schemas,_,_=selected(model)
    if (manifest.get('version') != 'dated-balance-partitions-1'
            or manifest.get('backend') != 'rust'
            or set(manifest.get('tables', {})) != set(schemas)):
        raise ValueError('generated partition manifest selection differs')
    if any(manifest.get('validation', {}).get(key) is not True
            for key in ('verified', 'journal_replayed', 'persisted_partitions_replayed')):
        raise ValueError('unverified generated candidate partitions')
    partition_rows = manifest.get('partition_rows')
    if type(partition_rows) is not int or not 1 <= partition_rows <= 65536:
        raise ValueError('generated partition row bound')
    retained, source_bytes = {}, {}
    serialized = len(manifest_bytes)
    for key, (name, maximum) in SOURCE_FILES.items():
        descriptor = manifest[key]
        file = (path.parent/descriptor['path']).resolve()
        if file.parent != path.parent or file.name != name:
            raise ValueError('generated source path differs')
        data = read_file(file, maximum, cancelled)
        if hashlib.sha256(data).hexdigest() != descriptor['sha256']:
            raise ValueError('generated source checksum differs')
        serialized += len(data)
        retained[key], source_bytes[key] = loads(data), data
    validate_sources(*(retained[key] for key in SOURCE_FILES))
    if not exact(retained['input'], manifest.get('specification')):
        raise ValueError('generated retained input identity differs')
    result = {}
    decoded = logical = count = 0
    for name, schema in schemas.items():
        table, parts = manifest['tables'][name], []
        if (table.get('schema') != {key: str(dtype) for key, dtype in schema.items()}
                or type(table.get('parts')) is not list):
            raise ValueError('generated declared table schema differs')
        for part in table['parts']:
            if (type(part.get('rows')) is not int or not 1 <= part['rows'] <= partition_rows
                    or type(part.get('bytes')) is not int or not 0 < part['bytes'] <= RECORD_BYTES):
                raise ValueError('generated partition declared bounds')
            serialized += part['bytes']; count += 1
            if serialized > SERIALIZED_BYTES or count + len(SOURCE_FILES) + 2 > MAX_OBJECTS:
                raise ValueError('generated serialized phase byte/object bound')
            file = (path.parent/part['path']).resolve()
            if file.parent != path.parent or file.suffix != '.parquet':
                raise ValueError('generated partition path differs')
            data = read_file(file, part['bytes'], cancelled)
            if len(data) != part['bytes'] or hashlib.sha256(data).hexdigest() != part['sha256']:
                raise ValueError('generated partition checksum differs')
            def check():
                if cancelled(): raise InterruptedError('generated partition import cancelled')
            decoded, logical = parquet_metadata(data, part['rows'], table['schema'],
                decoded=decoded, logical=logical, check=check,
                decoded_limit=DECODED_BYTES, logical_limit=DECODED_BYTES)
            frame = pl.read_parquet(io.BytesIO(data), n_rows=part['rows'] + 1)
            if frame.height != part['rows'] or frame.schema != schema:
                raise ValueError('generated persisted partition schema/rows differ')
            check()
            parts.append({'rows': part['rows'], 'ref': codec.objects.put(data, 'parquet')})
        result[name] = table_factory(table['schema'], parts)
    execution = {key: value for key, value in manifest.items() if key != 'tables'}
    for key, data in source_bytes.items():
        execution[key] = object_reference(codec.objects.put(data, 'bin'))
    result['execution'] = execution
    return result
