"""Explicit per-run configuration and bounded, content-keyed computation reuse.

Context variables isolate callers/threads; no compiled constants are mutated.
Only pure market calculations are cached, never portfolio-dependent results.
"""
from __future__ import annotations

from collections import OrderedDict
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass, field
from functools import wraps
import hashlib
import inspect

import numpy as np


@dataclass(frozen=True)
class RunConfig:
    n_paths: int = 128
    n_paths_base: int = 512
    horizon: int = 27
    deposit_segments: dict | None = None
    cd_ew_params: tuple | None = None
    compute_backend: str = 'rust'

    def __post_init__(self):
        if self.compute_backend not in ('python', 'rust'):
            raise ValueError('compute_backend must be python or rust')
        if min(self.n_paths, self.n_paths_base) < 1:
            raise ValueError("path counts must be positive")
        if not 1 <= self.horizon < 360:
            raise ValueError("horizon must be between 1 and 359 months")


@dataclass
class RunContext:
    config: RunConfig
    cache: OrderedDict = field(default_factory=OrderedDict)
    bytes: int = 0
    max_bytes: int = 128 * 1024 * 1024
    hits: int = 0
    misses: int = 0


_CURRENT: ContextVar[RunContext | None] = ContextVar("quant_run", default=None)


@contextmanager
def run_context(config: RunConfig | None = None):
    context = RunContext(config or RunConfig())
    token = _CURRENT.set(context)
    try:
        yield context
    finally:
        _CURRENT.reset(token)


def path_count(default: int, *, base: bool = False) -> int:
    ctx = _CURRENT.get()
    return (ctx.config.n_paths_base if base else ctx.config.n_paths) if ctx else default


def stress_horizons(default):
    ctx = _CURRENT.get()
    return np.arange(1, ctx.config.horizon + 1, dtype=np.int64) if ctx else default


def assumption(name, default):
    ctx = _CURRENT.get()
    value = getattr(ctx.config, name, None) if ctx else None
    return default if value is None else value


def _key(value):
    if isinstance(value, np.ndarray):
        a = np.ascontiguousarray(value)
        return (a.shape, a.dtype.str, hashlib.blake2b(a.view(np.uint8), digest_size=16).digest())
    if isinstance(value, (tuple, list)):
        return tuple(_key(v) for v in value)
    return value


def _size(value):
    if isinstance(value, np.ndarray):
        return value.nbytes
    if isinstance(value, (tuple, list)):
        return sum(_size(v) for v in value)
    return 0


def _readonly(value):
    if isinstance(value, np.ndarray):
        value.setflags(write=False)
    elif isinstance(value, (tuple, list)):
        for v in value:
            _readonly(v)


def run_cached(fn):
    """Cache a pure ndarray calculation within the explicitly installed run."""
    signature = inspect.signature(fn)

    @wraps(fn)
    def wrapped(*args, **kwargs):
        ctx = _CURRENT.get()
        if ctx is None:
            return fn(*args, **kwargs)
        bound = signature.bind(*args, **kwargs)
        bound.apply_defaults()
        key = (fn.__module__, fn.__name__, tuple(
            (k, _key(v)) for k, v in bound.arguments.items() if k != "quiet"))
        if key in ctx.cache:
            ctx.hits += 1
            ctx.cache.move_to_end(key)
            return ctx.cache[key]
        ctx.misses += 1
        result = fn(*args, **kwargs)
        size = _size(result)
        if size <= ctx.max_bytes:
            while ctx.cache and ctx.bytes + size > ctx.max_bytes:
                _, old = ctx.cache.popitem(last=False)
                ctx.bytes -= _size(old)
            _readonly(result)
            ctx.cache[key] = result
            ctx.bytes += size
        return result

    return wrapped
