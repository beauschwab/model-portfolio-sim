"""Top-level balance-sheet KPIs: EVE & duration gap, LCR, NSFR, CET1 and
capital projection. Built on the engines' modeled behavior wherever it
exists -- deposit runoff, CD maturities, MBS WALs -- with STYLIZED
regulatory weight tables (the calibration seam) where firm-internal
mappings would apply. Every table below is module data: replace with
your 12 CFR 249 / NSFR / standardized-approach internal mappings before
treating any ratio as more than directional.

EVE: market value of assets minus liabilities at solved model prices
(prices ARE market by construction -- OAS solved to targets). Rate
sensitivity from PARALLEL dv01s computed by +/-25bp full revaluations
per book (3 engine passes each, shared CRN); Delta-EVE per shock is
FIRST-ORDER (dv01 x shock). Convexity lives in the 9Q stress pack -- use
run_stress / run_deposit_stress for the nonlinear picture; the KPI table
says so in its method column. Duration gap = DurA - (L/A) x DurL with
Dur = dv01 * 1e4 / MV.

LCR (30-day): HQLA / net outflows. Level 1 = central-bank reserves
(IEDB); Level 2A = agency MBS at 85% with the 40% composition cap.
Outflows from deposit segments at stylized runoff rates, CD/LTD
contractual maturities <= 30d at par (REAL deck maturities, not buckets),
secured funding rollover assumptions on repo. Inflows capped at 75% of
outflows.

NSFR: ASF/RSF with maturity taken from the decks (CD/LTD/corp actual
maturities; deposit stability by segment) and RSF using modeled asset
class + remaining maturity.

Capital: standardized-approach credit RWA from category risk weights +
an op/market add-on calibrated so t0 RWA density matches the filing
(59.6% of assets, WFC 1Q26); CET1_0 set to the reported 10.3% ratio on
model RWA. Projection: CET1_{q+1} = CET1_q + NII_q x (1-tax) x
(1-payout) + Delta-AOCI_q from AFS stress marks (optional stress leg).
NOT modeled: provisions, opex, fee income, RWA migration -- this is the
NII-and-AOCI skeleton of a capital plan, not PPNR.
"""
from __future__ import annotations

import datetime as dt

import numpy as np
import polars as pl

# ---- stylized weight tables (REPLACE with internal regulatory mappings) -----
LCR_RUNOFF = {"DDA": 0.05, "NOW": 0.10, "SAV": 0.10, "MMDA": 0.20}
LCR_CD_RUNOFF = {"retail": 0.10, "brokered": 1.00}     # maturing <= 30d
LCR_SECURED = {"REPO": 0.25, "ST_BORROW": 1.00, "TRADING_L": 0.00}
LCR_INFLOW = {"RESALE": 0.50}
L2A_FACTOR, L2_CAP = 0.85, 0.40
NSFR_ASF = {"equity": 1.00, "DDA": 0.95, "NOW": 0.95, "SAV": 0.95,
            "MMDA": 0.90, "cd_retail_lt1y": 0.95, "cd_brokered_lt6m": 0.50,
            "cd_ge1y": 1.00, "ltd_ge1y": 1.00, "ltd_lt1y": 0.50,
            "wholesale_st": 0.00}
NSFR_RSF = {"IEDB": 0.00, "RESALE": 0.15, "TRADING_A": 0.50,
            "mbs_l2a": 0.15, "resi_mtg": 0.65, "loan_ge1y": 0.85,
            "loan_lt1y": 0.50}
RWA_W = {"agency_mbs": 0.20, "resi_mtg": 0.50, "corp_loan": 1.00,
         "auto": 1.00, "IEDB": 0.00, "RESALE": 0.10, "TRADING_A": 0.30}
RWA_DENSITY_TARGET = 0.596        # WFC 1Q26: RWA 1,315.6 / assets 2,205.8
CET1_RATIO_T0 = 0.103             # WFC 1Q26 standardized CET1
TAX, PAYOUT = 0.21, 0.45
NI_TO_NII = 0.43   # WFC 1Q26: net income 5,253 / NII 12,096 -- carries
#                    provisions, opex, and fee income net effect; replace
#                    with a planned ratio for forward quarters


