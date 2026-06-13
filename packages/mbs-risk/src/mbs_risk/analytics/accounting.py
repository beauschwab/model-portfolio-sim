"""Interest-income (NII) accounting framework -- the ACCRUAL view, built
from the UNDISCOUNTED expected interest/principal outputs the engines now
emit alongside the PV machinery. PV answers "what is it worth"; this
answers "what will it earn per month".

METHODS BY PRODUCT (documented simplifications, in the SR 11-7 spirit):

  MBS & fixed-income assets (effective interest, ASC 310-20 flavor):
    book yield y_s solves  price_s = sum_t cf_t (1 + y/12)^(-(t+1))  on
    the path-MEAN expected cashflow vector; income_t = bv_{t-1} * y/12
    with bv rolling forward bv += income - cash. Premium/discount
    amortization is therefore embedded in income automatically. STATIC
    level yield on time-0 expected cashflows -- no retrospective
    recalculation as realized prepayments deviate (the production
    refinement; disclose).

  Corporates/CDs (schedule products): period interest accruals are
    SMEARED evenly across accrual months (acc_m -> pay_m) so semiannual
    coupons book monthly, matching accrual accounting rather than cash
    timing. Corp assets get effective-interest treatment when purchased
    away from par; CDs are carried at contractual accrual (issued ~par).

  Deposits: interest expense at the modeled rate paid (Iout). Servicing
    cost is NONINTEREST expense and excluded from NII by construction.

  NII_m = sum(asset interest income) - sum(liability interest expense);
  NIM = annualized NII / average interest-earning asset balance.

All quantities are expectations across the SAME CRN path set used for
pricing -- the NII forecast and the risk numbers come from one model.
"""
from __future__ import annotations

import numpy as np
import polars as pl

from ..core.config import N_STEPS


def book_yield(cf: np.ndarray, price: np.ndarray, max_iter: int = 60
               ) -> np.ndarray:
    """Vectorized monthly-comp IRR: price = sum_t cf[:,t]*(1+y/12)^-(t+1).
    cf (S,T) expected per-unit cashflows, price (S,). Newton with bisection
    fallback bracket [-0.5, 1.0]."""
    S, T = cf.shape
    t = np.arange(1, T + 1)
    y = np.full(S, 0.05)
    lo, hi = np.full(S, -0.5), np.full(S, 1.0)

    def f(yv):
        d = (1.0 + yv[:, None] / 12.0) ** (-t[None, :])
        return (cf * d).sum(1) - price

    for _ in range(max_iter):
        d = (1.0 + y[:, None] / 12.0) ** (-t[None, :])
        pv = (cf * d).sum(1)
        err = pv - price
        if np.abs(err).max() < 1e-12:
            break
        dpv = -(cf * d * t[None, :] / (12.0 * (1.0 + y[:, None] / 12.0))
                ).sum(1)
        lo = np.where(err > 0, np.maximum(lo, y), lo)
        hi = np.where(err < 0, np.minimum(hi, y), hi)
        step = np.where(np.abs(dpv) > 1e-16, err / dpv, 0.0)
        y2 = y - step
        bad = (y2 <= lo) | (y2 >= hi) | ~np.isfinite(y2)
        y = np.where(bad, 0.5 * (lo + hi), y2)
    return y


def effective_income(cf: np.ndarray, price: np.ndarray, horizon: int
                     ) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """(income[s, 0:H], book_value[s, 0:H] end-of-month, y[s]) under the
    level-yield roll: inc = bv*y/12; bv += inc - cash."""
    y = book_yield(cf, price)
    S = cf.shape[0]
    inc = np.zeros((S, horizon))
    bvs = np.zeros((S, horizon))
    bv = price.copy()
    for m in range(horizon):
        inc[:, m] = bv * y / 12.0
        bv = bv + inc[:, m] - cf[:, m]
        bvs[:, m] = bv
    return inc, bvs, y


def smear_csr(per_off, acc_m, pay_m, vals, horizon: int, n_pos: int
              ) -> np.ndarray:
    """Spread per-period amounts evenly over accrual months acc_m..pay_m
    (inclusive of pay month) -> (n_pos, horizon) monthly accruals."""
    out = np.zeros((n_pos, horizon))
    for s in range(n_pos):
        for j in range(per_off[s], per_off[s + 1]):
            a, b = int(acc_m[j]), int(pay_m[j])
            if a > b:
                a = b
            n = b - a + 1
            v = vals[j] / n
            lo, hi = min(a, horizon), min(b + 1, horizon)
            if hi > lo:
                out[s, lo:hi] += v
    return out


