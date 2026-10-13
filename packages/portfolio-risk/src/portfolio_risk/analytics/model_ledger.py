"""Native modeled-source preparation, streamed replay and immutable verification."""
from copy import deepcopy
import json
import math
import os
import shutil
from pathlib import Path
import sys
import tempfile
import time
import uuid
from .balance_stream import PartitionWriter,_hash,_rust,_finalize,source_identity
from .dated_ledger import DATED_SCHEMAS,verify_dated_sources
from .credit_path_ledger import PathWriter
from .credit_path_tables import SCHEMAS as CREDIT_SCHEMAS
from .credit_path_verification import verify_sources as verify_credit_sources
from .ledger_native import library_path,validate_spec
from .modeled_json_identity import exact_json_identity
from .mortgage_mixed_transport import SCHEMA as MORTGAGE_SCHEMA,SCHEMAS as MORTGAGE_SCHEMAS,MortgageWriter
from .deposit_revision_transport import SCHEMA as REVISED_DEPOSIT_SCHEMA,SCHEMAS as REVISED_DEPOSIT_SCHEMAS,RevisedDepositWriter,VerificationFrames
from .deposit_continuation_transport import SCHEMA as CONTINUATION_DEPOSIT_SCHEMA,SCHEMAS as CONTINUATION_DEPOSIT_SCHEMAS,ContinuationDepositWriter
from .deposit_portfolio_transport import SCHEMA as PORTFOLIO_DEPOSIT_SCHEMA,WORKFLOW as PORTFOLIO_DEPOSIT_WORKFLOW,SCHEMAS as PORTFOLIO_DEPOSIT_SCHEMAS,PortfolioDepositWriter
from .commercial_ledger_transport import SCHEMA as COMMERCIAL_SCHEMA,WORKFLOW as COMMERCIAL_WORKFLOW,SCHEMAS as COMMERCIAL_SCHEMAS,CommercialWriter
SCHEMAS={PORTFOLIO_DEPOSIT_SCHEMA:PORTFOLIO_DEPOSIT_SCHEMAS,CONTINUATION_DEPOSIT_SCHEMA:CONTINUATION_DEPOSIT_SCHEMAS,REVISED_DEPOSIT_SCHEMA:REVISED_DEPOSIT_SCHEMAS,'graph-dated-ledger-1':DATED_SCHEMAS,'deposit-graph-dated-ledger-1':DATED_SCHEMAS,'deposit-graph-income-ledger-1':DATED_SCHEMAS,'driven-credit-ledger-1':CREDIT_SCHEMAS,MORTGAGE_SCHEMA:MORTGAGE_SCHEMAS}
SCHEMAS[COMMERCIAL_SCHEMA]=COMMERCIAL_SCHEMAS

def binary_path():
    name='portfolio-model-ledger.exe' if sys.platform=='win32' else 'portfolio-model-ledger'
    return Path(__file__).resolve().parents[4]/'portfolio-decision-native'/'target'/'release'/name


def _publish_directory(staging,final,check,*,owned_output_root=None):
    """Keep publication atomic through brief Windows reader/scanner sharing locks."""
    # Only the native producer supplies this flag for its fresh UUID destination.
    # API import happens after return, so these bytes have no external references.
    owned_root=Path(owned_output_root).resolve() if owned_output_root is not None else None
    if owned_root is not None and (final.parent.resolve()!=owned_root or final.exists() or final.is_symlink()):
        raise ValueError('unpublished destination must be new and inside the requested output root')
    retry_until=time.perf_counter()+1.
    while True:
        check()
        if owned_root is not None and (final.exists() or final.is_symlink()):
            raise ValueError('unpublished destination already exists')
        try:
            os.replace(staging,final)
        except PermissionError as error:
            if getattr(error,'winerror',None) not in {5,32,33} or time.perf_counter()>=retry_until:
                raise
            time.sleep(.02)
            continue
        try:
            check()
        except BaseException as error:
            if owned_root is not None:
                try:
                    if final.is_symlink() or final.resolve().parent!=owned_root:
                        raise ValueError('unpublished cleanup target left the requested output root')
                    # Unpublish the manifest first if a subsequent directory
                    # cleanup encounters a filesystem failure.
                    (final/'manifest.json').unlink(missing_ok=True)
                    shutil.rmtree(final)
                except (OSError,ValueError) as cleanup_error:
                    error.add_note(f'unpublished output cleanup failed: {cleanup_error}')
            raise
        return

