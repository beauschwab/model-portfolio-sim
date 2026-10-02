"""Conditional income paths from published forecasts (not risk-neutral prices).

Monthly means are conditioned on source anchors while rescaling base CRN
deviations. No OAS solve or valuation is performed. Never pass these paths to
risk/pricing: independently anchored tenors are not an arbitrage-free curve.
"""
from datetime import date
import numpy as np
import polars as pl

from ..core.config import DT, INC_LAG, N_STEPS, SHIFT
from ..models.models import yoy_from_hpi

DRIVERS = {"short_rate", "policy_rate", "rate_5y", "rate_10y", "mortgage_rate", "hpi"}


FORECAST_WARNINGS = [
        "Relative replay: source month 1 is applied to the current book's first projection month; book dates are not moved. This is not a historical backtest.",
        "Conditional NII/runoff only. No EVE, OAS recalibration, default losses, provisions, capital or scenario probabilities are inferred.",
        "Source Treasury/policy rates proxy model rates with zero basis. With multiple source tenors, missing tenors use linear interpolation and flat extrapolation; policy-only sources retain the initial model curve slope.",
        "Base Monte Carlo draws are reused. Shifted rate deviations are rescaled to source means, preserving the -2% rate floor. This is not an arbitrage-free pricing measure or an estimated real-world distribution.",
        "Beyond the last source anchor each driver stays at its final level, including for remaining-life cashflows. Missing mortgage/HPI drivers retain the existing model path.",
        "Quarterly average rates repeat within the quarter; dated rate endpoints interpolate linearly. Missing pre-first-endpoint values use the first endpoint.",
    ]


def month_number(day):
    d = date.fromisoformat(day)
    return d.year * 12 + d.month - 1


def compile_forecast(rows, scenario, start_period, horizon):
    """Relative replay: selected source month maps to book projection month 1.

    Quarterly average rates are held constant in their three months. Dated
    endpoints are linearly interpolated; HPI is log-interpolated and rebased
    against the preceding month. Only rate-bearing source months may start a run.
    """
    from ..core.quant_native import enabled,term_call
    warnings=FORECAST_WARNINGS.copy()
    if enabled():
        result=term_call('financial-controller-1',dict(op='forecast_compile',request=dict(rows=rows,
            scenario=scenario,start_period=start_period,horizon=horizon,months=N_STEPS)))
        result['targets']={k:np.asarray(v) for k,v in result['targets'].items()}
        if 'hpi' in result['targets']:
            warnings.append("HPI uses log interpolation between quarter ends, rebased at replay month zero. Mortgage incentives retain the engine's two-month lag.")
        return dict(scenario=scenario,start_period=start_period,horizon=horizon,warnings=warnings,**result)
    if not 1 <= horizon <= 120 or date.fromisoformat(start_period).day != 1:
        raise ValueError("forecast requires a month-start and horizon of 1..120 months")
    if scenario not in {r.get("scenario") for r in rows} or scenario not in {"baseline", "adverse", "median"}:
        raise ValueError("select a published baseline, adverse or median scenario")
    selected = [r for r in rows if r.get("scenario") == scenario and r.get("convention") != "longer_run"]
    start = month_number(start_period)
    if start not in {month_number(r["date"]) for r in selected if r.get("variable") in ("short_rate", "policy_rate")}:
        raise ValueError("start period must be a published rate-anchor month")
    targets, spans = {}, {}
    for variable in sorted(DRIVERS):
        values = [r for r in selected if r.get("variable") == variable]
        if not values:
            continue
        conventions = {r["convention"] for r in values}
        if len(conventions) != 1:
            raise ValueError(f"mixed period conventions for {variable}")
        convention = next(iter(conventions))
        if convention not in {"quarter_average", "quarter_end", "year_end", "date_end"}:
            raise ValueError(f"unsupported driver convention: {convention}")
        anchors = {}
        if variable == "hpi":
            values += [r for r in rows if r.get("scenario") == "history" and r.get("variable") == "hpi"]
        for r in values:
            unit = r["unit"]
            if unit not in ({"index"} if variable == "hpi" else {"percent", "decimal_rate"}):
                raise ValueError(f"unexpected unit for {variable}")
            v = float(r["value"]) / (100 if unit == "percent" else 1)
            if not np.isfinite(v) or (variable == "hpi" and v <= 0) or (variable != "hpi" and not -.019 < v <= 1):
                raise ValueError(f"invalid forecast value for {variable}")
            x = month_number(r["date"]) - start + 1
            positions = [x + j for j in range(3)] if convention == "quarter_average" else [x + 2 if convention == "quarter_end" else x]
            for pos in positions:
                if pos in anchors and anchors[pos] != v:
                    raise ValueError(f"conflicting forecast anchors for {variable}")
                anchors[pos] = v
        xs = sorted(anchors)
        ys = np.array([anchors[x] for x in xs])
        if xs[-1] < 1:
            raise ValueError(f"{variable} has no forward coverage")
        if variable == "hpi" and xs[0] > 0:
            raise ValueError("HPI requires a preceding quarter/history anchor; refetch the Fed package or select a later start")
        grid = np.arange(1, N_STEPS + 1)
        if variable == "hpi":
            origin = np.interp(0, xs, np.log(ys))
            target = np.exp(np.interp(grid, xs, np.log(ys)) - origin)
        else:
            target = np.interp(grid, xs, ys)
        targets[variable] = target
        spans[variable] = {"first_month": xs[0], "last_month": xs[-1], "tail_months_in_report": max(0, horizon - xs[-1])}
    if "short_rate" not in targets and "policy_rate" not in targets:
        raise ValueError("forecast has no supported short-rate driver")
    if "hpi" in targets:
        warnings.append("HPI uses log interpolation between quarter ends, rebased at replay month zero. Mortgage incentives retain the engine's two-month lag.")
    return {"scenario": scenario, "start_period": start_period, "horizon": horizon,
            "targets": targets, "coverage": spans, "warnings": warnings,
            "unused_variables": sorted({r.get("variable", r["series"]) for r in selected} - DRIVERS)}


