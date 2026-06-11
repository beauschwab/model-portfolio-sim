"""Non-maturity deposit (NMD) valuation on the LMM paths -- the MBS-OAS
structure with the signs flipped: balance runoff plays prepayment, the
deposit rate plays the coupon, and the liability is priced as the PV of
all outflows (interest + servicing + principal runoff + terminal balance).
A liability price of e.g. 96.5 implies a 3.5% deposit franchise premium.

DEPOSIT RATE MODEL ("logistic_beta_ecm", analog of the trending CC model):
  Long-run equilibrium:
      eq(ff) = a + ff * [b_min + (b_max - b_min) * sigmoid(k*(ff - pivot))]
  -- pass-through (beta) itself is logistic in the rate level: near-zero
  beta at the floor, rising toward b_max as rates rise.
  Short-run: asymmetric error correction (deposit-rate stickiness):
      d r_t = lam_up * max(eq_t - r_{t-1}, 0) + lam_dn * min(eq_t - r_{t-1}, 0)
  with lam_up < lam_dn typical (banks raise slower than they cut).
  Fit is JOINT NLS over the simulated recursion (see fit docstring):
  static fits on ECM-smoothed levels attenuate b_max severely, and a
  gap-inversion fixed point leaves the logistic plateau unidentified, so
  long-run and dynamics are estimated together against the observed
  series.
  The fed funds proxy along paths is the simulated 3m rate (SOFR/FF basis
  is absorbed into the fitted intercept -- disclose).

ATTRITION MODEL (analog of the prepay model):
  monthly attrition = base(segment) * age_curve(account age)
                      * size_mult(avg account size)
                      * [1 + amp * sigmoid(B*(gap - g0))]   (rate flight)
                      * (1 + vel_coef * max(d12m short, 0)) (macro velocity)
  gap = market short rate - rate paid (depositor opportunity cost).
  Age curve: young accounts churn fastest, decaying to a seasoned floor
  (inverse of MBS seasoning). Segment/anchor parameters below are
  STYLIZED -- fit to account-level panel data before production; they are
  plain (x, y) anchors and scalar dicts, same stance as prepay.py.

Valuation: A[s,t] = sum_p outflow * df on the monthly grid (deposit
interest cycles are monthly; no exact-time mapping needed), then the
standard pv_from_A / solve_oas_from_A with a widened lower bracket
(franchise premia imply negative liability OAS for sticky books). Risk =
same fixed-OAS CRN scenario loop -> KRDs ($/bp, positive = liability
gains value when rates fall) and vegas. Also reports WAL of the runoff.
"""
from __future__ import annotations

import numpy as np
import polars as pl
from numba import njit, prange
from scipy.optimize import least_squares

from .config import (CURVE_BUMP, N_PATHS_SENS, N_STEPS, SEED, SWAP_TENORS,
                     VOL_BUMP)
from .curve import bootstrap_curve, forwards_from_dfs
from .interfaces import register
from .kernels import _fsig, _spline_eval
from .prepay import nat_spline
from .pricing import pv_from_A, solve_oas_from_A
from .scenarios import CRN, build_rate_paths
from .vol import calibrate_abcd, factor_loadings

DEPOSIT_COLS = {"id", "balance", "segment", "age_months", "avg_account_size",
                "rate_paid", "price"}

# --- stylized attrition anchors: REPLACE with panel-data fits -----------------
SEGMENTS = {
    #            base monthly decay, flight amp, flight B, flight g0
    "DDA":  dict(base=0.015, amp=1.5, b=180.0, g0=0.020),
    "NOW":  dict(base=0.020, amp=2.0, b=200.0, g0=0.015),
    "SAV":  dict(base=0.022, amp=2.5, b=220.0, g0=0.012),
    "MMDA": dict(base=0.025, amp=4.0, b=260.0, g0=0.008),
}
AGE_KNOTS, AGE_COEFS = nat_spline(
    np.array([0.0, 6.0, 12.0, 24.0, 60.0, 120.0, 360.0]),
    np.array([2.50, 2.00, 1.60, 1.20, 1.00, 0.90, 0.85]))