def bucket_csr(per_off, pay_m, vals, horizon: int, n_pos: int) -> np.ndarray:
    """Per-period amounts at the pay month (cash timing) -> (n_pos, H)."""
    out = np.zeros((n_pos, horizon))
    for s in range(n_pos):
        for j in range(per_off[s], per_off[s + 1]):
            m = int(pay_m[j])
            if m < horizon:
                out[s, m] += vals[j]
    return out


# ----------------------------------------------------------------------------
def run_balance_sheet_nii(bs: dict, swap_rates, vol_pts, dep_hist,
                          horizon: int = 27, seed: int | None = None,
                          asof=None) -> dict:
    """Monthly NII forecast for a model balance sheet (see
    demo.model_balance_sheet). bs keys (any subset): 'mbs' (+'mbs_hists'),
    'loans' (corp frame), 'debt' (corp frame, liability), 'deposits',
    'cds'. Returns {'monthly': frame (month x product income, $),
    'summary': frame, 'book_yields': frame}. Sign convention: income
    positive for assets, expense positive in its own column; nii = income
    - expense. NIM uses average earning-asset balances from the engines.
    """
    import datetime as dt

    from ..products.cds import CDDeck, _cd_full
    from ..core.config import SEED
    from ..products.corp import CorpDeck, _corp_full
    from ..core.curve import bootstrap_curve, forwards_from_dfs
    from ..products.deposits import DepositDeck, LogisticBetaECM, _deposit_A
    from ..core.scenarios import CRN, build_rate_paths
    from ..core.vol import calibrate_abcd, factor_loadings
    from ..core.config import SWAP_TENORS, N_PATHS_SENS

    seed = SEED if seed is None else seed
    asof = asof or dt.date(2026, 6, 10)
    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    crn = CRN(N_PATHS_SENS, seed)
    rpaths = build_rate_paths(swap_rates, vol_pts, abcd0, B, crn)
    P = crn.n
    cols: dict[str, np.ndarray] = {}
    runoff: dict[str, np.ndarray] = {}
    yields: list[tuple[str, str, float, float]] = []
    earn_bal = np.zeros(horizon)     # avg earning assets $ for NIM

    # ---- MBS (effective interest on expected cashflows) ---------------------
    if "mbs" in bs:
        from ..core.scenarios import (build_paths, run_engine, setup,
                                solve_base_oas, port_delay)
        port = bs["mbs"]
        cc_hist, ps_hist = bs["mbs_hists"]
        models, B2, abcd2, sec, tgt, face = setup(port, swap_rates, vol_pts,
                                                  cc_hist, ps_hist)
        oas, _ = solve_base_oas(swap_rates, vol_pts, abcd2, B2, models, sec,
                                tgt, seed=seed, n_paths=P,
                                delay_y=port_delay(port))
        paths = build_paths(swap_rates, vol_pts, abcd2, B2, models, crn)
        _, _, _, _, _, Iout, Pacc = run_engine(paths, sec)
        bal = face
        px = tgt
        cf = (Iout + Pacc) / P
        if "book_yield" in port.columns:
            # AMORTIZED-COST BASIS: caller supplies the historical-cost
            # effective yield (e.g. filing avg yields) instead of the
            # market-implied IRR -- the model then accrues like the HOLDER
            # of the book, not a buyer at today's price. Book value rolls
            # from PAR-relative carrying value 1.0.
            y = port["book_yield"].to_numpy().astype(np.float64)
            S = cf.shape[0]
            inc = np.zeros((S, horizon)); bvs = np.zeros((S, horizon))
            bv = np.ones(S)
            for m in range(horizon):
                inc[:, m] = bv * y / 12.0
                bv = bv + inc[:, m] - cf[:, m]
                bvs[:, m] = bv
        else:
            inc, bvs, y = effective_income(cf, px, horizon)
        cols["mbs_income"] = (inc * bal[:, None]).sum(0)
        runoff["mbs"] = (Pacc[:, :horizon] / P * bal[:, None]).sum(0)
        earn_bal += (bvs * bal[:, None]).sum(0)
        yields += [("mbs", sid, float(yy), float(bb))
                   for sid, yy, bb in zip(port["cusip"].to_list(), y, bal)]

    # ---- corp assets / debt liabilities (effective interest, smeared) -------
    for key, lab in (("loans", "loan_income"), ("debt", "debt_expense")):
        if key not in bs:
            continue
        frame = bs[key]
        deck = CorpDeck(frame, asof)
        A, Icsr, Pcsr = _corp_full(deck, rpaths)
        bal = deck.face
        n = deck.n
        Im_far = smear_csr(deck.per_off, deck.acc_m, deck.pay_m, Icsr / P,
                           N_STEPS, n)
        Pm_far = bucket_csr(deck.per_off, deck.pay_m, Pcsr / P, N_STEPS, n)
        cf = Im_far + Pm_far
        if "book_yield" in frame.columns:
            y = frame["book_yield"].to_numpy().astype(np.float64)
            inc = np.zeros((n, horizon)); bvs = np.zeros((n, horizon))
            bv = np.ones(n)
            for m in range(horizon):
                inc[:, m] = bv * y / 12.0
                bv = bv + inc[:, m] - cf[:, m]
                bvs[:, m] = bv
        else:
            inc, bvs, y = effective_income(cf, deck.tgt, horizon)
        cols[lab] = (inc * bal[:, None]).sum(0)
        runoff[key] = (bucket_csr(deck.per_off, deck.pay_m, Pcsr / P,
                                  horizon, n) * bal[:, None]).sum(0)
        if key == "loans":
            earn_bal += (bvs * bal[:, None]).sum(0)
        yields += [(key, sid, float(yy), float(bb))
                   for sid, yy, bb in zip(frame["id"].to_list(), y, bal)]

    # ---- deposits (expense at rate paid) -------------------------------------
    if "deposits" in bs:
        frame = bs["deposits"]
        deck = DepositDeck(frame)
        m = LogisticBetaECM()
        params = m.fit(dep_hist)
        r0 = float(m.equilibrium(params, rpaths["short"][:, 0].mean()))
        dep = m.paths(rpaths["short"].astype(np.float64), params, r0)
        _, _, _, _, _, Iout = _deposit_A(deck, rpaths, dep, r0)
        cols["deposit_expense"] = (Iout[:, :horizon] / P
                                   * deck.bal[:, None]).sum(0)
        _, Pout_d, *_ = _deposit_A(deck, rpaths, dep, r0)
        runoff["deposits"] = (Pout_d[:, :horizon] / P
                              * deck.bal[:, None]).sum(0)

    # ---- money-market / markets balance sheet (spread-to-short) -------------
    if bs.get("mm") is not None:
        from ..products.mm import MMDeck, mm_earning_assets, mm_income
        deck = MMDeck(bs["mm"])
        inc_m, exp_m = mm_income(deck, rpaths["short"].astype(np.float64),
                                 horizon)
        cols["mm_income"] = inc_m
        cols["mm_expense"] = exp_m
        earn_bal += mm_earning_assets(deck)

    # ---- CDs (contractual accrual, smeared) ----------------------------------
    if "cds" in bs:
        frame = bs["cds"]
        deck = CDDeck(frame, asof)
        _, Icsr, _ = _cd_full(deck, rpaths)
        Im = smear_csr(deck.per_off, deck.acc_m, deck.pay_m, Icsr / P,
                       horizon, deck.n)
        cols["cd_expense"] = (Im * deck.bal[:, None]).sum(0)

    if bs.get("hedges") is not None:
        from ..products.hedges import HedgeDeck, swap_mtm_and_carry
        swaps, _ = bs["hedges"]
        hdeck = HedgeDeck(swaps, asof)
        _, carry = swap_mtm_and_carry(hdeck, rpaths, P, horizon)
        cols["hedge_carry_income"] = (carry
                                      * hdeck.notional[:, None]).sum(0)

    income = sum(v for k, v in cols.items() if k.endswith("income"))
    expense = sum(v for k, v in cols.items() if k.endswith("expense"))
    nii = income - expense
    months = np.arange(1, horizon + 1)
    monthly = pl.DataFrame({"month": months,
                            **{k: v for k, v in cols.items()},
                            "interest_income": income,
                            "interest_expense": expense,
                            "nii": nii})
    nim = (nii.sum() * 12.0 / horizon) / max(earn_bal.mean(), 1e-9) \
        if earn_bal.any() else float("nan")
    summary = pl.DataFrame({
        "metric": ["nii_total_$", "nii_annualized_$", "nim_model_%"],
        "value": [float(nii.sum()), float(nii.sum() * 12.0 / horizon),
                  float(nim * 100.0)]})
    by = pl.DataFrame(yields, schema=["book", "id", "book_yield", "balance"],
                      orient="row")
    return {"monthly": monthly, "summary": summary, "book_yields": by,
            "runoff": pl.DataFrame({"month": months,
                                    **{k: v for k, v in runoff.items()}}),
            "runoff_vectors": runoff}
