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

from ..core.runtime import path_count

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


def resolve_templates(overrides=None, names=None):
    """Temporary new-business spread assumptions; existing defaults are unchanged."""
    names = list(TEMPLATES) if names is None else list(names)
    if not names or len(set(names)) != len(names) or set(names) - set(TEMPLATES):
        raise ValueError('template_names must be unique known templates')
    if set(overrides or {}) - set(TEMPLATES):
        raise ValueError('unknown template override')
    result = {name: dict(TEMPLATES[name]) for name in names}
    for name, patch in (overrides or {}).items():
        if set(patch) - {'spread_bp'}:
            raise ValueError('only template spread_bp is supported')
        for field, value in patch.items():
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not np.isfinite(value) or not -1000 <= value <= 2000:
                raise ValueError('template spread_bp must be finite in [-1000, 2000]')
            if name in result:
                result[name][field] = float(value)
    return result


def _fwd_par(dfs_interp, h_y: float, tenor_y: float) -> float:
    """Forward par swap rate at h for tenor (annual fixed leg)."""
    ts = h_y + np.arange(1, int(tenor_y) + 1)
    d = dfs_interp(ts)
    d0 = dfs_interp(np.array([h_y]))[0]
    return float((d0 - d[-1]) / d.sum())


def build_unit_library(swap_rates, vol_pts, mbs_hists, dep_hist,
                       grid_m=None, horizon: int = 27, seed: int = 7,
                       asof: dt.date | None = None, *, template_overrides=None,
                       template_names=None) -> dict:
    """Run unit ($1) cohorts of all templates x purchase-month grid
    through the real engines on shared paths. Returns the unit tensor:
    per unit -- nii[h_shiftable H], runoff[H], balance[H], dv01,
    category weights. Engine passes are BATCHED: all MBS units price in
    one portfolio per curve bump; same for corp/cd/deposit units."""
    from ..analytics.accounting import bucket_csr, smear_csr
    from ..products.cds import CDDeck, _cd_full
    from ..core.config import N_PATHS_SENS, N_STEPS, SWAP_TENORS
    from ..products.corp import CorpDeck, _corp_full, corp_pv, corp_solve_oas
    from ..core.curve import bootstrap_curve, forwards_from_dfs
    from ..products.deposits import DepositDeck, LogisticBetaECM, _deposit_A
    from ..core.pricing import pv_from_A, solve_oas_from_A
    from ..core.scenarios import (CRN, build_paths, build_rate_paths, run_engine,
                            setup, solve_base_oas)
    from ..core.vol import calibrate_abcd, factor_loadings

    templates = resolve_templates(template_overrides, template_names)
    if not 1 <= horizon < 360:
        raise ValueError("horizon must be between 1 and 359 months")
    from ..core import quant_native as native
    if native.enabled():
        from ..core.lifecycle_native import unit_library
        return unit_library(swap_rates,vol_pts,mbs_hists,dep_hist,grid_m,horizon,seed,asof,templates)
    grid_m = range(0, horizon, 6) if grid_m is None else grid_m
    grid_m = tuple(sorted(set(int(h) for h in grid_m if 0 <= h < horizon)))
    if not grid_m:
        grid_m = (0,)
    asof = asof or dt.date(2026, 6, 10)
    B = factor_loadings()
    dfs0 = bootstrap_curve(SWAP_TENORS, swap_rates)   # quarterly DF grid
    _tg = np.arange(dfs0.shape[0]) * 0.25
    dfi = lambda ts: np.interp(np.asarray(ts, dtype=float), _tg, dfs0)
    abcd0 = calibrate_abcd(vol_pts, forwards_from_dfs(dfs0), dfs0, B)
    crn = CRN(path_count(N_PATHS_SENS), seed)
    P = crn.n
    rp = lambda sr: build_rate_paths(sr, vol_pts, abcd0, B, crn)
    d25 = 25e-4

    units: list[dict] = []   # metadata per unit, aligned with frames below

    # ---- MBS-kind units: one portfolio, one engine pass per bump ------------
    mbs_rows = []
    for tname, t in templates.items():
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
    if mbs_rows:
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
    for tname, t in templates.items():
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
    if corp_rows:
        cframe = pl.DataFrame(corp_rows)
        cdeck = CorpDeck(cframe, asof)
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
        corp_cash = bucket_csr(cdeck.per_off, cdeck.pay_m, Ic/P, horizon, n_c)
        corp_run = bucket_csr(cdeck.per_off, cdeck.pay_m, Pc / P, horizon, n_c)
        corp_bal = np.maximum(1.0 - np.cumsum(corp_run, 1) + corp_run, 0.0)

    # ---- CD units (liability) -----------------------------------------------------
    cd_rows = []
    for tname, t in templates.items():
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
    if cd_rows:
        cdd = CDDeck(pl.DataFrame(cd_rows), asof)
        Acd, Icd, Pcd = _cd_full(cdd, rp(swap_rates))
        cdoas, _ = corp_solve_oas(cdd, Acd, P)
        cd_pv0 = corp_pv(cdd, Acd, cdoas, P)
        cd_pvu = corp_pv(cdd, _cd_full(cdd, rp(swap_rates + d25))[0], cdoas, P)
        cd_pvd = corp_pv(cdd, _cd_full(cdd, rp(swap_rates - d25))[0], cdoas, P)
        cd_dv = (cd_pvd - cd_pvu) / 50.0
        cd_nii = smear_csr(cdd.per_off, cdd.acc_m, cdd.pay_m, Icd / P,
                           horizon, cdd.n)
        cd_cash = bucket_csr(cdd.per_off, cdd.pay_m, Icd/P, horizon, cdd.n)
        cd_run = bucket_csr(cdd.per_off, cdd.pay_m, Pcd / P, horizon, cdd.n)
        cd_bal = np.maximum(1.0 - np.cumsum(cd_run, 1) + cd_run, 0.0)

    # ---- deposit units (liability; growth cohorts) ---------------------------------
    dep_rows = []
    for tname, t in templates.items():
        if t["kind"] != "deposit":
            continue
        for h in grid_m:
            dep_rows.append(dict(
                id=f"{tname}@{h}", balance=1.0, segment=t["segment"],
                age_months=1.0, avg_account_size=5e4,
                rate_paid=0.0, svc_cost=0.0015, price=97.0))
            units.append(dict(template=tname, h=h, kind="deposit",
                              side=-1.0))
    if dep_rows:
        ddeck = DepositDeck(pl.DataFrame(dep_rows))
        mdl = LogisticBetaECM()
        params = mdl.fit(dep_hist)
        base_paths = rp(swap_rates)
        r0 = float(mdl.equilibrium(params, base_paths["short"][:, 0].mean()))
        # at-market deposit: paid rate anchored at equilibrium at h (forward)
        for i, row in enumerate(dep_rows):
            h = units[[u["kind"] for u in units].index("deposit") + i]["h"]
            row["rate_paid"] = round(float(mdl.equilibrium(
                params, _fwd_par(dfi, h / 12.0, 1))) + templates[row["id"].split("@")[0]].get("spread_bp", 0) * 1e-4, 4)
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
    cash_interest = np.zeros((U, horizon))
    im = ic = id_ = icd = 0
    for i, u in enumerate(units):
        if u["kind"] == "mbs":
            cash_interest[i] = mbs_nii[im]
            nii[i], run[i], bal[i], dv[i] = (mbs_nii[im], mbs_run[im],
                                             mbs_bal[im], mbs_dv[im])
            im += 1
        elif u["kind"] == "corp":
            cash_interest[i] = corp_cash[ic]
            nii[i], run[i], bal[i], dv[i] = (corp_nii[ic], corp_run[ic],
                                             corp_bal[ic], corp_dv[ic])
            ic += 1
        elif u["kind"] == "cd":
            cash_interest[i] = cd_cash[icd]
            nii[i], run[i], bal[i], dv[i] = (cd_nii[icd], cd_run[icd],
                                             cd_bal[icd], cd_dv[icd])
            icd += 1
        else:
            cash_interest[i] = dep_nii[id_]
            nii[i], run[i], bal[i], dv[i] = (dep_nii[id_], dep_run[id_],
                                             dep_bal[id_], dep_dv[id_])
            id_ += 1
    lib = {"units": units, "nii": nii, "runoff": run, "balance": bal,
            "cash_interest": cash_interest,
            "dv01": dv, "grid_m": list(grid_m), "horizon": horizon,
            "templates": {k: {kk: vv for kk, vv in v.items()
                              if kk != "kind"} | {"kind": v["kind"]}
                          for k, v in templates.items()}}
    prepare_library(lib)
    return lib


