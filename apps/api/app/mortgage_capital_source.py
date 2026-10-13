"""Unapplied completed-source adapter. No financial calculations or native calls.

The queued server binding is a durable input, not a second mutable repository.
Only the independent mortgage preparation/journal verifier grants the returned
ephemeral source permission. A new capital job supplies its existing check/deadline.
"""
from copy import deepcopy
from dataclasses import dataclass
import io
import json
import math
import time

import polars as pl
from portfolio_risk.analytics.mortgage_mixed_identity import exact_json_identity
from portfolio_risk.analytics.mortgage_mixed_transport import SCHEMAS

SCHEMA = 'mortgage-mixed-accounting-1'
MAX_MANIFEST_BYTES = 32 * 1024 * 1024
MAX_SOURCE_BYTES = 32 * 1024 * 1024
MAX_PARTITION_BYTES = 32 * 1024 * 1024
MAX_PARTITION_ROWS = 65536
MAX_TOTAL_PARTITION_BYTES = 32 * 1024 * 1024
MAX_PARTS = 4096
_SEAL = object()


def _probe(check):
    if check():
        raise InterruptedError('new capital source verification cancelled')


def _exact(actual, expected, message):
    if not exact_json_identity(actual, expected):
        raise ValueError(message)


def _json(data):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError('duplicate persisted source JSON key')
            result[key] = value
        return result

    def constant(value):
        raise ValueError(f'nonfinite persisted source JSON: {value}')

    def floating(value):
        result = float(value)
        if not math.isfinite(result):
            raise ValueError('overflowing persisted source JSON float')
        return result

    return json.loads(data, object_pairs_hook=pairs, parse_constant=constant, parse_float=floating)


def _descriptor(ref, form, limit):
    if (not isinstance(ref, dict) or ref.get('format') != form or
            type(ref.get('bytes')) is not int or not 0 < ref['bytes'] <= limit):
        raise ValueError('invalid bounded completed-source object descriptor')


def _row_binding(row):
    if row['status'] != 'done' or row['kind'] != 'modeled_balance_stress':
        raise ValueError('capital requires a completed modeled mortgage source job')
    if type(row['revision']) is not int or row['revision'] < 0:
        raise ValueError('invalid completed-source revision')
    return {key: deepcopy(row[key]) for key in ('id', 'kind', 'status', 'revision', 'request', 'result')}


def capture_binding(repo, source_job, check):
    """Caller is the server; never accept this binding from an API request body."""
    _probe(check)
    binding = _row_binding(repo.job(source_job))
    _descriptor(binding['request'], 'json', MAX_MANIFEST_BYTES)
    _descriptor(binding['result'], 'json', MAX_MANIFEST_BYTES)
    _probe(check)
    return binding


class StoredFrames:
    def __init__(self, codec, tables, completion, partition_rows, check):
        self.codec, self.check = codec, check
        self.schemas = {name: dict(schema) for name, schema in SCHEMAS.items()}
        self.completion = completion
        self.parts = {}
        total_parts = total_bytes = 0
        if set(tables) != {'/' + name for name in SCHEMAS}:
            raise ValueError('completed-source table coverage differs')
        for name, schema in SCHEMAS.items():
            _probe(check)
            table = tables['/' + name]
            if (table.get('format') != 'partitioned-parquet' or
                    table.get('schema') != {key: str(value) for key, value in schema.items()} or
                    type(table.get('rows')) is not int or table['rows'] < 0 or
                    type(table.get('parts')) is not list):
                raise ValueError('invalid completed-source partition descriptor')
            count = 0
            for part in table['parts']:
                _probe(check)
                if type(part.get('rows')) is not int or not 1 <= part['rows'] <= partition_rows:
                    raise ValueError('invalid completed-source partition rows')
                _descriptor(part.get('ref'), 'parquet', MAX_PARTITION_BYTES)
                count += part['rows']
                total_parts += 1
                total_bytes += part['ref']['bytes']
                if total_parts > MAX_PARTS or total_bytes > MAX_TOTAL_PARTITION_BYTES:
                    raise ValueError('completed-source partitions exceed retained admission')
            if count != table['rows']:
                raise ValueError('completed-source partition total differs')
            self.parts[name] = deepcopy(table['parts'])

    def finish(self):
        _probe(self.check)

    def frames(self, name):
        for part in self.parts[name]:
            _probe(self.check)
            data = self.codec.objects.get(part['ref'])
            _probe(self.check)
            frame = pl.read_parquet(io.BytesIO(data))
            if frame.height != part['rows'] or frame.schema != self.schemas[name]:
                raise ValueError('completed-source persisted partition rows/schema differ')
            _probe(self.check)
            yield frame

    def recheck_objects(self):
        """Read one checksummed body at a time; do not hydrate or repricer."""
        for parts in self.parts.values():
            for part in parts:
                _probe(self.check)
                self.codec.objects.get(part['ref'])
                _probe(self.check)


