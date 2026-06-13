"""Forward-starting purchase/origination programs -- hypothetical new
production scaled in at MARKET at monthly forward dates, for reinvestment
and alternative-strategy modeling.

"At market" is per-path: a fixed-rate cohort purchased at month h fixes
its coupon at that path's simulated reference rate (short or the emitted
2/5/10/30y par swap) plus a spread, at price = par -- so purchase MtM is
zero by construction and the program contributes ONLY carry (NII) and
forward balance/duration. Floaters reset monthly at ref + spread.

Program spec (dict):
  name, product label
  side:        "asset" | "liability"
  rate_ref:    "short" | "s2" | "s5" | "s10" | "s30"
  is_float:    bool
  spread_bp:   spread to the reference at fixing/reset
  term_m:      cohort term in months
  amort:       "bullet" | "annuity" | "cpr"   (cpr needs cpr_annual)
  start_m, end_m:  purchase window (inclusive months, 0-based)
  monthly_notional: $ purchased each month, OR
  reinvest_frac + reinvest_source: fraction of a source book's modeled
      monthly principal runoff (vector supplied by the caller; the NII
      framework's `runoff` output provides it) -- the reinvestment case.

SIMPLIFICATIONS (the promotion seams): cohort amortization for "cpr" is a
CONSTANT-CPR schedule, not the behavioral prepay model -- adequate for
new at-par production over a 27m window, and promotable by routing
cohorts through the MBS engine; forward dv01 is closed-form modified
duration on the expected coupon (first-order, no convexity, no OAS).
"""
from __future__ import annotations

import numpy as np
import polars as pl

REF_IDX = {"s2": 0, "s5": 1, "s10": 2, "s30": 3}


def _ref(paths, name):
    if name == "short":
        return paths["short"].astype(np.float64)
    return paths["swaps"][:, REF_IDX[name], :].astype(np.float64)


def _amort_factors(amort: str, term_m: int, cpr: float) -> np.ndarray:
    """Balance factor at each month-since-purchase 0..term_m-1."""
    k = np.arange(term_m, dtype=np.float64)
    if amort == "bullet":
        return np.ones(term_m)
    if amort == "annuity":
        return 1.0 - k / term_m
    smm = 1.0 - (1.0 - cpr) ** (1.0 / 12.0)
    return (1.0 - smm) ** k


def program_cashflows(prog: dict, paths, horizon: int,
                      runoff: np.ndarray | None = None
                      ) -> tuple[np.ndarray, np.ndarray]:
    """(expected monthly interest $, expected month-end balance $) over
    the horizon for one program, averaged across paths."""
    ref = _ref(paths, prog["rate_ref"])
    P = ref.shape[0]
    spr = prog["spread_bp"] * 1e-4
    term = int(prog["term_m"])
    fac = _amort_factors(prog.get("amort", "bullet"), term,
                         prog.get("cpr_annual", 0.06))
    inc = np.zeros(horizon)
    bal = np.zeros(horizon)
    for h in range(int(prog["start_m"]), int(prog["end_m"]) + 1):
        if h >= horizon:
            break
        if "monthly_notional" in prog:
            N = float(prog["monthly_notional"])
        else:
            N = float(prog["reinvest_frac"]) * float(runoff[h])
        if N <= 0:
            continue
        last = min(h + term, horizon)
        ks = np.arange(last - h)
        if prog.get("is_float"):
            r = np.maximum(ref[:, h:last] + spr, 0.0)      # monthly reset
            inc[h:last] += N * fac[ks] * r.mean(0) / 12.0
        else:
            cpn = np.maximum(ref[:, h] + spr, 0.0)         # fixed at h
            inc[h:last] += N * fac[ks] * cpn.mean() / 12.0
        bal[h:last] += N * fac[ks]
    return inc, bal


def fwd_dv01_profile(prog: dict, paths, horizon: int,
                     runoff: np.ndarray | None = None) -> np.ndarray:
    """First-order $ dv01 added by the program at each forward month:
    closed-form modified duration of the remaining term at the expected
    coupon (floaters ~ one reset period). Sign: asset +, liability -."""
    ref = _ref(paths, prog["rate_ref"])
    spr = prog["spread_bp"] * 1e-4
    term = int(prog["term_m"])
    fac = _amort_factors(prog.get("amort", "bullet"), term,
                         prog.get("cpr_annual", 0.06))
    sgn = 1.0 if prog["side"] == "asset" else -1.0
    out = np.zeros(horizon)
    for h in range(int(prog["start_m"]), int(prog["end_m"]) + 1):
        if h >= horizon:
            break
        N = (float(prog["monthly_notional"]) if "monthly_notional" in prog
             else float(prog["reinvest_frac"]) * float(runoff[h]))
        if N <= 0:
            continue
        y = float(np.maximum(ref[:, h] + spr, 0.0).mean())
        for m in range(h, min(h + term, horizon)):
            rem_y = (h + term - m) / 12.0
            if prog.get("is_float"):
                dur = 1.0 / 12.0
            else:
                # closed-form annuity-weighted modified duration proxy
                dur = (1.0 - (1.0 + y) ** (-rem_y)) / max(y, 1e-6)
                dur = min(dur / (1.0 + y), rem_y)
            out[m] += sgn * N * fac[m - h] * dur * 1e-4
    return out


def run_strategies(programs: list[dict], swap_rates, vol_pts,
                   runoff_by_book: dict[str, np.ndarray] | None = None,
                   horizon: int = 27, seed: int = 7) -> dict:
    """Evaluate a list of programs on one shared path set. Returns
    monthly incremental NII by program + balances + forward dv01."""
    from ..core.config import N_PATHS_SENS, SWAP_TENORS
    from ..core.curve import bootstrap_curve, forwards_from_dfs
    from ..core.scenarios import CRN, build_rate_paths
    from ..core.vol import calibrate_abcd, factor_loadings

    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    crn = CRN(N_PATHS_SENS, seed)
    paths = build_rate_paths(swap_rates, vol_pts, abcd0, B, crn)

    months = np.arange(1, horizon + 1)
    inc_cols, bal_cols, dv_cols = {}, {}, {}
    net = np.zeros(horizon)
    for prog in programs:
        ro = None
        if "reinvest_frac" in prog:
            src = prog.get("reinvest_source")
            if not runoff_by_book or src not in runoff_by_book:
                raise ValueError(f"program {prog['name']} needs runoff "
                                 f"vector for source '{src}'")
            ro = runoff_by_book[src]
        inc, bal = program_cashflows(prog, paths, horizon, ro)
        dv = fwd_dv01_profile(prog, paths, horizon, ro)
        sgn = 1.0 if prog["side"] == "asset" else -1.0
        inc_cols[prog["name"]] = sgn * inc
        bal_cols[prog["name"]] = bal
        dv_cols[prog["name"]] = dv
        net += sgn * inc
    return {
        "nii_incremental": pl.DataFrame({"month": months, **inc_cols,
                                         "net": net}),
        "balances": pl.DataFrame({"month": months, **bal_cols}),
        "fwd_dv01": pl.DataFrame({"month": months, **dv_cols}),
        "summary": pl.DataFrame({
            "metric": ["nii_incremental_total_$",
                       "nii_incremental_annualized_$",
                       "peak_balance_$", "fwd_dv01_at_h27_$"],
            "value": [float(net.sum()),
                      float(net.sum() * 12 / horizon),
                      float(sum(b for b in bal_cols.values()).max()
                            if bal_cols else 0.0),
                      float(sum(d for d in dv_cols.values())[-1]
                            if dv_cols else 0.0)]})}
