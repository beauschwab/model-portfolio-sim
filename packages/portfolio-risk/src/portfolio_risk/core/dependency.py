"""Bounded content-addressed calculation nodes, with batched cache misses.

Dependencies are represented by parent keys, not mutable dirty flags. Old job
snapshots can populate their own keys without publishing over newer inputs.
Only immutable numerical results belong here; never cache live application state.
"""
from __future__ import annotations

from collections import OrderedDict
from datetime import date, datetime
import hashlib
import json
import sys
from threading import RLock
from types import MappingProxyType
from collections.abc import Mapping

import numpy as np
import polars as pl


def _json(value):
    if isinstance(value, np.ndarray):
        return {"dtype": value.dtype.str, "shape": value.shape, "data": value.tolist()}
    if isinstance(value, np.generic):
        return value.item()
    if isinstance(value, (date, datetime)):
        return value.isoformat()
    if isinstance(value, pl.DataFrame):
        return {"schema": [(k, str(v)) for k, v in value.schema.items()], "rows": value.to_dicts()}
    raise TypeError(f"unsupported dependency input: {type(value).__name__}")


def fingerprint(*inputs) -> str:
    """Stable identity for explicit inputs; nonfinite floats fail closed."""
    encoded = json.dumps(inputs, default=_json, sort_keys=True, allow_nan=False,
                         separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def _freeze(value):
    if isinstance(value, np.ndarray):
        if value.dtype.hasobject:
            raise TypeError("object arrays cannot be cached")
        # A bytes owner prevents callers from re-enabling array writeability.
        return np.frombuffer(value.tobytes(), dtype=value.dtype).reshape(value.shape)
    if isinstance(value, Mapping):
        return MappingProxyType({k: _freeze(v) for k, v in value.items()})
    if isinstance(value, (tuple, list)):
        return tuple(_freeze(v) for v in value)
    if isinstance(value, np.generic):
        return value.item()
    if value is None or isinstance(value, (str, bytes, int, float, bool)):
        return value
    raise TypeError(f"unsupported cached result: {type(value).__name__}")


def _size(value):
    if isinstance(value, np.ndarray):
        return sys.getsizeof(value) + value.nbytes
    if isinstance(value, Mapping):
        return sys.getsizeof(value) + sum(_size(k) + _size(v) for k, v in value.items())
    if isinstance(value, tuple):
        return sys.getsizeof(value) + sum(_size(v) for v in value)
    return sys.getsizeof(value)


class DependencyCache:
    """Process-local segmented LRU, bounded by total bytes and node count.

    Computation is serialized by the reentrant lock. This matches the API's
    single quant worker and permits nested parent resolution without races.
    The budget excludes temporary batches, output frames and in-flight values.
    Shared paths/fits have a bounded protected tier within the same budget.
    Instrument results can borrow its unused space, but cannot evict its nodes.
    """

    _SHARED_STAGES = frozenset({'market_fit', 'mortgage_fit', 'deposit_fit',
                               'mortgage_paths', 'rate_paths', 'deposit_paths', 'auxiliary'})

    def __init__(self, max_bytes=128 * 1024 * 1024, max_entries=50_000):
        if max_bytes < 0 or max_entries < 0:
            raise ValueError("cache limits must be nonnegative")
        self.max_bytes = max_bytes
        self.max_entries = max_entries
        self.bytes = 0
        self._entries = OrderedDict()
        self._shared = OrderedDict()
        self._shared_bytes = 0
        self.shared_max_bytes = min(64 * 1024 * 1024, max_bytes // 4)
        self.shared_max_entries = min(1024, max_entries // 4)
        self._lock = RLock()
        self._native = None

    def clear(self):
        with self._lock:
            if self._native is not None:
                self._native.call("clear")
            self._entries.clear()
            self._shared.clear()
            self.bytes = 0
            self._shared_bytes = 0

    def info(self):
        with self._lock:
            if self._native is not None:
                return self._native.call("info")
            return {"entries": len(self._entries) + len(self._shared), "bytes": self.bytes,
                    "max_bytes": self.max_bytes, "max_entries": self.max_entries,
                    "shared_entries": len(self._shared), "shared_bytes": self._shared_bytes,
                    "shared_max_bytes": self.shared_max_bytes,
                    "shared_max_entries": self.shared_max_entries}

    def _get(self, key):
        for tier in (self._shared, self._entries):
            entry = tier.get(key)
            if entry is not None:
                tier.move_to_end(key)
                return entry
        return None

    def _evict(self, shared=False):
        _, (_, size) = (self._shared if shared else self._entries).popitem(last=False)
        self.bytes -= size
        if shared:
            self._shared_bytes -= size

    def _put(self, key, value, size):
        # Caller holds the reentrant cache lock. Oversized shared nodes use the
        # ordinary tier so even tiny caches retain their previous behavior.
        if not self.max_entries or size > self.max_bytes:
            return
        for shared, tier in ((True, self._shared), (False, self._entries)):
            old = tier.pop(key, None)
            if old is not None:
                self.bytes -= old[1]
                if shared:
                    self._shared_bytes -= old[1]
        protected = (key[0] in self._SHARED_STAGES and self.shared_max_entries > 0
                     and size <= self.shared_max_bytes)
        if not protected and size + self._shared_bytes > self.max_bytes:
            return
        if protected:
            while self._shared and (self._shared_bytes + size > self.shared_max_bytes
                                    or len(self._shared) >= self.shared_max_entries):
                self._evict(shared=True)
        while (self.bytes + size > self.max_bytes
               or len(self._entries) + len(self._shared) >= self.max_entries):
            if self._entries:
                self._evict()
            elif protected:
                self._evict(shared=True)
            else:
                # An ordinary large node must not displace shared dependencies.
                return
        (self._shared if protected else self._entries)[key] = (value, size)
        self.bytes += size
        if protected:
            self._shared_bytes += size


class Evaluation:
    """One request's node evaluation and actual hit/computation counts."""

    def __init__(self, cache: DependencyCache):
        self.cache = cache
        self.stats = {}

    def batch(self, stage, keys, build):
        """Call build(missing_indices) once; preserve caller order on return."""
        cache = self.cache
        stats = self.stats.setdefault(stage, {"reused": 0, "computed": 0, "batches": 0})
        with cache._lock:
            output = [None] * len(keys)
            missing = []
            for i, key in enumerate(keys):
                entry = cache._get((stage, key))
                if entry is None:
                    missing.append(i)
                else:
                    output[i] = entry[0]
                    stats["reused"] += 1
            if missing:
                values = list(build(missing))
                if len(values) != len(missing):
                    raise ValueError("batch backend returned the wrong number of results")
                frozen = [_freeze(value) for value in values]
                # Validate the whole result before admitting any node in this batch.
                stats["computed"] += len(missing)
                stats["batches"] += 1
                for i, value in zip(missing, frozen):
                    output[i] = value
                    key = (stage, keys[i])
                    size = _size(value) + _size(key) + 128
                    cache._put(key, value, size)
            return output

    def one(self, stage, key, build):
        return self.batch(stage, [key], lambda _: [build()])[0]