SIZE_X = np.array([1e3, 5e3, 25e3, 100e3, 250e3, 1e6])
SIZE_Y = np.array([0.80, 0.90, 1.00, 1.15, 1.35, 1.60])   # big balances fly
VEL_COEF = 4.0          # attrition accel per +100bp 12m rate rise: +4%... x
ATTR_CAP = 0.50


# --- deposit rate model --------------------------------------------------------
@register("deposit_rate", "logistic_beta_ecm")
class LogisticBetaECM:
    """Long-run logistic beta to fed funds + asymmetric error correction."""

    def fit(self, hist: pl.DataFrame) -> dict:
        """JOINT nonlinear least squares: simulate the asymmetric-ECM
        recursion forward under candidate (logistic params, lam_up,
        lam_dn) and fit the observed deposit-rate series directly. This
        estimates long-run and dynamics together -- a naive static fit on
        ECM-smoothed levels attenuates b_max severely (measured 0.22 vs a
        true 0.60), and a gap-inversion fixed point recovers the lambdas
        but leaves the logistic plateau unidentified where the rate
        history rarely visits. Joint NLS resolves both; one residual eval
        is a single 240-step recursion -- negligible."""
        ff = hist["ff"].to_numpy()
        r = hist["dep_rate"].to_numpy()
        T = len(r)

        def eq_fn(p, x):
            a, bmin, bmax, k, piv = p
            return a + x * (bmin + (bmax - bmin)
                            / (1.0 + np.exp(-k * (x - piv))))

        def resid(theta):
            p, lu, ld = theta[:5], theta[5], theta[6]
            eq = eq_fn(p, ff)
            rm = np.empty(T)
            rm[0] = r[0]
            for t in range(1, T):
                g = eq[t] - rm[t - 1]
                rm[t] = rm[t - 1] + (lu if g > 0 else ld) * g
            return rm - r

        p0 = least_squares(
            lambda p: eq_fn(p, ff) - r,
            x0=np.array([0.001, 0.05, 0.5, 100.0, 0.02]),
            bounds=([-0.01, 0.0, 0.05, 10.0, 0.0],
                    [0.02, 0.5, 1.0, 500.0, 0.08])).x
        sol = least_squares(
            resid, x0=np.concatenate([p0, [0.25, 0.25]]),
            bounds=([-0.01, 0.0, 0.05, 10.0, 0.0, 0.01, 0.01],
                    [0.02, 0.5, 1.0, 500.0, 0.08, 1.0, 1.0]))
        p, lam_up, lam_dn = sol.x[:5], float(sol.x[5]), float(sol.x[6])
        rmse = np.sqrt(np.mean(sol.fun ** 2))
        plateau = p[4] + 2.0 / p[3]          # pivot + 2/k ~ logistic top
        if ff.max() < plateau:
            print(f"[dep] WARNING: history max ff {ff.max()*1e2:.1f}% < "
                  f"logistic plateau ~{plateau*1e2:.1f}% -- high-rate beta "
                  f"is EXTRAPOLATION, not estimation. Equilibrium is "
                  f"reliable only up to ~{ff.max()*1e2:.1f}%.")
        print(f"[dep] b_max = {p[2]:.2f}  pivot = {p[4]*1e2:.1f}%  "
              f"lam_up = {lam_up:.2f}  lam_dn = {lam_dn:.2f}  "
              f"fit RMSE = {rmse*1e4:.1f}bp")
        return {"p": p, "lam_up": lam_up, "lam_dn": lam_dn}

    def equilibrium(self, params, ff):
        a, bmin, bmax, k, piv = params["p"]
        return a + ff * (bmin + (bmax - bmin)
                         / (1.0 + np.exp(-k * (ff - piv))))

    def paths(self, short: np.ndarray, params: dict, r0: float) -> np.ndarray:
        """short (P,T) -> deposit rate paths (P,T), asymmetric ECM."""
        eq = self.equilibrium(params, short)
        lu, ld = params["lam_up"], params["lam_dn"]
        P, T = short.shape
        r = np.empty((P, T))
        x = np.full(P, r0)
        for m in range(T):
            g = eq[:, m] - x
            x = x + np.where(g > 0, lu, ld) * g
            r[:, m] = np.maximum(x, 0.0)
        return r

    def shock_response(self, params, d, k_months, ff0):
        """Forward-shock template hook (deterministic eq shift at local
        beta, ECM-converged) -- for future deposit stress integration."""
        a, bmin, bmax, kk, piv = params["p"]
        s = 1.0 / (1.0 + np.exp(-kk * (ff0 - piv)))
        beta_loc = bmin + (bmax - bmin) * s \
            + ff0 * (bmax - bmin) * kk * s * (1 - s)
        lam = 0.5 * (params["lam_up"] + params["lam_dn"])
        return np.where(k_months > 0,
                        beta_loc * d * (1 - (1 - lam) ** k_months), 0.0)


