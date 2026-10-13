"""Protocol 5 raw transport and independently verified immutable publication."""
from copy import deepcopy
import json
import math
import os
from pathlib import Path
import tempfile
import time
import uuid

import polars as pl

from .balance_stream import PartitionWriter,_hash,_rust,binary_path,source_identity
from .ledger_native import library_path,validate_spec

from .credit_path_tables import MODEL_VERSION,SCHEMAS,JSON_FIELDS


class PathWriter(PartitionWriter):
    """Canonical JSON cells retain variable native source structures without inference."""
    def add(self,table,rows):
        if table in JSON_FIELDS:
            rows=[{**row,**{key:json.dumps(row[key],allow_nan=False,sort_keys=True,separators=(',',':'))
                for key in JSON_FIELDS[table]}} for row in rows]
        super().add(table,rows)


def run_credit_path_ledger(raw,directory,*,large_book=False,partition_rows=65536,progress=None,cancelled=None,timeout=1800.):
    """Publish after every persisted source, ordered posting and closing row verifies."""
    from .credit_path_verification import verify_sources
    if raw.get('schema')!=MODEL_VERSION:raise ValueError('unsupported realized credit ledger request')
    if type(partition_rows) is not int or not 1<=partition_rows<=65536:raise ValueError('invalid credit path partition bound')
    if not math.isfinite(timeout) or timeout<=0:raise ValueError('invalid credit path timeout')
    raw=deepcopy(raw);deadline=time.perf_counter()+timeout
    def check():
        if cancelled and cancelled():raise InterruptedError('credit path ledger cancelled')
        if time.perf_counter()>=deadline:raise TimeoutError('credit path ledger deadline exceeded')
        return False
    check();source_sha=source_identity();binary=binary_path();library=library_path()
    if not binary.is_file() or not library.is_file():raise RuntimeError('Rust credit path artifacts require rebuilding; no fallback')
    binary_sha,library_sha=_hash(binary),_hash(library)
    spec=validate_spec(raw,max_positions=60000 if large_book else 2000,max_work=90_000_000 if large_book else 3_000_000)
    check();root=Path(directory).resolve();root.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.partial-',dir=root) as attempt:
        staging=Path(attempt);writer=PathWriter(staging,partition_rows,schemas=SCHEMAS)
        _rust(raw,writer,progress,check,deadline-time.perf_counter(),completion_protocol=5,retain_input=True,journal_rows=True)
        writer.finish();check();validation=verify_sources(raw,spec,writer,check);check()
        if _hash(binary)!=binary_sha or _hash(library)!=library_sha or source_identity()!=source_sha:
            raise RuntimeError('credit path artifact/source identity changed during execution')
        input_path=staging/'dated-input.json'
        if json.loads(input_path.read_text(encoding='utf-8'))!=raw:raise ArithmeticError('immutable realized credit input differs')
        with input_path.open('r+b') as handle:os.fsync(handle.fileno())
        manifest=dict(version='credit-path-balance-partitions-1',model_version=MODEL_VERSION,backend='rust',
            source_sha256=source_sha,binary_sha256=binary_sha,validation_library_sha256=library_sha,
            input=dict(path=input_path.name,sha256=_hash(input_path)),specification=raw,large_book=large_book,
            partition_rows=partition_rows,validation=validation,
            tables={name:dict(schema={k:str(v) for k,v in schema.items()},parts=writer.parts[name]) for name,schema in SCHEMAS.items()})
        with (staging/'manifest.json').open('w',encoding='utf-8') as handle:
            json.dump(manifest,handle,allow_nan=False,indent=2);handle.flush();os.fsync(handle.fileno())
        final=root/('run-'+uuid.uuid4().hex);check();os.replace(staging,final)
    return final/'manifest.json'
