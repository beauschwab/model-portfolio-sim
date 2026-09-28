"""Explicit per-run forecast configuration.

Context variables isolate callers/threads; no compiled constants are mutated.
Forecast path counts and behavioral assumptions follow the captured API inputs.
"""
from __future__ import annotations

from collections import OrderedDict
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass, field

import numpy as np


@dataclass(frozen=True)
class RunConfig:
    n_paths: int = 128
    n_paths_base: int = 512
    horizon: int = 27
    deposit_segments: dict | None = None
    cd_ew_params: tuple | None = None

    def __post_init__(self):
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
