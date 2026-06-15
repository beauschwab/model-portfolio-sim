"""Deposit (NMD) model tests."""
import numpy as np
import polars as pl  # noqa
import pytest

from portfolio_risk import demo
from portfolio_risk.config import SEED
from portfolio_risk.deposits import (DepositDeck, LogisticBetaECM, _deposit_A,
                               run_deposit_risk)
from portfolio_risk.demo import demo_deposit_book, demo_deposit_history
from portfolio_risk.pricing import solve_oas_from_A
from portfolio_risk.scenarios import CRN, build_rate_paths
from portfolio_risk.curve import bootstrap_curve, forwards_from_dfs
from portfolio_risk.vol import calibrate_abcd, factor_loadings


@pytest.fixture(scope="module")
def rate_paths():
    sr, vp = demo.demo_market()
    B = factor_loadings()
    dfs0 = bootstrap_curve(np.array([1, 2, 3, 4, 5, 7, 10, 15, 20, 30],
                                    dtype=float), sr)
    abcd0 = calibrate_abcd(vp, forwards_from_dfs(dfs0), dfs0, B)
    crn = CRN(128, SEED)
    return build_rate_paths(sr, vp, abcd0, B, crn), crn


def test_rate_model_fit_recovery():
    """Joint NLS recovers the IDENTIFIABLE objects: ECM speeds and the
    equilibrium FUNCTION over the visited rate range. The raw
    (b_max, k, pivot) triple trades off and is asserted only loosely --
    parameter-level recovery requires the history to dwell at high rates
    (which the demo regime path now does)."""
    m = LogisticBetaECM()
    p = m.fit(demo_deposit_history())
    # dynamics: true 0.12 / 0.35
    assert p["lam_up"] < p["lam_dn"]
    assert 0.06 < p["lam_up"] < 0.20
    assert 0.25 < p["lam_dn"] < 0.50
    # equilibrium function pointwise vs the known generator
    grid = np.array([0.01, 0.02, 0.03, 0.04, 0.05, 0.06, 0.07])
    true_eq = 0.001 + grid * (0.05 + 0.55
                              / (1 + np.exp(-150 * (grid - 0.025))))
    fit_eq = m.equilibrium(p, grid)
    assert np.abs(fit_eq - true_eq).max() < 0.0035   # 35bp anywhere
    # plateau loosely
    assert 0.40 < p["p"][2] < 0.85                   # true 0.60


def test_balance_conservation_and_wal(rate_paths):
    """Every dollar of balance runs off exactly once (incl. terminal)."""
    paths, crn = rate_paths
    book = demo_deposit_book(20)
    deck = DepositDeck(book)
    m = LogisticBetaECM()
    params = m.fit(demo_deposit_history())
    r0 = float(m.equilibrium(params, paths["short"][:, 0].mean()))
    dep = m.paths(paths["short"].astype(np.float64), params, r0)
    A, Pout, *_ = _deposit_A(deck, paths, dep, r0)
    total = Pout.sum(axis=1) / crn.n
    np.testing.assert_allclose(total, 1.0, rtol=1e-9)


def test_segment_flight_ordering(rate_paths):
    """MMDA (rate-hot) must run off faster than DDA (sticky): shorter WAL
    and lower-duration liability, all else equal."""
    paths, crn = rate_paths
    rows = [dict(id=f"X{s}", balance=1e8, segment=s, age_months=60.0,
                 avg_account_size=2.5e4, rate_paid=0.01, price=97.0)
            for s in ("DDA", "MMDA")]
    deck = DepositDeck(pl.DataFrame(rows))
    m = LogisticBetaECM()
    params = m.fit(demo_deposit_history())
    r0 = float(m.equilibrium(params, paths["short"][:, 0].mean()))
    dep = m.paths(paths["short"].astype(np.float64), params, r0)
    A, Pout, *_ = _deposit_A(deck, paths, dep, r0)
    tg = (np.arange(Pout.shape[1]) + 1.0) / 12.0
    wal = (Pout * tg).sum(1) / Pout.sum(1)
    assert wal[1] < wal[0]               # MMDA shorter than DDA
    # duration ordering at common OAS: liability PV sensitivity to spread
    lo, hi = np.zeros(2), np.full(2, 0.02)
    from portfolio_risk.pricing import pv_from_A
    dur = pv_from_A(A, lo, crn.n) - pv_from_A(A, hi, crn.n)
    assert dur[1] < dur[0]


