"""Component-model interfaces and registry.

Every econometric component the engine consumes is swappable:

  kind        protocol            default implementation
  ----        --------            ----------------------
  cc          CCModel             TrendingCC   (partial-adjustment OLS)
  ps          SpreadModel         OUSpread     (AR(1)-fit OU)
  hpi         HPIModel            RateLinkedHPI
  prepay      prepay step fn      None -> fast open-coded kernels

Register alternatives with @register("kind", "name") and assemble a
ModelSuite to pass into build_paths / run_risk / run_stress.

PREPAY SWAP MECHANICS (read before writing a custom model):
The default prepay model is open-coded inside kernels.engine /
kernels.stress_engine for speed. A custom model supplies a numba-jitted
step function with EXACTLY this signature:

    @njit(inline="always", fastmath=True)
    def step(bal, burn_f, q, mtg_pm, hpi_pm, yoy_pm, season_m,
             wac_s, net12, r, la, ofh, sm, pp, knots, coefs,
             smm_lut, smm_scale, burn_lut, burn_scale):
        ...
        return cf, bal, burn_f, q       # state-forwarding tuple

and the engine specialization is built by kernels.make_generic_engine(step)
(cached per function). MEASURED COST of the generic path: ~25-30% slower
than the fast kernels (tuple pack/unpack across the call boundary defeats
LLVM register allocation) -- acceptable for research models; promote a
winning model into the open-coded MODEL-BLOCKs for production speed and
add it to the zero-shock invariant test.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable, Protocol, runtime_checkable

import numpy as np
import polars as pl

REGISTRY: dict[str, dict[str, type]] = {"cc": {}, "ps": {}, "hpi": {},
                                        "prepay": {}, "exercise": {},
                                        "deposit_rate": {}, "attrition": {}}


def register(kind: str, name: str):
    def deco(cls):
        REGISTRY.setdefault(kind, {})[name] = cls
        cls.registry_name = name
        return cls
    return deco


@runtime_checkable
class CCModel(Protocol):
    def fit(self, hist: pl.DataFrame) -> dict: ...
    def paths(self, swaps: np.ndarray, volfeat: np.ndarray,
              params: dict) -> np.ndarray: ...
    def shock_response(self, params: dict, d: float, k: np.ndarray
                       ) -> np.ndarray:
        """Deterministic CC response template to a forward-starting parallel
        shock d, k = months since shock. Required for stress shortcuts."""
        ...


@runtime_checkable
class SpreadModel(Protocol):
    def fit(self, hist: pl.DataFrame) -> dict: ...
    def paths(self, params: dict, spot: float, eps: np.ndarray
              ) -> np.ndarray: ...


@runtime_checkable
class HPIModel(Protocol):
    def paths(self, s10: np.ndarray, eps: np.ndarray) -> np.ndarray: ...
    def shock_multiplier(self, d: float, k: np.ndarray) -> np.ndarray: ...


@dataclass
class ModelSuite:
    """The full swappable component set. prepay_step=None selects the fast
    open-coded kernels; a jitted step function selects the generic engine."""
    cc: CCModel
    ps: SpreadModel
    hpi: HPIModel
    prepay_step: Callable | None = None
    extras: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def default(cls) -> "ModelSuite":
        from .models import OUSpread, RateLinkedHPI, TrendingCC
        return cls(cc=TrendingCC(), ps=OUSpread(), hpi=RateLinkedHPI())