def _mv(frame: pl.DataFrame) -> float:
    bal = frame["balance" if "balance" in frame.columns else
                "current_face" if "current_face" in frame.columns
                else "face"].to_numpy()
    px = frame["price"].to_numpy() / 100.0 if "price" in frame.columns \
        else np.ones(len(frame))
    return float((bal * px).sum())


def parallel_dv01s(bs: dict, swap_rates, vol_pts, dep_hist, seed: int = 7,
                   bump_bp: float = 25.0) -> dict[str, float]:
    """$ parallel dv01 per book via +/-bump full revaluations on shared
    CRN paths (base OAS solved once and held fixed -- engine invariant)."""
    from .cds import CDDeck, _cd_A
    from .config import N_PATHS_SENS, SWAP_TENORS
    from .corp import CorpDeck, _corp_A, corp_pv, corp_solve_oas
    from .curve import bootstrap_curve, forwards_from_dfs
    from .deposits import DepositDeck, LogisticBetaECM, _deposit_A
    from .pricing import pv_from_A, solve_oas_from_A
    from .scenarios import (CRN, build_paths, build_rate_paths, run_engine,
                            setup, solve_base_oas)
    from .vol import calibrate_abcd, factor_loadings

    d = bump_bp * 1e-4
    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    crn = CRN(N_PATHS_SENS, seed)
    out: dict[str, float] = {}
    asof = bs.get("asof") or dt.date.today()

    def rp(sr):
        return build_rate_paths(sr, vol_pts, abcd0, B, crn)

    if bs.get("mbs") is not None:
        port = bs["mbs"]
        cc_hist, ps_hist = bs["mbs_hists"]
        models, B2, a2, sec, tgt, face = setup(port, swap_rates, vol_pts,
                                               cc_hist, ps_hist)
        oas, _ = solve_base_oas(swap_rates, vol_pts, a2, B2, models, sec,
                                tgt, seed=seed, n_paths=crn.n)

        def pv(sr):
            paths = build_paths(sr, vol_pts, a2, B2, models, crn)
            A, *_ = run_engine(paths, sec)
            return float((pv_from_A(A, oas, crn.n) * face).sum())
        out["mbs"] = (pv(swap_rates - d) - pv(swap_rates + d)) \
            / (2 * bump_bp)

    for key in ("loans", "debt"):
        if bs.get(key) is None:
            continue
        deck = CorpDeck(bs[key], asof)
        A = _corp_A(deck, rp(swap_rates))
        oas, _ = corp_solve_oas(deck, A, crn.n)

        def pv(sr, deck=deck, oas=oas):
            return float((corp_pv(deck, _corp_A(deck, rp(sr)), oas, crn.n)
                          * deck.face).sum())
        out[key] = (pv(swap_rates - d) - pv(swap_rates + d)) / (2 * bump_bp)

    if bs.get("deposits") is not None:
        deck = DepositDeck(bs["deposits"])
        m = LogisticBetaECM()
        params = m.fit(dep_hist)
        base = rp(swap_rates)
        r0 = float(m.equilibrium(params, base["short"][:, 0].mean()))

        def dep_pv(sr, oas=None):
            paths = rp(sr)
            dep = m.paths(paths["short"].astype(np.float64), params, r0)
            A, *_ = _deposit_A(deck, paths, dep, r0)
            return A
        A0 = dep_pv(swap_rates)
        oas, _ = solve_oas_from_A(A0, crn.n, deck.tgt, lo0=-0.15)

        def pv(sr):
            return float((pv_from_A(dep_pv(sr), oas, crn.n)
                          * deck.bal).sum())
        out["deposits"] = (pv(swap_rates - d) - pv(swap_rates + d)) \
            / (2 * bump_bp)

    if bs.get("cds") is not None:
        deck = CDDeck(bs["cds"], asof)
        A = _cd_A(deck, rp(swap_rates))
        oas, _ = corp_solve_oas(deck, A, crn.n)

        def pv(sr):
            return float((corp_pv(deck, _cd_A(deck, rp(sr)), oas, crn.n)
                          * deck.bal).sum())
        out["cds"] = (pv(swap_rates - d) - pv(swap_rates + d)) / (2 * bump_bp)

    out["mm"] = 0.0   # monthly reset; duration ~ 0 by construction

    if bs.get("hedges") is not None:
        from .hedges import HedgeDeck, swap_mtm_and_carry, swaption_value
        swaps, swpns = bs["hedges"]
        deck = HedgeDeck(swaps, asof)

        def hpv(sr):
            paths = rp(sr)
            v = float((swap_mtm_and_carry(deck, paths, crn.n, 1)[0]
                       * deck.notional).sum())
            if swpns is not None:
                v += float((swaption_value(swpns, paths, crn.n)
                            * swpns["notional"].to_numpy()).sum())
            return v
        out["hedges"] = (hpv(swap_rates - d) - hpv(swap_rates + d)) \
            / (2 * bump_bp)
    return out