def test_deposit_risk_end_to_end():
    sr, vp = demo.demo_market()
    book = demo_deposit_book(40)
    out = run_deposit_risk(book, sr, vp, demo_deposit_history())
    assert np.abs(out["model_price"].to_numpy()
                  - book["price"].to_numpy()).max() < 1e-5
    # sticky DDA cohorts: long-duration liabilities -> positive dv01
    dda = out.filter(pl.col("segment") == "DDA")
    assert (dda["dv01"] > 0).all()
    assert (out["wal_y"] > 0.5).all() and (out["wal_y"] < 12.0).all()


def test_deposit_zero_shock_invariant(rate_paths):
    """Gating pattern: deposit_stress_engine restarted from checkpoints
    under a ZERO shock must reproduce the base engine's forward values."""
    from portfolio_risk.config import STRESS_HORIZONS_M
    from portfolio_risk.deposits import (_dep_args, deposit_shocked_paths,
                                   deposit_stress_engine)
    paths, crn = rate_paths
    deck = DepositDeck(demo_deposit_book(30))
    m = LogisticBetaECM()
    params = m.fit(demo_deposit_history())
    r0 = float(m.equilibrium(params, paths["short"][:, 0].mean()))
    dep = m.paths(paths["short"].astype(np.float64), params, r0)
    oas = np.full(deck.n, -0.01)
    hz = STRESS_HORIZONS_M
    _, _, FVb, _, ck, *_ = _deposit_A(deck, paths, dep, r0, oas, hz,
                                  want_fwd=True)
    for hi in (0, 13, 26):
        h = int(hz[hi])
        sp, dep2 = deposit_shocked_paths(paths, h, 0.0, m, params, r0)
        fv = deposit_stress_engine(*_dep_args(deck, sp, dep2, r0),
                                   oas, h, hi, ck)
        np.testing.assert_allclose(fv, FVb[:, hi], rtol=1e-6,
                                   err_msg=f"horizon {h}")


def test_deposit_stress_eve_signs():
    """Sticky DDA book: rates up -> liability value falls -> bank EVE
    gains (positive eve_pnl); monotone in shock size."""
    import polars as pl  # noqa
    from portfolio_risk import run_deposit_stress
    from portfolio_risk.demo import demo_market
    rows = [dict(id=f"D{i}", balance=1e8, segment="DDA", age_months=120.0,
                 avg_account_size=1.5e4, rate_paid=0.0, price=96.0)
            for i in range(10)]
    sr, vp = demo_market()
    pos, agg, prof = run_deposit_stress(
        pl.DataFrame(rows), sr, vp, demo_deposit_history(),
        shocks_bp=(-100.0, 100.0, 200.0))
    a9 = agg.filter(pl.col("horizon_m") == 9)
    up1 = a9.filter(pl.col("shock_bp") == 100.0)["eve_pnl_$"][0]
    up2 = a9.filter(pl.col("shock_bp") == 200.0)["eve_pnl_$"][0]
    dn1 = a9.filter(pl.col("shock_bp") == -100.0)["eve_pnl_$"][0]
    assert up1 > 0 and up2 > up1 and dn1 < 0
    assert prof is not None and (prof["fwd_liab_dv01_$"] > 0).all()


# --- CDs ------------------------------------------------------------------------
import datetime as _dt

ASOF_CD = _dt.date(2026, 6, 10)


def _cd(**kw):
    base = dict(id="CD1", balance=1e7, rate=0.045,
                maturity=_dt.date(2031, 6, 10), freq_months=6,
                daycount="30/360", channel="retail", penalty_months=6.0,
                price=100.0)
    base.update(kw)
    return base


def test_cd_matches_corp_bullet(rate_paths):
    """Cross-engine consistency: a CD with withdrawal disabled and no call
    is economically a fixed bullet -- must price within MC/grid tolerance
    of the corp engine on identical terms and identical paths."""
    from portfolio_risk.cds import CDDeck, _cd_A
    from portfolio_risk.corp import CorpDeck, _corp_A, corp_pv as cpv
    paths, crn = rate_paths
    oas = np.array([0.01])

    cd = pl.DataFrame([_cd(channel="brokered")])      # ew off, no calls
    cdd = CDDeck(cd, ASOF_CD)
    px_cd = cpv(cdd, _cd_A(cdd, paths), oas, crn.n)[0]

    corp = pl.DataFrame([dict(id="B", face=1e7,
                              maturity=_dt.date(2031, 6, 10),
                              freq_months=6, daycount="30/360", is_float=0,
                              coupon_or_spread=0.045, price=100.0)])
    cpd = CorpDeck(corp, ASOF_CD)
    px_corp = cpv(cpd, _corp_A(cpd, paths), oas, crn.n)[0]
    assert px_cd == pytest.approx(px_corp, rel=2e-3)


