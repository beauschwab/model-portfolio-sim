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
FEE_SCHEMA = 'commercial-generated-candidate-fee-journal-1'
FEE_WORKFLOW = 'commercial-generated-candidate-fee-preparation-1'
SELECTED_SCHEMAS = frozenset((SCHEMA, FEE_SCHEMA))
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


FEE_BOOK = {**dict.fromkeys(('scenario','position_id','fee_id','native_fee_snapshot_json'),pl.String),
    'day':pl.Int64,'path_key':pl.UInt64,**dict.fromkeys(('source_weight','allocation_factor'),pl.Float64)}
FEE_TABLES = dict(commercial_fee_lineage={**dict.fromkeys(('scenario','position_id','native_fee_group_json'),pl.String),
    **dict.fromkeys(('day','native_group_index','transaction_number'),pl.Int64),'path_key':pl.UInt64,
    **dict.fromkeys(('native_year','source_weight','allocation_factor'),pl.Float64)},
    commercial_fee_daily=FEE_BOOK, commercial_fee_closing=FEE_BOOK)
FEE_SCHEMAS = dict(SCHEMAS, **FEE_TABLES)


def selected(schema):
    if schema == SCHEMA: return WORKFLOW, 6, SCHEMAS, 'commercial_generated_complete', 'analyze_commercial_generated_candidate_journal'
    if schema == FEE_SCHEMA: return FEE_WORKFLOW, 7, FEE_SCHEMAS, 'commercial_generated_fee_complete', 'analyze_commercial_generated_candidate_fee_journal'
    raise ValueError('explicit generated candidate schema required')


class GeneratedWriter(CommercialWriter):
    """Flush metadata immediately and bound simultaneous buffered table bytes."""
    def __init__(self, root, partition_rows, *, check, model=SCHEMA):
        super().__init__(root, partition_rows, schemas=selected(model)[2])
        self.check = check
        self.serialized_bytes = 0
        self.object_count = 0

    def add(self, name, values):
        self.check()
        if name in FEE_TABLES:
            import math
            if name not in self.schemas: raise ValueError('unselected fee table')
            rows=[]
            field='native_fee_group' if name=='commercial_fee_lineage' else 'native_fee_snapshot'
            for value in values:
                row=dict(value)
                if type(row.get(field)) is not dict: raise ValueError('generated fee object required')
                row[field+'_json']=json.dumps(row.pop(field),allow_nan=False,separators=(',',':'))
                if set(row)!=set(self.schemas[name]): raise ValueError('generated fee table shape differs')
                for key,dtype in self.schemas[name].items():
                    item=row[key]
                    if item is None and key=='transaction_number': continue
                    if dtype in (pl.Int64,pl.UInt64):
                        if type(item) is not int or not (0<=item<1<<64 if dtype==pl.UInt64 else -(1<<63)<=item<1<<63): raise ValueError('generated fee integer identity required')
                    elif dtype==pl.Float64:
                        if type(item) not in (int,float) or not math.isfinite(item): raise ValueError('generated fee finite money required')
                    elif type(item) is not str: raise ValueError('generated fee text required')
                rows.append(row)
            PartitionWriter.add(self,name,rows)
            if sum(frame.estimated_size() for frames in self.buffers.values() for frame in frames)>=BUFFER_BYTES: self.finish()
            self.check()
            return
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
    model=header.get('model_schema')
    _, protocol, _, record, _=selected(model)
    fields = ('record complete schema protocol physical_paths maximum_live_full_physical_owners '
        'full_end_ordinal independent_persisted_verification opening_identity_verified '
        'mixed_ledger_adoption empirical_validation').split()
    if model==FEE_SCHEMA:fields.insert(6,'maximum_live_full_fee_owners')
    if (set(trailer) != set(fields) or trailer['record'] != record
            or trailer['complete'] is not True or trailer['schema'] != model
            or type(trailer['protocol']) is not int or trailer['protocol'] != protocol):
        raise ValueError('generated producer trailer identity differs')
    receipt = header['source_receipt']
    for key in ('physical_paths', 'maximum_live_full_physical_owners', 'full_end_ordinal'):
        if type(trailer[key]) is not int or trailer[key] != receipt[key]:
            raise ValueError('generated completion preparation differs')
    if model==FEE_SCHEMA and (type(trailer['maximum_live_full_fee_owners']) is not int or trailer['maximum_live_full_fee_owners']!=1 or receipt.get('maximum_live_full_fee_owners')!=1):
        raise ValueError('generated full fee owner coverage differs')
    if trailer['physical_paths'] < 128 or trailer['maximum_live_full_physical_owners'] != 1:
        raise ValueError('generated native owner path coverage differs')
    if any(trailer[key] is not False for key in fields[-4:]):
        raise ValueError('generated producer cannot grant verification or adoption')
