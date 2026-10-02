"""Optional Rust C-ABI backend and fused Numba control, behind CashflowBatch.

Rust uses borrowed contiguous inputs: no serialization or native-side input
copies. Python retains buffer owners across the synchronous ctypes call, which
releases the GIL. No native fallback: a requested unavailable backend is an error.
"""
import ctypes
import hashlib
import os
from pathlib import Path
import sys

import numpy as np
from numba import njit, prange, get_num_threads


def spreads(batch, oas):
    values = np.array(oas, dtype=np.float64, order='C', copy=True)
    if values.shape != (len(batch.offsets) - 1,) or not np.isfinite(values).all():
        raise ValueError('one finite decimal OAS is required per instrument')
    return values


@njit(parallel=True, cache=True)
def _numba_price(offsets, times, values, oas, paths):
    out = np.empty(len(oas))
    for i in prange(len(oas)):
        total = 0.
        for j in range(offsets[i], offsets[i+1]):
            total += values[j] * np.exp(-oas[i] * times[j])
        out[i] = total / paths
    return out


class NumbaDiscountBackend:
    identity = 'numba-csr-v1'

    def price(self, batch, oas):
        result = _numba_price(batch.offsets, batch.times, batch.values, spreads(batch, oas), batch.n_paths)
        if not np.isfinite(result).all():
            raise ValueError('backend produced a nonfinite price')
        return result


_LIBRARY_NAME = ('portfolio_risk_native.dll' if sys.platform == 'win32' else
                 'libportfolio_risk_native.dylib' if sys.platform == 'darwin' else 'libportfolio_risk_native.so')
_DEFAULT_LIBRARY_PATH = Path(__file__).resolve().parents[4] / 'portfolio-risk-native' / 'target' / 'release' / _LIBRARY_NAME


def library_path():
    configured = os.environ.get('PORTFOLIO_RISK_RUST_LIB')
    if configured:
        return Path(configured).resolve()
    return _DEFAULT_LIBRARY_PATH


class RustDiscountBackend:
    def __init__(self):
        path = library_path()
        if not path.is_file():
            raise RuntimeError('Rust backend is not built. Run scripts/build_native.py or set PORTFOLIO_RISK_RUST_LIB.')
        self.identity = 'rust-csr-abi1-' + hashlib.sha256(path.read_bytes()).hexdigest()[:16]
        self._lib = ctypes.CDLL(str(path))
        abi = self._lib.portfolio_risk_abi_version
        abi.argtypes, abi.restype = [], ctypes.c_uint32
        if abi() != 1:
            raise RuntimeError('unsupported Rust pricing ABI')
        self._price = self._lib.portfolio_risk_price
        vector = np.ctypeslib.ndpointer(dtype=np.float64, ndim=1, flags=('C_CONTIGUOUS', 'ALIGNED'))
        offsets = np.ctypeslib.ndpointer(dtype=np.int64, ndim=1, flags=('C_CONTIGUOUS', 'ALIGNED'))
        output = np.ctypeslib.ndpointer(dtype=np.float64, ndim=1, flags=('C_CONTIGUOUS', 'ALIGNED', 'WRITEABLE'))
        self._price.argtypes = [ctypes.c_size_t, ctypes.c_size_t, offsets, vector, vector, vector,
                               ctypes.c_size_t, ctypes.c_size_t, output]
        self._price.restype = ctypes.c_int

    def price(self, batch, oas):
        oas = spreads(batch, oas)
        out = np.empty(len(oas), dtype=np.float64)
        status = self._price(len(oas), len(batch.values), batch.offsets, batch.times,
                             batch.values, oas, batch.n_paths, get_num_threads(), out)
        if status:
            raise ValueError(f'Rust pricing failed (status {status}); no fallback was used')
        return out


def discount_backend(name):
    from .batch import NumpyDiscountBackend
    if name == 'numpy':
        return NumpyDiscountBackend()
    if name == 'numba':
        return NumbaDiscountBackend()
    if name == 'rust':
        return RustDiscountBackend()
    raise ValueError(f'unknown pricing backend: {name}')