# --- attrition + valuation kernel -------------------------------------------------
@njit(parallel=True, fastmath=True, cache=True)
def deposit_engine(dep, short, vel, df, age_knots, age_coefs,
                   off, base, size_m, fl_amp, fl_b, fl_g0, age0, svc,
                   vel_coef, attr_cap, oas, horizons, want_fwd):
    """Returns (A, Pout, FV, BAL, ck_bal):
      A[s,t]    = sum_p liability outflow * deflator (pricing at any OAS)
      Pout[s,t] = sum_p UNDISCOUNTED principal runoff (WAL / profiles)
      FV[s,h]   = sum_p fwd liability value at horizons[h], fixed OAS
      BAL[s,h]  = sum_p balance entering horizon month
      ck_bal[s,p,h] = per-path balance checkpoints (stress restarts; the
      deposit state is balance-only -- no burnout analog)
      Iout[s,t] = UNDISCOUNTED expected interest expense (ex-servicing;
      servicing is noninterest expense -- appended LAST).
    Outflow_t = bal*(rate_paid + svc)/12 + bal*attr_t; terminal balance
    paid at T-1. rate_paid = dep path + per-cohort offset, floored at 0."""
    P, T = dep.shape
    S = off.shape[0]
    H = horizons.shape[0]
    A = np.zeros((S, T))
    Pout = np.zeros((S, T))
    Iout = np.zeros((S, T))
    FV = np.zeros((S, H))
    BAL = np.zeros((S, H))
    ck_bal = np.zeros((S, P, H), dtype=np.float32)
    for s in prange(S):
        o = off[s]
        b0 = base[s]
        szm = size_m[s]
        amp, bb, g0 = fl_amp[s], fl_b[s], fl_g0[s]
        a0 = age0[s]
        sv = svc[s] / 12.0
        Arow, Prow = A[s], Pout[s]
        buf = np.empty(T)
        disc = np.empty(T)
        if want_fwd:
            oa = oas[s]
            for m in range(T):
                disc[m] = np.exp(-oa * (m + 1) / 12.0)
        for p in range(P):
            bal = 1.0
            kf = 0
            last = T
            for m in range(T):
                if want_fwd and kf < H and m == horizons[kf]:
                    ck_bal[s, p, kf] = bal
                    BAL[s, kf] += bal
                    kf += 1
                # --- DEPOSIT-MODEL-BLOCK (keep identical to stress) -------
                r = dep[p, m] + o
                if r < 0.0:
                    r = 0.0
                gap = short[p, m] - r
                attr = b0 * _spline_eval(a0 + m, age_knots, age_coefs) * szm
                attr *= 1.0 + amp * _fsig(bb * (gap - g0))
                v = vel[p, m]
                if v > 0.0:
                    attr *= 1.0 + vel_coef * v
                if attr > attr_cap:
                    attr = attr_cap
                prin = bal * attr
                if m == T - 1:
                    prin = bal                      # terminal runoff
                cf = bal * (r / 12.0 + sv) + prin
                Iout[s, m] += bal * r / 12.0
                bal -= prin
                # --- end DEPOSIT-MODEL-BLOCK -------------------------------
                buf[m] = cf * df[p, m]
                Prow[m] += prin
                if bal <= 1e-12:
                    last = m + 1
                    break
            for m in range(last):
                Arow[m] += buf[m]
            if want_fwd:
                k = H - 1
                while k >= 0 and horizons[k] > last - 1:
                    k -= 1
                vv = 0.0
                for m in range(last - 1, -1, -1):
                    vv += buf[m] * disc[m]
                    while k >= 0 and horizons[k] == m:
                        FV[s, k] += vv / (df[p, m - 1] * disc[m - 1])
                        k -= 1
    return A, Pout, FV, BAL, ck_bal, Iout