def eve_summary(bs: dict, dv01s: dict[str, float],
                shocks_bp=(-200, -100, 100, 200)) -> dict:
    mv_a = sum(_mv(bs[k]) for k in ("mbs", "loans") if bs.get(k) is not None)
    mm_a = mm_l = 0.0
    if bs.get("mm") is not None:
        mm = bs["mm"]
        mm_a = float(mm.filter(pl.col("side") == "asset")["balance"].sum())
        mm_l = float(mm.filter(pl.col("side") == "liability")
                     ["balance"].sum())
    mv_a += mm_a
    mv_l = sum(_mv(bs[k]) for k in ("debt", "deposits", "cds")
               if bs.get(k) is not None) + mm_l
    eve = mv_a - mv_l
    dv_a = dv01s.get("mbs", 0) + dv01s.get("loans", 0) \
        + dv01s.get("mm", 0) + dv01s.get("hedges", 0.0)
    dv_l = dv01s.get("debt", 0) + dv01s.get("deposits", 0) \
        + dv01s.get("cds", 0)
    dv_net = dv_a - dv_l
    dur_a = dv_a * 1e4 / max(mv_a, 1e-9)
    dur_l = dv_l * 1e4 / max(mv_l, 1e-9)
    rows = [{"shock_bp": s, "d_eve_$": -dv_net * s,
             "d_eve_pct_eve": -dv_net * s / eve * 100.0,
             "method": "first-order (parallel dv01); convexity in 9Q stress"}
            for s in shocks_bp]
    worst = max(abs(r["d_eve_pct_eve"]) for r in rows)
    return {"eve_$": eve, "mv_assets_$": mv_a, "mv_liabilities_$": mv_l,
            "irrbb_outlier": bool(worst > 15.0),
            "irrbb_worst_pct_eve": worst,
            "dv01_net_$": dv_net, "dur_assets_y": dur_a, "dur_liab_y": dur_l,
            "duration_gap_y": dur_a - (mv_l / mv_a) * dur_l,
            "eve_duration_y": dv_net * 1e4 / max(eve, 1e-9),
            "hedge_dv01_$": dv01s.get("hedges", 0.0),
            "sensitivity": rows}