def run_model_ledger(raw,directory,*,large_book=False,partition_rows=65536,progress=None,cancelled=None,timeout=1800.):
    if raw.get('schema') not in SCHEMAS:raise ValueError('unsupported modeled source ledger schema')
    if type(partition_rows) is not int or not 1<=partition_rows<=65536:raise ValueError('invalid partition bound')
    if not math.isfinite(timeout) or not .001<=timeout<=1800.:raise ValueError('invalid model ledger deadline')
    raw=deepcopy(raw);model=raw['schema'];credit=model=='driven-credit-ledger-1';mortgage=model==MORTGAGE_SCHEMA;revised=model==REVISED_DEPOSIT_SCHEMA;portfolio=model==PORTFOLIO_DEPOSIT_SCHEMA;continuation=model in (CONTINUATION_DEPOSIT_SCHEMA,PORTFOLIO_DEPOSIT_SCHEMA);commercial=model==COMMERCIAL_SCHEMA;strict_model=mortgage or revised or continuation or commercial;deadline=time.perf_counter()+timeout
    envelope=dict(request=raw,execution_timeout_ms=max(1,int(timeout*1000)))
    if len(json.dumps(envelope,allow_nan=False,separators=(',',':')).encode())>(1024*1024 if strict_model else 32*1024*1024):raise ValueError('modeled ledger input exceeds its declared byte bound')
    def check():
        if cancelled and cancelled():raise InterruptedError('modeled ledger cancelled')
        if time.perf_counter()>=deadline:raise TimeoutError('modeled ledger deadline exceeded')
        return False
    check();binary=binary_path();library=library_path()
    if not binary.is_file() or not library.is_file():raise RuntimeError('Rust modeled ledger artifacts require rebuilding; no fallback')
    binary_sha,library_sha,source_sha=_hash(binary),_hash(library),source_identity()
    root=Path(directory).resolve();root.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.partial-',dir=root) as attempt:
        staging=Path(attempt);writer=(CommercialWriter if commercial else PortfolioDepositWriter if portfolio else ContinuationDepositWriter if continuation else RevisedDepositWriter if revised else MortgageWriter if mortgage else PathWriter if credit else PartitionWriter)(staging,partition_rows,schemas=SCHEMAS[model]);header={}
        producer_completion={}
        if continuation:writer.check=check
        _rust(envelope,writer,progress,check,deadline-time.perf_counter(),executable=binary,workflow=header,retain_input=True,
            completion_protocol=6 if commercial else 5 if credit else 3,journal_rows=credit,completion=producer_completion if strict_model else None)
        if set(header)!={'schema','model_schema','prepared_request','source_receipt'} or header['schema']!=(COMMERCIAL_WORKFLOW if commercial else PORTFOLIO_DEPOSIT_WORKFLOW if portfolio else 'model-ledger-preparation-1') or header['model_schema']!=model:
            raise ArithmeticError('native modeled ledger preparation identity differs')
        if strict_model:
            completion_path=staging/'producer-completion.json'
            with completion_path.open('w',encoding='utf-8') as handle:
                json.dump(producer_completion,handle,allow_nan=False,separators=(',',':'));handle.flush();os.fsync(handle.fileno())
            writer.completion=json.loads(completion_path.read_text(encoding='utf-8'))
            if not exact_json_identity(writer.completion,{'complete':True,'protocol':6 if commercial else 3}):raise ArithmeticError('retained native completion differs')
        prepared_path=staging/'prepared-source.json'
        with prepared_path.open('w',encoding='utf-8') as handle:
            json.dump(header,handle,allow_nan=False,separators=(',',':'));handle.flush();os.fsync(handle.fileno())
        # Read retained bytes; both source verification and journal replay use persisted inputs.
        saved=json.loads(prepared_path.read_text(encoding='utf-8'));prepared=saved['prepared_request'];receipt=saved['source_receipt']
        if commercial:
            from .commercial_ledger_reference import verify_preparation,verify_stream
        elif portfolio:
            from .deposit_portfolio_reference import verify_preparation,verify_stream
        elif continuation:
            from .deposit_continuation_reference import verify_preparation,verify_stream
        elif revised:
            from .deposit_revised_income_reference import verify_preparation,verify_stream
        elif mortgage:
            from .mortgage_mixed_reference import verify_preparation,verify_stream
        elif credit:
            from .driven_credit_reference import verify_preparation
        elif model=='deposit-graph-income-ledger-1':
            from .deposit_income_reference import verify_preparation
        elif model=='deposit-graph-dated-ledger-1':
            from .deposit_schedule_reference import verify_preparation
        else:
            from .consumer_schedule_reference import verify_preparation
        source_validation=verify_preparation(raw,prepared,receipt,check)
        spec=None if strict_model else validate_spec(prepared,max_positions=60000 if large_book else 2000,max_work=90000000 if large_book else 3000000)
        writer.finish();check()
        if commercial or revised or continuation:validation=verify_stream(raw,source_validation,VerificationFrames(writer,lambda:json.loads(completion_path.read_text(encoding='utf-8'))),check)
        elif mortgage:validation=verify_stream(raw,source_validation,writer,check)
        elif credit:validation=verify_credit_sources(prepared,spec,writer,check)
        else:
            validation=_finalize(spec,writer,check,native_reports=True)
            validation.update(verify_dated_sources(prepared,spec,writer,check))
        if not strict_model:validation.update(source_validation)
        check()
        if _hash(binary)!=binary_sha or _hash(library)!=library_sha or source_identity()!=source_sha:raise RuntimeError('modeled ledger artifact/source identity changed')
        input_path=staging/'dated-input.json'
        retained_envelope=json.loads(input_path.read_text())
        retained_preparation=json.loads(prepared_path.read_text())
        if (not exact_json_identity(retained_envelope,envelope) if strict_model else retained_envelope!=envelope) or (not exact_json_identity(retained_preparation,saved) if strict_model else retained_preparation!=saved):raise ArithmeticError('immutable modeled inputs changed')
        with input_path.open('r+b') as handle:os.fsync(handle.fileno())
        manifest=dict(version='credit-path-balance-partitions-1' if credit else 'dated-balance-partitions-1',model_version=model,backend='rust',
            source_sha256=source_sha,binary_sha256=binary_sha,validation_library_sha256=library_sha,
            input=dict(path=input_path.name,sha256=_hash(input_path)),specification=envelope,
            modeled_source=dict(path=prepared_path.name,sha256=_hash(prepared_path)),large_book=large_book,partition_rows=partition_rows,validation=validation,
            tables={name:dict(schema={k:str(v) for k,v in schema.items()},parts=writer.parts[name]) for name,schema in SCHEMAS[model].items()})
        if strict_model:manifest['producer_completion']=dict(path=completion_path.name,sha256=_hash(completion_path))
        with (staging/'manifest.json').open('w',encoding='utf-8') as handle:
            json.dump(manifest,handle,allow_nan=False,indent=2);handle.flush();os.fsync(handle.fileno())
        def publish_check():
            check()
            if strict_model:
                current=final if final.exists() else staging
                if (_hash(current/'dated-input.json')!=manifest['input']['sha256'] or
                        _hash(current/'prepared-source.json')!=manifest['modeled_source']['sha256'] or
                        _hash(current/'producer-completion.json')!=manifest['producer_completion']['sha256'] or
                        not exact_json_identity(json.loads((current/'producer-completion.json').read_text(encoding='utf-8')),producer_completion)):
                    raise ArithmeticError('immutable mortgage producer inputs/source/completion changed')

            if continuation or commercial:
                # Selected successor only: bind each already independently
                # replayed immutable partition through the final rename fence.
                current=final if final.exists() else staging
                for name,table in manifest['tables'].items():
                    for part in table['parts']:
                        check()
                        path=(current/part['path']).resolve()
                        if path.parent!=current.resolve() or path.stat().st_size!=part['bytes'] or _hash(path)!=part['sha256']:
                            raise ArithmeticError('performing deposit persisted partition changed after replay')
                        check()

            if _hash(binary)!=binary_sha or _hash(library)!=library_sha or source_identity()!=source_sha:
                raise RuntimeError('modeled ledger artifact/source identity changed before publication')
            check()
        final=root/('run-'+uuid.uuid4().hex);_publish_directory(staging,final,publish_check,owned_output_root=root)
    return final/'manifest.json'
