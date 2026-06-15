"""Conventions, corporate engine, and model-swap tests."""
import datetime as dt

import numpy as np
import polars as pl
import pytest
from numba import njit

from portfolio_risk import demo
from portfolio_risk.config import SEED
from portfolio_risk.conventions import (BDC, Calendar, DayCount, gen_schedule,
                                  us_bond_holidays, year_fraction)
from portfolio_risk.corp import CorpDeck, _corp_A, corp_pv, corp_solve_oas, run_corp_risk
from portfolio_risk.interfaces import ModelSuite
from portfolio_risk.pricing import solve_oas_from_A
from portfolio_risk.scenarios import (CRN, build_paths, run_engine, setup,
                                solve_base_oas)

ASOF = dt.date(2026, 6, 10)


# --- conventions --------------------------------------------------------------
def test_day_counts():
    d1, d2 = dt.date(2026, 1, 15), dt.date(2026, 7, 15)
    assert year_fraction(d1, d2, DayCount.THIRTY_360) == pytest.approx(0.5)
    assert year_fraction(d1, d2, DayCount.ACT_360) == pytest.approx(181 / 360)
    assert year_fraction(d1, d2, DayCount.ACT_365F) == pytest.approx(181 / 365)
    # ACT/ACT across a year boundary
    a, b = dt.date(2027, 12, 1), dt.date(2028, 2, 1)   # 2028 leap
    yf = year_fraction(a, b, DayCount.ACT_ACT)
    assert yf == pytest.approx(31 / 365 + 31 / 366)


def test_us_holidays_and_adjust():
    h = us_bond_holidays(2026)
    assert dt.date(2026, 1, 1) in h
    assert dt.date(2026, 11, 26) in h          # Thanksgiving 4th Thu
    assert dt.date(2026, 7, 3) in h            # July 4 2026 is Sat -> Fri obs
    assert dt.date(2026, 4, 3) in h            # Good Friday 2026
    cal = Calendar("US")
    # July 4 weekend: Sat 2026-07-04 -> following = Mon 07-06
    assert cal.adjust(dt.date(2026, 7, 4), BDC.FOLLOWING) \
        == dt.date(2026, 7, 6)
    # month-end MF rolls backward
    assert cal.adjust(dt.date(2026, 5, 31), BDC.MODIFIED_FOLLOWING).month == 5


def test_schedule_taus_sum():
    cal = Calendar("US")
    sched = gen_schedule(ASOF, dt.date(2031, 6, 10), 6,
                         DayCount.THIRTY_360, cal)
    total = sum(s[3] for s in sched)
    assert total == pytest.approx(5.0, abs=0.05)
    assert all(cal.is_business_day(s[2]) for s in sched)


# --- corporate engine -----------------------------------------------------------
@pytest.fixture(scope="module")
def market_paths():
    sr, vp = demo.demo_market()
    cch, psh = demo.demo_histories()
    port = demo.demo_portfolio(8)
    models, B, abcd0, sec, tgt, face = setup(port, sr, vp, cch, psh)
    crn = CRN(128, SEED)
    return build_paths(sr, vp, abcd0, B, models, crn), crn


def _contract(**kw):
    base = dict(id="C1", face=1e6, maturity=dt.date(2031, 6, 10),
                freq_months=6, daycount="30/360", is_float=0,
                coupon_or_spread=0.05, price=100.0)
    base.update(kw)
    return base


def test_floater_near_par(market_paths):
    """A floater paying index+s, discounted at OAS=s, prices near par
    (basic identity up to monthly-grid timing and cap/floor absence)."""
    paths, crn = market_paths
    c = pl.DataFrame([_contract(id="FLT", is_float=1, coupon_or_spread=0.01,
                                freq_months=3, daycount="ACT/360")])
    deck = CorpDeck(c, ASOF)
    px = corp_pv(deck, _corp_A(deck, paths), np.array([0.01]), crn.n)[0]
    assert abs(px - 1.0) < 0.015


def test_callable_below_bullet(market_paths):
    """A call option held by the issuer must make the bond cheaper, and a
    put held by the investor must make it richer, at equal OAS."""
    paths, crn = market_paths
    oas = np.array([0.01])

    bullet = pl.DataFrame([_contract()])
    call_sched = [(dt.date(2028, 6, 10) + dt.timedelta(days=182 * i), 1.0)
                  for i in range(6)]
    callable_ = pl.DataFrame([_contract()]).with_columns(
        pl.Series("call_schedule", [call_sched], dtype=pl.Object))
    puttable = pl.DataFrame([_contract()]).with_columns(
        pl.Series("put_schedule", [call_sched], dtype=pl.Object))

    def _px(f):
        d = CorpDeck(f, ASOF)
        return corp_pv(d, _corp_A(d, paths), oas, crn.n)[0]
    px_b, px_c, px_p = _px(bullet), _px(callable_), _px(puttable)
    assert px_c < px_b < px_p


def test_sink_schedule_shortens_duration(market_paths):
    paths, crn = market_paths
    sink = [(dt.date(2027 + i, 6, 10), 0.2) for i in range(4)]
    sinker = pl.DataFrame([_contract()]).with_columns(
        pl.Series("amort_type", ["sink"]),
        pl.Series("sink_schedule", [sink], dtype=pl.Object))
    bullet = pl.DataFrame([_contract()])
    o, hi = np.array([0.0]), np.array([0.05])

    def _dur(f):
        d = CorpDeck(f, ASOF)
        A = _corp_A(d, paths)
        return corp_pv(d, A, o, crn.n)[0] - corp_pv(d, A, hi, crn.n)[0]
    assert _dur(sinker) < _dur(bullet)