def lcr(bs: dict, asof: dt.date) -> dict:
    """Stylized 30-day LCR. CD/LTD outflows use REAL contractual
    maturities from the frames; deposits use segment runoff rates."""
    hqla_l1 = hqla_l2 = 0.0
    if bs.get("mm") is not None:
        hqla_l1 += float(bs["mm"].filter(pl.col("id") == "IEDB")
                         ["balance"].sum())
    if bs.get("mbs") is not None:
        agency = bs["mbs"].filter(~pl.col("cusip").str.starts_with("HL"))
        hqla_l2 += _mv(agency) * L2A_FACTOR
    hqla_l2 = min(hqla_l2, hqla_l1 * L2_CAP / (1 - L2_CAP))
    hqla = hqla_l1 + hqla_l2

    out = 0.0
    if bs.get("deposits") is not None:
        d = bs["deposits"]
        for seg, w in LCR_RUNOFF.items():
            out += float(d.filter(pl.col("segment") == seg)
                         ["balance"].sum()) * w
    cutoff = asof + dt.timedelta(days=30)
    if bs.get("cds") is not None:
        cd = bs["cds"].filter(pl.col("maturity") <= cutoff)
        for ch, w in LCR_CD_RUNOFF.items():
            out += float(cd.filter(pl.col("channel") == ch)
                         ["balance"].sum()) * w
    if bs.get("debt") is not None:
        out += float(bs["debt"].filter(pl.col("maturity") <= cutoff)
                     ["face"].sum())
    if bs.get("mm") is not None:
        for cat, w in LCR_SECURED.items():
            out += float(bs["mm"].filter(pl.col("id") == cat)
                         ["balance"].sum()) * w
    inflow = 0.0
    if bs.get("mm") is not None:
        for cat, w in LCR_INFLOW.items():
            inflow += float(bs["mm"].filter(pl.col("id") == cat)
                            ["balance"].sum()) * w
    inflow = min(inflow, 0.75 * out)
    net_out = max(out - inflow, 1e-9)
    return {"hqla_l1_$": hqla_l1, "hqla_l2a_$": hqla_l2, "hqla_$": hqla,
            "outflows_$": out, "inflows_capped_$": inflow,
            "net_outflows_$": net_out, "lcr_pct": hqla / net_out * 100.0}


def nsfr(bs: dict, asof: dt.date) -> dict:
    """Stylized NSFR with deck maturities driving the ASF/RSF buckets."""
    y1 = asof + dt.timedelta(days=365)
    m6 = asof + dt.timedelta(days=182)
    asf = 0.0
    if bs.get("deposits") is not None:
        d = bs["deposits"]
        for seg in LCR_RUNOFF:
            asf += float(d.filter(pl.col("segment") == seg)
                         ["balance"].sum()) * NSFR_ASF[seg]
    if bs.get("cds") is not None:
        cd = bs["cds"]
        asf += float(cd.filter(pl.col("maturity") >= y1)
                     ["balance"].sum()) * NSFR_ASF["cd_ge1y"]
        lt = cd.filter(pl.col("maturity") < y1)
        asf += float(lt.filter(pl.col("channel") == "retail")
                     ["balance"].sum()) * NSFR_ASF["cd_retail_lt1y"]
        asf += float(lt.filter((pl.col("channel") == "brokered")
                               & (pl.col("maturity") < m6))
                     ["balance"].sum()) * NSFR_ASF["cd_brokered_lt6m"]
    if bs.get("debt") is not None:
        db = bs["debt"]
        asf += float(db.filter(pl.col("maturity") >= y1)["face"].sum()) \
            * NSFR_ASF["ltd_ge1y"]
        asf += float(db.filter(pl.col("maturity") < y1)["face"].sum()) \
            * NSFR_ASF["ltd_lt1y"]
    eve_proxy = sum(_mv(bs[k]) for k in ("mbs", "loans")
                    if bs.get(k) is not None) \
        - sum(_mv(bs[k]) for k in ("debt", "deposits", "cds")
              if bs.get(k) is not None)
    asf += max(eve_proxy, 0.0) * NSFR_ASF["equity"]

    rsf = 0.0
    if bs.get("mm") is not None:
        for cat in ("IEDB", "RESALE", "TRADING_A"):
            rsf += float(bs["mm"].filter(pl.col("id") == cat)
                         ["balance"].sum()) * NSFR_RSF[cat]
    if bs.get("mbs") is not None:
        mbs = bs["mbs"]
        hl = mbs.filter(pl.col("cusip").str.starts_with("HL"))
        rsf += _mv(hl) * NSFR_RSF["resi_mtg"]
        rsf += _mv(mbs.filter(~pl.col("cusip").str.starts_with("HL"))) \
            * NSFR_RSF["mbs_l2a"]
    if bs.get("loans") is not None:
        ln = bs["loans"]
        rsf += float(ln.filter(pl.col("maturity") >= y1)["face"].sum()) \
            * NSFR_RSF["loan_ge1y"]
        rsf += float(ln.filter(pl.col("maturity") < y1)["face"].sum()) \
            * NSFR_RSF["loan_lt1y"]
    return {"asf_$": asf, "rsf_$": rsf,
            "nsfr_pct": asf / max(rsf, 1e-9) * 100.0}


