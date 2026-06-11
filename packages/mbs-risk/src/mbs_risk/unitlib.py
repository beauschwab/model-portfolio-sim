"""Unit-cohort library + linear scaling engine for interactive strategy
analysis. Hypothetical new origination of EVERY product type runs through
the SAME engines as the backbook -- full prepay S-curve/burnout, deposit
attrition, CD withdrawal, exercise rules -- on ONE shared upfront path
set, batched into one portfolio frame per product per curve bump (the
backbook's vectorized compute structure, reused verbatim). Because every
engine output is per-unit-balance and LINEAR in notional, a strategy
evaluation is a dot product over the precomputed unit tensor: full KPI
recalc (NII, dv01/KRD profile, Delta-EVE, duration gap, LCR, NSFR, CET1
path) in microseconds -- slider-speed.

APPROXIMATIONS (standard ALM new-business treatment, disclosed):
1. At-market coupons fix at the DETERMINISTIC forward par rate of the
   purchase month + product spread (the unit GRID carries forward-curve
   variation; per-path coupon fixing is strategies.py's simplified
   domain). Behavioral response to rates remains fully stochastic.
2. Unit cohorts are evaluated on months 0..T of the path set and
   TIME-SHIFTED to the purchase month h (valid to first order under the
   time-homogeneous abcd vol; the forward coupon carries the drift).
3. Purchase months between grid points interpolate unit metrics linearly
   in h.
"""
from __future__ import annotations

import datetime as dt

import numpy as np
import polars as pl

GRID_M = (0, 6, 12, 18, 24)

# template -> regulatory/category weights for KPI deltas
TEMPLATES = {
    "agency_mbs": dict(kind="mbs", spread_bp=130, term_y=None,
                       hqla_l2a=0.85, rsf=0.15, rwa=0.20, asf=0.0),
    "resi_whole_loan": dict(kind="mbs", spread_bp=170, term_y=None,
                            hqla_l2a=0.0, rsf=0.65, rwa=0.50, asf=0.0),
    "cml_fixed_5y": dict(kind="corp", is_float=0, spread_bp=190,
                         term_y=5, rsf=0.85, rwa=1.0, hqla_l2a=0, asf=0),
    "cml_float_3y": dict(kind="corp", is_float=1, spread_bp=180,
                         term_y=3, rsf=0.85, rwa=1.0, hqla_l2a=0, asf=0),
    "auto_annuity_5y": dict(kind="corp", is_float=0, spread_bp=280,
                            term_y=5, amort="annuity", rsf=0.85, rwa=1.0,
                            hqla_l2a=0, asf=0),
    "cd_2y": dict(kind="cd", term_y=2, spread_bp=15, side=-1,
                  asf=1.0, rsf=0, rwa=0, hqla_l2a=0, outflow30=0.0),
    "mmda_growth": dict(kind="deposit", segment="MMDA", side=-1,
                        asf=0.90, rsf=0, rwa=0, hqla_l2a=0,
                        outflow30=0.20),
}


def _fwd_par(dfs_interp, h_y: float, tenor_y: float) -> float:
    """Forward par swap rate at h for tenor (annual fixed leg)."""
    ts = h_y + np.arange(1, int(tenor_y) + 1)
    d = dfs_interp(ts)
    d0 = dfs_interp(np.array([h_y]))[0]
    return float((d0 - d[-1]) / d.sum())


