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

Disclosed simplifications: CET1 uses NII retention only (no AOCI). Base
regulatory components are static; overlay quantities use active balances.
LCR applies both affine branches of the exact L2A cap. Monthly funding
requires matched liabilities unless an additional outside-book committed
cash_budget is supplied (its cost is not automatically priced). DV01 scales
with outstanding balance. Purchase-month grid = the library's grid.
"""
from __future__ import annotations

import numpy as np
from scipy.optimize import linprog


def _kpi_vectors(lib: dict, base: dict) -> dict:
    from ..analytics.kpis import NI_TO_NII, PAYOUT
    from .unitlib import allocation_vectors
    units = lib["units"]
    # U x metric x month; the same coefficients used in interactive replay.
    x = np.stack([allocation_vectors(lib, u["template"], u["h"]) for u in units])
    e, l, n, c = (base[k] for k in ("eve", "lcr", "nsfr", "capital"))
    return dict(
        nii=x[:, 0].sum(1), dv01=x[:, 2], hqla=x[:, 3], out30=x[:, 4],
        asf=x[:, 5], rsf=x[:, 6], rwa=x[:, 7, -1], funding=x[:, 8] - x[:, 9],
        base=dict(nii=base.get("nii_total_$", 0.0), dv01=e["dv01_net_$"], eve=e["eve_$"],
                  l1=l.get("hqla_l1_$", l["hqla_$"]),
                  l2=l.get("hqla_l2a_uncapped_$", l.get("hqla_l2a_$", 0.0)),
                  nco=l["net_outflows_$"], asf=n["asf_$"], rsf=n["rsf_$"],
                  cet1=c["cet1_path"][-1]["cet1_$"], rwa=c["rwa_total_$"],
                  ni=NI_TO_NII * (1 - PAYOUT)), U=len(units), units=units)


def optimize_balance_sheet(
        scen_libs: list[tuple[dict, dict]],     # [(lib, base_kpis), ...]
        lcr_min: float = 1.10, nsfr_min: float = 1.05,
        cet1_min: float = 0.10, eve_limit: float = 0.15,
        commercial: list[dict] | None = None,
        max_total_assets: float | None = None, cash_budget: float = 0.0,
        capital_limits: list[dict] | None = None) -> dict:
    """Robust LP. `commercial` rows: {label, template (or 'ALL_ASSET'/
    'ALL_LIAB'), sense ('>='|'<='), rhs} on total notional per template.
    Returns optimal allocation, binding constraints, and shadow prices
    (duals in worst-case-NII dollars per unit of constraint)."""
    if not scen_libs:
        raise ValueError("at least one scenario is required")
    if cash_budget < 0 or not np.isfinite(cash_budget):
        raise ValueError("cash_budget must be finite and nonnegative")
    from ..core.quant_native import enabled
    if enabled():
        # Serialization only: native code owns coefficient construction, LP rows,
        # solving, financial replay and result validation. Python is the test oracle.
        from .decision import NativeDecision
        scenarios = [dict(library={
            **{key: lib[key] for key in ('units', 'templates', 'horizon')},
            **{key: np.asarray(lib[key]).tolist() for key in ('nii', 'balance', 'dv01')},
        }, base=base) for lib, base in scen_libs]
        return NativeDecision().call(
            op='optimize_library', schema='strategy-library-1', scenarios=scenarios,
            constraints=dict(lcr_min=lcr_min, nsfr_min=nsfr_min, cet1_min=cet1_min,
                             eve_limit=eve_limit, cash_budget=cash_budget,
                             max_total_assets=max_total_assets, commercial=commercial or [],
                             capital_limits=capital_limits or []))
    K0 = _kpi_vectors(*scen_libs[0])
    U = K0["U"]
    units = K0["units"]
    from ..analytics.treasury import validate_capital_limits, replay_capital_limits
    validate_capital_limits(capital_limits or [], units, len(scen_libs), scen_libs[0][0]['horizon'])
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
        if K["units"] != units:
            raise ValueError("scenario unit grids must match")
        if B["eve"] <= 0:
            raise ValueError("positive base EVE is required")
        row(-K["nii"], 1.0, B["nii"], f"{tag}:worst_case_nii")
        from ..analytics.kpis import L2_CAP
        for m in range(lib["horizon"]):
            h, o = K["hqla"][:, m], K["out30"][:, m]
            # Both affine branches of the exact Level 2A composition cap.
            row(lcr_min * o - h, 0, B["l1"] + B["l2"] - lcr_min * B["nco"], f"{tag}:m{m}:lcr_assets")
            row(lcr_min * o, 0, B["l1"] / (1 - L2_CAP) - lcr_min * B["nco"], f"{tag}:m{m}:lcr_cap")
            row(nsfr_min * K["rsf"][:, m] - K["asf"][:, m], 0,
                B["asf"] - nsfr_min * B["rsf"], f"{tag}:m{m}:nsfr")
            cap = eve_limit * B["eve"] / 200.0
            row(K["dv01"][:, m], 0, cap - B["dv01"], f"{tag}:m{m}:eve_lo")
            row(-K["dv01"][:, m], 0, cap + B["dv01"], f"{tag}:m{m}:eve_hi")
            row(K["funding"][:, m], 0, cash_budget, f"{tag}:m{m}:funding")
        row(cet1_min * K["rwa"] - B["ni"] * K["nii"], 0,
            B["cet1"] - cet1_min * B["rwa"], f"{tag}:cet1_horizon")

    for limit in capital_limits or []:
        row(limit['required_ratio']*np.array(limit['denominator_per_unit'])-np.array(limit['numerator_per_unit']),
            0.,limit['numerator']-limit['required_ratio']*limit['denominator'],f"capital:{limit['label']}")
    tot = np.zeros(U)
    for i, u in enumerate(units):
        tot[i] = 1.0 if u["side"] > 0 else 0.0
    if max_total_assets is not None:
        row(tot, 0.0, max_total_assets, "cap:total_assets")
    for c in (commercial or []):
        if c["sense"] not in (">=", "<="):
            raise ValueError("commercial sense must be >= or <=")
        if c["template"] not in {u["template"] for u in units} | {"ALL_ASSET", "ALL_LIAB"}:
            raise ValueError("unknown commercial template")
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
             for i in range(U) if x[i] > 1e-8]
    from .unitlib import evaluate_strategy
    replay = [evaluate_strategy(lib, alloc, base) for lib, base in scen_libs]
    actual = min(base.get("nii_total_$", 0.0) + r["nii_total_$"]
                 for (_, base), r in zip(scen_libs, replay))
    if not np.isclose(actual, res.x[-1], rtol=1e-7, atol=0.01):
        raise RuntimeError("optimizer objective failed independent allocation replay")
    for r in replay:
        k, path = r["kpis"], r["kpi_path"]
        if (np.min(path["lcr_pct"]) < lcr_min * 100 - 1e-5
                or np.min(path["nsfr_pct"]) < nsfr_min * 100 - 1e-5
                or k["cet1_horizon_pct"] < cet1_min * 100 - 1e-5
                or np.max(np.abs(path["d_eve_pct_eve_+200"])) > eve_limit * 100 + 1e-5
                or np.max(r["funding_gap"]) > cash_budget + max(0.01, cash_budget * 1e-7)):
            raise RuntimeError("optimizer constraints failed independent allocation replay")
    sides = {u["template"]: u["side"] for u in units}
    replay_assets = sum(a["notional"] for a in alloc if sides[a["template"]] > 0)
    if max_total_assets is not None and replay_assets > max_total_assets + max(0.01, max_total_assets * 1e-8):
        raise RuntimeError("optimizer asset cap failed allocation replay")
    for constraint in commercial or []:
        target = constraint["template"]
        amount = sum(a["notional"] for a in alloc if target == a["template"]
                     or (target == "ALL_ASSET" and sides[a["template"]] > 0)
                     or (target == "ALL_LIAB" and sides[a["template"]] < 0))
        rhs = constraint["rhs"]
        tolerance = max(0.01, abs(rhs) * 1e-8)
        if (constraint["sense"] == ">=" and amount < rhs - tolerance
                or constraint["sense"] == "<=" and amount > rhs + tolerance):
            raise RuntimeError("optimizer commercial constraint failed allocation replay")
    return {"feasible": True, "validated": True,
            "capital_replay": replay_capital_limits(capital_limits or [],units,alloc),
            "validation_scope": "linear coefficient replay only",
            "dynamic_validated": False, "dynamic_validation_status": "not_run",
            "cash_budget_$": cash_budget, "horizon_months": scen_libs[0][0]["horizon"],
            "worst_case_nii_$": float(res.x[-1]),
            "allocation": alloc,
            "binding_constraints": binding,
            "total_new_assets_$": float(tot @ x)}
