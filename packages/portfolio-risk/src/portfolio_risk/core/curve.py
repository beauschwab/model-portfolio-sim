"""Zero curve bootstrap from par swap rates (annual fixed leg)."""
from __future__ import annotations

import numpy as np
from scipy.optimize import brentq

from ..core.config import N_FWD, TENOR


def bootstrap_curve(tenors: np.ndarray, rates: np.ndarray) -> np.ndarray:
    """Sequential log-DF bootstrap, brentq per pillar, log-linear DF interp.
    Returns discount factors on the quarterly grid (N_FWD+1,), flat-zero
    extrapolated beyond the last pillar."""
    kt, kl = [0.0], [0.0]
    for T, r in zip(tenors, rates):
        pay = np.arange(1.0, T + 0.5)

        def resid(lnd):
            d = np.exp(np.interp(pay, kt + [T], kl + [lnd]))
            return r * d.sum() + d[-1] - 1.0

        kl.append(brentq(resid, -5.0, 0.5))
        kt.append(T)
    kt, kl = np.array(kt), np.array(kl)
    grid = np.arange(N_FWD + 1) * TENOR
    z_last = -kl[-1] / kt[-1]
    lng = np.where(grid <= kt[-1], np.interp(grid, kt, kl), -z_last * grid)
    return np.exp(lng)


def forwards_from_dfs(dfs: np.ndarray) -> np.ndarray:
    return (dfs[:-1] / dfs[1:] - 1.0) / TENOR


def market_discount_factors_to_par(times, discounts) -> dict:
    """Project dated market DFs onto the engine's annual-payment par grid.

    ``times`` are ACT/365F years from the source valuation date. Source DFs
    already incorporate the provider's market conventions. Re-express them
    as annual unit-accrual par rates; do not mislabel vendor par quotes as
    engine quotes. Sparse-pillar reconstruction and >30y extrapolation remain
    model approximations. Negative rates are supported (DFs need not decrease).
    """
    from .config import SWAP_TENORS, SHIFT
    t, d = np.asarray(times, dtype=float), np.asarray(discounts, dtype=float)
    if (t.ndim != 1 or d.shape != t.shape or len(t) < 2
            or not np.all(np.isfinite(t)) or not np.all(np.isfinite(d))
            or np.any(np.diff(t) <= 0) or np.any(d <= 0)
            or t[0] != 0 or abs(d[0] - 1) > 1e-10 or t[-1] < 30):
        raise ValueError("discount curve requires increasing times from zero to >=30y, positive DFs and D(0)=1")
    annual = np.exp(np.interp(np.arange(1, 31), t, np.log(d)))
    rates = np.array([(1 - annual[int(T) - 1]) / annual[:int(T)].sum()
                      for T in SWAP_TENORS])
    projected = bootstrap_curve(SWAP_TENORS, rates)
    if np.any(forwards_from_dfs(projected) <= -SHIFT):
        raise ValueError("discount curve falls outside the shifted LMM forward-rate domain")
    grid = np.arange(1, 121) * TENOR
    target = np.exp(np.interp(grid, t, np.log(d)))
    return {"swap_rates": rates.tolist(), "swap_tenors": SWAP_TENORS.tolist(),
            "max_df_error_30y": float(np.max(np.abs(projected[1:121] - target))),
            "max_zero_error_bp_30y": float(np.max(np.abs(
                np.log(projected[1:121] / target) / grid)) * 1e4),
            "convention": "ACT/365F source times; log-DF interpolation; annual unit-accrual model par rates",
            "classification": "derived"}
