"""Hedge products: interest-rate swaps (payer/receiver) and European
swaptions, with ASC 815 (FAS 133) designation semantics.

SWAPS -- zero new kernel code: a receiver swap is long a fixed bond and
short a par floater on the same dates (principal exchange nets), so both
legs price through the corp engine via two CorpDecks; PV_swap = side x
(PV_fixed_leg - PV_float_leg), side = +1 receiver / -1 payer. Convention
defaults: fixed semi 30/360, float quarterly ACT/360 at the simulated 3m
rate + spread. Net settlement accruals (the swap CARRY that lands in NII
under both FVH and CFH) come from the legs' undiscounted Icsr outputs.

SWAPTIONS -- MC on the emitted par-swap-rate paths (tenors limited to
{2,5,10,30}y, the LMM outputs) with the CASH-SETTLED annuity
A(s) = sum_i tau/(1+tau*s)^i evaluated at the realized rate: payoff =
max(side*(s_T(t_e) - K), 0) * A(s) * df(t_e). Standard cash-settled
formula; an approximation to physical settlement -- disclosed. Payer
minus receiver at the same strike equals the cash-settled forward by
path identity (tested).

ASC 815 designations (column `designation`):
  "fvh"      fair-value hedge: MtM to earnings offset by hedged-item
             basis adjustment; model surfaces the swap MtM and carry,
             ineffectiveness measurement is out of scope.
  "cfh"      cash-flow hedge: effective MtM to AOCI -- and AOCI from CFH
             is EXCLUDED from CET1 (unlike AFS AOCI), encoded in the KPI
             capital layer; carry reclassifies into NII as settlements
             accrue (which is what the accrual output books).
  "economic" undesignated: MtM straight to earnings.
All designations contribute identically to EVE/duration/dv01 -- the
economics don't care about the accounting; the capital path does.
"""
from __future__ import annotations

import datetime as dt

import numpy as np
import polars as pl

from .config import CURVE_BUMP, N_PATHS_SENS, SEED, SWAP_TENORS, VOL_BUMP
from .corp import CorpDeck, _corp_full, corp_pv
from .curve import bootstrap_curve, forwards_from_dfs
from .scenarios import CRN, build_rate_paths
from .vol import calibrate_abcd, factor_loadings

SWAP_COLS = {"id", "notional", "side", "fixed_rate", "maturity",
             "designation", "hedged_item"}
SWPN_COLS = {"id", "notional", "side", "strike", "expiry_m", "tenor_y",
             "designation", "hedged_item"}
SWPN_TENORS = (2.0, 5.0, 10.0, 30.0)   # the LMM's emitted par-rate paths


class HedgeDeck:
    """Swap legs as two CorpDecks (fixed bond / par floater)."""

    def __init__(self, swaps: pl.DataFrame, asof: dt.date):
        missing = SWAP_COLS - set(swaps.columns)
        if missing:
            raise ValueError(f"swap book missing columns: {missing}")
        self.frame = swaps
        self.side = np.where(np.array(swaps["side"].to_list()) ==
                             "receiver", 1.0, -1.0)
        self.notional = swaps["notional"].to_numpy().astype(np.float64)
        spr = (swaps["float_spread"].to_numpy().astype(np.float64)
               if "float_spread" in swaps.columns
               else np.zeros(len(swaps)))
        fx = [dict(id=r["id"], face=1.0, maturity=r["maturity"],
                   freq_months=6, daycount="30/360", is_float=0,
                   coupon_or_spread=r["fixed_rate"], price=100.0)
              for r in swaps.to_dicts()]
        fl = [dict(id=r["id"], face=1.0, maturity=r["maturity"],
                   freq_months=3, daycount="ACT/360", is_float=1,
                   coupon_or_spread=float(s), price=100.0)
              for r, s in zip(swaps.to_dicts(), spr)]
        self.fix = CorpDeck(pl.DataFrame(fx), asof)
        self.flt = CorpDeck(pl.DataFrame(fl), asof)
        self.n = len(swaps)


def swap_mtm_and_carry(deck: HedgeDeck, paths, n_paths: int, horizon: int
                       ) -> tuple[np.ndarray, np.ndarray]:
    """(MtM per unit notional, monthly net-settlement carry (n, H) per
    unit) -- carry > 0 adds to NII. Principal exchanges cancel in MtM;
    they are stripped from carry via the legs' interest-only Icsr."""
    from .accounting import smear_csr
    zero = np.zeros(deck.n)
    Af, If, _ = _corp_full(deck.fix, paths)
    Al, Il, _ = _corp_full(deck.flt, paths)
    mtm = deck.side * (corp_pv(deck.fix, Af, zero, n_paths)
                       - corp_pv(deck.flt, Al, zero, n_paths))
    cf = smear_csr(deck.fix.per_off, deck.fix.acc_m, deck.fix.pay_m,
                   If / n_paths, horizon, deck.n)
    cl = smear_csr(deck.flt.per_off, deck.flt.acc_m, deck.flt.pay_m,
                   Il / n_paths, horizon, deck.n)
    return mtm, deck.side[:, None] * (cf - cl)


