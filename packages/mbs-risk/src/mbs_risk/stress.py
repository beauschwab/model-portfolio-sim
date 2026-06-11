"""Stress capital: 9Q monthly forward valuation + forward-starting
instantaneous parallel shocks, fixed OAS. Stressed passes restart from
per-path state checkpoints captured during the base forward pass."""
from __future__ import annotations

import time

import numpy as np
import polars as pl

from .config import (MOY, N_PATHS_SENS, PREPAY_PARAMS, RATIONAL_SIGMOID,
                     SEASONALITY, SEED, STRESS_HORIZONS_M, STRESS_SHOCKS_BP)
from .kernels import stress_engine
from .prepay import (BURN_LUT, BURN_SCALE, LTV_COEFS, LTV_KNOTS, SMM_LUT,
                     SMM_SCALE)
from .scenarios import (CRN, build_paths, run_engine, setup, shocked_paths,
                        solve_base_oas)


def run_stress(port: pl.DataFrame, swap_rates, vol_pts, cc_hist, ps_hist,
               shocks_bp=STRESS_SHOCKS_BP, seed: int = SEED, suite=None):
    """-> (positions_long, horizon_aggregates, fwd_dv01_profile).
    Checkpoint memory: S * P * H * 2 * 4B (10k x 128 x 27 -> ~276 MB)."""
    if suite is not None and suite.prepay_step is not None:
        raise NotImplementedError(
            "run_stress requires the default prepay model: stress_engine is "
            "the fast open-coded kernel and would diverge from a custom "
            "prepay step. Promote the custom model into both MODEL-BLOCKs "
            "(kernels.py) and rerun the zero-shock invariant test.")
    models, B, abcd0, sec, tgt, face = setup(
        port, swap_rates, vol_pts, cc_hist, ps_hist)
    oas, _ = solve_base_oas(swap_rates, vol_pts, abcd0, B, models, sec, tgt,
                            seed, suite=suite)

    crn = CRN(N_PATHS_SENS, seed)
    hz = STRESS_HORIZONS_M
    nh = len(hz)
    S = len(port)
    base = build_paths(swap_rates, vol_pts, abcd0, B, models, crn,
                       suite=suite)

    t0 = time.perf_counter()
    _, FVb, BALb, ck_bal, ck_burn, *_ = run_engine(base, sec, oas, hz,
                                               want_fwd=True, suite=suite)
    FVb /= crn.n
    BALb /= crn.n
    print(f"[stress] base forward valuation + checkpoints: "
          f"{time.perf_counter()-t0:.1f}s")

    def stressed_fv(paths, h, h_idx):
        return stress_engine(
            paths["mtg"], paths["hpi"], paths["yoy"], paths["df"],
            MOY, SEASONALITY, PREPAY_PARAMS, LTV_KNOTS, LTV_COEFS,
            SMM_LUT, SMM_SCALE, BURN_LUT, BURN_SCALE,
            *sec, oas, int(h), int(h_idx), ck_bal, ck_burn,
            RATIONAL_SIGMOID) / crn.n

    FVs = np.empty((len(shocks_bp), nh, S))
    t0 = time.perf_counter()
    for j, dbp in enumerate(shocks_bp):
        for hi, h in enumerate(hz):
            sp = shocked_paths(base, int(h), dbp, models, suite=suite)
            FVs[j, hi] = stressed_fv(sp, h, hi)
        print(f"[stress] shock {dbp:+.0f}bp x {nh} horizons done "
              f"({time.perf_counter()-t0:.1f}s cum)")

    fwd_dv01 = None
    sb = list(shocks_bp)
    if -100.0 in sb and 100.0 in sb:
        jm, jp = sb.index(-100.0), sb.index(100.0)
        fwd_dv01 = (FVs[jm] - FVs[jp]) / 200.0 * face[None, :]   # (H,S) $/bp

    frames = []
    for j, dbp in enumerate(shocks_bp):
        for hi, h in enumerate(hz):
            frames.append(pl.DataFrame({
                "cusip": port["cusip"],
                "horizon_m": np.full(S, h, dtype=np.int64),
                "shock_bp": np.full(S, dbp),
                "fwd_value_base": face * FVb[:, hi],
                "fwd_price_base": 100.0 * FVb[:, hi]
                                  / np.maximum(BALb[:, hi], 1e-12),
                "fwd_value_shock": face * FVs[j, hi],
                "stress_pnl": face * (FVs[j, hi] - FVb[:, hi]),
            }))
    pos = pl.concat(frames)

    agg = (pos.group_by(["horizon_m", "shock_bp"])
              .agg(pl.col("stress_pnl").sum().alias("pnl_$"),
                   pl.col("fwd_value_base").sum().alias("base_mv_$"))
              .sort(["shock_bp", "horizon_m"]))
    prof = None
    if fwd_dv01 is not None:
        prof = pl.DataFrame({"horizon_m": hz,
                             "fwd_dv01_$": fwd_dv01.sum(axis=1)})
    return pos, agg, prof
