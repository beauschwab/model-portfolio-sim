"""Synthetic market data, model-fit histories, and portfolio for self-
contained runs and tests. Replace with real feeds in production."""
from __future__ import annotations

import numpy as np
import polars as pl

from .core.config import CAL_EXPIRIES, CAL_TENORS, DT


def demo_market():
    swap_rates = np.array([0.0425, 0.0405, 0.0395, 0.0392, 0.0391,
                           0.0398, 0.0408, 0.0422, 0.0430, 0.0436])
    e, n = np.meshgrid(CAL_EXPIRIES, CAL_TENORS, indexing="ij")
    vol = 0.26 - 0.012 * np.log(e) - 0.010 * np.log(n)
    return swap_rates, np.column_stack([e.ravel(), n.ravel(), vol.ravel()])


def demo_histories(n_months: int = 180, seed: int = 31):
    rng = np.random.default_rng(seed)
    T = n_months
    s10 = 0.04 + np.cumsum(rng.normal(0, 0.0018, T))
    s2 = s10 - 0.004 + np.cumsum(rng.normal(0, 0.0012, T))
    s5 = 0.5 * (s2 + s10) + rng.normal(0, 0.0005, T)
    s30 = s10 + 0.003 + rng.normal(0, 0.0006, T)
    vols = np.clip(0.24 + np.cumsum(rng.normal(0, 0.004, (T, 6)), 0) * 0.3,
                   0.10, 0.60)
    fair = (0.010 + 0.85 * s10 + 0.10 * (s10 - s2) + 0.04 * s30
            + 0.015 * vols[:, 0])
    cc = np.empty(T)
    cc[0] = fair[0]
    for m in range(1, T):
        cc[m] = cc[m - 1] + 0.35 * (fair[m] - cc[m - 1]) + rng.normal(0, 4e-4)
    cc_hist = pl.DataFrame({"cc": cc, "s2": s2, "s5": s5, "s10": s10,
                            "s30": s30,
                            **{f"v{j}": vols[:, j] for j in range(6)}})
    ps = np.empty(T)
    ps[0] = 0.012
    for m in range(1, T):
        ps[m] = ps[m - 1] + 3.0 * (0.012 - ps[m - 1]) * DT \
            + 0.004 * np.sqrt(DT) * rng.standard_normal()
    return cc_hist, pl.DataFrame({"ps": ps})


def demo_portfolio(n: int = 10_000, seed: int = 11) -> pl.DataFrame:
    rng = np.random.default_rng(seed)
    wac = rng.uniform(0.030, 0.075, n)
    return pl.DataFrame({
        "cusip": [f"DEMO{i:06d}" for i in range(n)],
        "current_face": rng.uniform(1e6, 5e7, n).round(0),
        "factor": rng.uniform(0.35, 1.0, n).round(4),
        "wac": wac.round(5),
        "net_coupon": (wac - 0.005).round(5),
        "wam": rng.integers(180, 360, n).astype(float),
        "age": rng.integers(0, 120, n).astype(float),
        "oltv": rng.uniform(0.55, 0.97, n).round(3),
        "fico": rng.integers(620, 800, n).astype(float),
        "avg_loan_size": rng.uniform(80e3, 550e3, n).round(0),
        "state": rng.choice(["CA", "NY", "TX", "FL", "OH"], n).tolist(),
        "channel": rng.choice(["R", "B", "C"], n).tolist(),
        "price": rng.uniform(85.0, 104.0, n).round(3),
    })


