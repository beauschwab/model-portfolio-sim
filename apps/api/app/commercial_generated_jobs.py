"""Independent persisted generated-source replay before the final SQL guard."""
from copy import deepcopy
import io
import math
import time

import polars as pl

from portfolio_risk.analytics.bounded_native_records import loads
from portfolio_risk.analytics.commercial_generated_transport import (SCHEMA, SCHEMAS,
    RECORD_BYTES, SERIALIZED_BYTES, DECODED_BYTES, MAX_OBJECTS, VerificationFrames)
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity as exact
from portfolio_risk.analytics._deposit_continuation_partition_bounds import parquet_metadata
from .commercial_generated_artifacts import SOURCE_FILES, validate_sources


def verify_persisted(request, result, *, job, stopping, expected_deadline):
    from . import persistence as db
    from portfolio_risk.analytics import model_ledger, ledger_native
    from portfolio_risk.analytics.balance_stream import _hash, source_identity
    execution, raw = result['execution'], request['specification']
    deadline = request.get('publication_deadline_unix')
    if (raw.get('schema') != SCHEMA or execution.get('model_version') != SCHEMA
            or type(deadline) is not float or not math.isfinite(deadline)
            or not exact(deadline, expected_deadline)
            or not exact(execution.get('publication_deadline_unix'), deadline)
            or job is None or not exact(execution.get('request_manifest'), job['request'])):
        raise ValueError('generated persisted selection/deadline/queued authority differs')
    bound_job = deepcopy({key: job[key] for key in ('id', 'owner', 'token')})
    last_read = [float('-inf')]

    def check(force=False):
        if stopping(): raise InterruptedError('generated publication cancelled')
        if time.time() >= deadline: raise TimeoutError('generated original publication deadline exceeded')
        if force or time.monotonic() - last_read[0] >= .1:
            row = db.REPO.job(bound_job['id'])
            if (row['status'] != 'running' or row['owner'] != bound_job['owner']
                    or row['token'] != bound_job['token'] or not db.REPO.owns(bound_job['owner'])):
                raise InterruptedError('generated publication ownership lost')
            last_read[0] = time.monotonic()
        return False

    def identities():
        check(True)
        for path, key in ((model_ledger.binary_path(), 'binary_sha256'),
                          (ledger_native.library_path(), 'validation_library_sha256')):
            if (not path.is_file() or _hash(path) != request[key]
                    or execution.get(key) != request[key]):
                raise RuntimeError('MODEL_VERSION_CHANGED: generated native artifact differs')
            check(True)
        if source_identity() != request['source_sha256'] or execution.get('source_sha256') != request['source_sha256']:
            raise RuntimeError('MODEL_VERSION_CHANGED: generated source differs')
        check(True)

    def get(descriptor, maximum):
        check(True)
        if (descriptor.get('format') != 'bin' or type(descriptor.get('bytes')) is not int
                or not 0 < descriptor['bytes'] <= maximum):
            raise ValueError('generated bounded source descriptor differs')
        data = db.CODEC.objects.get_bounded(descriptor, maximum)
        check(True)
        return loads(data)

    identities()
    saved = {key: get(execution[key], maximum) for key, (_, maximum) in SOURCE_FILES.items()}
    validate_sources(*(saved[key] for key in SOURCE_FILES))
    if not exact(saved['input']['request'], raw):
        raise ValueError('generated uploaded raw source differs from queued input')
    if set(result) != set(SCHEMAS) | {'execution', 'revision'}:
        raise ValueError('generated persisted table inventory differs')
    partition_rows = execution.get('partition_rows')
    if type(partition_rows) is not int or not 1 <= partition_rows <= 65536:
        raise ValueError('generated persisted partition row bound')

    class StoredFrames:
        def __init__(self):
            self.schemas, self.parts = SCHEMAS, {}
            count = len(SOURCE_FILES) + 2
            serialized = sum(execution[key]['bytes'] for key in SOURCE_FILES)
            for name, schema in SCHEMAS.items():
                table = result[name]
                if (table.get('format') != 'partitioned-parquet'
                        or table.get('schema') != {key: str(dtype) for key, dtype in schema.items()}
                        or type(table.get('parts')) is not list or type(table.get('rows')) is not int
                        or table['rows'] < 0):
                    raise ValueError('generated persisted table descriptor differs')
                rows = 0
                for part in table['parts']:
                    check()
                    descriptor = part['ref']; size = descriptor.get('bytes')
                    if (type(part.get('rows')) is not int or not 1 <= part['rows'] <= partition_rows
                            or descriptor.get('format') != 'parquet' or type(size) is not int
                            or not 0 < size <= RECORD_BYTES):
                        raise ValueError('generated persisted partition descriptor differs')
                    count += 1; serialized += size; rows += part['rows']
                    if count > MAX_OBJECTS or serialized > SERIALIZED_BYTES:
                        raise ValueError('generated persisted serialized phase byte/object bound')
                if rows != table['rows']: raise ValueError('generated persisted table row coverage differs')
                self.parts[name] = deepcopy(table['parts'])
        def admit(self):
            decoded = logical = 0
            for name, parts in self.parts.items():
                for part in parts:
                    check(True)
                    data = db.CODEC.objects.get_bounded(part['ref'], RECORD_BYTES)
                    decoded, logical = parquet_metadata(data, part['rows'], result[name]['schema'],
                        decoded=decoded, logical=logical, check=check,
                        decoded_limit=DECODED_BYTES, logical_limit=DECODED_BYTES)
                    check(True)
        def frames(self, name):
            decoded = logical = 0
            for part in self.parts[name]:
                check(True)
                data = db.CODEC.objects.get_bounded(part['ref'], RECORD_BYTES)
                decoded, logical = parquet_metadata(data, part['rows'], result[name]['schema'],
                    decoded=decoded, logical=logical, check=check,
                    decoded_limit=DECODED_BYTES, logical_limit=DECODED_BYTES)
                frame = pl.read_parquet(io.BytesIO(data), n_rows=part['rows'] + 1)
                if frame.height != part['rows'] or frame.schema != SCHEMAS[name]:
                    raise ValueError('generated uploaded partition schema/rows differ')
                check(True)
                yield frame

    from portfolio_risk.analytics.commercial_generated_candidate_reference import verify_preparation, verify_stream
    header = saved['modeled_source']
    source = verify_preparation(raw, header['prepared_request'], header['source_receipt'], check)
    stored = StoredFrames()
    stored.admit()
    frames = VerificationFrames(stored,
        lambda: get(execution['producer_completion'], 128),
        lambda: get(execution['producer_trailer'], SOURCE_FILES['producer_trailer'][1]))
    validation = verify_stream(raw, source, frames, check)
    if not exact(validation, execution.get('validation')):
        raise ValueError('generated independently replayed receipt differs')
    identities()