@njit(parallel=True, fastmath=True, cache=True)
def deposit_stress_engine(dep, short, vel, df, age_knots, age_coefs,
                          off, base, size_m, fl_amp, fl_b, fl_g0, age0, svc,
                          vel_coef, attr_cap, oas, h, h_idx, ck_bal):
    """Forward liability value at single horizon h under (already-shocked)
    paths, restarting each (s, p) from the base balance checkpoint at h
    (pre-h months are unshocked by construction). Returns FV path-sums."""
    P, T = dep.shape
    S = off.shape[0]
    FV = np.zeros(S)
    for s in prange(S):
        o = off[s]
        b0 = base[s]
        szm = size_m[s]
        amp, bb, g0 = fl_amp[s], fl_b[s], fl_g0[s]
        a0 = age0[s]
        sv = svc[s] / 12.0
        oa = oas[s]
        eo = np.exp(-oa / 12.0)
        div_oas = np.exp(-oa * h / 12.0)
        acc = 0.0
        for p in range(P):
            bal = float(ck_bal[s, p, h_idx])
            if bal <= 1e-12:
                continue
            e = div_oas * eo
            vsum = 0.0
            for m in range(h, T):
                # --- DEPOSIT-MODEL-BLOCK (keep identical to engine) -------
                r = dep[p, m] + o
                if r < 0.0:
                    r = 0.0
                gap = short[p, m] - r
                attr = b0 * _spline_eval(a0 + m, age_knots, age_coefs) * szm
                attr *= 1.0 + amp * _fsig(bb * (gap - g0))
                v = vel[p, m]
                if v > 0.0:
                    attr *= 1.0 + vel_coef * v
                if attr > attr_cap:
                    attr = attr_cap
                prin = bal * attr
                if m == T - 1:
                    prin = bal
                cf = bal * (r / 12.0 + sv) + prin
                bal -= prin
                # --- end DEPOSIT-MODEL-BLOCK -------------------------------
                vsum += cf * df[p, m] * e
                e *= eo
                if bal <= 1e-12:
                    break
            acc += vsum / (df[p, h - 1] * div_oas)
        FV[s] = acc
    return FV


class DepositDeck:
    def __init__(self, book: pl.DataFrame):
        missing = DEPOSIT_COLS - set(book.columns)
        if missing:
            raise ValueError(f"deposit book missing columns: {missing}")
        S = len(book)
        seg = [SEGMENTS.get(x, SEGMENTS["SAV"])
               for x in book["segment"].to_list()]
        self.base = np.array([g["base"] for g in seg])
        self.fl_amp = np.array([g["amp"] for g in seg])
        self.fl_b = np.array([g["b"] for g in seg])
        self.fl_g0 = np.array([g["g0"] for g in seg])
        sz = np.clip(book["avg_account_size"].to_numpy(), SIZE_X[0],
                     SIZE_X[-1])
        self.size_m = np.interp(sz, SIZE_X, SIZE_Y)
        self.age0 = book["age_months"].to_numpy().astype(np.float64)
        self.rate_paid = book["rate_paid"].to_numpy().astype(np.float64)
        self.svc = (book["svc_cost"].to_numpy().astype(np.float64)
                    if "svc_cost" in book.columns else np.full(S, 0.0))
        self.bal = book["balance"].to_numpy().astype(np.float64)
        self.tgt = book["price"].to_numpy().astype(np.float64) / 100.0
        self.n = S


def _vel(short):
    v = np.zeros_like(short)
    v[:, 12:] = short[:, 12:] - short[:, :-12]
    return v


def _dep_args(deck, paths, dep_paths, r0):
    short = np.ascontiguousarray(paths["short"].astype(np.float64))
    return (np.ascontiguousarray(dep_paths), short, _vel(short),
            np.ascontiguousarray(paths["df"].astype(np.float64)),
            AGE_KNOTS, AGE_COEFS,
            deck.rate_paid - r0, deck.base, deck.size_m, deck.fl_amp,
            deck.fl_b, deck.fl_g0, deck.age0, deck.svc, VEL_COEF, ATTR_CAP)


