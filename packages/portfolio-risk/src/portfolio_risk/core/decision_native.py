"""Bounded transport for the owned native decision ABI; no financial formulas."""
from functools import lru_cache
import ctypes
import hashlib
import json
import os
from pathlib import Path


_DECISION_NAME = ('portfolio_decision_native.dll' if os.name == 'nt'
    else 'libportfolio_decision_native.dylib' if __import__('sys').platform == 'darwin' else 'libportfolio_decision_native.so')
_DECISION_DEFAULT = Path(__file__).resolve().parents[4] / 'portfolio-decision-native/target/release' / _DECISION_NAME


def _file_stamp(path):
    stat=path.stat()
    return stat.st_size,stat.st_mtime_ns,stat.st_ino


@lru_cache(maxsize=4)
def _load_decision(path):
    source=Path(path);stamp=_file_stamp(source)
    lib=ctypes.CDLL(path)
    identity='decision-abi-v1:'+hashlib.sha256(source.read_bytes()).hexdigest()
    if _file_stamp(source)!=stamp:
        raise RuntimeError('MODEL_VERSION_CHANGED: native decision binary changed; restart worker')
    lib.decision_request.argtypes=[ctypes.c_char_p]
    lib.decision_request.restype=ctypes.c_void_p
    lib.decision_free.argtypes=[ctypes.c_void_p]
    lib.decision_free.restype=None
    return lib,identity,stamp


class NativeDecision:
    """Owned JSON ABI. No fallback, borrowed pointers, or per-instrument FFI calls."""
    def __init__(self, path=None):
        configured=path or os.environ.get('PORTFOLIO_DECISION_LIBRARY')
        path=Path(configured).resolve() if configured else _DECISION_DEFAULT
        if not path.is_file():
            raise RuntimeError('Rust decision library is not built. Run scripts/build_decision_native.py with cmake and libclang.')
        self.lib,self.identity,stamp = _load_decision(str(path))
        if _file_stamp(path)!=stamp:
            raise RuntimeError('MODEL_VERSION_CHANGED: native decision binary changed; restart worker')
        self.bytes_sent = 0

    def call(self, **request):
        import datetime
        def encode(value):
            if isinstance(value, datetime.date): return value.toordinal()
            raise TypeError(f'Unsupported decision input {type(value).__name__}')
        payload = json.dumps(request, default=encode, allow_nan=False, separators=(',', ':')).encode()
        if len(payload) > 128 * 1024 * 1024:
            raise ValueError('native request exceeds 128 MiB')
        self.bytes_sent += len(payload)
        pointer = self.lib.decision_request(payload)
        if not pointer:
            raise RuntimeError('native decision returned a null response')
        try:
            result = json.loads(ctypes.string_at(pointer))
        finally:
            self.lib.decision_free(pointer)
        if 'error' in result:
            raise ValueError(result['error'])
        return result['ok']