def demo_deposit_history(n_months: int = 360, seed: int = 41
                         ) -> pl.DataFrame:
    """Synthetic fed funds + deposit rate generated from KNOWN logistic-
    beta/ECM params (b_max=0.60, pivot=2.5%, lam_up=0.12, lam_dn=0.35).
    The ff path includes a deliberate sustained hiking regime to 7-8% --
    without high-rate visits the logistic plateau is UNIDENTIFIED (the
    real-world deposit-beta problem); the fitter warns in that case."""
    rng = np.random.default_rng(seed)
    T = n_months
    # regime-switching target: low / mid / high blocks
    tgt = np.concatenate([np.full(T // 3, 0.015), np.full(T // 3, 0.045),
                          np.full(T - 2 * (T // 3), 0.075)])
    ff = np.empty(T)
    ff[0] = 0.02
    for m in range(1, T):
        ff[m] = np.clip(ff[m - 1] + 0.08 * (tgt[m] - ff[m - 1])
                        + rng.normal(0.0, 0.0012), 0.0, 0.09)
    beta = 0.05 + 0.55 / (1.0 + np.exp(-150.0 * (ff - 0.025)))
    eq = 0.001 + ff * beta
    r = np.empty(T)
    r[0] = eq[0]
    for m in range(1, T):
        g = eq[m] - r[m - 1]
        lam = 0.12 if g > 0 else 0.35
        r[m] = max(r[m - 1] + lam * g + rng.normal(0, 1e-4), 0.0)
    return pl.DataFrame({"ff": ff, "dep_rate": r})


def demo_deposit_book(n: int = 200, seed: int = 13) -> pl.DataFrame:
    rng = np.random.default_rng(seed)
    segs = rng.choice(["DDA", "NOW", "SAV", "MMDA"], n,
                      p=[0.35, 0.15, 0.30, 0.20])
    rate = np.where(segs == "DDA", 0.0,
                    np.where(segs == "MMDA",
                             rng.uniform(0.025, 0.04, n),
                             rng.uniform(0.005, 0.02, n)))
    return pl.DataFrame({
        "id": [f"DEP{i:05d}" for i in range(n)],
        "balance": rng.uniform(5e6, 5e8, n).round(0),
        "segment": segs.tolist(),
        "age_months": rng.integers(1, 240, n).astype(float),
        "avg_account_size": rng.uniform(2e3, 5e5, n).round(0),
        "rate_paid": rate.round(4),
        "svc_cost": np.full(n, 0.0015),
        "price": rng.uniform(93.0, 99.5, n).round(2),
    })


def demo_cd_book(n: int = 100, seed: int = 17,
                 asof=None) -> pl.DataFrame:
    import datetime as _dt
    asof = asof or _dt.date(2026, 6, 10)
    rng = np.random.default_rng(seed)
    ch = rng.choice(["retail", "brokered"], n, p=[0.7, 0.3])
    rows = []
    for i in range(n):
        term_m = int(rng.choice([6, 12, 18, 24, 36, 60]))
        rows.append(dict(
            id=f"CD{i:05d}", balance=float(rng.uniform(1e6, 1e8)),
            rate=float(rng.uniform(0.030, 0.052)),
            maturity=asof + _dt.timedelta(days=int(term_m * 30.44)),
            freq_months=int(rng.choice([0, 6])) if term_m >= 12 else 0,
            daycount="ACT/365F", channel=str(ch[i]),
            penalty_months=3.0 if term_m <= 12 else 6.0,
            price=float(rng.uniform(98.0, 101.5)),
        ))
    df = pl.DataFrame(rows)
    calls = [[(asof + _dt.timedelta(days=180), 1.0),
              (asof + _dt.timedelta(days=365), 1.0)]
             if c == "brokered" else None for c in ch]
    return df.with_columns(pl.Series("call_schedule", calls, dtype=pl.Object))


def model_balance_sheet(scale: float = 0.01, seed: int = 23,
                        asof=None, basis: str = "market",
                        include_markets_bs: bool = True) -> dict:
    """WFC-PROPORTIONAL model balance sheet (NOT Wells Fargo's actual
    positions -- a synthetic book sized to the published mix). Source:
    Wells Fargo 1Q26 Quarterly Supplement (8-K Ex 99.2, filed 2026-04-14),
    period-end Mar 31, 2026, $MM:
      AFS debt securities 222,873 (avg yield 4.44%); HTM 204,080 (2.27%)
      Loans 1,016,787 (avg 5.62%): consumer ~337B (Home Lending 198.5B,
        Card 52.3B, Auto 54.3B, Personal 13.6B, CSBB 17.9B),
        commercial remainder ~680B
      Deposits 1,454,939: noninterest-bearing 365,712; interest-bearing
        1,089,227 (avg rate 1.90%; avg deposit cost 1.43% all-in)
      Long-term debt 183,941 (avg rate 5.25%); equity 180,313
      1Q26 NII 12,096; NIM 2.47%; 2026 NII guide ~$50B
    Mapping to engines (modeled subset of the earning balance sheet):
      securities -> MBS book (AFS higher-coupon + HTM low-coupon
        underwater mix); Home Lending -> MBS whole-loan proxy (flagged);
      commercial loans -> corp deck (60% SOFR floaters / 40% fixed);
      auto -> corp annuity fixed; LT debt -> corp deck as LIABILITY;
      noninterest DDA + savings/MMDA -> NMD deposit book;
      time deposits (ASSUMED ~100B split of interest-bearing; the
        supplement does not break out time deposits) -> CD book.
    `basis="amortized_cost"` attaches filing average yields as
    book_yield overrides on the securities books, so income accrues at
    the HOLDER's historical-cost basis (AFS 4.44%/HTM 2.27%/HL 5.62%)
    rather than the market-implied IRR at today's prices -- the dominant
    driver of model-vs-reported NIM otherwise (+50-90bp). It also pins
    IB deposit rates to the reported 1.90% average cost.
    `include_markets_bs` adds the spread-to-short money-market book
    (IEDB 141.2@3.38, resale 215.6@3.67, trading 221.7@3.89 vs repo
    234.4@3.74, ST borrow 32.3@4.04, trading liab 53.6@3.15) whose ~1.5%
    net spread dilutes the headline NIM by ~38bp (the "Markets balance
    sheet growth" effect WFC's CFO flagged on the 1Q26 call). With both
    on, model NIM reconciles to the reported 2.47% within tolerance.
    Still excluded: credit-card revolvers, equity securities.
    `scale` shrinks all balances (default 1% of WFC)."""
    import datetime as _dt
    asof = asof or _dt.date(2026, 6, 10)
    rng = np.random.default_rng(seed)
    MM = 1e6 * scale

    # --- securities + home lending as MBS book --------------------------------
    def pools(n, bal_tot, net_lo, net_hi, px_lo, px_hi, pfx, age_hi=120):
        b = rng.dirichlet(np.ones(n)) * bal_tot
        net = rng.uniform(net_lo, net_hi, n)
        return pl.DataFrame({
            "cusip": [f"{pfx}{i:04d}" for i in range(n)],
            "current_face": b, "net_coupon": net.round(4),
            "wac": (net + rng.uniform(0.0045, 0.0065, n)).round(4),
            "wam": rng.integers(240, 358, n).astype(float),
            "age": rng.integers(3, age_hi, n).astype(float),
            "oltv": rng.uniform(0.60, 0.85, n).round(3),
            "factor": rng.uniform(0.55, 1.0, n).round(3),
            "fico": rng.integers(680, 800, n).astype(float),
            "avg_loan_size": rng.uniform(2.2e5, 4.5e5, n).round(0),
            "state": rng.choice(["CA", "TX", "FL", "NY", "OTHER"], n),
            "channel": rng.choice(["retail", "broker"], n, p=[0.8, 0.2]),
            "price": rng.uniform(px_lo, px_hi, n).round(3),
        })
    mbs = pl.concat([
        pools(40, 222_873 * MM, 0.040, 0.055, 97.0, 102.0, "AFS"),   # 4.44%
        pools(40, 204_080 * MM, 0.018, 0.030, 78.0, 90.0, "HTM",     # 2.27%
              age_hi=70),
        pools(40, 198_516 * MM, 0.030, 0.062, 88.0, 101.0, "HL"),    # 5.62% bk
    ])

    # --- commercial + auto loans (corp deck) -----------------------------------
    def corp_rows(n, tot, pfx, float_p, cpn_rng, spr_rng, term_rng, amort):
        b = rng.dirichlet(np.ones(n)) * tot
        rows = []
        for i in range(n):
            fl = rng.random() < float_p
            t = int(rng.uniform(*term_rng))
            rows.append(dict(
                id=f"{pfx}{i:04d}", face=float(b[i]),
                maturity=asof + _dt.timedelta(days=int(t * 30.44)),
                freq_months=3 if fl else 6, daycount="ACT/360",
                is_float=int(fl),
                coupon_or_spread=float(rng.uniform(*spr_rng) if fl
                                       else rng.uniform(*cpn_rng)),
                amort_type=amort, price=float(rng.uniform(98.5, 101.0)),
            ))
        return pl.DataFrame(rows)
    loans = pl.concat([
        corp_rows(60, 680_000 * MM, "CML", 0.60, (0.048, 0.068),
                  (0.012, 0.028), (12, 84), "bullet"),
        corp_rows(30, 54_279 * MM, "AUTO", 0.0, (0.055, 0.085),
                  (0, 0), (24, 72), "annuity"),
    ])
    debt = corp_rows(25, 183_941 * MM, "LTD", 0.25, (0.046, 0.060),
                     (0.008, 0.018), (24, 240), "bullet")   # 5.25% avg

    # --- deposits: DDA (noninterest) + savings/MMDA (interest-bearing NMD) ----
    def dep_rows(n, tot, segs, pseg, rate_fn, pfx):
        b = rng.dirichlet(np.ones(n)) * tot
        sg = rng.choice(segs, n, p=pseg)
        return pl.DataFrame({
            "id": [f"{pfx}{i:04d}" for i in range(n)],
            "balance": b, "segment": sg.tolist(),
            "age_months": rng.integers(6, 240, n).astype(float),
            "avg_account_size": rng.uniform(3e3, 4e5, n).round(0),
            "rate_paid": np.array([rate_fn(s) for s in sg]).round(4),
            "svc_cost": np.full(n, 0.0015),
            "price": rng.uniform(94.0, 99.0, n).round(2),
        })
    deposits = pl.concat([
        dep_rows(40, 365_712 * MM, ["DDA"], [1.0], lambda s: 0.0, "NIB"),
        dep_rows(60, 989_227 * MM, ["NOW", "SAV", "MMDA"],
                 [0.25, 0.35, 0.40],
                 lambda s: {"NOW": 0.004, "SAV": 0.010,
                            "MMDA": 0.032}[s] * (0.8 + 0.4 * rng.random()),
                 "IB"),
    ])
    cds = demo_cd_book(40, seed=seed + 1, asof=asof).with_columns(
        (pl.col("balance") / pl.col("balance").sum()
         * 100_000 * MM).alias("balance"))
    if basis == "amortized_cost":
        by = np.where([c.startswith("HTM") for c in mbs["cusip"].to_list()],
                      0.0227,
                      np.where([c.startswith("AFS")
                                for c in mbs["cusip"].to_list()],
                               0.0444, 0.0562))
        mbs = mbs.with_columns(pl.Series("book_yield", by))
        loans = loans.with_columns(pl.lit(0.0562).alias("book_yield"))
        debt = debt.with_columns(pl.lit(0.0525).alias("book_yield"))
        # pin IB deposit cost to the reported 1.90% average
        ib = deposits["id"].str.starts_with("IB")
        cur = deposits.filter(ib)
        w = (cur["rate_paid"] * cur["balance"]).sum() / cur["balance"].sum()
        deposits = deposits.with_columns(
            pl.when(ib).then(pl.col("rate_paid") * (0.0190 / w))
              .otherwise(pl.col("rate_paid")).alias("rate_paid"))

    mm = None
    if include_markets_bs:
        # period-end balances ($MM) and avg rates, spread to IEDB 3.38% ~ ff
        rows = [("IEDB", 141_241, "asset", 0),
                ("RESALE", 215_599, "asset", 29),
                ("TRADING_A", 221_711, "asset", 51),
                ("REPO", 234_371, "liability", 36),
                ("ST_BORROW", 32_282, "liability", 66),
                ("TRADING_L", 53_647, "liability", -23)]
        mm = pl.DataFrame([dict(id=i, balance=b * MM, side=s,
                                spread_bp=float(sp), category=i)
                           for i, b, s, sp in rows])

    # --- balance the cut: assets - liabilities must equal modeled equity
    # (WFC total equity 180,313MM x scale). Without this plug the modeled
    # EVE is an artifact of which categories were excluded from each side;
    # deposits absorb the adjustment (they are the funding residual in
    # practice too).
    equity = 180_313 * MM
    a = (mbs["current_face"] * mbs["price"] / 100).sum() \
        + (loans["face"] * loans["price"] / 100).sum()
    li = (debt["face"] * debt["price"] / 100).sum() \
        + (cds["balance"] * cds["price"] / 100).sum()
    if mm is not None:
        a += mm.filter(pl.col("side") == "asset")["balance"].sum()
        li += mm.filter(pl.col("side") == "liability")["balance"].sum()
    dep_target = a - li - equity
    dep_mv = (deposits["balance"] * deposits["price"] / 100).sum()
    deposits = deposits.with_columns(
        (pl.col("balance") * dep_target / dep_mv).alias("balance"))

    return {"mbs": mbs, "loans": loans, "debt": debt,
            "deposits": deposits, "cds": cds, "mm": mm, "equity": equity,
            "mbs_hists": demo_histories(),
            "asof": asof, "scale": scale,
            "source": "WFC 1Q26 Quarterly Supplement, Mar 31 2026 ($MM): "
                      "AFS 222,873@4.44%/HTM 204,080@2.27%, loans "
                      "1,016,787@5.62%, deposits 1,454,939 (NIB 365,712; "
                      "IB 1,089,227@1.90%), LTD 183,941@5.25%; NIM 2.47%"}


def demo_hedge_book(scale: float = 0.01, asof=None) -> tuple:
    """ASC 815 hedge book sized to pull the demo balance sheet's IRRBB
    outlier (-27% EVE @ +200bp, asset-long 1.3y gap) back inside: ~$340B
    full-scale pay-fixed (FVH on AFS securities + CFH on forecasted
    deposit rollover funding cost) less receive-fixed converting LTD to
    floating (FVH) -- directionally how a G-SIB hedges; sizes synthetic."""
    import datetime as _dt
    asof = asof or _dt.date(2026, 6, 10)
    MM = 1e6 * scale

    def mat(y):
        return asof + _dt.timedelta(days=int(y * 365.25))
    swaps = pl.DataFrame([
        dict(id="PAY_AFS_5Y", notional=260_000 * MM, side="payer",
             fixed_rate=0.0395, maturity=mat(5), designation="fvh",
             hedged_item="mbs:AFS"),
        dict(id="PAY_DEP_3Y", notional=190_000 * MM, side="payer",
             fixed_rate=0.0385, maturity=mat(3), designation="cfh",
             hedged_item="deposits:rollover"),
        dict(id="RCV_LTD_7Y", notional=60_000 * MM, side="receiver",
             fixed_rate=0.0410, maturity=mat(7), designation="fvh",
             hedged_item="debt:LTD"),
    ])
    swpns = pl.DataFrame([
        dict(id="PAYER_1Y5Y", notional=50_000 * MM, side="payer",
             strike=0.0475, expiry_m=12, tenor_y=5.0,
             designation="economic", hedged_item="tail:+200"),
        dict(id="RCVR_1Y10Y", notional=30_000 * MM, side="receiver",
             strike=0.0330, expiry_m=12, tenor_y=10.0,
             designation="economic", hedged_item="tail:-150/extension"),
    ])
    return swaps, swpns
