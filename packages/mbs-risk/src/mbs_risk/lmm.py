"""Shifted-lognormal LIBOR/SOFR market model, spot measure, monthly log-Euler.
Drift via the factorized running-sum trick: O(n_fwd * k) per step."""
from __future__ import annotations

import numpy as np
from numba import njit, prange

from .config import DT, N_FWD, N_STEPS, SHIFT, TENOR
from .vol import abcd


@njit(inline="always", fastmath=True)
def _par_rate(F, eta, n_q):
    P, ann = 1.0, 0.0
    end = min(eta + n_q, N_FWD)
    for j in range(eta, end):
        P /= (1.0 + TENOR * F[j])
        ann += TENOR * P
    return (1.0 - P) / ann


@njit(parallel=True, fastmath=True, cache=True)
def lmm_simulate(F0, sig_tab, B, Z, shift, df_out, swaps_out, short_out):
    """X = F + shift is lognormal. Outputs MMA deflators (P,T) and pathwise
    par swap rates at (2,5,10,30)y tenors (P,4,T)."""
    n_paths = Z.shape[0]
    n_fac = B.shape[1]
    sq = np.sqrt(DT)
    for p in prange(n_paths):
        F = F0.copy()
        df = 1.0
        for m in range(N_STEPS):
            t = m * DT
            eta = int(t / TENOR + 1e-9)
            swaps_out[p, 0, m] = _par_rate(F, eta, 8)
            swaps_out[p, 1, m] = _par_rate(F, eta, 20)
            swaps_out[p, 2, m] = _par_rate(F, eta, 40)
            swaps_out[p, 3, m] = _par_rate(F, eta, 120)
            short_out[p, m] = F[eta]
            df /= (1.0 + F[eta] * DT)
            df_out[p, m] = df
            S = np.zeros(n_fac)
            for i in range(eta, N_FWD):
                si = sig_tab[m, i]
                X = F[i] + shift
                w = TENOR * X * si / (1.0 + TENOR * F[i])
                mu = 0.0
                dW = 0.0
                for k in range(n_fac):
                    S[k] += w * B[i, k]
                    mu += B[i, k] * S[k]
                    dW += B[i, k] * Z[p, m, k]
                mu *= si
                F[i] = X * np.exp((mu - 0.5 * si * si) * DT
                                  + si * sq * dW) - shift


def simulate_rates(F0, abcd_p, B, Z):
    """-> (mma deflators (P,T), swaps (P,4,T), short 3m rate (P,T))."""
    tg = np.arange(N_STEPS) * DT
    Ti = (np.arange(N_FWD) + 1) * TENOR
    sig_tab = abcd(np.maximum(Ti[None, :] - tg[:, None], 1e-6), abcd_p)
    P = Z.shape[0]
    df = np.empty((P, N_STEPS))
    swaps = np.empty((P, 4, N_STEPS))
    short = np.empty((P, N_STEPS))
    lmm_simulate(F0, sig_tab, B, Z, SHIFT, df, swaps, short)
    return df, swaps, short
