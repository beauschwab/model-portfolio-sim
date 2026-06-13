"""LMM vol structure: factor loadings, Rebonato abcd, shifted-Black swaption
approximation, surface calibration, deterministic vol-feature paths."""
from __future__ import annotations

import numpy as np
from scipy.optimize import least_squares

from ..core.config import CC_VOL_POINTS, N_FACTORS, N_FWD, N_STEPS, DT, SHIFT, TENOR


def factor_loadings() -> np.ndarray:
    """PCA-reduced exponential-decay correlation, rows unit-normalized."""
    T = (np.arange(N_FWD) + 1) * TENOR
    rho = np.exp(-0.10 * np.abs(T[:, None] - T[None, :]))
    w, V = np.linalg.eigh(rho)
    idx = np.argsort(w)[::-1][:N_FACTORS]
    B = V[:, idx] * np.sqrt(w[idx])[None, :]
    B /= np.linalg.norm(B, axis=1, keepdims=True)
    return np.ascontiguousarray(B)


def abcd(tau, p):
    a, b, c, d = p
    return (a + b * tau) * np.exp(-c * tau) + d


def model_swaption_vol(t, expiry, tenor, p, F0, dfs, B) -> float:
    """Market-lognormal-equivalent ATM vol under shifted dynamics; weights
    frozen at the t=0 curve (Rebonato approximation)."""
    i0 = int(round((t + expiry) / TENOR))
    nq = int(round(tenor / TENOR))
    i1 = min(i0 + nq, N_FWD)
    idx = np.arange(i0, i1)
    P = dfs[idx + 1]
    ann = TENOR * P.sum()
    S0 = (dfs[i0] - dfs[i1]) / ann
    aF = (TENOR * P / ann) * (F0[idx] + SHIFT) / S0
    grid = np.linspace(t, t + expiry, 21)
    tau = np.maximum(idx[None, :] * TENOR - grid[:, None], 1e-6)
    V = np.einsum("gn,n,nk->gk", abcd(tau, p), aF, B[idx])
    return np.sqrt(np.trapezoid((V * V).sum(axis=1), grid) / expiry)


def calibrate_abcd(vol_pts, F0, dfs, B, x0=None, quiet=False) -> np.ndarray:
    """vol_pts rows: (expiry_y, tenor_y, lognormal ATM vol). Warm-startable."""
    def resid(p):
        return np.array([model_swaption_vol(0.0, e, n, p, F0, dfs, B) - v
                         for e, n, v in vol_pts])
    sol = least_squares(
        resid, x0=np.array([0.05, 0.10, 0.50, 0.12]) if x0 is None else x0,
        bounds=([-0.5, -0.5, 0.01, 0.0], [1.0, 1.0, 5.0, 1.0]))
    if not quiet:
        print(f"[cal] abcd = {np.round(sol.x, 4)}  RMSE = "
              f"{np.sqrt(np.mean(sol.fun**2))*1e4:.1f} bp vol")
    return sol.x


def vol_feature_paths(p, F0, dfs, B) -> np.ndarray:
    """Deterministic forward-vol features (6, N_STEPS) for the CC model.
    Limitation: a deterministic-vol LMM has no stochastic implied vol; these
    are time-decay paths off the t=0 curve. SV-LMM needed for vol dynamics."""
    out = np.empty((len(CC_VOL_POINTS), N_STEPS))
    tg = np.arange(N_STEPS) * DT
    for j, (e, nten) in enumerate(CC_VOL_POINTS):
        nq = int(round(nten / TENOR))
        i0 = np.round((tg + e) / TENOR).astype(int)
        valid = i0 + nq < N_FWD
        i0c = np.minimum(i0, N_FWD - nq - 1)
        idx = i0c[:, None] + np.arange(nq)[None, :]
        P = dfs[idx + 1]
        ann = TENOR * P.sum(axis=1)
        S0 = (dfs[i0c] - dfs[i0c + nq]) / ann
        aF = TENOR * P * (F0[idx] + SHIFT) / (ann * S0)[:, None]
        g = tg[:, None] + np.linspace(0.0, e, 21)[None, :]
        tau = np.maximum(idx[:, None, :] * TENOR - g[:, :, None], 1e-6)
        V = np.einsum("tgn,tn,tnk->tgk", abcd(tau, p), aF, B[idx])
        integ = np.trapezoid((V * V).sum(-1), g[0] - g[0, 0], axis=1)
        v = np.sqrt(integ / e)
        v[~valid] = v[valid][-1]
        out[j] = v
    return out