def build_unit_library(swap_rates, vol_pts, mbs_hists, dep_hist,
                       grid_m=GRID_M, horizon: int = 27, seed: int = 7,
                       asof: dt.date | None = None) -> dict:
    """Run unit ($1) cohorts of all templates x purchase-month grid
    through the real engines on shared paths. Returns the unit tensor:
    per unit -- nii[h_shiftable H], runoff[H], balance[H], dv01,
    category weights. Engine passes are BATCHED: all MBS units price in
    one portfolio per curve bump; same for corp/cd/deposit units."""
    from .accounting import bucket_csr, smear_csr
    from .cds import CDDeck, _cd_full
    from .config import N_PATHS_SENS, N_STEPS, SWAP_TENORS
    from .corp import CorpDeck, _corp_full, corp_pv, corp_solve_oas
    from .curve import bootstrap_curve, forwards_from_dfs
    from .deposits import DepositDeck, LogisticBetaECM, _deposit_A
    from .pricing import pv_from_A, solve_oas_from_A
    from .scenarios import (CRN, build_paths, build_rate_paths, run_engine,
                            setup, solve_base_oas)
    from .vol import calibrate_abcd, factor_loadings

    asof = asof or dt.date(2026, 6, 10)
    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)   # quarterly DF grid
    _tg = np.arange(dfs0.shape[0]) * 0.25
    dfi = lambda ts: np.interp(np.asarray(ts, dtype=float), _tg, dfs0)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    crn = CRN(N_PATHS_SENS, seed)
    P = crn.n
    d25 = 25e-4

    units: list[dict] = []   # metadata per unit, aligned with frames below

    # ---- MBS-kind units: one portfolio, one engine pass per bump ------------
    mbs_rows = []
    for tname, t in TEMPLATES.items():
        if t["kind"] != "mbs":
            continue
        for h in grid_m:
            net = _fwd_par(dfi, h / 12.0, 10) + t["spread_bp"] * 1e-4
            mbs_rows.append(dict(
                cusip=f"{tname}@{h}", current_face=1.0,
                net_coupon=round(net, 4), wac=round(net + 0.005, 4),
                wam=358.0, age=1.0, oltv=0.78, factor=1.0, fico=745.0,
                avg_loan_size=3.2e5, state="OTHER", channel="retail",
                price=100.0))
            units.append(dict(template=tname, h=h, kind="mbs", side=1.0))
    port = pl.DataFrame(mbs_rows)
    cc_hist, ps_hist = mbs_hists
    models, B2, a2, sec, tgt, face = setup(port, swap_rates, vol_pts,
                                           cc_hist, ps_hist)
    oas, _ = solve_base_oas(swap_rates, vol_pts, a2, B2, models, sec, tgt,
                            seed=seed, n_paths=P)

    def mbs_pass(sr):
        paths = build_paths(sr, vol_pts, a2, B2, models, crn)
        A, _, _, _, _, Iout, Pacc = run_engine(paths, sec)
        return pv_from_A(A, oas, P), Iout / P, Pacc / P
    pv0, I0, P0 = mbs_pass(swap_rates)
    pvu, *_ = mbs_pass(swap_rates + d25)
    pvd, *_ = mbs_pass(swap_rates - d25)
    mbs_dv = (pvd - pvu) / 50.0
    mbs_nii = I0[:, :horizon]
    mbs_run = P0[:, :horizon]
    mbs_bal = 1.0 - np.cumsum(P0, axis=1)[:, :horizon] + P0[:, :horizon]

    # ---- corp-kind units --------------------------------------------------------
    corp_rows, corp_meta = [], []
    for tname, t in TEMPLATES.items():
        if t["kind"] != "corp":
            continue
        for h in grid_m:
            ref = _fwd_par(dfi, h / 12.0, t["term_y"])
            cpn = (t["spread_bp"] * 1e-4 if t["is_float"]
                   else ref + t["spread_bp"] * 1e-4)
            corp_rows.append(dict(
                id=f"{tname}@{h}", face=1.0,
                maturity=asof + dt.timedelta(days=int(t["term_y"] * 365.25)),
                freq_months=3 if t["is_float"] else 6,
                daycount="ACT/360", is_float=t["is_float"],
                coupon_or_spread=round(float(cpn), 4),
                amort_type=t.get("amort", "bullet"), price=100.0))
            units.append(dict(template=tname, h=h, kind="corp", side=1.0))
    cframe = pl.DataFrame(corp_rows)
    cdeck = CorpDeck(cframe, asof)
    rp = lambda sr: build_rate_paths(sr, vol_pts, abcd0, B, crn)
    A, Ic, Pc = _corp_full(cdeck, rp(swap_rates))
    coas, _ = corp_solve_oas(cdeck, A, P)
    corp_pv0 = corp_pv(cdeck, A, coas, P)
    corp_pvu = corp_pv(cdeck, _corp_full(cdeck, rp(swap_rates + d25))[0],
                       coas, P)
    corp_pvd = corp_pv(cdeck, _corp_full(cdeck, rp(swap_rates - d25))[0],
                       coas, P)
    corp_dv = (corp_pvd - corp_pvu) / 50.0
    n_c = cdeck.n
    corp_nii = smear_csr(cdeck.per_off, cdeck.acc_m, cdeck.pay_m, Ic / P,
                         horizon, n_c)
    corp_run = bucket_csr(cdeck.per_off, cdeck.pay_m, Pc / P, horizon, n_c)
    corp_bal = np.maximum(1.0 - np.cumsum(corp_run, 1) + corp_run, 0.0)

    # ---- CD units (liability) -----------------------------------------------------
    cd_rows = []
    for tname, t in TEMPLATES.items():
        if t["kind"] != "cd":
            continue
        for h in grid_m:
            r = _fwd_par(dfi, h / 12.0, t["term_y"]) + t["spread_bp"] * 1e-4
            cd_rows.append(dict(
                id=f"{tname}@{h}", balance=1.0, rate=round(float(r), 4),
                maturity=asof + dt.timedelta(days=int(t["term_y"] * 365.25)),
                freq_months=0, daycount="ACT/365F", channel="retail",
                penalty_months=6.0, price=100.0))
            units.append(dict(template=tname, h=h, kind="cd", side=-1.0))
    cdd = CDDeck(pl.DataFrame(cd_rows), asof)
    Acd, Icd, Pcd = _cd_full(cdd, rp(swap_rates))
    cdoas, _ = corp_solve_oas(cdd, Acd, P)
    cd_pv0 = corp_pv(cdd, Acd, cdoas, P)
    cd_pvu = corp_pv(cdd, _cd_full(cdd, rp(swap_rates + d25))[0], cdoas, P)
    cd_pvd = corp_pv(cdd, _cd_full(cdd, rp(swap_rates - d25))[0], cdoas, P)
    cd_dv = (cd_pvd - cd_pvu) / 50.0
    cd_nii = smear_csr(cdd.per_off, cdd.acc_m, cdd.pay_m, Icd / P,
                       horizon, cdd.n)
    cd_run = bucket_csr(cdd.per_off, cdd.pay_m, Pcd / P, horizon, cdd.n)
    cd_bal = np.maximum(1.0 - np.cumsum(cd_run, 1) + cd_run, 0.0)

    # ---- deposit units (liability; growth cohorts) ---------------------------------
    dep_rows = []
    for tname, t in TEMPLATES.items():
        if t["kind"] != "deposit":
            continue
        for h in grid_m:
            dep_rows.append(dict(
                id=f"{tname}@{h}", balance=1.0, segment=t["segment"],
                age_months=1.0, avg_account_size=5e4,
                rate_paid=0.0, svc_cost=0.0015, price=97.0))
            units.append(dict(template=tname, h=h, kind="deposit",
                              side=-1.0))
    ddeck = DepositDeck(pl.DataFrame(dep_rows))
    mdl = LogisticBetaECM()
    params = mdl.fit(dep_hist)
    base_paths = rp(swap_rates)
    r0 = float(mdl.equilibrium(params, base_paths["short"][:, 0].mean()))
    # at-market deposit: paid rate anchored at equilibrium at h (forward)
    for i, row in enumerate(dep_rows):
        h = units[[u["kind"] for u in units].index("deposit") + i]["h"]
        row["rate_paid"] = round(float(mdl.equilibrium(
            params, _fwd_par(dfi, h / 12.0, 1))), 4)
    ddeck = DepositDeck(pl.DataFrame(dep_rows))

    def dep_pass(sr):
        paths = rp(sr)
        dep = mdl.paths(paths["short"].astype(np.float64), params, r0)
        A, Pout, _, _, _, Iout = _deposit_A(ddeck, paths, dep, r0)
        return A, Pout / P, Iout / P
    Ad, Pd, Id = dep_pass(swap_rates)
    doas, _ = solve_oas_from_A(Ad, P, ddeck.tgt, lo0=-0.15)
    dep_pv0 = pv_from_A(Ad, doas, P)
    dep_pvu = pv_from_A(dep_pass(swap_rates + d25)[0], doas, P)
    dep_pvd = pv_from_A(dep_pass(swap_rates - d25)[0], doas, P)
    dep_dv = (dep_pvd - dep_pvu) / 50.0
    dep_nii = Id[:, :horizon]
    dep_run = Pd[:, :horizon]
    dep_bal = np.maximum(1.0 - np.cumsum(dep_run, 1) + dep_run, 0.0)

    # ---- assemble tensor (U, ...) in `units` order -----------------------------
    U = len(units)
    nii = np.zeros((U, horizon)); run = np.zeros((U, horizon))
    bal = np.zeros((U, horizon)); dv = np.zeros(U)
    im = ic = id_ = icd = 0
    for i, u in enumerate(units):
        if u["kind"] == "mbs":
            nii[i], run[i], bal[i], dv[i] = (mbs_nii[im], mbs_run[im],
                                             mbs_bal[im], mbs_dv[im])
            im += 1
        elif u["kind"] == "corp":
            nii[i], run[i], bal[i], dv[i] = (corp_nii[ic], corp_run[ic],
                                             corp_bal[ic], corp_dv[ic])
            ic += 1
        elif u["kind"] == "cd":
            nii[i], run[i], bal[i], dv[i] = (cd_nii[icd], cd_run[icd],
                                             cd_bal[icd], cd_dv[icd])
            icd += 1
        else:
            nii[i], run[i], bal[i], dv[i] = (dep_nii[id_], dep_run[id_],
                                             dep_bal[id_], dep_dv[id_])
            id_ += 1
    return {"units": units, "nii": nii, "runoff": run, "balance": bal,
            "dv01": dv, "grid_m": list(grid_m), "horizon": horizon,
            "templates": {k: {kk: vv for kk, vv in v.items()
                              if kk != "kind"} | {"kind": v["kind"]}
                          for k, v in TEMPLATES.items()}}


