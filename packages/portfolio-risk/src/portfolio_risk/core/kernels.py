"""Numba kernels.

DESIGN NOTE -- duplicated month logic, deliberately:
The per-month cashflow model appears open-coded in BOTH `engine` and
`stress_engine`. A shared @njit(inline="always") helper returning a state
tuple was measured ~28% slower (numba inlines the IR but LLVM loses
register allocation across the tuple pack/unpack), so we pay the
duplication. Drift protection: tests/test_engine.py::test_zero_shock_invariant
requires stress_engine restarted from engine's checkpoints under a zero
shock to reproduce engine's forward values exactly -- any divergence in the
month logic between the two kernels fails the suite. If you change the
prepay/cashflow model, change BOTH blocks (marked MODEL-BLOCK) and rerun.

Inner loop is transcendental-free: Pade(7,6) rational logistics, linear
LUTs for the SMM 12th root and burnout exp, annuity factor by recursion.
Path arrays may be float32 (storage/bandwidth); scalar math is float64.
"""
from __future__ import annotations

import numpy as np
from numba import njit, prange


# --- scalar helpers (single-value returns: these inline cleanly) -------------
@njit(inline="always", fastmath=True)
def _fsig(z):
    """sigmoid(z) = 0.5*(1+tanh(z/2)), tanh via Pade(7,6); abs err < ~1e-5."""
    x = 0.5 * z
    if x >= 6.0:
        return 1.0
    if x <= -6.0:
        return 0.0
    x2 = x * x
    t = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2))) \
        / (135135.0 + x2 * (62370.0 + x2 * (3150.0 + 28.0 * x2)))
    if t > 1.0:
        t = 1.0
    elif t < -1.0:
        t = -1.0
    return 0.5 + 0.5 * t


@njit(inline="always", fastmath=True)
def _sig_exact(z):
    if z > 30.0:
        return 1.0
    if z < -30.0:
        return 0.0
    return 1.0 / (1.0 + np.exp(-z))


@njit(inline="always", fastmath=True)
def _lut(u, lut):
    n = lut.shape[0]
    if u <= 0.0:
        return lut[0]
    i = int(u)
    if i >= n - 1:
        return lut[n - 1]
    f = u - i
    return lut[i] * (1.0 - f) + lut[i + 1] * f


@njit(inline="always", fastmath=True)
def _spline_eval(x, knots, coefs):
    K = knots.shape[0]
    if x <= knots[0]:
        x = knots[0]
    elif x >= knots[K - 1]:
        x = knots[K - 1]
    i = 0
    for j in range(K - 1):
        if x >= knots[j]:
            i = j
    dx = x - knots[i]
    o = 4 * i
    return ((coefs[o] * dx + coefs[o + 1]) * dx + coefs[o + 2]) * dx + coefs[o + 3]


