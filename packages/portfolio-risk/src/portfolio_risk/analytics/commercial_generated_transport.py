"""Bounded storage layout for explicitly selected generated commercial journals.

These are transport phase limits, separate from native graph/owner admission.
They do not grant financial authority or promise an aggregate process RSS cap.
"""
import json

import polars as pl

from .balance_stream import PartitionWriter, _hash
from .commercial_ledger_transport import CommercialWriter, SCHEMAS as LEDGER_SCHEMAS

SCHEMA = 'commercial-generated-candidate-journal-1'
WORKFLOW = 'commercial-generated-candidate-preparation-1'
RAW_BYTES = 1024 * 1024
RECORD_BYTES = 32 * 1024 * 1024
MANIFEST_BYTES = 16 * 1024 * 1024
SERIALIZED_BYTES = 512 * 1024 * 1024
DECODED_BYTES = 2 * 1024 * 1024 * 1024
MAX_OBJECTS = 65536
BUFFER_BYTES = 8 * 1024 * 1024
TRAILER_BYTES = 8192
METADATA_TABLES = frozenset('commercial_candidate_units commercial_candidate_graphs '
    'commercial_candidate_native_sources commercial_candidate_original_sources '
    'commercial_candidate_original_acquisition_proofs'.split())
METADATA_SCHEMA = {'unit_index': pl.Int64, 'source_index': pl.Int64, 'native_json': pl.String}
SCHEMAS = dict(LEDGER_SCHEMAS, **{name: METADATA_SCHEMA for name in METADATA_TABLES})


class GeneratedWriter(CommercialWriter):
    """Flush metadata immediately and bound simultaneous buffered table bytes."""
    def __init__(self, root, partition_rows, *, check):
        super().__init__(root, partition_rows, schemas=SCHEMAS)
        self.check = check
        self.serialized_bytes = 0
        self.object_count = 0

    def add(self, name, values):
        self.check()
        if name in METADATA_TABLES:
            for raw in values:
                if set(raw) != set(METADATA_SCHEMA):
                    raise ValueError('generated metadata columns differ')
                for key in ('unit_index', 'source_index'):
                    if type(raw[key]) is not int or not 0 <= raw[key] < 1 << 63:
                        raise ValueError('generated metadata integer identity required')
                if type(raw['native_json']) is not dict:
                    raise ValueError('generated metadata native object required')
                encoded = json.dumps(raw['native_json'], allow_nan=False, separators=(',', ':'))
                if len(encoded.encode()) > RECORD_BYTES:
                    raise ValueError('generated metadata record byte bound')
                # One source is written before accepting the next path. The
                # generic ledger writer must not stringify this JSON again.
                PartitionWriter.add(self, name, [dict(raw, native_json=encoded)])
                self.flush(name)
            return
        super().add(name, values)
        if sum(frame.estimated_size() for frames in self.buffers.values() for frame in frames) >= BUFFER_BYTES:
            self.finish()
        self.check()

    def flush(self, name):
        self.check()
        first = len(self.parts[name])
        super().flush(name)
        for part in self.parts[name][first:]:
            if part['bytes'] > RECORD_BYTES:
                raise ValueError('generated partition byte bound')
            self.serialized_bytes += part['bytes']
            self.object_count += 1
            if self.serialized_bytes > SERIALIZED_BYTES or self.object_count + 6 > MAX_OBJECTS:
                raise ValueError('generated serialized phase byte/object bound')
        self.check()

    def admit_persisted(self):
        """Inspect every partition before decoding any financial replay table."""
        from ._deposit_continuation_partition_bounds import parquet_metadata
        decoded = logical = 0
        for name, schema in self.schemas.items():
            for part in self.parts[name]:
                self.check()
                path = self.root/part['path']
                if path.stat().st_size != part['bytes'] or part['bytes'] > RECORD_BYTES:
                    raise ValueError('generated persisted partition byte identity differs')
                with path.open('rb') as source: data = source.read(RECORD_BYTES + 1)
                if len(data) != part['bytes'] or _hash(path) != part['sha256']:
                    raise ValueError('generated persisted partition checksum differs')
                decoded, logical = parquet_metadata(data, part['rows'],
                    {key: str(dtype) for key, dtype in schema.items()},
                    decoded=decoded, logical=logical, check=self.check,
                    decoded_limit=DECODED_BYTES, logical_limit=DECODED_BYTES)
                self.check()


class VerificationFrames:
    """Reread persisted completion records; never confer producer authority."""
    def __init__(self, frames, completion, trailer):
        self._frames, self._completion, self._trailer = frames, completion, trailer
        self.schemas = {name: {key: str(dtype) for key, dtype in schema.items()}
                        for name, schema in frames.schemas.items()}
    @property
    def completion(self): return self._completion()
    @property
    def trailer(self): return self._trailer()
    def frames(self, name): return self._frames.frames(name)


def validate_trailer(header, trailer):
    fields = ('record complete schema protocol physical_paths maximum_live_full_physical_owners '
        'full_end_ordinal independent_persisted_verification opening_identity_verified '
        'mixed_ledger_adoption empirical_validation').split()
    if (set(trailer) != set(fields) or trailer['record'] != 'commercial_generated_complete'
            or trailer['complete'] is not True or trailer['schema'] != SCHEMA
            or type(trailer['protocol']) is not int or trailer['protocol'] != 6):
        raise ValueError('generated producer trailer identity differs')
    receipt = header['source_receipt']
    for key in ('physical_paths', 'maximum_live_full_physical_owners', 'full_end_ordinal'):
        if type(trailer[key]) is not int or trailer[key] != receipt[key]:
            raise ValueError('generated completion preparation differs')
    if trailer['physical_paths'] < 128 or trailer['maximum_live_full_physical_owners'] != 1:
        raise ValueError('generated native owner path coverage differs')
    if any(trailer[key] is not False for key in fields[-4:]):
        raise ValueError('generated producer cannot grant verification or adoption')
