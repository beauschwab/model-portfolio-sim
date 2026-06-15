"""Overlay balance-sheet optimization: a scenario-ROBUST linear program
over the unit library's allocation space. Because v0.14 made every risk
and forecast metric LINEAR in notionals, the whole problem -- absolute
ratio floors, commercial business-plan constraints, and KPIs holding
across multiple market scenarios simultaneously -- is an LP solved by
HiGHS in milliseconds, with DUALS: the shadow price of each binding
constraint is the marginal worst-case NII cost of tightening it by one
unit (the number the ALCO debate is actually about).

  max_{x>=0, t}  t                                (worst-case 27m NII)
  s.t.  NII_base_s + n_s . x >= t            for every scenario s
        LCR_s(x)  >= lcr_min                 (affine; per scenario)
        NSFR_s(x) >= nsfr_min
        CET1_q9_s(x) >= cet1_min             (linearized: static RWA add)
        |dEVE+200_s(x)| <= eve_limit x EVE   (two rows per scenario)
        A_commercial . x {<=,>=} b           (business plan: min
            origination, funding mix, per-template caps, total size)

Scenario robustness = constraint-set intersection: each scenario gets its
own unit tensor (engines re-run on the shifted market -- behavioral
models fully live per scenario) and its own base-KPI components; a
feasible x satisfies every ratio in EVERY scenario. Infeasibility is
reported with the violated row labels -- itself the useful answer
("you cannot hit the loan plan and hold LCR 120 in the bear steepener").

Disclosed simplifications: CET1 row uses NII-retention only (no AOCI
leg); LCR L2A composition cap linearized at the base mix; purchase-month
grid = the library's grid.
"""
from __future__ import annotations

import numpy as np
from scipy.optimize import linprog


def _kpi_vectors(lib: dict, base: dict) -> dict:
    """Per-allocation-column linear coefficients for each KPI, plus base
    constants, for ONE scenario's (library, base_kpis) pair."""
    from ..analytics.kpis import NI_TO_NII, PAYOUT
    units = lib["units"]
    U = len(units)
    tdef = lib["templates"]
    nii = lib["nii"].sum(axis=1)                       # per-unit 27m NII
    side = np.array([u["side"] for u in units])
    dv = lib["dv01"] * side
    w = lambda key: np.array([tdef[u["template"]].get(key, 0.0)
                              for u in units])
    e, l, n, c = base["eve"], base["lcr"], base["nsfr"], base["capital"]
    return dict(
        nii=side * nii, dv01=dv,
        hqla=w("hqla_l2a"), out30=w("outflow30"),
        asf=w("asf"), rsf=w("rsf"), rwa=w("rwa"),
        bal_asset=(side > 0).astype(float),
        bal_liab=(side < 0).astype(float),
        base=dict(nii=0.0, dv01=e["dv01_net_$"], eve=e["eve_$"],
                  hqla=l["hqla_$"], nco=l["net_outflows_$"],
                  asf=n["asf_$"], rsf=n["rsf_$"],
                  cet1=c["cet1_path"][-1]["cet1_$"],
                  rwa=c["rwa_total_$"], ni=NI_TO_NII * (1 - PAYOUT)),
        U=U, units=units)


def optimize_balance_sheet(
        scen_libs: list[tuple[dict, dict]],     # [(lib, base_kpis), ...]
        lcr_min: float = 1.10, nsfr_min: float = 1.05,
        cet1_min: float = 0.10, eve_limit: float = 0.15,
        commercial: list[dict] | None = None,
        max_total_assets: float | None = None) -> dict:
    """Robust LP. `commercial` rows: {label, template (or 'ALL_ASSET'/
    'ALL_LIAB'), sense ('>='|'<='), rhs} on total notional per template.
    Returns optimal allocation, binding constraints, and shadow prices
    (duals in worst-case-NII dollars per unit of constraint)."""
    K0 = _kpi_vectors(*scen_libs[0])
    U = K0["U"]
    units = K0["units"]
    nv = U + 1                                   # x (U) + t (epigraph)
    A_ub, b_ub, labels = [], [], []

    def row(coefs_x, t_coef, rhs, label):        # coefs.x + t_coef*t <= rhs
        A_ub.append(np.concatenate([coefs_x, [t_coef]]))
        b_ub.append(rhs)
        labels.append(label)

    for si, (lib, base) in enumerate(scen_libs):
        K = _kpi_vectors(lib, base)
        B = K["base"]
        tag = f"s{si}"
        row(-K["nii"], 1.0, 0.0, f"{tag}:worst_case_nii")     # t<=NII_s(x)
        # LCR: hqla_b + h.x >= m*(nco_b + o.x)
        row(-(K["hqla"] - lcr_min * K["out30"]), 0.0,
            B["hqla"] - lcr_min * B["nco"], f"{tag}:lcr>={lcr_min:.2f}")
        row(-(K["asf"] - nsfr_min * K["rsf"]), 0.0,
            B["asf"] - nsfr_min * B["rsf"], f"{tag}:nsfr>={nsfr_min:.2f}")
        # CET1 q9: (cet1_b + ni*nii.x) >= c*(rwa_b + rwa.x)
        row(-(B["ni"] * K["nii"] - cet1_min * K["rwa"]), 0.0,
            B["cet1"] - cet1_min * B["rwa"], f"{tag}:cet1>={cet1_min:.2f}")
        # |(dv_b + d.x)*200| <= eve_limit*EVE  (two-sided)
        cap = eve_limit * B["eve"] / 200.0
        row(K["dv01"], 0.0, cap - B["dv01"], f"{tag}:eve+200_lo")
        row(-K["dv01"], 0.0, cap + B["dv01"], f"{tag}:eve+200_hi")

    tot = np.zeros(U)
    for i, u in enumerate(units):
        tot[i] = 1.0 if u["side"] > 0 else 0.0
    if max_total_assets is not None:
        row(tot, 0.0, max_total_assets, "cap:total_assets")
    for c in (commercial or []):
        sel = np.array([
            1.0 if (c["template"] == u["template"]
                    or (c["template"] == "ALL_ASSET" and u["side"] > 0)
                    or (c["template"] == "ALL_LIAB" and u["side"] < 0))
            else 0.0 for u in units])
        if c["sense"] == ">=":
            row(-sel, 0.0, -c["rhs"], f"comm:{c['label']}>= {c['rhs']:.3g}")
        else:
            row(sel, 0.0, c["rhs"], f"comm:{c['label']}<= {c['rhs']:.3g}")

    cvec = np.zeros(nv)
    cvec[-1] = -1.0                              # maximize t
    bounds = [(0, None)] * U + [(None, None)]
    res = linprog(cvec, A_ub=np.array(A_ub), b_ub=np.array(b_ub),
                  bounds=bounds, method="highs")
    if not res.success:
        return {"feasible": False, "message": res.message,
                "labels": labels}
    x = res.x[:U]
    slack = np.array(b_ub) - np.array(A_ub) @ res.x
    duals = -res.ineqlin.marginals               # $ worst-NII per unit rhs
    binding = [dict(constraint=labels[i],
                    shadow_price=float(duals[i]))
               for i in range(len(labels)) if slack[i] < 1e-3
               and abs(duals[i]) > 1e-12]
    alloc = [dict(template=units[i]["template"],
                  purchase_m=units[i]["h"], notional=float(x[i]))
             for i in range(U) if x[i] > 1.0]
    return {"feasible": True,
            "worst_case_nii_$": float(res.x[-1]),
            "allocation": alloc,
            "binding_constraints": binding,
            "total_new_assets_$": float(tot @ x)}