def swaption_value(book: pl.DataFrame, paths, n_paths: int) -> np.ndarray:
    """Per-unit-notional MC value, cash-settled annuity at the realized
    rate. side: 'payer' pays fixed if exercised (gains as rates rise)."""
    missing = SWPN_COLS - set(book.columns)
    if missing:
        raise ValueError(f"swaption book missing columns: {missing}")
    sw = paths["swaps"]            # (P, 4, T) array: [s2, s5, s10, s30]
    df = paths["df"]
    out = np.empty(len(book))
    for i, r in enumerate(book.to_dicts()):
        t = float(r["tenor_y"])
        if t not in SWPN_TENORS:
            raise ValueError(f"tenor {t} not in emitted paths "
                             f"{SWPN_TENORS}")
        m = int(r["expiry_m"])
        s = sw[:, SWPN_TENORS.index(t), m].astype(np.float64)
        sgn = 1.0 if r["side"] == "payer" else -1.0
        n = int(t * 2)                      # semiannual cash annuity
        ann = np.zeros_like(s)
        d = 1.0 / (1.0 + 0.5 * np.maximum(s, 0.0))
        f = d.copy()
        for _ in range(n):
            ann += 0.5 * f
            f *= d
        pay = np.maximum(sgn * (s - r["strike"]), 0.0) * ann
        out[i] = float((pay * df[:, m]).sum() / n_paths)
    return out


def run_hedge_risk(swaps: pl.DataFrame, swpns: pl.DataFrame | None,
                   asof: dt.date, swap_rates, vol_pts, seed: int = SEED,
                   horizon: int = 27) -> dict:
    """MtM, dv01, KRDs (+ vegas on swaptions), NII carry for the hedge
    book. Same fixed-everything CRN scenario loop as all risk drivers."""
    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    crn = CRN(N_PATHS_SENS, seed)
    deck = HedgeDeck(swaps, asof)

    def rp(sr, vp=vol_pts, recal=False):
        return build_rate_paths(sr, vp, abcd0, B, crn,
                                recalibrate=recal, abcd_warm=abcd0)

    base = rp(swap_rates)
    mtm, carry = swap_mtm_and_carry(deck, base, crn.n, horizon)
    sv = swaption_value(swpns, base, crn.n) if swpns is not None else None

    def book_pv(paths):
        v = float((swap_mtm_and_carry(deck, paths, crn.n, 1)[0]
                   * deck.notional).sum())
        if swpns is not None:
            v += float((swaption_value(swpns, paths, crn.n)
                        * swpns["notional"].to_numpy()).sum())
        return v

    cols = {}
    dv01 = 0.0
    for i, ten in enumerate(SWAP_TENORS):
        up = swap_rates.copy(); up[i] += CURVE_BUMP
        dn = swap_rates.copy(); dn[i] -= CURVE_BUMP
        k = (book_pv(rp(dn)) - book_pv(rp(up))) / 2.0
        cols[f"krd01_{int(ten)}y"] = k
        dv01 += k
    vega = None
    if swpns is not None:
        vp_u = vol_pts.copy(); vp_u[:, 2] += VOL_BUMP
        vp_d = vol_pts.copy(); vp_d[:, 2] -= VOL_BUMP
        vega = (book_pv(rp(swap_rates, vp_u, True))
                - book_pv(rp(swap_rates, vp_d, True))) \
            / (2 * VOL_BUMP) * 0.01
    pos = swaps.with_columns(
        pl.Series("mtm_$", mtm * deck.notional),
        pl.Series("carry_y1_$", carry[:, :12].sum(1) * deck.notional))
    return {"positions": pos,
            "swaptions": (swpns.with_columns(
                pl.Series("value_$", sv * swpns["notional"].to_numpy()))
                if swpns is not None else None),
            "book_dv01_$": dv01, "book_krd": cols,
            "book_vega_$": vega,
            "carry_monthly_$": (carry * deck.notional[:, None]).sum(0),
            "mtm_total_$": float((mtm * deck.notional).sum())
            + (float((sv * swpns["notional"].to_numpy()).sum())
               if swpns is not None else 0.0)}
