"""Experimental daily-batch journal backends with identical output schemas.

Financial events and policies remain in balance_stress. This boundary reduces
ordered balanced postings and checks daily GL state, not product economics.
The columnar Python control shares packing/output costs with Rust. The original
Journal remains the independent reference and production default.
"""
from array import array
import ctypes
import hashlib
import math
import os
from pathlib import Path
import sys
from functools import lru_cache

import numpy as np
import polars as pl

from .journal import SCHEMA


def library_path():
    configured = os.environ.get('PORTFOLIO_LEDGER_RUST_LIB')
    if configured:
        return Path(configured).resolve()
    name = ('portfolio_ledger_native.dll' if sys.platform == 'win32' else
            'libportfolio_ledger_native.dylib' if sys.platform == 'darwin' else 'libportfolio_ledger_native.so')
    return Path(__file__).resolve().parents[4]/'portfolio-ledger-native'/'target'/'release'/name


@lru_cache(maxsize=4)
def _spec_library(path):
    lib=ctypes.CDLL(path)
    invoke=lib.portfolio_balance_validate
    invoke.argtypes=[ctypes.c_char_p,ctypes.c_size_t]
    invoke.restype=ctypes.c_void_p
    lib.portfolio_balance_free.argtypes=[ctypes.c_void_p]
    lib.portfolio_balance_free.restype=None
    return lib


def validate_spec(raw, *, max_positions=2000, max_work=3_000_000):
    """Rust owns defaults, domains, links, work admission and opening reconciliation."""
    import json
    from .balance_stress import Account,Position,NettingSet,Policy,Scenario,Cashflow
    data=json.dumps(dict(raw=raw,max_positions=max_positions,max_work=max_work),allow_nan=False,separators=(',',':')).encode()
    if len(data)>100_000_000:raise ValueError('specification exceeds 100 MB')
    lib=_spec_library(str(library_path()))
    pointer=lib.portfolio_balance_validate(data,len(data))
    if not pointer:raise RuntimeError('native specification allocation failed')
    try:result=json.loads(ctypes.string_at(pointer))
    finally:lib.portfolio_balance_free(pointer)
    if 'error' in result:raise ValueError(result['error'])
    spec=result['ok']
    for key,cls in [('accounts',Account),('positions',Position),('netting_sets',NettingSet),('policies',Policy),('scenarios',Scenario),('cashflows',Cashflow)]:
        spec[key]=[cls(**row) for row in spec[key]]
    return spec


def python_apply(previous, offsets, keys, values, expected):
    """Independent Python reduction of the same batch, with reference tolerances."""
    if (len(previous) != len(expected) or len(keys) != len(values) or
        len(offsets) == 0 or offsets[0] != 0 or offsets[-1] != len(values) or
        np.any(np.diff(offsets) <= 0) or np.any(keys < 0) or np.any(keys >= len(previous)) or
        not all(np.isfinite(v).all() for v in (previous, values, expected))):
        raise ValueError('invalid ledger batch')
    staged = previous.copy()
    for lo, hi in zip(offsets[:-1], offsets[1:]):
        amounts = values[lo:hi]
        scale = sum(abs(float(v)) for v in amounts)
        if not math.isfinite(scale):
            raise ArithmeticError('ledger overflow')
        if abs(math.fsum(amounts)) > 1e-9*max(1., scale):
            raise ArithmeticError('unbalanced journal transaction')
        for j in range(lo, hi):
            staged[keys[j]] += values[j]
    if not np.isfinite(staged).all():
        raise ArithmeticError('ledger overflow')
    if np.any(np.abs(staged-expected) > np.maximum(1e-8, 1e-9*np.maximum(np.abs(staged), np.abs(expected)))):
        raise ArithmeticError('subledger mismatch')
    return staged


class RustLedgerKernel:
    def __init__(self):
        path = library_path()
        if not path.is_file():
            raise RuntimeError('Rust ledger is not built. Run scripts/build_ledger_native.py; no fallback was used.')
        self.identity = 'rust-ledger-abi1-'+hashlib.sha256(path.read_bytes()).hexdigest()
        self._lib = ctypes.CDLL(str(path))
        abi = self._lib.portfolio_ledger_abi_version
        abi.argtypes, abi.restype = [], ctypes.c_uint32
        if abi() != 1:
            raise RuntimeError('unsupported Rust ledger ABI')
        vector = np.ctypeslib.ndpointer(dtype=np.float64, ndim=1, flags=('C_CONTIGUOUS', 'ALIGNED'))
        integer = np.ctypeslib.ndpointer(dtype=np.int64, ndim=1, flags=('C_CONTIGUOUS', 'ALIGNED'))
        output = np.ctypeslib.ndpointer(dtype=np.float64, ndim=1, flags=('C_CONTIGUOUS', 'ALIGNED', 'WRITEABLE'))
        self._apply = self._lib.portfolio_ledger_apply
        self._apply.argtypes = [ctypes.c_size_t]*3 + [vector, integer, integer, vector, vector, output]
        self._apply.restype = ctypes.c_int

    def __call__(self, previous, offsets, keys, values, expected):
        # Own inputs across the GIL-releasing call; caller mutation cannot race it.
        previous, values, expected = [np.array(a, dtype=np.float64, order='C', copy=True) for a in (previous, values, expected)]
        for values_integer in (offsets, keys):
            original = np.asarray(values_integer)
            if original.size and (original.dtype.kind not in 'iu' or
                                  (original.dtype.kind == 'u' and np.any(original > np.iinfo(np.int64).max))):
                raise ValueError('ledger offsets and keys must be signed-range integers')
        offsets, keys = [np.array(a, dtype=np.int64, order='C', copy=True) for a in (offsets, keys)]
        if any(a.ndim != 1 for a in (previous, offsets, keys, values, expected)) or len(previous) != len(expected) or len(keys) != len(values) or not len(offsets):
            raise ValueError('invalid ledger dimensions')
        result = np.empty_like(previous)
        status = self._apply(len(previous), len(offsets)-1, len(values), previous, offsets, keys, values, expected, result)
        if status == 1:
            raise ValueError('invalid native ledger batch')
        if status:
            reason = {2: 'unbalanced transaction', 3: 'subledger mismatch', 4: 'overflow', 5: 'panic'}.get(status, 'unknown')
            raise ArithmeticError(f'Rust ledger {reason} (status {status}); no fallback was used')
        return result


