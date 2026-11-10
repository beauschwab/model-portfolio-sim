"""Native generated candidate stream with immutable independently replayed output."""
from copy import deepcopy
import json
import math
import os
from pathlib import Path
import tempfile
import time
import uuid

from .balance_stream import _hash, _rust, source_identity
from .bounded_native_records import loads
from .commercial_generated_transport import (SCHEMA, WORKFLOW, SCHEMAS, RAW_BYTES,
    RECORD_BYTES, MANIFEST_BYTES, SERIALIZED_BYTES, TRAILER_BYTES, GeneratedWriter,
    VerificationFrames, validate_trailer, selected)
from .ledger_native import library_path
from .modeled_json_identity import exact_json_identity as exact


def _save(path, value, maximum):
    data = json.dumps(value, allow_nan=False, separators=(',', ':')).encode()
    if len(data) > maximum: raise ValueError('generated metadata byte bound')
    with path.open('wb') as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())
    return {'path': path.name, 'sha256': _hash(path)}


def run_generated_candidate_ledger(raw, directory, *, large_book=False, partition_rows=65536,
        progress=None, cancelled=None, timeout=1800., executable, publish_directory):
    # This is a distinct generated tier. large_book never raises its admission.
    started = time.perf_counter()
    if (type(timeout) not in (int, float) or not math.isfinite(timeout)
            or not .001 <= timeout <= 1800.):
        raise ValueError('invalid generated candidate deadline')
    deadline = started + timeout
    if type(partition_rows) is not int or not 1 <= partition_rows <= 65536:
        raise ValueError('invalid generated partition bound')
    model=raw.get('schema')
    workflow, protocol, schemas, _, operation=selected(model)
    raw = deepcopy(raw)
    envelope = dict(op=operation, schema=model,
        request=raw, execution_timeout_ms=max(1, int(timeout * 1000)))
    if len(json.dumps(envelope, allow_nan=False, separators=(',', ':')).encode()) > RAW_BYTES:
        raise ValueError('generated candidate raw input byte bound')

    def check():
        if cancelled and cancelled(): raise InterruptedError('generated candidate cancelled')
        if time.perf_counter() >= deadline: raise TimeoutError('generated candidate deadline exceeded')
        return False

    check()
    binary, library = executable, library_path()
    if not binary.is_file() or not library.is_file():
        raise RuntimeError('generated candidate native artifacts require rebuilding; no fallback')
    binary_sha, library_sha, source_sha = _hash(binary), _hash(library), source_identity()
    root = Path(directory).resolve()
    root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.partial-', dir=root) as attempt:
        staging = Path(attempt)
        writer = GeneratedWriter(staging, partition_rows, check=check, model=model)
        header, completion, trailer = {}, {}, {}
        _rust(envelope, writer, progress, check, deadline - time.perf_counter(),
            executable=binary, workflow=header, completion_protocol=protocol, retain_input=True,
            completion=completion, completion_trailer=trailer, decode_record=loads,
            record_bytes=RECORD_BYTES)
        if (set(header) != {'schema', 'model_schema', 'prepared_request', 'source_receipt'}
                or header['schema'] != workflow or header['model_schema'] != model):
            raise ArithmeticError('generated preparation header identity differs')
        if not exact(completion, {'complete': True, 'protocol': protocol}):
            raise ArithmeticError('generated native Sim completion differs')
        validate_trailer(header, trailer)
        references = {
            'modeled_source': _save(staging/'prepared-source.json', header, RECORD_BYTES),
            'producer_completion': _save(staging/'producer-completion.json', completion, 128),
            'producer_trailer': _save(staging/'producer-trailer.json', trailer, TRAILER_BYTES),
        }
        input_path = staging/'dated-input.json'
        if not exact(loads(input_path.read_bytes()), envelope):
            raise ArithmeticError('generated immutable raw input differs')
        with input_path.open('r+b') as output: os.fsync(output.fileno())
        references['input'] = {'path': input_path.name, 'sha256': _hash(input_path)}
        writer.finish()
        writer.admit_persisted()
        from .commercial_generated_candidate_reference import verify_preparation, verify_stream
        saved_header = loads((staging/'prepared-source.json').read_bytes())
        source = verify_preparation(raw, saved_header['prepared_request'], saved_header['source_receipt'], check)
        frames = VerificationFrames(writer,
            lambda: loads((staging/'producer-completion.json').read_bytes()),
            lambda: loads((staging/'producer-trailer.json').read_bytes()))
        validation = verify_stream(raw, source, frames, check)
        check()
        manifest = dict(version='dated-balance-partitions-1', model_version=model, backend='rust',
            binary_sha256=binary_sha, validation_library_sha256=library_sha, source_sha256=source_sha,
            specification=envelope, large_book=large_book, partition_rows=partition_rows,
            validation=validation, **references,
            tables={name: dict(schema={key: str(dtype) for key, dtype in schema.items()},
                parts=writer.parts[name]) for name, schema in schemas.items()})
        metadata_bytes = sum((staging/ref['path']).stat().st_size for ref in references.values())
        if metadata_bytes + writer.serialized_bytes > SERIALIZED_BYTES:
            raise ValueError('generated aggregate serialized phase byte bound')
        _save(staging/'manifest.json', manifest, MANIFEST_BYTES)
        if metadata_bytes + writer.serialized_bytes + (staging/'manifest.json').stat().st_size > SERIALIZED_BYTES:
            raise ValueError('generated aggregate serialized phase byte bound')
        manifest_sha = _hash(staging/'manifest.json')
        final = root/('run-'+uuid.uuid4().hex)

        def publish_check():
            check()
            current = final if final.exists() else staging
            if _hash(current/'manifest.json') != manifest_sha:
                raise ArithmeticError('generated manifest changed after replay')
            for ref in references.values():
                check()
                if _hash(current/ref['path']) != ref['sha256']:
                    raise ArithmeticError('generated retained source/completion changed after replay')
            for table in manifest['tables'].values():
                for part in table['parts']:
                    check()
                    path = (current/part['path']).resolve()
                    if (path.parent != current.resolve() or path.stat().st_size != part['bytes']
                            or _hash(path) != part['sha256']):
                        raise ArithmeticError('generated persisted partition changed after replay')
            if (_hash(binary) != binary_sha or _hash(library) != library_sha
                    or source_identity() != source_sha):
                raise RuntimeError('generated native artifact/source identity changed')
            check()

        publish_directory(staging, final, publish_check, owned_output_root=root)
    return final/'manifest.json'