# --- main engine ----------------------------------------------------------------
@njit(parallel=True, fastmath=True, cache=True)
def engine(mtg, hpi, yoy, df, moy, season, pp, knots, coefs,
           smm_lut, smm_scale, burn_lut, burn_scale,
           wac, net, wam, age, oltv, fac, horig, smult,
           oas, horizons, want_fwd, rational):
    """
    Returns (A, FV, BAL, ck_bal, ck_burn, Iout, Pacc) -- path-SUMS:
      A[s,t]   = sum_p cf*df                      (pricing at any OAS)
      FV[s,h]  = sum_p fwd value at horizons[h], fixed OAS, per unit balance
      BAL[s,h] = sum_p balance entering horizon month
      ck_bal/ck_burn[s,p,h] = per-path state at horizon (stress restarts)
      Iout[s,t]/Pacc[s,t] = UNDISCOUNTED expected interest / principal cf
      (accrual inputs for the accounting layer; new outputs appended LAST
      so existing `a, fv, bal, cb, cu, *_` unpacks stay valid).
    """
    P, T = mtg.shape
    S = wac.shape[0]
    H = horizons.shape[0]
    A = np.zeros((S, T))
    FV = np.zeros((S, H))
    BAL = np.zeros((S, H))
    ck_bal = np.zeros((S, P, H), dtype=np.float32)
    ck_burn = np.zeros((S, P, H), dtype=np.float32)
    Iout = np.zeros((S, T))
    Pacc = np.zeros((S, T))
    refi_max, refi_a, refi_b = pp[0], pp[1], pp[2]
    turnover, cpr_cap = pp[4], pp[5]
    hpa_beta, lock_floor, lock_slope = pp[6], pp[7], pp[8]
    inv12 = 1.0 / 12.0

    for s in prange(S):
        wac_s = wac[s]
        net12 = net[s] * inv12
        r = wac_s * inv12
        q0 = (1.0 + r) ** (-int(wam[s]))
        age_s = age[s]
        ofh = oltv[s] * fac[s] / horig[s]
        sm = smult[s]
        Arow = A[s]

        buf = np.empty(T)
        disc = np.empty(T)
        if want_fwd:
            o = oas[s]
            for m in range(T):
                disc[m] = np.exp(-o * (m + 1) * inv12)

        for p in range(P):
            bal = 1.0
            burn_f = 1.0
            q = q0
            kf = 0
            last = T
            for m in range(T):
                if want_fwd and kf < H and m == horizons[kf]:
                    ck_bal[s, p, kf] = bal
                    ck_burn[s, p, kf] = burn_f
                    BAL[s, kf] += bal
                    kf += 1

                # --- MODEL-BLOCK (keep identical to stress_engine) --------
                inc = wac_s - mtg[p, m]
                if rational:
                    refi = refi_max * _fsig(refi_a + refi_b * inc)
                    lock = lock_floor + (1.0 - lock_floor) \
                        * _fsig(lock_slope * inc)
                else:
                    refi = refi_max * _sig_exact(refi_a + refi_b * inc)
                    lock = lock_floor + (1.0 - lock_floor) \
                        * _sig_exact(lock_slope * inc)
                refi *= burn_f
                if inc > 0.0:
                    burn_f *= _lut(inc * burn_scale, burn_lut)
                cltv = ofh * bal / hpi[p, m]
                refi *= _spline_eval(cltv, knots, coefs) * sm
                la = age_s + m
                ramp = la * (1.0 / 30.0) if la < 30.0 else 1.0
                hk = 1.0 + hpa_beta * yoy[p, m]
                if hk < 0.3:
                    hk = 0.3
                cpr = turnover * ramp * season[moy[m]] * hk * lock + refi
                if cpr > cpr_cap:
                    cpr = cpr_cap
                smm = _lut(cpr * smm_scale, smm_lut)
                pmt = bal * r / (1.0 - q)
                q *= (1.0 + r)
                sched = pmt - bal * r
                if sched > bal:
                    sched = bal
                prepay = (bal - sched) * smm
                cf = bal * net12 + sched + prepay
                Iout[s, m] += bal * net12
                Pacc[s, m] += sched + prepay
                bal -= sched + prepay
                # --- end MODEL-BLOCK ---------------------------------------

                buf[m] = cf * df[p, m]
                if bal <= 1e-10:
                    last = m + 1
                    break

            for m in range(last):
                Arow[m] += buf[m]

            if want_fwd:
                k = H - 1
                while k >= 0 and horizons[k] > last - 1:
                    k -= 1
                v = 0.0
                for m in range(last - 1, -1, -1):
                    v += buf[m] * disc[m]
                    while k >= 0 and horizons[k] == m:
                        FV[s, k] += v / (df[p, m - 1] * disc[m - 1])
                        k -= 1
    return A, FV, BAL, ck_bal, ck_burn, Iout, Pacc


