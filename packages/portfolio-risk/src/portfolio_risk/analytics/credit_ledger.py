"""Native joint daily credit transport with independent immutable publication."""
from copy import deepcopy
import json
import math
import os
from pathlib import Path
import tempfile
import time
import uuid

import polars as pl

from .balance_stream import PartitionWriter, _hash, _rust, binary_path, source_identity
from .ledger_native import library_path, validate_spec
from .credit_verification import verify_credit_sources

MODEL_VERSION = 'credit-ledger-1'
BASE = {**dict.fromkeys(('run_id','scenario','account'),pl.String),'severity':pl.Float64}
DATED = dict(BASE,day=pl.Int64,ordinal=pl.Int64)
POSITION = dict(DATED,position=pl.String)
SCHEMAS = {
    'journal':dict(DATED,transaction_number=pl.Int64,event=pl.String,gl_account=pl.String,instrument_id=pl.String,value=pl.Float64),
    'credit_states':dict(POSITION,contract_id=pl.String,contract_revision=pl.String,probability=pl.List(pl.Float64),
        matured_probability=pl.Float64,**dict.fromkeys(('live_principal','live_undrawn','live_accrued_interest','live_book_adjustment'),pl.List(pl.Float64)),
        **dict.fromkeys(('allowance','default_principal','default_accrued_interest','default_book_adjustment'),pl.Float64)),
    'credit_claims':dict(POSITION,**dict.fromkeys(('claim_id','economic_source_id','economic_source_revision','recovery_review_id','recovery_review_revision'),pl.String),
        default_ordinal=pl.Int64,**dict.fromkeys(('initial_principal','economic_principal_loss','outstanding_principal','outstanding_accrued_interest',
            'outstanding_book_adjustment','principal_charged_off','recovery_cash_collected','recovery_applied_to_principal',
            'remaining_recovery_cash','recovery_tail_after_reporting'),pl.Float64)),
    'credit_lineage':dict(POSITION,**dict.fromkeys(('event_id','event','source_id','source_revision','policy_id','policy_revision','claim_id'),pl.String),
        **dict.fromkeys(('earned_interest','economic_principal_loss','contingent_draw_cash','prepay_cash','recovery_cash'),pl.Float64)),
    'daily_balance':dict(DATED,entity=pl.String,currency=pl.String,**dict.fromkeys(('cash','assets','equity','allowance','reconciliation_error'),pl.Float64)),
    'trial_balance':dict(BASE,gl_account=pl.String,instrument_id=pl.String,balance=pl.Float64),
    'summary':dict(BASE,**dict.fromkeys(('final_cash','final_assets','final_equity','max_reconciliation_error'),pl.Float64)),
}


def run_credit_ledger(raw, directory, *, large_book=False, partition_rows=65536, progress=None, cancelled=None, timeout=1800.):
    """Publish only after native admission and complete independent persisted replay."""
    if raw.get('schema')!=MODEL_VERSION: raise ValueError('unsupported credit ledger request')
    if type(partition_rows) is not int or not 1<=partition_rows<=65536: raise ValueError('invalid credit partition bound')
    if not math.isfinite(timeout) or timeout<=0: raise ValueError('invalid credit timeout')
    raw=deepcopy(raw);deadline=time.perf_counter()+timeout
    def check():
        if cancelled and cancelled(): raise InterruptedError('credit ledger cancelled')
        if time.perf_counter()>=deadline: raise TimeoutError('credit ledger deadline exceeded')
        return False
    check();source_sha=source_identity();binary=binary_path();library=library_path()
    if not binary.is_file() or not library.is_file(): raise RuntimeError('Rust credit artifacts must be rebuilt; no fallback')
    binary_sha,library_sha=_hash(binary),_hash(library)
    spec=validate_spec(raw,max_positions=60000 if large_book else 2000,max_work=90_000_000 if large_book else 3_000_000)
    check();root=Path(directory).resolve();root.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.partial-',dir=root) as attempt:
        staging=Path(attempt);writer=PartitionWriter(staging,partition_rows,schemas=SCHEMAS)
        _rust(raw,writer,progress,check,deadline-time.perf_counter(),completion_protocol=4,retain_input=True,journal_rows=True)
        writer.finish();check();validation=verify_credit_sources(raw,spec,writer,check);check()
        if _hash(binary)!=binary_sha or _hash(library)!=library_sha or source_identity()!=source_sha:
            raise RuntimeError('credit ledger artifact/source identity changed during execution')
        input_path=staging/'dated-input.json'
        if json.loads(input_path.read_text(encoding='utf-8'))!=raw: raise ArithmeticError('immutable credit input differs')
        with input_path.open('r+b') as handle: os.fsync(handle.fileno())
        manifest=dict(version='credit-balance-partitions-1',model_version=MODEL_VERSION,backend='rust',
            source_sha256=source_sha,binary_sha256=binary_sha,validation_library_sha256=library_sha,
            input=dict(path=input_path.name,sha256=_hash(input_path)),specification=raw,large_book=large_book,
            partition_rows=partition_rows,validation=validation,
            tables={name:dict(schema={k:str(v) for k,v in schema.items()},parts=writer.parts[name]) for name,schema in SCHEMAS.items()})
        with (staging/'manifest.json').open('w',encoding='utf-8') as handle:
            json.dump(manifest,handle,allow_nan=False,indent=2);handle.flush();os.fsync(handle.fileno())
        final=root/('run-'+uuid.uuid4().hex);check();os.replace(staging,final)
    return final/'manifest.json'
