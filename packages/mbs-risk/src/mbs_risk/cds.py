"""Certificates of deposit -- term-deposit liabilities priced under the
SECURITIES construct (schedule-driven, convention-exact accruals,
exact-time discounting via the corp CSR machinery), with two embedded
options mapped onto patterns already in the package:

  EARLY-WITHDRAWAL PUT (retail; depositor's option, the CD analog of
  prepayment): per-period withdrawal hazard
      ew = [base + amp * sigmoid(B * (incentive - g0))] * tau,  capped
      incentive = reinvest proxy (simulated short rate) - cd_rate
                  - penalty amortized over the REMAINING term
  The penalty (k months of interest, standard Reg-D-style forfeiture) is
  an upfront cost the rational depositor spreads over the remaining life,
  so the option goes ITM later/deeper for short remaining terms. The bank
  pays withdrawn principal minus the forfeited interest.

  ISSUER CALL (brokered/callable CDs; bank's option): the corp
  EXERCISE-BLOCK rule -- call at schedule price when the simulated short
  rate has fallen below coupon - threshold (refinance cheaper). Same
  rule-based caveat as corp: not option-exact; LSMC is the seam.

Channel convention: "retail" -> withdrawal on, no call; "brokered" ->
withdrawal off (death-put only, modeled as zero), call schedule honored.
Override per contract with ew_mult / call_schedule as needed.

Rollover/retention at maturity is OUT OF SCOPE here: this prices the
EXISTING book to contractual maturity (terminal principal at maturity).
Franchise value of the rollover stream belongs to dynamic balance-sheet
modeling, not liability pricing.

Pricing/risk reuse: CDDeck exposes (per_off, t_pay, tgt, n) so corp_pv /
corp_solve_oas apply unchanged; run_cd_risk is the standard fixed-OAS CRN
scenario loop. Sign convention as deposits: positive dv01 = liability
value rises when rates fall.
"""
from __future__ import annotations

import datetime as dt

import numpy as np
import polars as pl
from numba import njit, prange

from .config import (CURVE_BUMP, N_PATHS_SENS, N_STEPS, SEED, SWAP_TENORS,
                     VOL_BUMP)
from .conventions import BDC, Calendar, DayCount, gen_schedule
from .corp import corp_pv, corp_solve_oas
from .curve import bootstrap_curve, forwards_from_dfs
from .kernels import _fsig
from .scenarios import CRN, build_rate_paths
from .vol import calibrate_abcd, factor_loadings

CD_COLS = {"id", "balance", "rate", "maturity", "freq_months", "daycount",
           "channel", "penalty_months", "price"}

# early-withdrawal behavioral params: [base_annual, amp, B, g0, cap_annual]
# STYLIZED -- fit to vintage-level early-redemption panels before production.
CD_EW_PARAMS = np.array([0.05, 4.0, 250.0, 0.010, 0.60])


class CDDeck:
    """CSR-packed CD book. Duck-typed to corp pricing (per_off/t_pay/tgt/n)."""

    def __init__(self, book: pl.DataFrame, asof: dt.date,
                 cal: Calendar | None = None,
                 bdc: BDC = BDC.MODIFIED_FOLLOWING):
        missing = CD_COLS - set(book.columns)
        if missing:
            raise ValueError(f"CD book missing columns: {missing}")
        cal = cal or Calendar("US")
        rows = book.to_dicts()

        per_off = [0]
        pay_m, pay_frac, acc_m, tau, t_pay, rem_y = [], [], [], [], [], []
        call_px_l = []
        for r in rows:
            freq = r["freq_months"] or 0
            if freq <= 0:                      # interest at maturity
                pay = cal.adjust(r["maturity"], bdc)
                from .conventions import year_fraction
                sched = [(asof, pay, pay,
                          year_fraction(asof, pay, DayCount(r["daycount"])))]
            else:
                sched = gen_schedule(asof, r["maturity"], freq,
                                     DayCount(r["daycount"]), cal, bdc)
            t_mat = (cal.adjust(r["maturity"], bdc) - asof).days / 365.0

            def _tm(d):
                return min(int(min((d - asof).days / 365.0,
                                   N_STEPS / 12.0 - 1e-9) * 12.0),
                           N_STEPS - 1)
            calls = {_tm(d): px for d, px in (r.get("call_schedule") or [])}

            for (a0, a1, pay, yf) in sched:
                tp = min((pay - asof).days / 365.0, N_STEPS / 12.0 - 1e-9)
                m = min(int(tp * 12.0), N_STEPS - 1)
                pay_m.append(m)
                pay_frac.append(min(max(tp - m / 12.0, 0.0), 1.0 / 12.0))
                acc_m.append(min(int(max((a0 - asof).days, 0)
                                     / 365.0 * 12.0), N_STEPS - 1))
                tau.append(yf)
                t_pay.append(tp)
                rem_y.append(max(t_mat - tp + yf, yf))   # remaining at start
                call_px_l.append(calls.get(m, -1.0))
            per_off.append(len(pay_m))

        self.per_off = np.array(per_off, dtype=np.int64)
        self.pay_m = np.array(pay_m, dtype=np.int64)
        self.pay_frac = np.array(pay_frac)
        self.acc_m = np.array(acc_m, dtype=np.int64)
        self.tau = np.array(tau)
        self.t_pay = np.array(t_pay)
        self.rem_y = np.array(rem_y)
        self.call_px = np.array(call_px_l)
        self.rate = book["rate"].to_numpy().astype(np.float64)
        self.pen_m = book["penalty_months"].to_numpy().astype(np.float64)
        ch = book["channel"].to_list()
        ew_mult = (book["ew_mult"].to_numpy().astype(np.float64)
                   if "ew_mult" in book.columns else np.ones(len(rows)))
        self.ew_mult = np.where([c == "brokered" for c in ch], 0.0, ew_mult)
        self.call_thr = (book["call_threshold"].to_numpy().astype(np.float64)
                         if "call_threshold" in book.columns
                         else np.full(len(rows), 0.005))
        self.bal = book["balance"].to_numpy().astype(np.float64)
        self.tgt = book["price"].to_numpy().astype(np.float64) / 100.0
        self.n = len(rows)


