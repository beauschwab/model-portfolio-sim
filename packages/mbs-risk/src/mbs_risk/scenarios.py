"""Scenario machinery: common random numbers, path building, deterministic
forward-starting shock templates, portfolio extraction, base OAS solve."""
from __future__ import annotations

import numpy as np
import polars as pl

from . import models as mdl
from .config import (ADT, DT, HPI_BETA, HPI_MU, INC_LAG, MOY, N_FACTORS,
                     N_PATHS_BASE, N_STEPS, PREPAY_PARAMS, RATIONAL_SIGMOID,
                     SEASONALITY, SEED, SWAP_TENORS)
from .curve import bootstrap_curve, forwards_from_dfs
from .interfaces import ModelSuite
from .kernels import engine, make_generic_engine
from .lmm import simulate_rates
from .prepay import (BURN_LUT, BURN_SCALE, LTV_COEFS, LTV_KNOTS, SMM_LUT,
                     SMM_SCALE, static_multipliers)
from .pricing import solve_oas_from_A
from .vol import calibrate_abcd, factor_loadings, vol_feature_paths

REQUIRED_COLS = {"cusip", "current_face", "factor", "wac", "net_coupon",
                 "wam", "age", "oltv", "fico", "avg_loan_size", "state",
                 "channel", "price"}


class CRN:
    """One set of Gaussian draws shared across all scenario revaluations."""

    def __init__(self, n_paths: int, seed: int):
        rng = np.random.default_rng(seed)
        half = n_paths // 2
        Zb = rng.standard_normal((half, N_STEPS, N_FACTORS))
        self.Z = np.concatenate([Zb, -Zb], axis=0)        # antithetic
        self.eps_ps = np.random.default_rng(seed + 101)\
            .standard_normal((n_paths, N_STEPS))
        self.eps_h = np.random.default_rng(seed + 202)\
            .standard_normal((n_paths, N_STEPS))
        self.n = n_paths


def build_rate_paths(swap_rates, vol_pts, abcd_p, B, crn,
                     recalibrate=False, abcd_warm=None) -> dict:
    """Rate-only path set {df, short, swaps} for books that need no
    mortgage-side models (corporates, deposits)."""
    dfs = bootstrap_curve(SWAP_TENORS, swap_rates)
    F0 = forwards_from_dfs(dfs)
    if recalibrate:
        abcd_p = calibrate_abcd(vol_pts, F0, dfs, B, x0=abcd_warm, quiet=True)
    df, swaps, short = simulate_rates(F0, abcd_p, B, crn.Z)
    return {"df": df, "short": short, "swaps": swaps}


def build_paths(swap_rates, vol_pts, abcd_p, B, models, crn,
                recalibrate=False, abcd_warm=None, suite=None) -> dict:
    """Market state -> ADT-cast path arrays {mtg, hpi, yoy, df, short}.
    suite: interfaces.ModelSuite (None -> default components)."""
    if suite is None:
        suite = ModelSuite.default()
    dfs = bootstrap_curve(SWAP_TENORS, swap_rates)
    F0 = forwards_from_dfs(dfs)
    if recalibrate:
        abcd_p = calibrate_abcd(vol_pts, F0, dfs, B, x0=abcd_warm, quiet=True)
    df, swaps, short = simulate_rates(F0, abcd_p, B, crn.Z)
    volfeat = vol_feature_paths(abcd_p, F0, dfs, B)
    cc = suite.cc.paths(swaps, volfeat, models["cc"])
    ps = suite.ps.paths(models["ps"], models["ps_spot"], crn.eps_ps)
    mtg = cc + ps
    if INC_LAG > 0:
        mtg = np.concatenate(
            [np.repeat(mtg[:, :1], INC_LAG, axis=1), mtg[:, :-INC_LAG]], 1)
    H = suite.hpi.paths(swaps[:, 2, :], crn.eps_h)
    return {"mtg": mtg.astype(ADT), "hpi": H.astype(ADT),
            "yoy": mdl.yoy_from_hpi(H).astype(ADT), "df": df.astype(ADT),
            "short": short.astype(ADT), "swaps": swaps}


