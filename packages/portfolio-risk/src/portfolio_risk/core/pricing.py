"""Pricing from the A matrix: PV at any OAS vector, vectorized Newton OAS
solve with bisection bracket. All cheap numpy -- the kernel never re-runs."""
from __future__ import annotations

import numpy as np

from ..core.config import TGRID


def _teff(delay_y):
    """Effective discount times: TGRID plus per-security payment delay
    (years). MBS best practice: stated-delay discounting (FN 24d etc.).
    Applied at the OAS-discounting layer; the pathwise MMA deflator stops
    at month-end (residual: delay discounted at OAS+0 vs OAS+short -- bp-
    level, documented)."""
    if delay_y is None:
        return TGRID[None, :]
    return TGRID[None, :] + np.asarray(delay_y)[:, None]


def pv_from_A(A: np.ndarray, oas: np.ndarray, n_paths: int,
              delay_y=None) -> np.ndarray:
    E = np.exp(-oas[:, None] * _teff(delay_y))
    return (A * E).sum(axis=1) / n_paths


def solve_oas_from_A(A, n_paths, target, tol=1e-8, max_iter=40,
                     delay_y=None, lo0=-0.05, hi0=0.30):
    S = A.shape[0]
    T = _teff(delay_y)
    oas = np.zeros(S)
    lo, hi = np.full(S, lo0), np.full(S, hi0)
    for _ in range(max_iter):
        E = np.exp(-oas[:, None] * T)
        px = (A * E).sum(1) / n_paths
        dpx = -(A * E * T).sum(1) / n_paths
        err = px - target
        lo = np.where(err > 0, np.maximum(lo, oas), lo)
        hi = np.where(err < 0, np.minimum(hi, oas), hi)
        if np.max(np.abs(err)) < tol:
            break
        step = np.where(np.abs(dpx) > 1e-12, -err / dpx, 0.0)
        cand = oas + step
        bad = (cand <= lo) | (cand >= hi) | ~np.isfinite(cand)
        oas = np.where(bad, 0.5 * (lo + hi), cand)
    return oas, px