@njit(parallel=True, fastmath=True, cache=True)
def cd_engine(short, df, per_off, pay_m, pay_frac, acc_m, tau, rem_y,
              call_px, rate, pen_m, ew_mult, call_thr, ewp):
    """Returns (Acsr, Icsr, Pcsr): discounted outflow sums + UNDISCOUNTED
    expected interest / principal (withdrawals net of penalty, calls,
    maturity) for the accounting layer.
    Per period: interest on opening balance; early withdrawal at the
    period hazard (incentive read at accrual-start month, penalty
    amortized over remaining term); issuer call check; terminal principal
    at the final period. ewp = CD_EW_PARAMS."""
    S = per_off.shape[0] - 1
    P = short.shape[0]
    A = np.zeros(pay_m.shape[0])
    Icsr = np.zeros(pay_m.shape[0])
    Pcsr = np.zeros(pay_m.shape[0])
    base, amp, bb, g0, cap = ewp[0], ewp[1], ewp[2], ewp[3], ewp[4]
    for s in prange(S):
        j0, j1 = per_off[s], per_off[s + 1]
        c = rate[s]
        pen = c * pen_m[s] / 12.0          # penalty as fraction of principal
        ewm = ew_mult[s]
        thr = call_thr[s]
        for p in range(P):
            bal = 1.0
            for j in range(j0, j1):
                m = pay_m[j]
                cf = bal * c * tau[j]
                Icsr[j] += cf
                pj = 0.0
                # --- WITHDRAWAL-BLOCK (depositor's put) --------------------
                if ewm > 0.0:
                    inc = short[p, acc_m[j]] - c - pen / rem_y[j]
                    ew_a = base + amp * _fsig(bb * (inc - g0))
                    if ew_a > cap:
                        ew_a = cap
                    ew = ew_a * ewm * tau[j]
                    if ew > 0.95:
                        ew = 0.95
                    wd = bal * ew
                    cf += wd * (1.0 - pen)         # forfeited interest kept
                    pj += wd * (1.0 - pen)
                    bal -= wd
                # --- CALL (issuer's option; rule-based, see corp caveat) ---
                cpx = call_px[j]
                if cpx >= 0.0 and bal > 0.0 and short[p, m] < c - thr:
                    cf += bal * cpx
                    pj += bal * cpx
                    bal = 0.0
                if j == j1 - 1 and bal > 0.0:      # maturity principal
                    cf += bal
                    pj += bal
                    bal = 0.0
                Pcsr[j] += pj
                dprev = df[p, m - 1] if m > 0 else 1.0
                A[j] += cf * dprev / (1.0 + short[p, m] * pay_frac[j])
                if bal <= 0.0:
                    break
    return A, Icsr, Pcsr


def _cd_full(deck: CDDeck, paths) -> tuple:
    return cd_engine(
        np.ascontiguousarray(paths["short"].astype(np.float64)),
        np.ascontiguousarray(paths["df"].astype(np.float64)),
        deck.per_off, deck.pay_m, deck.pay_frac, deck.acc_m, deck.tau,
        deck.rem_y, deck.call_px, deck.rate, deck.pen_m, deck.ew_mult,
        deck.call_thr, CD_EW_PARAMS)


def _cd_A(deck: CDDeck, paths):
    """Pricing-only view (Acsr); use _cd_full for accrual outputs."""
    return _cd_full(deck, paths)[0]


def run_cd_risk(book: pl.DataFrame, asof: dt.date, swap_rates, vol_pts,
                seed: int = SEED, cal: Calendar | None = None
                ) -> pl.DataFrame:
    """Liability OAS + KRDs + vegas for a CD book, same fixed-OAS CRN
    scenario methodology. No fitted histories needed (rate is contractual;
    withdrawal params are config -- fit to redemption panels)."""
    deck = CDDeck(book, asof, cal=cal)
    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    crn = CRN(N_PATHS_SENS, seed)
    base = build_rate_paths(swap_rates, vol_pts, abcd0, B, crn)

    A = _cd_A(deck, base)
    oas, px = corp_solve_oas(deck, A, crn.n)

    def scen_pv(sr, vp, recal):
        paths = build_rate_paths(sr, vp, abcd0, B, crn,
                                 recalibrate=recal, abcd_warm=abcd0)
        return corp_pv(deck, _cd_A(deck, paths), oas, crn.n)

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
        pl.Series("dv01", dv01),
        *[pl.Series(k, v) for k, v in cols.items()])
