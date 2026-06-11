"""Corporate bonds and loans on the same LMM paths and A-matrix machinery.

Supported: fixed or floating coupons (index = simulated 3m rate), period
caps/floors, custom amortization (bullet / annuity / explicit sinking-fund
schedules), call and put schedules with rule-based exercise.

Pipeline: contract frame + conventions -> CorpDeck (CSR-packed period and
exercise arrays on the monthly model grid, with convention-exact accrual
year fractions) -> corp_engine -> A[s,t] -> identical OAS solve / risk
scenario loop as MBS (fixed-OAS central differences under CRN).

EXERCISE MODEL -- read this before trusting callable OAS:
Exercise is RULE-BASED, not option-exact: the issuer calls when the
simulated refinancing proxy (5y par rate + the position's solved OAS...
approximated by call_thr below) is sufficiently below the coupon; the
holder puts on the mirror condition. This mirrors the prepay-intensity
philosophy of the MBS side and produces sensible negative convexity, but
it is NOT optimal exercise: exact OAS-to-call requires Longstaff-Schwartz
regression on the paths. The exercise check is an isolated block in
corp_engine designed to be replaced by an LSMC continuation-value rule;
expect rule-based OAS on deep ITM callables to be biased (typically a few
to tens of bp rich vs optimal exercise).

Timing: coupon AMOUNTS are day-count exact, and DISCOUNTING is exact-time:
each cashflow is deflated to its true pay date by extending the monthly
MMA deflator within the pay month at that month's simulated short rate,
    df(t) = df[p, m-1] / (1 + short[p, m] * (t - m/12)),
which is exact within the model's own monthly discretization. The OAS
layer likewise discounts per period at exact pay times (CSR A-vector,
not a monthly A-matrix). Residual timing error: none beyond the monthly
rate discretization itself.
"""
from __future__ import annotations

import datetime as dt

import numpy as np
import polars as pl
from numba import njit, prange

from .config import N_PATHS_SENS, N_STEPS, SEED, SWAP_TENORS, CURVE_BUMP, \
    VOL_BUMP
from .conventions import BDC, Calendar, DayCount, gen_schedule
from .scenarios import CRN, build_paths
from .vol import calibrate_abcd, factor_loadings
from .curve import bootstrap_curve, forwards_from_dfs

CORP_COLS = {"id", "face", "maturity", "freq_months", "daycount", "is_float",
             "coupon_or_spread", "price"}


class CorpDeck:
    """CSR-packed contract representation consumed by corp_engine."""

    def __init__(self, contracts: pl.DataFrame, asof: dt.date,
                 cal: Calendar | None = None,
                 bdc: BDC = BDC.MODIFIED_FOLLOWING):
        missing = CORP_COLS - set(contracts.columns)
        if missing:
            raise ValueError(f"contracts missing columns: {missing}")
        cal = cal or Calendar("US")
        S = len(contracts)
        rows = contracts.to_dicts()

        per_off = [0]
        pay_m, fix_m, tau, prin = [], [], [], []
        t_pay_l, pay_frac_l, fix_w_l = [], [], []
        call_px_l, put_px_l = [], []
        acc_m_l = []
        for r in rows:
            sched = gen_schedule(asof, r["maturity"], r["freq_months"],
                                 DayCount(r["daycount"]), cal, bdc)
            n = len(sched)
            # amortization: fraction of ORIGINAL face paid per period
            amort = r.get("amort_type", "bullet")
            pr = np.zeros(n)
            if amort == "bullet":
                pr[-1] = 1.0
            elif amort == "annuity":
                pr[:] = 1.0 / n      # linear principal (loan-style); swap in
                #                      true annuity split if coupon-dependent
            elif amort == "sink":
                sk = {d: a for d, a in r["sink_schedule"]}  # [(date, frac)]
                for j, (_, _, pay, _) in enumerate(sched):
                    for d, a in list(sk.items()):
                        if abs((pay - d).days) <= 20:
                            pr[j] += a
                            sk.pop(d)
                pr[-1] += max(0.0, 1.0 - pr.sum())
            else:
                raise ValueError(f"amort_type {amort}")

            def _tm(d):
                return min(int(min((d - asof).days / 365.0,
                                   N_STEPS / 12.0 - 1e-9) * 12.0),
                           N_STEPS - 1)
            calls = {_tm(d): px for d, px in (r.get("call_schedule") or [])}
            puts = {_tm(d): px for d, px in (r.get("put_schedule") or [])}
            for j, (a0, a1, pay, yf) in enumerate(sched):
                tp = min((pay - asof).days / 365.0, N_STEPS / 12.0 - 1e-9)
                m = min(int(tp * 12.0), N_STEPS - 1)
                frac = min(max(tp - m / 12.0, 0.0), 1.0 / 12.0)
                tf = min(max((a0 - asof).days, 0) / 365.0,
                         N_STEPS / 12.0 - 1e-9)
                mf = min(int(tf * 12.0), N_STEPS - 2)
                pay_m.append(m)
                pay_frac_l.append(frac)
                t_pay_l.append(tp)
                acc_m_l.append(mf)           # accrual-start month (= fixing)
                fix_m.append(mf)
                fix_w_l.append(min(max(tf * 12.0 - mf, 0.0), 1.0))
                tau.append(yf)
                prin.append(pr[j])
                call_px_l.append(calls.get(m, -1.0))
                put_px_l.append(puts.get(m, -1.0))
            per_off.append(len(pay_m))

        self.per_off = np.array(per_off, dtype=np.int64)
        self.pay_m = np.array(pay_m, dtype=np.int64)
        self.acc_m = np.array(acc_m_l, dtype=np.int64)
        self.pay_frac = np.array(pay_frac_l)
        self.t_pay = np.array(t_pay_l)
        self.fix_w = np.array(fix_w_l)
        self.fix_m = np.array(fix_m, dtype=np.int64)
        self.tau = np.array(tau)
        self.prin = np.array(prin)
        self.call_px = np.array(call_px_l)
        self.put_px = np.array(put_px_l)
        self.is_float = contracts["is_float"].to_numpy().astype(np.int64)
        self.cpn = contracts["coupon_or_spread"].to_numpy().astype(np.float64)
        self.cap = (contracts["cap"].to_numpy().astype(np.float64)
                    if "cap" in contracts.columns else np.full(S, 10.0))
        self.floor = (contracts["floor"].to_numpy().astype(np.float64)
                      if "floor" in contracts.columns else np.full(S, -10.0))
        self.call_thr = (contracts["call_threshold"].to_numpy()
                         .astype(np.float64) if "call_threshold"
                         in contracts.columns else np.full(S, 0.005))
        self.face = contracts["face"].to_numpy().astype(np.float64)
        self.tgt = contracts["price"].to_numpy().astype(np.float64) / 100.0
        self.n = S