METRICS = ("nii", "balance", "dv01", "hqla", "out30", "asf", "rsf", "rwa", "assets", "liabilities")


def _interp_unit(lib, template: str, h: int):
    idx = [i for i, u in enumerate(lib["units"]) if u["template"] == template]
    if not idx:
        raise ValueError(f"unknown template {template}")
    hs = np.array([lib["units"][i]["h"] for i in idx], dtype=float)
    j = max(0, min(int(np.searchsorted(hs, h, side="right") - 1), len(hs) - 2))
    a, b = idx[j], idx[min(j + 1, len(idx) - 1)]
    w = float(np.clip((h - hs[j]) / max(hs[min(j + 1, len(hs) - 1)] - hs[j], 1), 0, 1))
    mix = lambda x: (1 - w) * x[a] + w * x[b]
    return mix(lib["nii"]), mix(lib["balance"]), mix(lib["dv01"]), lib["units"][a]["side"]


def allocation_vectors(lib, template, h):
    """All coefficients share one purchase-date and outstanding-balance convention.

    Liquidity/EVE: beginning of each month. Capital RWA: final-month opening
    balance. DV01 is a disclosed balance-scaled approximation, not re-aged risk.
    """
    H = lib["horizon"]
    if not isinstance(h, (int, np.integer)) or h < 0:
        raise ValueError("purchase_m must be a nonnegative integer")
    if template not in lib["templates"]:
        raise ValueError(f"unknown template {template}")
    if h >= H:
        return np.zeros((len(METRICS), H))
    ready = lib.get("vectors", {}).get((template, h))
    if ready is not None:
        return ready
    inc, bal, dv, side = _interp_unit(lib, template, h)
    t = lib["templates"][template]
    x = np.zeros((len(METRICS), H))
    k = H - h
    x[0, h:] = side * inc[:k]
    x[1, h:] = np.maximum(bal[:k], 0)
    x[2, h:] = side * dv * x[1, h:] / max(float(bal[0]), 1e-12)
    for i, key in enumerate(("hqla_l2a", "outflow30", "asf", "rsf", "rwa"), 3):
        x[i] = t.get(key, 0.0) * x[1]
    x[8 if side > 0 else 9] = x[1]
    return x