class BatchJournal:
    """Compact journal construction; verify submits one ordered batch per day.

    A failed verify publishes no GL state. The containing simulation must abort
    (the pending event buffer is retained for diagnostics, not silently retried).
    Instances are private to one simulation and must not be shared across threads.
    """
    def __init__(self, scenario, kernel):
        self.scenario, self.kernel = scenario, kernel
        self.ids, self.labels, self.posted = {}, [], set()
        self.keys, self.values, self.offsets = array('q'), array('d'), array('q', [0])
        self.days, self.accounts, self.events = [], [], []
        self.state = np.zeros(0, dtype=np.float64)
        self.flushed = 0

    def _key(self, label):
        code = self.ids.get(label)
        if code is None:
            code = len(self.labels)
            self.ids[label] = code
            self.labels.append(label)
        return code

    def post(self, day, account, event, changes):
        entries = [(key, float(v)) for key, v in changes.items() if v]
        if not entries:
            return
        if not all(math.isfinite(v) for _, v in entries):
            raise ValueError('nonfinite journal entry')
        if self.days and day < self.days[-1]:
            raise ValueError('journal days must be ordered')
        for (gl, instrument), value in entries:
            code = self._key((account, gl, instrument))
            self.posted.add(code)
            self.keys.append(code)
            self.values.append(value)
        self.offsets.append(len(self.keys))
        self.days.append(day)
        self.accounts.append(account)
        self.events.append(event)

    def verify(self, expected):
        for key in expected:
            self._key(key)
        want = np.zeros(len(self.labels))
        for key, value in expected.items():
            want[self.ids[key]] = value
        previous = np.pad(self.state, (0, len(want)-len(self.state)))
        start = self.offsets[self.flushed]
        offsets = np.frombuffer(self.offsets, dtype=np.int64)[self.flushed:].copy()-start
        keys = np.frombuffer(self.keys, dtype=np.int64)[start:]
        values = np.frombuffer(self.values, dtype=np.float64)[start:]
        staged = self.kernel(previous, offsets, keys, values, want)
        self.state = staged
        self.flushed = len(self.days)

    def close(self):
        if self.flushed != len(self.days):
            raise RuntimeError('journal must pass a checkpoint before close')
        keys = np.frombuffer(self.keys, dtype=np.int64)
        values = np.frombuffer(self.values, dtype=np.float64)
        offsets = np.frombuffer(self.offsets, dtype=np.int64)
        replayed = self.kernel(np.zeros(len(self.state)), offsets, keys, values, self.state)
        if not np.allclose(replayed, self.state, rtol=1e-12, atol=1e-9):
            raise ArithmeticError('journal replay failed')
        trial = [dict(scenario=self.scenario, account=a, gl_account=g, instrument_id=i, balance=float(replayed[code]))
                 for (a, g, i), code in sorted(self.ids.items()) if code in self.posted]
        if not len(keys):
            return pl.DataFrame(schema=SCHEMA), trial
        tx = np.repeat(np.arange(len(self.days)), np.diff(offsets))
        frame = pl.DataFrame({
            'transaction_id': pl.Series([f'{self.scenario}:{i+1}' for i in range(len(self.days))]).gather(tx),
            'day': pl.Series(self.days, dtype=pl.Int64).gather(tx),
            'account': pl.Series(self.accounts).gather(tx),
            'event': pl.Series(self.events).gather(tx),
            'gl_account': pl.Series([k[1] for k in self.labels]).gather(keys),
            'instrument_id': pl.Series([k[2] for k in self.labels]).gather(keys),
            'debit': np.maximum(values, 0.), 'credit': np.maximum(-values, 0.),
        }).with_columns(pl.lit(self.scenario).alias('scenario')).select(list(SCHEMA))
        return frame, trial


def journal_factory(backend):
    from .journal import Journal
    if backend == 'python':
        return Journal, 'python-journal-v1'
    if backend == 'columnar':
        return lambda scenario: BatchJournal(scenario, python_apply), 'python-columnar-journal-v1'
    if backend == 'rust':
        kernel = RustLedgerKernel()
        return lambda scenario: BatchJournal(scenario, kernel), kernel.identity
    raise ValueError(f'unknown journal backend: {backend}')