def shocked_paths(base: dict, h: int, shock_bp: float, models,
                  suite=None) -> dict:
    """Deterministic templates for a forward-starting instantaneous parallel
    rate shock at horizon month h. No re-simulation. CC and HPI responses
    delegate to the suite components (shock_response / shock_multiplier),
    so swapped models keep stress consistent. Vol-feature weights NOT
    re-shocked (second order). OAS held fixed by the caller."""
    if suite is None:
        suite = ModelSuite.default()
    d = shock_bp * 1e-4
    t = np.arange(N_STEPS)
    on = t >= h
    k = np.where(on, t - h + 1.0, 0.0)

    cs = suite.cc.shock_response(models["cc"], d, k)
    if INC_LAG > 0:
        cs = np.concatenate([np.zeros(INC_LAG), cs[:-INC_LAG]])

    g_df = np.where(on, (1.0 + d * DT) ** (-k), 1.0)
    g_h = suite.hpi.shock_multiplier(d, k)

    hpi2 = base["hpi"].astype(np.float64) * g_h[None, :]
    return {"mtg": (base["mtg"].astype(np.float64) + cs[None, :]).astype(ADT),
            "hpi": hpi2.astype(ADT),
            "yoy": mdl.yoy_from_hpi(hpi2).astype(ADT),
            "df": (base["df"].astype(np.float64) * g_df[None, :]).astype(ADT),
            "short": base["short"], "swaps": base["swaps"]}


def run_engine(paths, sec, oas=None, horizons=None, want_fwd=False,
               suite=None):
    """Thin wrapper binding config/prepay data into the kernel call.
    suite.prepay_step=None -> fast open-coded kernel; jitted step fn ->
    generic specialization (~25-30% slower, see interfaces.py)."""
    S = sec[0].shape[0]
    if oas is None:
        oas = np.zeros(S)
    if horizons is None:
        horizons = np.zeros(1, dtype=np.int64)
    kern = engine
    if suite is not None and suite.prepay_step is not None:
        kern = make_generic_engine(suite.prepay_step)
    return kern(paths["mtg"], paths["hpi"], paths["yoy"], paths["df"],
                  MOY, SEASONALITY, PREPAY_PARAMS, LTV_KNOTS, LTV_COEFS,
                  SMM_LUT, SMM_SCALE, BURN_LUT, BURN_SCALE,
                  *sec, oas, horizons, want_fwd, RATIONAL_SIGMOID)


def extract_sec(port: pl.DataFrame):
    missing = REQUIRED_COLS - set(port.columns)
    if missing:
        raise ValueError(f"portfolio missing columns: {missing}")
    sec = tuple(np.ascontiguousarray(port[c].to_numpy().astype(np.float64))
                for c in ["wac", "net_coupon", "wam", "age", "oltv", "factor"])
    horig = (port["hpi_orig_ratio"].to_numpy().astype(np.float64)
             if "hpi_orig_ratio" in port.columns
             else (1.0 + HPI_MU) ** (sec[3] / 12.0))
    return sec + (horig, static_multipliers(port))


def setup(port, swap_rates, vol_pts, cc_hist, ps_hist, ps_spot=0.012):
    models = {"cc": mdl.fit_current_coupon(cc_hist),
              "ps": mdl.fit_ps_spread(ps_hist), "ps_spot": ps_spot}
    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    sec = extract_sec(port)
    tgt = port["price"].to_numpy().astype(np.float64) / 100.0
    face = port["current_face"].to_numpy().astype(np.float64)
    return models, B, abcd0, sec, tgt, face


def port_delay(port: pl.DataFrame):
    """Per-security payment delay in YEARS from optional pay_delay_days
    column (program stated delay, e.g. FN 24). None if absent."""
    if "pay_delay_days" in port.columns:
        return port["pay_delay_days"].to_numpy().astype(np.float64) / 365.0
    return None


def solve_base_oas(swap_rates, vol_pts, abcd0, B, models, sec, tgt,
                   seed=SEED, n_paths=N_PATHS_BASE, suite=None,
                   delay_y=None):
    crn = CRN(n_paths, seed)
    paths = build_paths(swap_rates, vol_pts, abcd0, B, models, crn,
                        suite=suite)
    A, *_ = run_engine(paths, sec, suite=suite)
    return solve_oas_from_A(A, n_paths, tgt, delay_y=delay_y)
