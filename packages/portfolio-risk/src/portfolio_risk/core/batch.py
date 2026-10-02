"""Application-owned numerical boundary for prepared cashflow pricing.

Values are discounted cashflow PATH SUMS per unit original principal; times
and OAS are years and continuous decimal spreads. Offsets identify instruments.
No product objects, Python callbacks, RNG, calibration or scenario state need
to cross a future native boundary. Backends must return one finite PV per row.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol

import numpy as np


@dataclass(frozen=True)
class CashflowBatch:
    offsets: np.ndarray
    times: np.ndarray
    values: np.ndarray
    n_paths: int

    def __post_init__(self):
        offsets = np.asarray(self.offsets)
        times, values = np.asarray(self.times), np.asarray(self.values)
        if (offsets.ndim != 1 or not np.issubdtype(offsets.dtype, np.integer)
                or len(offsets) < 1 or offsets[0] != 0
                or times.ndim != 1 or values.ndim != 1
                or offsets[-1] != len(values) or len(times) != len(values)
                or np.any(offsets < 0) or np.any(offsets > len(values))
                or np.any(offsets[1:] <= offsets[:-1])):
            raise ValueError("invalid CSR cashflow dimensions or empty instrument")
        if (not isinstance(self.n_paths, (int, np.integer)) or self.n_paths < 1
                or not np.isfinite(times).all() or not np.isfinite(values).all()
                or (times < 0).any()):
            raise ValueError("cashflows require finite values, nonnegative times and positive paths")
        for name, value, dtype in (("offsets", offsets, np.int64), ("times", times, np.float64),
                                   ("values", values, np.float64)):
            value = np.asarray(value, dtype=dtype)
            object.__setattr__(self, name, np.frombuffer(value.tobytes(), dtype=dtype))

    @classmethod
    def from_rows(cls, rows, n_paths):
        """Rows are (path_sum_values, times) pairs, in requested output order."""
        rows = list(rows)
        offsets = np.cumsum([0] + [len(row[0]) for row in rows], dtype=np.int64)
        values = np.concatenate([row[0] for row in rows]) if rows else np.empty(0)
        times = np.concatenate([row[1] for row in rows]) if rows else np.empty(0)
        return cls(offsets, times, values, n_paths)


class DiscountBackend(Protocol):
    # Change this identity whenever a backend's numerical implementation changes.
    identity: str

    def price(self, batch: CashflowBatch, oas: np.ndarray) -> np.ndarray: ...


class NumpyDiscountBackend:
    identity = "numpy-csr-v1"

    def price(self, batch, oas):
        oas = np.asarray(oas, dtype=np.float64)
        if oas.shape != (len(batch.offsets) - 1,) or not np.isfinite(oas).all():
            raise ValueError("one finite decimal OAS is required per instrument")
        if not len(oas):
            return np.empty(0)
        with np.errstate(over="raise", invalid="raise"):
            amounts = batch.values * np.exp(-np.repeat(oas, np.diff(batch.offsets)) * batch.times)
            result = np.add.reduceat(amounts, batch.offsets[:-1]) / batch.n_paths
        if not np.isfinite(result).all():
            raise ValueError("backend produced a nonfinite price")
        return result