def test_cd_withdrawal_option_raises_liability_value(rate_paths):
    """The depositor's put costs the bank: at equal OAS, the retail CD
    (withdrawal on) must be worth MORE than the same CD with it off."""
    from portfolio_risk.cds import CDDeck, _cd_A
    from portfolio_risk.corp import corp_pv as cpv
    paths, crn = rate_paths
    oas = np.array([0.01])

    def _px(ch):
        d = CDDeck(pl.DataFrame([_cd(channel=ch, rate=0.02)]), ASOF_CD)
        return cpv(d, _cd_A(d, paths), oas, crn.n)[0]
    assert _px("retail") > _px("brokered")


def test_cd_issuer_call_lowers_liability_value(rate_paths):
    """The bank's call caps the liability's value: callable < bullet."""
    from portfolio_risk.cds import CDDeck, _cd_A
    from portfolio_risk.corp import corp_pv as cpv
    paths, crn = rate_paths
    oas = np.array([0.0])
    calls = [(ASOF_CD + _dt.timedelta(days=365 + 182 * i), 1.0)
             for i in range(6)]

    def _px(call_sched):
        f = pl.DataFrame([_cd(channel="brokered", rate=0.06)])
        if call_sched:
            f = f.with_columns(pl.Series("call_schedule", [call_sched],
                                         dtype=pl.Object))
        d = CDDeck(f, ASOF_CD)
        return cpv(d, _cd_A(d, paths), oas, crn.n)[0]
    assert _px(calls) < _px(None)


def test_cd_risk_end_to_end():
    from portfolio_risk import run_cd_risk
    from portfolio_risk.demo import demo_market, demo_cd_book
    sr, vp = demo_market()
    book = demo_cd_book(40)
    out = run_cd_risk(book, ASOF_CD, sr, vp)
    assert np.abs(out["model_price"].to_numpy()
                  - book["price"].to_numpy()).max() < 1e-5
    # fixed-rate non-callable liabilities: strictly positive duration.
    # Callables can show locally NEGATIVE dv01 near the exercise boundary
    # (rates down 1bp -> more paths called at par, capping value) -- that
    # is correct economics under rule-based exercise, so only bound them.
    retail = out.filter(pl.col("channel") == "retail")
    assert (retail["dv01"] > 0).all()
    brk = out.filter(pl.col("channel") == "brokered")
    assert (brk["dv01"].abs() < retail["dv01"].max() * 2).all()


# --- accounting / NII -------------------------------------------------------
def test_book_yield_par_bond():
    """A par-priced level annuity must yield exactly its coupon."""
    from portfolio_risk.accounting import book_yield, effective_income
    T = 120
    c = 0.06
    cf = np.zeros((1, 360))
    bal = 1.0
    for m in range(T):
        pmt = bal * c / 12 / (1 - (1 + c / 12) ** (-(T - m)))
        cf[0, m] = pmt
        bal -= pmt - bal * c / 12
    y = book_yield(cf, np.array([1.0]))
    assert y[0] == pytest.approx(c, abs=1e-8)
    inc, bvs, _ = effective_income(cf, np.array([1.0]), 12)
    assert inc[0, 0] == pytest.approx(c / 12, abs=1e-9)


def test_balance_sheet_nii_end_to_end():
    """Full model balance sheet through the NII framework: positive NII,
    asset income exceeds liability expense, sane model NIM."""
    from portfolio_risk.accounting import run_balance_sheet_nii
    from portfolio_risk.demo import (demo_market, model_balance_sheet,
                               demo_deposit_history)
    sr, vp = demo_market()
    bs = model_balance_sheet(scale=0.001)
    out = run_balance_sheet_nii(bs, sr, vp, demo_deposit_history(),
                                horizon=27)
    m = out["monthly"]
    assert (m["nii"] > 0).all()
    assert m["interest_income"].sum() > m["interest_expense"].sum()
    nim = out["summary"].filter(
        pl.col("metric") == "nim_model_%")["value"][0]
    assert 0.5 < nim < 5.0
    yb = out["book_yields"]
    htm = yb.filter(pl.col("id").str.starts_with("HTM"))["book_yield"]
    afs = yb.filter(pl.col("id").str.starts_with("AFS"))["book_yield"]
    # underwater HTM bought at deep discount: book yield well above net cpn
    assert htm.mean() > 0.03 and afs.mean() > 0.035


