"""Zero curve bootstrap from par swap rates (annual fixed leg)."""
from __future__ import annotations

import numpy as np
from scipy.optimize import brentq

from .config import N_FWD, TENOR


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