# --- dedicated stress engine -------------------------------------------------------
@njit(parallel=True, fastmath=True, cache=True)
def stress_engine(mtg, hpi, yoy, df, moy, season, pp, knots, coefs,
                  smm_lut, smm_scale, burn_lut, burn_scale,
                  wac, net, wam, age, oltv, fac, horig, smult,
                  oas, h, h_idx, ck_bal, ck_burn, rational):
    """Forward value at single horizon month h under (already-shocked) path
    arrays, restarting each (s, p) from base checkpointed state at h.
    Valid because pre-h months are unshocked by construction. Skips pre-h
    months, A accumulation, cashflow buffer and backward pass entirely.
    Returns FV path-sums (S,)."""
    P, T = mtg.shape
    S = wac.shape[0]
    FV = np.zeros(S)
    refi_max, refi_a, refi_b = pp[0], pp[1], pp[2]
    turnover, cpr_cap = pp[4], pp[5]
    hpa_beta, lock_floor, lock_slope = pp[6], pp[7], pp[8]
    inv12 = 1.0 / 12.0

    for s in prange(S):
        wac_s = wac[s]
        net12 = net[s] * inv12
        r = wac_s * inv12
        q_h = (1.0 + r) ** (h - int(wam[s]))     # annuity factor at month h
        age_s = age[s]
        ofh = oltv[s] * fac[s] / horig[s]
        sm = smult[s]
        o = oas[s]
        eo = np.exp(-o * inv12)
        div_oas = np.exp(-o * h * inv12)

        acc = 0.0
        for p in range(P):
            bal = float(ck_bal[s, p, h_idx])
            if bal <= 1e-10:
                continue
            burn_f = float(ck_burn[s, p, h_idx])
            q = q_h
            e = div_oas * eo
            v = 0.0
            for m in range(h, T):
                # --- MODEL-BLOCK (keep identical to engine) ----------------
                inc = wac_s - mtg[p, m]
                if rational:
                    refi = refi_max * _fsig(refi_a + refi_b * inc)
                    lock = lock_floor + (1.0 - lock_floor) \
                        * _fsig(lock_slope * inc)
                else:
                    refi = refi_max * _sig_exact(refi_a + refi_b * inc)
                    lock = lock_floor + (1.0 - lock_floor) \
                        * _sig_exact(lock_slope * inc)
                refi *= burn_f
                if inc > 0.0:
                    burn_f *= _lut(inc * burn_scale, burn_lut)
                cltv = ofh * bal / hpi[p, m]
                refi *= _spline_eval(cltv, knots, coefs) * sm
                la = age_s + m
                ramp = la * (1.0 / 30.0) if la < 30.0 else 1.0
                hk = 1.0 + hpa_beta * yoy[p, m]
                if hk < 0.3:
                    hk = 0.3
                cpr = turnover * ramp * season[moy[m]] * hk * lock + refi
                if cpr > cpr_cap:
                    cpr = cpr_cap
                smm = _lut(cpr * smm_scale, smm_lut)
                pmt = bal * r / (1.0 - q)
                q *= (1.0 + r)
                sched = pmt - bal * r
                if sched > bal:
                    sched = bal
                prepay = (bal - sched) * smm
                cf = bal * net12 + sched + prepay
                bal -= sched + prepay
                # --- end MODEL-BLOCK ---------------------------------------

                v += cf * df[p, m] * e
                e *= eo
                if bal <= 1e-10:
                    break
            acc += v / (df[p, h - 1] * div_oas)
        FV[s] = acc
    return FV


# --- generic engine factory for swappable prepay models -----------------------
# Custom prepay step functions (see interfaces.py for the required signature)
# compile into engine specializations here. MEASURED ~25-30% slower than the
# fast open-coded kernels above -- research path, not production path.
_GENERIC_CACHE: dict = {}


def make_generic_engine(step):
    """step: @njit'd month-step fn returning (cf, bal, burn_f, q)."""
    if step in _GENERIC_CACHE:
        return _GENERIC_CACHE[step]

    @njit(parallel=True, fastmath=True)
    def generic_engine(mtg, hpi, yoy, df, moy, season, pp, knots, coefs,
                       smm_lut, smm_scale, burn_lut, burn_scale,
                       wac, net, wam, age, oltv, fac, horig, smult,
                       oas, horizons, want_fwd, rational):
        P, T = mtg.shape
        S = wac.shape[0]
        H = horizons.shape[0]
        A = np.zeros((S, T))
        FV = np.zeros((S, H))
        BAL = np.zeros((S, H))
        ck_bal = np.zeros((S, P, H), dtype=np.float32)
        ck_burn = np.zeros((S, P, H), dtype=np.float32)
        Iout = np.zeros((S, T))
        Pacc = np.zeros((S, T))
        inv12 = 1.0 / 12.0
        for s in prange(S):
            wac_s = wac[s]
            net12 = net[s] * inv12
            r = wac_s * inv12
            q0 = (1.0 + r) ** (-int(wam[s]))
            age_s = age[s]
            ofh = oltv[s] * fac[s] / horig[s]
            sm = smult[s]
            Arow = A[s]
            buf = np.empty(T)
            disc = np.empty(T)
            if want_fwd:
                o = oas[s]
                for m in range(T):
                    disc[m] = np.exp(-o * (m + 1) * inv12)
            for p in range(P):
                bal = 1.0
                burn_f = 1.0
                q = q0
                kf = 0
                last = T
                for m in range(T):
                    if want_fwd and kf < H and m == horizons[kf]:
                        ck_bal[s, p, kf] = bal
                        ck_burn[s, p, kf] = burn_f
                        BAL[s, kf] += bal
                        kf += 1
                    bal0 = bal
                    cf, bal, burn_f, q = step(
                        bal, burn_f, q, mtg[p, m], hpi[p, m], yoy[p, m],
                        season[moy[m]], wac_s, net12, r, age_s + m, ofh, sm,
                        pp, knots, coefs, smm_lut, smm_scale,
                        burn_lut, burn_scale)
                    Iout[s, m] += bal0 * net12
                    Pacc[s, m] += cf - bal0 * net12
                    buf[m] = cf * df[p, m]
                    if bal <= 1e-10:
                        last = m + 1
                        break
                for m in range(last):
                    Arow[m] += buf[m]
                if want_fwd:
                    k = H - 1
                    while k >= 0 and horizons[k] > last - 1:
                        k -= 1
                    v = 0.0
                    for m in range(last - 1, -1, -1):
                        v += buf[m] * disc[m]
                        while k >= 0 and horizons[k] == m:
                            FV[s, k] += v / (df[p, m - 1] * disc[m - 1])
                            k -= 1
        return A, FV, BAL, ck_bal, ck_burn, Iout, Pacc

    _GENERIC_CACHE[step] = generic_engine
    return generic_engine