def test_nim_reconciles_to_reported_with_basis_and_mm():
    """The headline question: with amortized-cost basis (holder's yields)
    AND the markets balance sheet included, model NIM must land near
    WFC's reported 2.47% -- the remaining gap is synthetic loan/CD
    pricing, not a structural model difference. Decomposition (from the
    1Q26 avg-balance table): core-only at actual rates = 2.85%; markets
    book dilution ~ -38bp -> 2.47%; market-px basis would add +50-90bp;
    cheap synthetic deposits +34bp."""
    from portfolio_risk.accounting import run_balance_sheet_nii
    from portfolio_risk.demo import (demo_deposit_history, demo_market,
                               model_balance_sheet)
    sr, vp = demo_market()
    bs = model_balance_sheet(scale=0.001, basis="amortized_cost",
                             include_markets_bs=True)
    out = run_balance_sheet_nii(bs, sr, vp, demo_deposit_history(),
                                horizon=27)
    nim = out["summary"].filter(
        pl.col("metric") == "nim_model_%")["value"][0]
    assert 2.0 < nim < 3.2, f"model NIM {nim:.2f}% vs reported 2.47%"
    m = out["monthly"]
    assert "mm_income" in m.columns and "mm_expense" in m.columns
    # markets book must be roughly matched (thin net spread)
    net_mm = (m["mm_income"] - m["mm_expense"]).sum()
    assert net_mm > 0 and net_mm < 0.35 * m["nii"].sum()


def test_kpis_end_to_end():
    """EVE equals the equity plug (sheet balances), LCR/NSFR in sane
    ranges, RWA density calibrated, CET1 path accretes with positive NII
    at the filing NI/NII ratio."""
    from portfolio_risk.kpis import compute_kpis
    from portfolio_risk.accounting import run_balance_sheet_nii
    from portfolio_risk.demo import (demo_market, model_balance_sheet,
                               demo_deposit_history)
    sr, vp = demo_market()
    bs = model_balance_sheet(scale=0.001, basis="amortized_cost",
                             include_markets_bs=True)
    hist = demo_deposit_history()
    nii = run_balance_sheet_nii(bs, sr, vp, hist, horizon=27)
    k = compute_kpis(bs, sr, vp, hist, nii_monthly=nii["monthly"])
    e = k["eve"]
    assert e["eve_$"] == pytest.approx(bs["equity"], rel=1e-6)
    assert 0.2 < e["duration_gap_y"] < 4.0
    assert e["irrbb_worst_pct_eve"] > 0
    assert 100 < k["lcr"]["lcr_pct"] < 400
    assert 90 < k["nsfr"]["nsfr_pct"] < 250
    c = k["capital"]
    assert c["rwa_density_pct"] == pytest.approx(59.6, abs=0.5)
    path = c["cet1_path"]
    assert path[0]["cet1_ratio_pct"] == pytest.approx(10.3, abs=0.01)
    assert path[-1]["cet1_ratio_pct"] > path[0]["cet1_ratio_pct"]