def capital(bs: dict, nii_monthly: pl.DataFrame | None,
            stress_aoci_q: list[float] | None = None) -> dict:
    """Standardized credit RWA + density-calibrated add-on; CET1 path
    accreting retained NII, with optional AFS-mark AOCI leg."""
    rwa = 0.0
    if bs.get("mbs") is not None:
        mbs = bs["mbs"]
        rwa += _mv(mbs.filter(pl.col("cusip").str.starts_with("HL"))) \
            * RWA_W["resi_mtg"]
        rwa += _mv(mbs.filter(~pl.col("cusip").str.starts_with("HL"))) \
            * RWA_W["agency_mbs"]
    if bs.get("loans") is not None:
        ln = bs["loans"]
        rwa += float(ln.filter(pl.col("id").str.starts_with("AUTO"))
                     ["face"].sum()) * RWA_W["auto"]
        rwa += float(ln.filter(~pl.col("id").str.starts_with("AUTO"))
                     ["face"].sum()) * RWA_W["corp_loan"]
    if bs.get("mm") is not None:
        for cat in ("IEDB", "RESALE", "TRADING_A"):
            rwa += float(bs["mm"].filter(pl.col("id") == cat)
                         ["balance"].sum()) * RWA_W[cat]
    assets = sum(_mv(bs[k]) for k in ("mbs", "loans")
                 if bs.get(k) is not None)
    if bs.get("mm") is not None:
        assets += float(bs["mm"].filter(pl.col("side") == "asset")
                        ["balance"].sum())
    addon = max(RWA_DENSITY_TARGET * assets - rwa, 0.0)
    rwa_total = rwa + addon
    cet1 = CET1_RATIO_T0 * rwa_total
    path = [{"quarter": 0, "cet1_$": cet1,
             "cet1_ratio_pct": cet1 / rwa_total * 100.0, "drivers": "t0"}]
    if nii_monthly is not None:
        nii = nii_monthly["nii"].to_numpy()
        nq = len(nii) // 3
        for q in range(nq):
            retained = nii[q * 3:(q + 1) * 3].sum() * NI_TO_NII \
                * (1 - PAYOUT)
            aoci = (stress_aoci_q[q] if stress_aoci_q
                    and q < len(stress_aoci_q) else 0.0)
            cet1 += retained + aoci
            path.append({"quarter": q + 1, "cet1_$": cet1,
                         "cet1_ratio_pct": cet1 / rwa_total * 100.0,
                         "drivers": f"retained {retained/1e6:.0f}M"
                                    + (f", AOCI {aoci/1e6:+.0f}M"
                                       if aoci else "")})
    return {"rwa_credit_$": rwa, "rwa_addon_$": addon,
            "rwa_total_$": rwa_total, "rwa_density_pct":
            rwa_total / max(assets, 1e-9) * 100.0,
            "cet1_t0_$": CET1_RATIO_T0 * rwa_total,
            "cet1_path": path,
            "cfh_aoci_note": "cash-flow-hedge AOCI is EXCLUDED from "
            "CET1 (12 CFR 217.22(b)); AFS AOCI flows through for "
            "Category I/II -- pass only AFS marks via stress_aoci_q",
            "note": "retained earnings = NII x NI_TO_NII (0.43, "
                    "filing-calibrated net effect of provisions/opex/"
                    "fees) x (1-payout); RWA static -- a capital-plan "
                    "skeleton, not PPNR"}


def compute_kpis(bs: dict, swap_rates, vol_pts, dep_hist,
                 nii_monthly: pl.DataFrame | None = None,
                 seed: int = 7) -> dict:
    asof = bs.get("asof") or dt.date.today()
    dv = parallel_dv01s(bs, swap_rates, vol_pts, dep_hist, seed=seed)
    return {"eve": eve_summary(bs, dv), "dv01s": dv,
            "lcr": lcr(bs, asof), "nsfr": nsfr(bs, asof),
            "capital": capital(bs, nii_monthly)}