@dataclass(frozen=True)
class VerifiedSource:
    raw: dict
    prepared: dict
    source_receipt: dict
    frames: StoredFrames
    verification: dict
    provenance: dict
    _seal: object
    _recheck: object

    def recheck(self):
        if self._seal is not _SEAL:
            raise ValueError('unverified completed-source permission')
        self._recheck()


def _verify(raw, header, frames, check):
    from portfolio_risk.analytics.mortgage_mixed_reference import verify_preparation, verify_stream
    control = verify_preparation(raw, header['prepared_request'], header['source_receipt'], check)
    return verify_stream(raw, control, frames, check)


def reopen_verified(repo, codec, binding, check):
    """Reopen under the NEW capital check; never reset or extend its deadline.

    The source's old deadline is finite retained provenance, not this operation's
    runtime deadline. Its original publication must already be complete.
    """
    binding = deepcopy(binding)
    if type(binding) is not dict or set(binding) != {'id', 'kind', 'status', 'revision', 'request', 'result'}:
        raise ValueError('complete server-owned source binding required')

    last_source_read = [float('-inf')]

    def current(force=False):
        _probe(check)
        now = time.monotonic()
        if force or now - last_source_read[0] >= 0.1:
            _exact(_row_binding(repo.job(binding['id'])), binding, 'completed-source binding changed')
            last_source_read[0] = now
        _probe(check)
        return False

    def forced():
        return current(True)

    current(True)
    _descriptor(binding['request'], 'json', MAX_MANIFEST_BYTES)
    _descriptor(binding['result'], 'json', MAX_MANIFEST_BYTES)
    queued = codec.metadata(binding['request'], {'operation', 'args'})
    current(True)
    if (queued.get('operation') != 'store.run_modeled_balance_stress' or
            type(queued.get('args')) not in (list, tuple) or len(queued['args']) != 1 or
            type(queued['args'][0]) is not dict):
        raise ValueError('source queued operation/raw request mismatch')
    request = queued['args'][0]
    raw = request.get('specification')
    if type(raw) is not dict or raw.get('schema') != SCHEMA:
        raise ValueError('completed-source model schema mismatch')
    execution = codec.metadata(binding['result'], {'execution'}).get('execution')
    current(True)
    if type(execution) is not dict or execution.get('model_version') != SCHEMA:
        raise ValueError('completed-source execution model mismatch')
    _exact(execution.get('request_manifest'), binding['request'], 'source queued manifest changed')
    for field in ('binary_sha256', 'validation_library_sha256', 'source_sha256'):
        expected = request.get(field)
        if (type(expected) is not str or len(expected) != 64 or
                any(char not in '0123456789abcdef' for char in expected)):
            raise ValueError('missing completed-source artifact identity')
        _exact(execution.get(field), expected, 'source executed identity differs from its queued input')
    old_deadline = execution.get('publication_deadline_unix')
    if type(old_deadline) not in (int, float) or not math.isfinite(old_deadline):
        raise ValueError('missing completed-source original deadline provenance')
    bound = execution.get('partition_rows')
    if type(bound) is not int or not 1 <= bound <= MAX_PARTITION_ROWS:
        raise ValueError('invalid completed-source partition bound')
    objects = {}
    for field, limit in (('input', 1024 * 1024), ('modeled_source', MAX_SOURCE_BYTES), ('producer_completion', 128)):
        descriptor = execution.get(field)
        _descriptor(descriptor, 'bin', limit)
        current(True)
        objects[field] = _json(codec.objects.get(descriptor))
        current(True)
    envelope = objects['input']
    if (type(envelope) is not dict or set(envelope) != {'request', 'execution_timeout_ms'} or
            type(envelope['execution_timeout_ms']) is not int or not 1 <= envelope['execution_timeout_ms'] <= 1800000):
        raise ValueError('completed-source retained input envelope mismatch')
    _exact(envelope['request'], raw, 'completed-source retained raw input changed')
    _exact(objects['producer_completion'], {'complete': True, 'protocol': 3}, 'source producer did not complete')
    header = objects['modeled_source']
    if (type(header) is not dict or set(header) != {'schema', 'model_schema', 'prepared_request', 'source_receipt'} or
            header['schema'] != 'model-ledger-preparation-1' or header['model_schema'] != SCHEMA):
        raise ValueError('completed-source prepared header mismatch')
    tables = codec.tables(binding['result'])
    current(True)
    frames = StoredFrames(codec, tables, objects['producer_completion'], bound, forced)
    # Private encoded registry survives changes to exposed wrapper dictionaries.
    retained_parts = json.dumps(frames.parts, allow_nan=False, sort_keys=True).encode()
    retained_schemas = {name: {key: str(value) for key, value in schema.items()}
                        for name, schema in frames.schemas.items()}
    retained_objects = codec.objects
    current(True)
    verification = _verify(raw, header, frames, current)
    _exact(verification, execution.get('validation'), 'completed-source independent receipt differs')
    current(True)
    def persisted_recheck():
        # Use the basic job check, never a composite Fence.check/read_current
        # callback that would recursively call this source hook.
        current(True)
        for field in objects:
            _exact(_json(codec.objects.get(execution[field])), objects[field], 'completed-source retained object changed')
            current(True)
        for parts in _json(retained_parts).values():
            for part in parts:
                current(True)
                codec.objects.get(part['ref'])
                current(True)
        current(True)

    # Reread every retained body after verification; descriptors alone cannot
    # detect an overwritten local/S3 partition after its financial oracle read.
    persisted_recheck()
    provenance = dict(binding=binding, source_deadline_unix=old_deadline,
                      source_objects={key: deepcopy(dict(execution[key])) for key in objects})
    snapshots = {name: deepcopy(value) for name, value in dict(raw=raw,
        prepared=header['prepared_request'], source_receipt=header['source_receipt'],
        verification=verification, provenance=provenance).items()}

    def recheck():
        current(True)
        for name, expected in snapshots.items():
            _exact(getattr(verified, name), expected, 'verified source returned snapshot changed')
        if (verified.frames is not frames or frames.codec is not codec or
                codec.objects is not retained_objects or frames.check is not forced):
            raise ValueError('verified source transport authority changed')
        _exact(frames.parts, _json(retained_parts), 'verified source partition registry changed')
        _exact(frames.completion, objects['producer_completion'], 'verified source completion snapshot changed')
        description = {name: {key: str(value) for key, value in schema.items()}
                       for name, schema in frames.schemas.items()}
        _exact(description, retained_schemas, 'verified source schema registry changed')
        persisted_recheck()

    verified = VerifiedSource(deepcopy(raw), deepcopy(header['prepared_request']),
                              deepcopy(header['source_receipt']), frames,
                              deepcopy(verification), provenance, _SEAL, recheck)
    verified.recheck()
    return verified