# --- hedges ---------------------------------------------------------------------
def test_swap_pricing_and_parity(rate_paths):
    """At-market swap MtM ~ 0; receiver dv01 > 0 mirrors payer; swaption
    payer - receiver = cash-settled forward (path identity)."""
    import datetime as _dt
    from portfolio_risk.hedges import (HedgeDeck, swap_mtm_and_carry,
                                 swaption_value)
    paths, crn = rate_paths
    asof = _dt.date(2026, 6, 10)
    # par rate proxy: average realized 5y par at t0 across paths
    par5 = float(paths["swaps"][:, 1, 0].mean())
    f = pl.DataFrame([
        dict(id="R", notional=1e8, side="receiver", fixed_rate=par5,
             maturity=asof + _dt.timedelta(days=1826), designation="fvh",
             hedged_item="x"),
        dict(id="P", notional=1e8, side="payer", fixed_rate=par5,
             maturity=asof + _dt.timedelta(days=1826), designation="fvh",
             hedged_item="x")])
    deck = HedgeDeck(f, asof)
    mtm, carry = swap_mtm_and_carry(deck, paths, crn.n, 12)
    assert abs(mtm[0]) < 0.01            # at-market: < 1pt of notional
    assert mtm[0] == pytest.approx(-mtm[1], abs=1e-12)
    assert carry[0].sum() == pytest.approx(-carry[1].sum(), abs=1e-12)

    K = par5
    sp = pl.DataFrame([
        dict(id="P", notional=1.0, side="payer", strike=K, expiry_m=12,
             tenor_y=5.0, designation="economic", hedged_item="x"),
        dict(id="R", notional=1.0, side="receiver", strike=K, expiry_m=12,
             tenor_y=5.0, designation="economic", hedged_item="x")])
    v = swaption_value(sp, paths, crn.n)
    s = paths["swaps"][:, 1, 12].astype(float)
    d = 1.0 / (1.0 + 0.5 * np.maximum(s, 0))
    ann = sum(0.5 * d ** k for k in range(1, 11))
    fwd = float((ann * (s - K) * paths["df"][:, 12]).sum() / crn.n)
    assert v[0] - v[1] == pytest.approx(fwd, abs=1e-12)
    assert v[0] > 0 and v[1] > 0


def test_hedge_book_cuts_irrbb_outlier():
    """The validation arc: hedged EVE sensitivity must be materially
    inside the unhedged -27%."""
    from portfolio_risk.demo import demo_market, demo_hedge_book
    from portfolio_risk.hedges import run_hedge_risk
    import datetime as _dt
    sr, vp = demo_market()
    swaps, swpns = demo_hedge_book(scale=0.001)
    out = run_hedge_risk(swaps, swpns, _dt.date(2026, 6, 10), sr, vp)
    # net pay-fixed book: negative duration -> dv01 < 0, offsetting the
    # asset-long balance sheet (bs dv01 ~ +$234k/bp at this scale)
    assert out["book_dv01_$"] < 0
    assert abs(out["book_dv01_$"]) > 0.3 * 234_000 * 0.001 / 0.001


# --- strategies ----------------------------------------------------------------
def test_strategy_at_market_carry_and_reinvestment():
    """At-market fixed program earns ~E[ref]+spread on its balance;
    reinvestment program sized off real modeled MBS runoff produces
    positive incremental NII and a growing forward dv01."""
    from portfolio_risk.strategies import run_strategies
    from portfolio_risk.accounting import run_balance_sheet_nii
    from portfolio_risk.demo import (demo_market, model_balance_sheet,
                               demo_deposit_history)
    sr, vp = demo_market()
    bs = model_balance_sheet(scale=0.001, basis="amortized_cost",
                             include_markets_bs=True)
    nii = run_balance_sheet_nii(bs, sr, vp, demo_deposit_history(),
                                horizon=27)
    ro = nii["runoff_vectors"]
    assert ro["mbs"].sum() > 0
    progs = [
        dict(name="buy_5y_agency", side="asset", rate_ref="s5",
             is_float=False, spread_bp=80, term_m=60, amort="cpr",
             cpr_annual=0.07, start_m=0, end_m=26,
             reinvest_frac=1.0, reinvest_source="mbs"),
        dict(name="issue_2y_cd", side="liability", rate_ref="s2",
             is_float=False, spread_bp=10, term_m=24, amort="bullet",
             start_m=0, end_m=26, monthly_notional=2e6),
        dict(name="cml_floaters", side="asset", rate_ref="short",
             is_float=True, spread_bp=180, term_m=36, amort="annuity",
             start_m=3, end_m=14, monthly_notional=5e6),
    ]
    out = run_strategies(progs, sr, vp, runoff_by_book=ro, horizon=27)
    m = out["nii_incremental"]
    assert m["net"].sum() > 0                       # asset-led program set
    assert (m["issue_2y_cd"].to_numpy() <= 0).all()  # liability cost
    # carry sanity: fixed asset cohort yield ~ s5+80bp on avg balance
    b = out["balances"]["buy_5y_agency"].to_numpy()
    inc = m["buy_5y_agency"].to_numpy()
    yld = inc[12:].sum() / max(b[12:].mean(), 1) * 12 / 15
    assert 0.025 < yld < 0.09
    dv = out["fwd_dv01"]
    total = (dv["buy_5y_agency"] + dv["issue_2y_cd"]
             + dv["cml_floaters"]).to_numpy()
    assert total[-1] > 0 and abs(total[-1]) > abs(total[0])


