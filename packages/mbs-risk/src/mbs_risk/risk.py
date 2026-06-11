"""Spot risk: 10 par-swap KRD01s + 9 swaption-point vegas, fixed-OAS central
differences under common random numbers."""
from __future__ import annotations

import numpy as np
import polars as pl

from .config import CURVE_BUMP, N_PATHS_SENS, SEED, SWAP_TENORS, VOL_BUMP
from .pricing import pv_from_A
from .scenarios import (CRN, build_paths, port_delay, run_engine, setup,
                        solve_base_oas)


def run_risk(port: pl.DataFrame, swap_rates, vol_pts, cc_hist, ps_hist,
             seed: int = SEED, suite=None) -> pl.DataFrame:
    models, B, abcd0, sec, tgt, face = setup(
        port, swap_rates, vol_pts, cc_hist, ps_hist)
    delay = port_delay(port)
    oas, px = solve_base_oas(swap_rates, vol_pts, abcd0, B, models, sec, tgt,
                             seed, suite=suite, delay_y=delay)
    crn = CRN(N_PATHS_SENS, seed)

    def scen_pv(sr, vp, recal):
        paths = build_paths(sr, vp, abcd0, B, models, crn,
                            recalibrate=recal, abcd_warm=abcd0, suite=suite)
        A, *_ = run_engine(paths, sec, suite=suite)
        return pv_from_A(A, oas, crn.n, delay_y=delay)

    # SCENARIO-BATCHED revaluation (v0.15): build all 38 bumped path
    # sets (paths share Z by CRN), stack along the path axis, and price
    # in ONE batched_pv_engine launch -- cores stay saturated through
    # the whole risk run instead of paying parallel ramp-up 38 times.
    # Path builds remain per-scenario numpy (cheap); custom-prepay
    # suites fall back to the sequential loop (generic kernel has no
    # batched variant yet).
    if suite is not None and suite.prepay_step is not None:
        return _run_risk_sequential(port, swap_rates, vol_pts, scen_pv,
                                    oas, px, face)
    scen_defs = []
    for i, ten in enumerate(SWAP_TENORS):
        up = swap_rates.copy(); up[i] += CURVE_BUMP
        dn = swap_rates.copy(); dn[i] -= CURVE_BUMP
        scen_defs += [(dn, vol_pts, False), (up, vol_pts, False)]
    for j in range(vol_pts.shape[0]):
        u = vol_pts.copy(); u[j, 2] += VOL_BUMP
        d = vol_pts.copy(); d[j, 2] -= VOL_BUMP
        scen_defs += [(d, True), (u, True)]
        scen_defs[-2] = (swap_rates, d, True)
        scen_defs[-1] = (swap_rates, u, True)
    NS = len(scen_defs)
    P = crn.n
    stk = {k: [] for k in ("mtg", "hpi", "yoy", "df")}
    for (sr, vp, recal) in scen_defs:
        pth = build_paths(sr, vp, abcd0, B, models, crn,
                          recalibrate=recal, abcd_warm=abcd0)
        for k in stk:
            stk[k].append(pth[k])
    stacked = {k: np.ascontiguousarray(np.concatenate(v, axis=0))
               for k, v in stk.items()}
    scen = np.repeat(np.arange(NS, dtype=np.int64), P)
    from .config import (MOY, PREPAY_PARAMS, RATIONAL_SIGMOID,
                         SEASONALITY)
    from .prepay import (BURN_LUT, BURN_SCALE, LTV_COEFS, LTV_KNOTS,
                         SMM_LUT, SMM_SCALE)
    from .kernels import batched_pv_engine
    dly = (np.asarray(delay, dtype=np.float64) if delay is not None
           else np.zeros(len(port)))
    PV = batched_pv_engine(
        stacked["mtg"], stacked["hpi"], stacked["yoy"], stacked["df"],
        scen, NS, MOY, SEASONALITY, PREPAY_PARAMS, LTV_KNOTS, LTV_COEFS,
        SMM_LUT, SMM_SCALE, BURN_LUT, BURN_SCALE, *sec, oas, dly,
        RATIONAL_SIGMOID) / P
    cols, dv01 = {}, np.zeros(len(port))
    for i, ten in enumerate(SWAP_TENORS):
        krd = face * (PV[2 * i] - PV[2 * i + 1]) / 2.0       # $ per 1bp
        cols[f"krd01_{int(ten)}y"] = krd
        dv01 += krd
    o = 2 * len(SWAP_TENORS)
    for j in range(vol_pts.shape[0]):
        e, n = vol_pts[j, 0], vol_pts[j, 1]
        cols[f"vega_{int(e)}x{int(n)}"] = face * (
            PV[o + 2 * j] - PV[o + 2 * j + 1]) \
            / (2.0 * VOL_BUMP) * 0.01                        # $ per vol-pt
    return port.with_columns(
        pl.Series("oas_bps", oas * 1e4),
        pl.Series("model_price", px * 100.0),
        pl.Series("dv01", dv01),
        *[pl.Series(k, v) for k, v in cols.items()])


def _run_risk_sequential(port, swap_rates, vol_pts, scen_pv, oas, px, face):
    """Pre-v0.15 sequential loop, kept for custom-prepay suites."""
    cols, dv01 = {}, np.zeros(len(port))
    for i, ten in enumerate(SWAP_TENORS):
        up = swap_rates.copy(); up[i] += CURVE_BUMP
        dn = swap_rates.copy(); dn[i] -= CURVE_BUMP
        krd = face * (scen_pv(dn, vol_pts, False)
                      - scen_pv(up, vol_pts, False)) / 2.0
        cols[f"krd01_{int(ten)}y"] = krd
        dv01 += krd
    for j in range(vol_pts.shape[0]):
        e, n = vol_pts[j, 0], vol_pts[j, 1]
        up = vol_pts.copy(); up[j, 2] += VOL_BUMP
        dn = vol_pts.copy(); dn[j, 2] -= VOL_BUMP
        cols[f"vega_{int(e)}x{int(n)}"] = face * (
            scen_pv(swap_rates, up, True) - scen_pv(swap_rates, dn, True)
        ) / (2.0 * VOL_BUMP) * 0.01
    return port.with_columns(
        pl.Series("oas_bps", oas * 1e4),
        pl.Series("model_price", px * 100.0),
        pl.Series("dv01", dv01),
        *[pl.Series(k, v) for k, v in cols.items()])