def test_exact_time_discounting(market_paths):
    """THE FIX: two zero-coupon-style bullets maturing 12 days apart must
    price differently by ~face*r*(12/365). Under month-grid snapping both
    mapped to the same month and priced identically."""
    paths, crn = market_paths
    o = np.array([0.0])
    base_mat = dt.date(2030, 6, 10)
    pxs = []
    for shift in (0, 12):
        c = pl.DataFrame([_contract(coupon_or_spread=0.0, freq_months=120,
                                    maturity=base_mat
                                    + dt.timedelta(days=shift))])
        d = CorpDeck(c, ASOF)
        pxs.append(corp_pv(d, _corp_A(d, paths), o, crn.n)[0])
    rel = (pxs[0] - pxs[1]) / pxs[0]
    # short rate ~4%: 12 days of discounting ~ 0.04*12/365 = 13.2bp
    assert 0.0005 < rel < 0.0025
    # and the per-period exact pay times are stored
    dck = CorpDeck(pl.DataFrame([_contract()]), ASOF)
    assert np.all(np.abs(dck.t_pay - ((dck.pay_m + 1) / 12.0)) <= 1.0 / 12.0)


# --- model swapping --------------------------------------------------------------
def test_custom_prepay_model_swaps_in():
    """A constant-CPR prepay step via the generic engine: runs, prices
    differ from the S-curve default, and OAS still solves."""
    from portfolio_risk.kernels import _lut

    @njit(inline="always", fastmath=True)
    def const_cpr_step(bal, burn_f, q, mtg_pm, hpi_pm, yoy_pm, season_m,
                       wac_s, net12, r, la, ofh, sm, pp, knots, coefs,
                       smm_lut, smm_scale, burn_lut, burn_scale):
        smm = _lut(0.08 * smm_scale, smm_lut)          # flat 8 CPR
        pmt = bal * r / (1.0 - q)
        q *= (1.0 + r)
        sched = pmt - bal * r
        if sched > bal:
            sched = bal
        prepay = (bal - sched) * smm
        cf = bal * net12 + sched + prepay
        bal -= sched + prepay
        return cf, bal, burn_f, q

    port = demo.demo_portfolio(100)
    sr, vp = demo.demo_market()
    cch, psh = demo.demo_histories()
    models, B, abcd0, sec, tgt, face = setup(port, sr, vp, cch, psh)
    suite = ModelSuite.default()
    suite.prepay_step = const_cpr_step

    oas_c, px_c = solve_base_oas(sr, vp, abcd0, B, models, sec, tgt,
                                 n_paths=64, suite=suite)
    oas_d, px_d = solve_base_oas(sr, vp, abcd0, B, models, sec, tgt,
                                 n_paths=64)
    assert np.abs(px_c - tgt).max() < 1e-7             # both reprice
    assert np.abs(px_d - tgt).max() < 1e-7
    assert np.abs(oas_c - oas_d).max() > 1e-4          # models differ


def test_stress_rejects_custom_prepay():
    from portfolio_risk.stress import run_stress
    suite = ModelSuite.default()
    suite.prepay_step = lambda *a: None
    with pytest.raises(NotImplementedError):
        run_stress(demo.demo_portfolio(4), *demo.demo_market(),
                   *demo.demo_histories(), suite=suite)


def test_exact_fixing_interpolation():
    """THE FIX: floating coupon fixes at the exact fixing date via linear
    interpolation between bracketing monthly short-rate observations.
    Verified against a hand-computed price on a controlled 1-path market
    with a deterministic short-rate ramp."""
    from portfolio_risk.config import N_STEPS, DT

    # controlled market: 1 path, short rate ramps 1bp/month from 3%
    short = (0.03 + 0.0001 * np.arange(N_STEPS))[None, :]
    df = np.cumprod(1.0 / (1.0 + short[0] * DT))[None, :]
    paths = {"short": short, "df": df,
             "swaps": np.zeros((1, 4, N_STEPS)),
             "hpi": None, "yoy": None, "mtg": None}

    # two-period quarterly floater; second period fixes mid-month
    c = pl.DataFrame([_contract(id="FIXTEST", is_float=1,
                                coupon_or_spread=0.005, freq_months=4,
                                daycount="ACT/360",
                                maturity=ASOF + dt.timedelta(days=257))])
    deck = CorpDeck(c, ASOF)
    assert deck.fix_w[1] > 0.05            # genuinely mid-month fixing

    A = _corp_A(deck, paths)
    px = corp_pv(deck, A, np.array([0.0]), 1)[0]

    # hand replication from deck attributes + the documented formulas
    exp_px = 0.0
    face = 1.0
    for j in range(deck.per_off[0], deck.per_off[1]):
        mf, w = deck.fix_m[j], deck.fix_w[j]
        cpn = short[0, mf] * (1.0 - w) + short[0, mf + 1] * w + 0.005
        cf = face * cpn * deck.tau[j] + deck.prin[j]
        face -= deck.prin[j]
        m = deck.pay_m[j]
        dprev = df[0, m - 1] if m > 0 else 1.0
        exp_px += cf * dprev / (1.0 + short[0, m] * deck.pay_frac[j])
    assert px == pytest.approx(exp_px, rel=1e-12)

    # and the interpolation MATTERS: month-floor fixing gives a lower
    # coupon on the rising ramp -> different price
    exp_floor = 0.0
    face = 1.0
    for j in range(deck.per_off[0], deck.per_off[1]):
        cpn = short[0, deck.fix_m[j]] + 0.005
        cf = face * cpn * deck.tau[j] + deck.prin[j]
        face -= deck.prin[j]
        m = deck.pay_m[j]
        dprev = df[0, m - 1] if m > 0 else 1.0
        exp_floor += cf * dprev / (1.0 + short[0, m] * deck.pay_frac[j])
    assert abs(px - exp_floor) > 1e-7
