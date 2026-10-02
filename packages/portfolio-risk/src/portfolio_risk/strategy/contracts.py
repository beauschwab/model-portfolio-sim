"""Prepared coefficient contracts. Typing adds no work to interactive evaluation.

Amounts scale per supplied unit notional. NII and balances are monetary units,
DV01 is monetary units per bp, horizons/grid/purchase dates are months, and
regulatory template weights are explicit research assumptions. Vector rows use
unitlib.METRICS order. Model/source identity is pinned by the durable publisher,
not inferred from the coefficient values. Missing fields in historical libraries
require an explicit rebuild; they are never silently repriced by an evaluator.
"""
from __future__ import annotations

from typing import Any, TypedDict, TYPE_CHECKING
if TYPE_CHECKING:
    import numpy as np


class _RequiredUnitIdentity(TypedDict):
    template: str
    h: int
    side: float


class UnitIdentity(_RequiredUnitIdentity, total=False):
    kind: str


class _RequiredPreparedLibrary(TypedDict):
    units: list[UnitIdentity]
    templates: dict[str, dict[str, Any]]
    grid_m: list[int]
    horizon: int
    nii: np.ndarray
    balance: np.ndarray
    dv01: np.ndarray


class PreparedLibrary(_RequiredPreparedLibrary, total=False):
    runoff: np.ndarray
    cash_interest: np.ndarray
    vectors: dict[tuple[str, int], np.ndarray]