@njit(parallel=True, fastmath=True, cache=True)
def batched_pv_engine(mtg, hpi, yoy, df, scen, n_scen, moy, season, pp,
                      knots, coefs, smm_lut, smm_scale, burn_lut,
                      burn_scale, wac, net, wam, age, oltv, fac, horig,
                      smult, oas, delay_y, rational):
    """Scenario-BATCHED fixed-OAS PV: all bumped path sets stacked along
    the path axis with scen[p] ids -> PV[n_scen, S] path-sums in ONE
    kernel launch. Exists so the 29-revaluation risk loop saturates cores
    once instead of paying parallel ramp-up per scenario (optimization
    pass v0.15). MODEL-BLOCK is the third copy of the per-month logic --
    gated by test_batched_pv_matches_engine (single scenario must equal
    pv_from_A of the base engine to ~1e-12)."""
    P, T = mtg.shape
    S = wac.shape[0]
    PV = np.zeros((n_scen, S))
    refi_max, refi_a, refi_b = pp[0], pp[1], pp[2]
    turnover, cpr_cap = pp[4], pp[5]
    hpa_beta, lock_floor, lock_slope = pp[6], pp[7], pp[8]
    for s in prange(S):
        wac_s = wac[s]
        net12 = net[s] / 12.0
        r = wac_s / 12.0
        age_s = age[s]
        ofh = oltv[s] * fac[s] / horig[s]
        sm = smult[s]
        q0 = (1.0 + r) ** (-int(wam[s]))
        oa = oas[s]
        eo = np.exp(-oa / 12.0)
        d0 = np.exp(-oa * delay_y[s])
        for p in range(P):
            bal = 1.0
            burn_f = 1.0
            q = q0
            e = d0 * eo
            v = 0.0
            for m in range(T):
                # --- MODEL-BLOCK (keep identical to stress_engine) --------
                inc = wac_s - mtg[p, m]
                if rational:
                    refi = refi_max * _fsig(refi_a + refi_b * inc)
                    lock = lock_floor + (1.0 - lock_floor) \
                        * _fsig(lock_slope * inc)
                else:
                    refi = refi_max * _sig_exact(refi_a + refi_b * inc)
                    lock = lock_floor + (1.0 - lock_floor) \
                        * _sig_exact(lock_slope * inc)
                refi *= burn_f
                if inc > 0.0:
                    burn_f *= _lut(inc * burn_scale, burn_lut)
                cltv = ofh * bal / hpi[p, m]
                refi *= _spline_eval(cltv, knots, coefs) * sm
                la = age_s + m
                ramp = la * (1.0 / 30.0) if la < 30.0 else 1.0
                hk = 1.0 + hpa_beta * yoy[p, m]
                if hk < 0.3:
                    hk = 0.3
                cpr = turnover * ramp * season[moy[m]] * hk * lock + refi
                if cpr > cpr_cap:
                    cpr = cpr_cap
                smm = _lut(cpr * smm_scale, smm_lut)
                pmt = bal * r / (1.0 - q)
                q *= (1.0 + r)
                sched = pmt - bal * r
                if sched > bal:
                    sched = bal
                prepay = (bal - sched) * smm
                cf = bal * net12 + sched + prepay
                bal -= sched + prepay
                                # --- end MODEL-BLOCK -------------------------------------
                v += cf * df[p, m] * e
                e *= eo
                if bal <= 1e-12:
                    break
            PV[scen[p], s] += v
    return PV