def _deposit_A(deck: DepositDeck, paths, dep_paths, r0, oas=None,
               horizons=None, want_fwd=False):
    """Per-cohort rate offset anchors each cohort's paid rate at its
    observed spot: rate_s(t) = model_path(t) + (rate_paid_s - r0)."""
    if oas is None:
        oas = np.zeros(deck.n)
    if horizons is None:
        horizons = np.zeros(1, dtype=np.int64)
    return deposit_engine(*_dep_args(deck, paths, dep_paths, r0),
                          oas, horizons, want_fwd)


def run_deposit_risk(book: pl.DataFrame, swap_rates, vol_pts,
                     dep_hist: pl.DataFrame, seed: int = SEED,
                     rate_model=None) -> pl.DataFrame:
    """Liability OAS + 10 KRD01s + 9 vegas + WAL for an NMD book.
    Sign convention: positive krd/dv01 = liability VALUE rises when rates
    fall (sticky low-beta books -> long-duration liabilities -> the
    bank's EVE hedge asset). Negative OAS = franchise premium priced in."""
    rate_model = rate_model or LogisticBetaECM()
    deck = DepositDeck(book)
    params = rate_model.fit(dep_hist)

    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    crn = CRN(N_PATHS_SENS, seed)
    base = build_rate_paths(swap_rates, vol_pts, abcd0, B, crn)
    r0 = float(rate_model.equilibrium(params, base["short"][:, 0].mean()))

    def value(paths):
        dep = rate_model.paths(paths["short"].astype(np.float64), params, r0)
        return _deposit_A(deck, paths, dep, r0)

    A, Pout, *_ = value(base)
    oas, px = solve_oas_from_A(A, crn.n, deck.tgt, lo0=-0.15)
    tg = (np.arange(N_STEPS) + 1.0) / 12.0
    wal = (Pout * tg[None, :]).sum(1) / np.maximum(Pout.sum(1), 1e-12)

    def scen_pv(sr, vp, recal):
        paths = build_rate_paths(sr, vp, abcd0, B, crn,
                                 recalibrate=recal, abcd_warm=abcd0)
        return pv_from_A(value(paths)[0], oas, crn.n)

    cols, dv01 = {}, np.zeros(deck.n)
    for i, ten in enumerate(SWAP_TENORS):
        up = swap_rates.copy(); up[i] += CURVE_BUMP
        dn = swap_rates.copy(); dn[i] -= CURVE_BUMP
        krd = deck.bal * (scen_pv(dn, vol_pts, False)
                          - scen_pv(up, vol_pts, False)) / 2.0
        cols[f"krd01_{int(ten)}y"] = krd
        dv01 += krd
    for j in range(vol_pts.shape[0]):
        e, n = vol_pts[j, 0], vol_pts[j, 1]
        up = vol_pts.copy(); up[j, 2] += VOL_BUMP
        dn = vol_pts.copy(); dn[j, 2] -= VOL_BUMP
        cols[f"vega_{int(e)}x{int(n)}"] = deck.bal * (
            scen_pv(swap_rates, up, True) - scen_pv(swap_rates, dn, True)
        ) / (2.0 * VOL_BUMP) * 0.01
    return book.with_columns(
        pl.Series("oas_bps", oas * 1e4),
        pl.Series("model_price", px * 100.0),
        pl.Series("premium_pct", (1.0 - px) * 100.0),
        pl.Series("wal_y", wal),
        pl.Series("dv01", dv01),
        *[pl.Series(k, v) for k, v in cols.items()])


# --- 9Q deposit stress capital ------------------------------------------------
def deposit_shocked_paths(base: dict, h: int, shock_bp: float,
                          rate_model, params, r0) -> tuple[dict, np.ndarray]:
    """Forward-starting parallel shock at horizon month h for deposits.
    EXACT (no linearized template): the deposit-rate recursion is re-run
    on the shifted short path -- identical pre-h by construction, full
    nonlinear logistic-beta + asymmetric-ECM response post-h. Velocity
    recomputes too (the 12m rate jump accelerates attrition -- deposit
    flight under shock is captured, not assumed away). Deflator gets the
    standard (1 + d*dt)^-(t-h+1) template."""
    d = shock_bp * 1e-4
    T = base["short"].shape[1]
    t = np.arange(T)
    k = np.where(t >= h, t - h + 1.0, 0.0)
    short2 = base["short"].astype(np.float64) + d * (t >= h)[None, :]
    g_df = np.where(t >= h, (1.0 + d / 12.0) ** (-k), 1.0)
    paths2 = {"short": short2,
              "df": base["df"].astype(np.float64) * g_df[None, :]}
    dep2 = rate_model.paths(short2, params, r0)
    return paths2, dep2