def prepare_library(lib):
    """Precompute integer-month interpolation outside the interactive path."""
    from ..core.quant_native import enabled
    if enabled():
        from ..core.lifecycle_native import prepare_coefficients
        lib['vectors']=prepare_coefficients(lib)
        return
    lib["vectors"] = {(name, h): allocation_vectors(lib, name, h)
                      for name in lib["templates"] for h in range(lib["horizon"])}


def evaluate_strategy(lib: dict, allocations: list[dict], base_kpis: dict | None = None) -> dict:
    from ..core import quant_native as native
    if native.enabled():
        from ..core.lifecycle_native import evaluate_strategy as evaluate
        return evaluate(lib,allocations,base_kpis)
    H = lib["horizon"]
    x = np.zeros((len(METRICS), H))
    for a in allocations:
        N = float(a["notional"])
        if not np.isfinite(N) or N < 0:
            raise ValueError("notional must be finite and nonnegative")
        x += N * allocation_vectors(lib, a["template"], a["purchase_m"])
    nii, bal, dv, hqla, out30, asf, rsf, rwa, assets, liabilities = x
    out = {"nii_incremental": nii, "balance": bal, "fwd_dv01": dv,
           "nii_total_$": float(nii.sum()), "dv01_at_t0_$": float(dv[0]),
           "funding_gap": assets - liabilities, "horizon_months": H,
           "dv01_method": "base unit DV01 scaled by outstanding balance"}
    if base_kpis:
        from ..analytics.kpis import L2_CAP, NI_TO_NII, PAYOUT
        e, l, n, c = (base_kpis[key] for key in ("eve", "lcr", "nsfr", "capital"))
        l1 = l.get("hqla_l1_$", l["hqla_$"])
        l2 = l.get("hqla_l2a_uncapped_$", l.get("hqla_l2a_$", 0.0))
        total_hqla = l1 + np.minimum(l2 + hqla, l1 * L2_CAP / (1 - L2_CAP))
        eve = e["eve_$"]
        if eve <= 0:
            raise ValueError("positive base EVE is required for relative EVE limits")
        d_eve = -(e["dv01_net_$"] + dv) * 200 / eve * 100
        lcr = total_hqla / np.maximum(l["net_outflows_$"] + out30, 1e-9) * 100
        nsfr = (n["asf_$"] + asf) / np.maximum(n["rsf_$"] + rsf, 1e-9) * 100
        cet1 = (c["cet1_path"][-1]["cet1_$"] + nii.sum() * NI_TO_NII * (1 - PAYOUT)) / (c["rwa_total_$"] + rwa[-1]) * 100
        out["kpis"] = {
            "d_eve_pct_eve_+200": float(d_eve[0]),
            "duration_gap_y": float((e["dv01_net_$"] + dv[0]) * 1e4 / (e["mv_assets_$"] + assets[0])),
            "lcr_pct": float(lcr[0]), "nsfr_pct": float(nsfr[0]),
            "cet1_q9_pct": float(cet1), "cet1_horizon_pct": float(cet1),
        }
        out["kpi_path"] = {"d_eve_pct_eve_+200": d_eve, "lcr_pct": lcr, "nsfr_pct": nsfr}
    return out