@njit(parallel=True, fastmath=True, cache=True)
def corp_engine(short, swap5, df, per_off, pay_m, pay_frac, fix_m, fix_w,
                tau, prin, call_px, put_px, is_float, cpn, cap, floor,
                call_thr):
    """Returns (Acsr, Icsr, Pcsr): Acsr[j] = sum_p cf_j * df_exact --
    per-PERIOD discounted cashflow sums; Icsr/Pcsr = UNDISCOUNTED expected
    interest accrual / principal (incl. maturity + exercised redemptions)
    path-sums for the accounting layer. Acsr discounting is exact-time:
    cashflow sums (CSR-aligned with the deck), exact-time deflation:
    df(t) = df[p,m-1] / (1 + short[p,m]*(t - m/12)) within pay month m.
    Floating coupon fixes at the EXACT period-start date: the index is
    the 3m simulated rate linearly interpolated in time between the two
    monthly observations bracketing the fixing date, plus spread, clamped
    to [floor, cap]. (Remaining index limitation: the front 3m forward is
    used regardless of accrual frequency -- no 1m/6m index curves.)
    Rule-based exercise (see module docstring)."""
    S = per_off.shape[0] - 1
    P = short.shape[0]
    A = np.zeros(pay_m.shape[0])
    Icsr = np.zeros(pay_m.shape[0])
    Pcsr = np.zeros(pay_m.shape[0])
    for s in prange(S):
        j0, j1 = per_off[s], per_off[s + 1]
        fl = is_float[s] == 1
        c0 = cpn[s]
        lo, hi = floor[s], cap[s]
        thr = call_thr[s]
        for p in range(P):
            face = 1.0
            for j in range(j0, j1):
                m = pay_m[j]
                if fl:
                    mf = fix_m[j]
                    w = fix_w[j]
                    c = short[p, mf] * (1.0 - w) + short[p, mf + 1] * w + c0
                    if c > hi:
                        c = hi
                    elif c < lo:
                        c = lo
                else:
                    c = c0
                intr = face * c * tau[j]
                cf = intr + prin[j]
                Icsr[j] += intr
                pj = prin[j]
                face -= prin[j]
                if face < 0.0:
                    cf += face
                    pj += face
                    face = 0.0
                # --- EXERCISE-BLOCK (replace with LSMC for option-exact) --
                cpx = call_px[j]
                if cpx >= 0.0 and face > 0.0 and not fl \
                        and swap5[p, m] < c - thr:
                    cf += face * cpx
                    pj += face * cpx
                    face = 0.0
                ppx = put_px[j]
                if ppx >= 0.0 and face > 0.0 \
                        and swap5[p, m] > c + thr:
                    cf += face * ppx
                    pj += face * ppx
                    face = 0.0
                # --- end EXERCISE-BLOCK ------------------------------------
                Pcsr[j] += pj
                dprev = df[p, m - 1] if m > 0 else 1.0
                A[j] += cf * dprev / (1.0 + short[p, m] * pay_frac[j])
                if face <= 0.0:
                    break
    return A, Icsr, Pcsr