def run_deposit_stress(book: pl.DataFrame, swap_rates, vol_pts,
                       dep_hist: pl.DataFrame, shocks_bp=None,
                       seed: int = SEED, rate_model=None):
    """9Q monthly forward valuation + forward-starting shocks for an NMD
    book. Returns (positions_long, horizon_aggregates, fwd_dv01_profile).
    SIGN CONVENTION: stress_pnl is the LIABILITY value change;
    eve_pnl = -stress_pnl is the bank's EVE impact (sticky books GAIN EVE
    when rates rise: liability value falls faster than assets reprice)."""
    from .config import STRESS_HORIZONS_M, STRESS_SHOCKS_BP
    shocks_bp = shocks_bp or STRESS_SHOCKS_BP
    rate_model = rate_model or LogisticBetaECM()
    deck = DepositDeck(book)
    params = rate_model.fit(dep_hist)

    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    crn = CRN(N_PATHS_SENS, seed)
    base = build_rate_paths(swap_rates, vol_pts, abcd0, B, crn)
    r0 = float(rate_model.equilibrium(params, base["short"][:, 0].mean()))
    dep0 = rate_model.paths(base["short"].astype(np.float64), params, r0)

    A, _, *_ = _deposit_A(deck, base, dep0, r0)
    oas, _ = solve_oas_from_A(A, crn.n, deck.tgt, lo0=-0.15)

    hz = STRESS_HORIZONS_M
    nh = len(hz)
    _, _, FVb, BALb, ck_bal, *_ = _deposit_A(deck, base, dep0, r0, oas, hz,
                                         want_fwd=True)
    FVb /= crn.n
    BALb /= crn.n

    FVs = np.empty((len(shocks_bp), nh, deck.n))
    for j, dbp in enumerate(shocks_bp):
        for hi, h in enumerate(hz):
            sp, dep2 = deposit_shocked_paths(base, int(h), dbp,
                                             rate_model, params, r0)
            args = _dep_args(deck, sp, dep2, r0)
            FVs[j, hi] = deposit_stress_engine(
                *args, oas, int(h), hi, ck_bal) / crn.n

    fwd_dv01 = None
    sb = list(shocks_bp)
    if -100.0 in sb and 100.0 in sb:
        jm, jp = sb.index(-100.0), sb.index(100.0)
        fwd_dv01 = (FVs[jm] - FVs[jp]) / 200.0 * deck.bal[None, :]

    frames = []
    for j, dbp in enumerate(shocks_bp):
        for hi, h in enumerate(hz):
            dpnl = deck.bal * (FVs[j, hi] - FVb[:, hi])
            frames.append(pl.DataFrame({
                "id": book["id"],
                "segment": book["segment"],
                "horizon_m": np.full(deck.n, h, dtype=np.int64),
                "shock_bp": np.full(deck.n, dbp),
                "fwd_liab_value_base": deck.bal * FVb[:, hi],
                "fwd_balance": deck.bal * BALb[:, hi],
                "stress_pnl": dpnl,           # liability value change
                "eve_pnl": -dpnl,             # bank EVE impact
            }))
    pos = pl.concat(frames)
    agg = (pos.group_by(["horizon_m", "shock_bp"])
              .agg(pl.col("eve_pnl").sum().alias("eve_pnl_$"),
                   pl.col("fwd_liab_value_base").sum().alias("liab_mv_$"),
                   pl.col("fwd_balance").sum().alias("balance_$"))
              .sort(["shock_bp", "horizon_m"]))
    prof = None
    if fwd_dv01 is not None:
        prof = pl.DataFrame({"horizon_m": hz,
                             "fwd_liab_dv01_$": fwd_dv01.sum(axis=1)})
    return pos, agg, prof