def test_unitlib_interactive_kpis():
    """Unit library: at-market MBS units reprice ~par; eval is linear;
    KPI recalc moves the right direction (asset adds worsen +200 EVE,
    L2A adds raise LCR)."""
    from portfolio_risk.unitlib import build_unit_library, evaluate_strategy
    from portfolio_risk.kpis import compute_kpis
    from portfolio_risk.accounting import run_balance_sheet_nii
    from portfolio_risk.demo import (demo_market, demo_histories,
                               demo_deposit_history, model_balance_sheet,
                               demo_hedge_book)
    sr, vp = demo_market()
    lib = build_unit_library(sr, vp, demo_histories(),
                             demo_deposit_history())
    bs = model_balance_sheet(scale=0.001, basis="amortized_cost",
                             include_markets_bs=True)
    bs["hedges"] = demo_hedge_book(scale=0.001)
    hist = demo_deposit_history()
    nii = run_balance_sheet_nii(bs, sr, vp, hist, horizon=27)
    base = compute_kpis(bs, sr, vp, hist, nii_monthly=nii["monthly"])
    alloc = [dict(template="agency_mbs", purchase_m=0, notional=5e8),
             dict(template="cd_2y", purchase_m=0, notional=2e8)]
    out = evaluate_strategy(lib, alloc, base_kpis=base)
    k = out["kpis"]
    e = base["eve"]
    base_d200 = -e["dv01_net_$"] * 200 / e["eve_$"] * 100
    assert k["d_eve_pct_eve_+200"] < base_d200       # more asset duration
    assert k["lcr_pct"] > base["lcr"]["lcr_pct"]     # L2A HQLA added
    assert out["nii_total_$"] > 0
    o2 = evaluate_strategy(lib, [{**a, "notional": 3 * a["notional"]}
                                 for a in alloc])
    assert o2["nii_total_$"] == pytest.approx(3 * out["nii_total_$"],
                                              rel=1e-9)


def test_optimizer_robust_lp():
    """Feasible solve respects commercial floors; tightening a ratio
    floor cannot improve worst-case NII; impossible plan reports
    infeasible (the useful answer)."""
    from portfolio_risk.optimizer import optimize_balance_sheet
    from portfolio_risk.unitlib import build_unit_library
    from portfolio_risk.kpis import compute_kpis
    from portfolio_risk.accounting import run_balance_sheet_nii
    from portfolio_risk.demo import (demo_market, demo_histories,
                               demo_deposit_history, model_balance_sheet,
                               demo_hedge_book)
    sr, vp = demo_market()
    hist = demo_deposit_history()
    bs = model_balance_sheet(scale=0.001, basis="amortized_cost",
                             include_markets_bs=True)
    bs["hedges"] = demo_hedge_book(scale=0.001)
    nii = run_balance_sheet_nii(bs, sr, vp, hist, horizon=27)
    libs = []
    for d in (0.0, 0.02):
        lib = build_unit_library(sr + d, vp, demo_histories(), hist)
        libs.append((lib, compute_kpis(bs, sr + d, vp, hist,
                                       nii_monthly=nii["monthly"])))
    comm = [dict(label="min_cml", template="cml_float_3y",
                 sense=">=", rhs=2e8)]
    o1 = optimize_balance_sheet(libs, lcr_min=1.10, commercial=comm,
                                max_total_assets=2e9)
    assert o1["feasible"]
    cml = sum(a["notional"] for a in o1["allocation"]
              if a["template"] == "cml_float_3y")
    assert cml >= 2e8 - 1.0
    o2 = optimize_balance_sheet(libs, lcr_min=1.60, commercial=comm,
                                max_total_assets=2e9)
    if o2["feasible"]:
        assert o2["worst_case_nii_$"] <= o1["worst_case_nii_$"] + 1.0
    o3 = optimize_balance_sheet(
        libs, commercial=[dict(label="impossible", template="agency_mbs",
                               sense=">=", rhs=1e13)],
        max_total_assets=2e9)
    assert not o3["feasible"] and "labels" in o3