def _corp_full(deck: CorpDeck, paths) -> tuple:
    """-> Acsr (n_periods,) per-period discounted cashflow path-sums."""
    return corp_engine(
        np.ascontiguousarray(paths["short"].astype(np.float64)),
        np.ascontiguousarray(paths["swaps"][:, 1, :]),
        np.ascontiguousarray(paths["df"].astype(np.float64)),
        deck.per_off, deck.pay_m, deck.pay_frac, deck.fix_m, deck.fix_w,
        deck.tau, deck.prin, deck.call_px, deck.put_px, deck.is_float,
        deck.cpn, deck.cap, deck.floor, deck.call_thr)


def _corp_A(deck: CorpDeck, paths) -> np.ndarray:
    """Pricing-only view (Acsr); use _corp_full for accrual outputs."""
    return _corp_full(deck, paths)[0]


def corp_pv(deck: CorpDeck, Acsr, oas, n_paths) -> np.ndarray:
    """PV per security from the CSR A-vector at exact pay times."""
    rep = np.repeat(oas, np.diff(deck.per_off))
    contrib = Acsr * np.exp(-rep * deck.t_pay)
    return np.add.reduceat(contrib, deck.per_off[:-1]) / n_paths


def corp_solve_oas(deck: CorpDeck, Acsr, n_paths, tol=1e-8, max_iter=40):
    """Vectorized Newton on exact-time OAS discounting."""
    S = deck.n
    seg = deck.per_off[:-1]
    cnt = np.diff(deck.per_off)
    oas = np.zeros(S)
    lo, hi = np.full(S, -0.05), np.full(S, 0.30)
    for _ in range(max_iter):
        rep = np.repeat(oas, cnt)
        E = Acsr * np.exp(-rep * deck.t_pay)
        px = np.add.reduceat(E, seg) / n_paths
        dpx = -np.add.reduceat(E * deck.t_pay, seg) / n_paths
        err = px - deck.tgt
        lo = np.where(err > 0, np.maximum(lo, oas), lo)
        hi = np.where(err < 0, np.minimum(hi, oas), hi)
        if np.max(np.abs(err)) < tol:
            break
        step = np.where(np.abs(dpx) > 1e-12, -err / dpx, 0.0)
        cand = oas + step
        bad = (cand <= lo) | (cand >= hi) | ~np.isfinite(cand)
        oas = np.where(bad, 0.5 * (lo + hi), cand)
    return oas, px


def run_corp_risk(contracts: pl.DataFrame, asof: dt.date, swap_rates,
                  vol_pts, cc_hist, ps_hist, seed: int = SEED,
                  cal: Calendar | None = None) -> pl.DataFrame:
    """OAS + 10 KRD01s + 9 vegas for a corporate book, same scenario
    methodology as MBS (fixed-OAS central differences, CRN, abcd fixed for
    curve bumps / recalibrated for vol bumps). cc/ps histories are needed
    only because build_paths constructs the full path set; corp cashflows
    consume short/swap5/df."""
    from .scenarios import setup as _setup  # reuse fits/calibration
    deck = CorpDeck(contracts, asof, cal=cal)

    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    from . import models as mdl
    models = {"cc": mdl.fit_current_coupon(cc_hist),
              "ps": mdl.fit_ps_spread(ps_hist), "ps_spot": 0.012}

    crn = CRN(N_PATHS_SENS, seed)
    base = build_paths(swap_rates, vol_pts, abcd0, B, models, crn)
    A = _corp_A(deck, base)
    oas, px = corp_solve_oas(deck, A, crn.n)

    def scen_pv(sr, vp, recal):
        paths = build_paths(sr, vp, abcd0, B, models, crn,
                            recalibrate=recal, abcd_warm=abcd0)
        return corp_pv(deck, _corp_A(deck, paths), oas, crn.n)

    cols, dv01 = {}, np.zeros(deck.n)
    for i, ten in enumerate(SWAP_TENORS):
        up = swap_rates.copy(); up[i] += CURVE_BUMP
        dn = swap_rates.copy(); dn[i] -= CURVE_BUMP
        krd = deck.face * (scen_pv(dn, vol_pts, False)
                           - scen_pv(up, vol_pts, False)) / 2.0
        cols[f"krd01_{int(ten)}y"] = krd
        dv01 += krd
    for j in range(vol_pts.shape[0]):
        e, n = vol_pts[j, 0], vol_pts[j, 1]
        up = vol_pts.copy(); up[j, 2] += VOL_BUMP
        dn = vol_pts.copy(); dn[j, 2] -= VOL_BUMP
        cols[f"vega_{int(e)}x{int(n)}"] = deck.face * (
            scen_pv(swap_rates, up, True) - scen_pv(swap_rates, dn, True)
        ) / (2.0 * VOL_BUMP) * 0.01
    return contracts.with_columns(
        pl.Series("oas_bps", oas * 1e4),
        pl.Series("model_price", px * 100.0),
        pl.Series("dv01", dv01),
        *[pl.Series(k, v) for k, v in cols.items()])