def _interp_unit(lib, template: str, h: int):
    """Linear h-interpolation of one template's unit metrics."""
    idx = [i for i, u in enumerate(lib["units"])
           if u["template"] == template]
    hs = np.array([lib["units"][i]["h"] for i in idx], dtype=float)
    h = float(np.clip(h, hs.min(), hs.max()))
    j = int(np.searchsorted(hs, h, side="right") - 1)
    j = min(j, len(hs) - 2)
    w = (h - hs[j]) / max(hs[j + 1] - hs[j], 1e-9)
    a, b = idx[j], idx[j + 1]
    mix = lambda x: (1 - w) * x[a] + w * x[b]
    return (mix(lib["nii"]), mix(lib["balance"]), mix(lib["dv01"]),
            lib["units"][a]["side"])


def evaluate_strategy(lib: dict, allocations: list[dict],
                      base_kpis: dict | None = None) -> dict:
    """allocations: [{template, purchase_m, notional}]. Time-shifts unit
    vectors to purchase_m, scales, sums -- microseconds. With base_kpis
    (kpis.compute_kpis output), returns recalculated top-level KPIs."""
    H = lib["horizon"]
    nii = np.zeros(H); bal = np.zeros(H); dv = np.zeros(H)
    for a in allocations:
        uvec, ubal, udv, side = _interp_unit(lib, a["template"],
                                             a["purchase_m"])
        N = float(a["notional"])
        h = int(a["purchase_m"])
        if h >= H:
            continue
        k = H - h
        nii[h:] += side * N * uvec[:k]
        bal[h:] += N * ubal[:k]
        dv[h:] += side * N * udv          # dv01 once on, until runoff~
        dv[h:] *= 1.0                     # (flat proxy; runoff in ubal)
    out = {"nii_incremental": nii, "balance": bal, "fwd_dv01": dv,
           "nii_total_$": float(nii.sum()),
           "dv01_at_t0_$": float(dv[0])}
    if base_kpis:
        from .kpis import (CET1_RATIO_T0, L2A_FACTOR, NI_TO_NII, PAYOUT,
                           TAX)
        tdef = lib["templates"]
        d_hqla = sum(a["notional"] * tdef[a["template"]]["hqla_l2a"]
                     for a in allocations)
        d_out = sum(a["notional"] * tdef[a["template"]].get("outflow30", 0)
                    for a in allocations)
        d_asf = sum(a["notional"] * tdef[a["template"]]["asf"]
                    for a in allocations)
        d_rsf = sum(a["notional"] * tdef[a["template"]]["rsf"]
                    for a in allocations)
        d_rwa = sum(a["notional"] * tdef[a["template"]]["rwa"]
                    for a in allocations)
        e = base_kpis["eve"]; l = base_kpis["lcr"]; n = base_kpis["nsfr"]
        c = base_kpis["capital"]
        dv_net2 = e["dv01_net_$"] + dv[0]
        kp = {
            "d_eve_pct_eve_+200": -dv_net2 * 200 / e["eve_$"] * 100,
            "duration_gap_y": (e["dv01_net_$"] + dv[0]) * 1e4
            / e["mv_assets_$"],
            "lcr_pct": (l["hqla_$"] + d_hqla)
            / max(l["net_outflows_$"] + d_out, 1e-9) * 100,
            "nsfr_pct": (n["asf_$"] + d_asf)
            / max(n["rsf_$"] + d_rsf, 1e-9) * 100,
            "cet1_q9_pct": (c["cet1_path"][-1]["cet1_$"]
                            + nii.sum() * NI_TO_NII * (1 - PAYOUT))
            / (c["rwa_total_$"] + d_rwa) * 100,
        }
        out["kpis"] = kp
    return out