def condition_paths(base, plan):
    """Copy-on-write recentering; same underlying draws, never mutate base paths."""
    t = plan["targets"]
    out = dict(base)
    short = t.get("short_rate", t.get("policy_rate"))
    def center(values, mean):
        return values.astype(np.float64) - values.mean(axis=0, dtype=np.float64) + mean
    def rate_center(values, mean):
        shifted = values.astype(np.float64) + SHIFT
        if np.min(shifted) <= 0 or np.min(mean + SHIFT) <= 0:
            raise ValueError("rates cross the supported shifted-rate floor")
        return shifted / shifted.mean(axis=0) * (mean + SHIFT) - SHIFT
    out["short"] = rate_center(base["short"], short)
    out["df"] = np.cumprod(1 / (1 + out["short"] * DT), axis=1)
    anchors = [(0.25, short)] + [(tenor, t[k]) for tenor, k in ((5, "rate_5y"), (10, "rate_10y")) if k in t]
    # Single short-rate source: preserve the base term structure via a parallel
    # shift. Multiple source tenors: direct proxy/interpolation, explicitly assumed.
    if len(anchors) == 1:
        slope = base["swaps"][:, :, 0].mean(axis=0, dtype=np.float64) - base["short"][:, 0].mean(dtype=np.float64)
        target = short[None, :] + slope[:, None]
        out["swaps"] = rate_center(base["swaps"], target)
    else:
        curve = np.array([np.interp([2, 5, 10, 30], [a[0] for a in anchors],
                                   [a[1][m] for a in anchors]) for m in range(N_STEPS)]).T
        out["swaps"] = rate_center(base["swaps"], curve)
    if "mtg" in base and "mortgage_rate" in t:
        target = t["mortgage_rate"].copy()
        if INC_LAG:
            target = np.r_[base["mtg"][:, :INC_LAG].mean(axis=0, dtype=np.float64), target[:-INC_LAG]]
        out["mtg"] = center(base["mtg"], target)
    if "hpi" in base and "hpi" in t:
        out["hpi"] = base["hpi"].astype(float) / base["hpi"].mean(axis=0, dtype=np.float64) * t["hpi"]
        out["yoy"] = yoy_from_hpi(out["hpi"])
    if any(not np.isfinite(v).all() for v in out.values()):
        raise ValueError("non-finite conditioned path")
    return out


def run_forecast_nii(bs, swap_rates, vol_pts, dep_hist, plan, seed=7):
    from .accounting import run_balance_sheet_nii
    from ..core.scenarios import CRN
    from ..core.runtime import path_count
    from ..core.config import N_PATHS_SENS
    horizon = plan["horizon"]
    from ..core.quant_native import enabled,term_call
    if enabled():
        from ..core.lifecycle_native import accounting_request,accounting_result
        request=accounting_request(bs,swap_rates,vol_pts,dep_hist,horizon,seed,bs['asof'],None,None,False,None,False)
        output=term_call('financial-controller-1',dict(op='forecast_replay',books=request,
            forecast=dict(targets={k:np.asarray(v).tolist() for k,v in plan['targets'].items()})))
        base=accounting_result(output['base'],horizon)
        conditional=accounting_result(output['conditional'],horizon)
        monthly=conditional['monthly'].with_columns(base['monthly']['nii'].alias('base_nii'),pl.Series('delta_nii',output['delta_nii']))
    else:
        crn = CRN(path_count(N_PATHS_SENS), seed)
        base = run_balance_sheet_nii(bs, swap_rates, vol_pts, dep_hist, horizon=horizon,
                                     seed=seed, asof=bs["asof"], capture_anchor=True, crn=crn)
        anchor = base.pop("accounting_anchor")
        conditional = run_balance_sheet_nii(bs, swap_rates, vol_pts, dep_hist, horizon=horizon,
            seed=seed, asof=bs["asof"], forecast_plan=plan, accounting_anchor=anchor, crn=crn)
        monthly = conditional["monthly"].with_columns(
            base["monthly"]["nii"].alias("base_nii"),
            (conditional["monthly"]["nii"] - base["monthly"]["nii"]).alias("delta_nii"))
    targets = pl.DataFrame({"month": np.arange(1, horizon + 1),
                           **{k: v[:horizon] for k, v in plan["targets"].items()}})
    warnings = plan["warnings"] + [
        "Fixed-rate book yields and deposit opening rates are held at base values. Floating income uses contractual reset cashflows plus the base effective-income adjustment; this is an accrual approximation.",
        "Money-market balances remain constant under the existing model; swap settlements enter income, while swaption value changes do not."]
    return {"scenario": plan["scenario"], "start_period": plan["start_period"],
            "method": "conditional monthly income and runoff; relative replay",
            "monthly": monthly, "summary": conditional["summary"], "base_summary": base["summary"],
            "runoff": conditional["runoff"], "base_runoff": base["runoff"],
            "drivers": targets, "coverage": plan["coverage"], "warnings": warnings,
            "unused_variables": plan["unused_variables"]}
