"""Rates Workbench API -- FastAPI wrapper over the portfolio-risk engine.

Run:  uvicorn app.main:app --reload --port 8000   (from apps/api)
Docs: http://localhost:8000/docs

Design notes:
- Long computations run on a single worker thread (numba kernels already
  saturate cores); clients poll GET /jobs/{id}.
- Books are Polars frames keyed by name; PUT replaces wholesale (the UI
  edits client-side and submits the full book -- simple and auditable).
- Assumption patches mutate the documented module-level surfaces; numba
  freezes constants at first compile, so prepay-vector changes require a
  process restart to affect the MBS kernel -- the endpoint says so.
"""
from __future__ import annotations

import numpy as np
import polars as pl
from fastapi import FastAPI, HTTPException, Response
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse

from . import store, market_data
from .schemas import (AssumptionPatch, JobStatus, Market, MarketScenario,
                      RiskSettings, RunRequest, ForecastRequest)

app = FastAPI(title="Rates Workbench API", version="0.1.0")
app.add_middleware(CORSMiddleware, allow_origins=["*"],
                   allow_methods=["*"], allow_headers=["*"])


@app.on_event("startup")
def _seed():
    store.seed_demo()


# ---- balance sheet / books ---------------------------------------------------
@app.get("/books")
def list_books():
    return {k: {"positions": len(v),
                "balance": float(v["balance" if "balance" in v.columns
                                 else "current_face" if "current_face"
                                 in v.columns else "face"].sum())}
            for k, v in store.BOOKS.items()}


@app.get("/books/{name}")
def get_book(name: str):
    if name not in store.BOOKS:
        raise HTTPException(404, f"unknown book {name}")
    return Response(store.to_arrow_envelope(store.BOOKS[name]),
                    media_type=store.ARROW_ENVELOPE_MIME)


@app.put("/books/{name}")
def put_book(name: str, rows: list[dict]):
    if name not in store.BOOKS:
        raise HTTPException(404, f"unknown book {name}")
    try:
        with store._LOCK:
            store.BOOKS[name] = pl.DataFrame(rows)
            store.changed()
    except Exception as e:
        raise HTTPException(422, f"bad book payload: {e}")
    return {"ok": True, "positions": len(rows)}


# ---- market data ----------------------------------------------------------------
@app.get("/market")
def get_market():
    return store.market_view()


@app.put("/market")
def put_market(m: Market):
    if len(m.swap_rates) != 10:
        raise HTTPException(422, "expect 10 pillar rates")
    store.replace_market(m)
    return {"ok": True}


# ---- settings + assumptions -------------------------------------------------
@app.get("/settings")
def get_settings() -> RiskSettings:
    return store.SETTINGS


@app.put("/settings")
def put_settings(s: RiskSettings):
    with store._LOCK:
        store.SETTINGS = s
        store.changed()
    return {"ok": True}


@app.get("/assumptions")
def get_assumptions():
    from portfolio_risk.config import PREPAY_PARAMS
    from portfolio_risk.deposits import SEGMENTS
    from portfolio_risk.cds import CD_EW_PARAMS
    return {"prepay": {"vector": list(map(float, PREPAY_PARAMS)),
                       "names": ["refi_max", "refi_a", "refi_b", "burn_k",
                                 "turnover", "cpr_cap", "hpa_beta",
                                 "lock_floor", "lock_slope"]},
            "deposit_segments": store.snapshot()["assumptions"].get("deposit_segments", SEGMENTS),
            "cd_ew_params": store.snapshot()["assumptions"].get("cd_ew_params", list(map(float, CD_EW_PARAMS))),
            "note": ("numba freezes module constants at first kernel "
                     "compile; prepay changes need a process restart to "
                     "reach the MBS kernel (engine AGENTS.md invariant 5)")}


@app.put("/assumptions")
def put_assumptions(p: AssumptionPatch):
    import copy
    applied = []
    with store._LOCK:
        updated = copy.deepcopy(store.ASSUMPTIONS)
        if p.deposit_segments is not None:
            for seg, vals in p.deposit_segments.items():
                if seg not in updated["deposit_segments"] or set(vals) - {"base", "amp", "b", "g0"}:
                    raise HTTPException(422, "unknown deposit segment or parameter")
                if any(v < 0 for v in vals.values()) or any(vals.get(k, 0) > 1 for k in ("base", "amp", "g0")):
                    raise HTTPException(422, "deposit parameters are outside their supported domain")
                updated["deposit_segments"][seg].update(vals)
                applied.append(f"deposit:{seg}")
        if p.cd_ew_params is not None:
            if len(p.cd_ew_params) != 5 or any(v < 0 for v in p.cd_ew_params):
                raise HTTPException(422, "expect five nonnegative CD withdrawal parameters")
            updated["cd_ew_params"] = list(p.cd_ew_params)
            applied.append("cd_ew_params")
        if applied:
            store.ASSUMPTIONS.clear()
            store.ASSUMPTIONS.update(updated)
            store.changed()
        if p.prepay:
            applied.append("prepay:RESTART_REQUIRED (numba constant freezing)")
    return {"applied": applied}


# ---- scenarios -------------------------------------------------------------------
@app.get("/scenarios")
def list_scenarios():
    return {k: v.model_dump() for k, v in store.SCENARIOS.items()}


@app.put("/scenarios/{name}")
def put_scenario(name: str, sc: MarketScenario):
    if sc.name != name:
        raise HTTPException(422, "scenario name must match its URL")
    with store._LOCK:
        store.SCENARIOS[name] = sc
        store.changed()
    return {"ok": True}


@app.delete("/scenarios/{name}")
def del_scenario(name: str):
    with store._LOCK:
        store.SCENARIOS.pop(name, None)
        store.changed()
    return {"ok": True}


# ---- runs ----------------------------------------------------------------------------
@app.post("/run")
def run(req: RunRequest) -> JobStatus:
    sr, vp, spr = store.MARKET["swap_rates"], store.MARKET["vol_pts"], 0.0
    if req.scenario:
        sc = store.SCENARIOS.get(req.scenario)
        if sc is None:
            raise HTTPException(404, f"unknown scenario {req.scenario}")
        if req.kind == "nii":
            jid = store.submit("scenario_nii", store.run_scenario_grid, sc)
            return JobStatus(**store.JOBS[jid])
        sr, vp, spr = store.apply_scenario(sc, 0)
    books = req.books or list(store.BOOKS)
    plan = store.compute_run_plan(
        "scenario_nii" if (req.kind == "nii" and req.scenario) else req.kind, books)
    if req.kind == "risk":
        jid = store.submit("risk", store.run_risk_all, books, sr, vp, spr, plan=plan)
    elif req.kind in ("stress", "deposit_stress"):
        jid = store.submit("stress", store.run_stress_all, books, sr, vp, plan=plan)
    elif req.kind == "nii":
        jid = store.submit("nii", store.run_nii, sr, vp, plan=plan)
    elif req.kind == "kpis":
        jid = store.submit("kpis", store.run_kpis, sr, vp, plan=plan)
    elif req.kind == "unitlib":
        jid = store.submit("unitlib", store.build_unitlib_job, sr, vp, plan=plan)
    elif req.kind == "strategy":
        jid = store.submit("strategy", store.run_strategy_job, sr, vp, plan=plan)
    else:
        raise HTTPException(422, f"unknown kind {req.kind}")
    return JobStatus(**store.JOBS[jid])


@app.get("/jobs/{jid}")
def job(jid: str) -> JobStatus:
    try:
        return JobStatus(**store.job_status(jid))
    except KeyError:
        raise HTTPException(404, "unknown or expired job")


@app.get("/jobs/{jid}/result")
def job_result(jid: str):
    """Computed frames for a finished job, as an Arrow IPC envelope. Polling
    GET /jobs/{jid} stays cheap JSON; the heavy result is fetched once here."""
    try:
        return Response(store.job_result(jid), media_type=store.ARROW_ENVELOPE_MIME)
    except KeyError:
        raise HTTPException(404, "unknown or expired job")
    except RuntimeError as e:
        raise HTTPException(409, str(e))


@app.post("/optimize")
def optimize(opt: dict):
    """Robust balance-sheet optimization (job): base market + named
    MarketScenarios; absolute ratio floors + commercial plan rows; LP
    with worst-case-NII objective; returns allocation + binding
    constraints with shadow prices."""
    sr, vp = store.MARKET["swap_rates"], store.MARKET["vol_pts"]
    plan = store.compute_run_plan("optimize", list(store.BOOKS))
    n_markets = 1 + len([n for n in opt.get("scenarios", []) if n in store.SCENARIOS])
    plan["scenario_markets"] = n_markets
    plan["monte_carlo_paths"] = int(store.SETTINGS.n_paths)
    jid = store.submit("optimize", store.run_optimize_job, sr, vp, opt, plan=plan)
    return JobStatus(**store.JOBS[jid])


@app.post("/strategy/eval")
def strategy_eval(allocations: list[dict]):
    """SYNCHRONOUS interactive evaluation (~sub-ms): time-shifted unit
    tensor dot product + closed-form KPI recalc. Requires the unit
    library (POST /run kind='unitlib', ~20s one-time)."""
    try:
        return Response(store.to_arrow_envelope(
            store.eval_strategy_sync(allocations)),
            media_type=store.ARROW_ENVELOPE_MIME)
    except RuntimeError as e:
        raise HTTPException(409, str(e))


@app.get("/programs")
def list_programs():
    return store.PROGRAMS


@app.put("/programs/{name}")
def put_program(name: str, prog: dict):
    prog["name"] = name
    with store._LOCK:
        store.PROGRAMS[name] = prog
        store.changed()
    return {"ok": True}


@app.delete("/programs/{name}")
def del_program(name: str):
    with store._LOCK:
        store.PROGRAMS.pop(name, None)
        store.changed()
    return {"ok": True}


@app.get("/hedges")
def get_hedges():
    if store.HEDGES is None:
        payload = {"swaps": [], "swaptions": []}
    else:
        sw, sp = store.HEDGES
        payload = {"swaps": sw, "swaptions": sp}
    return Response(store.to_arrow_envelope(payload),
                    media_type=store.ARROW_ENVELOPE_MIME)


@app.get("/health")
def health():
    return {"ok": True, "books": list(store.BOOKS)}


@app.exception_handler(market_data.DataError)
async def research_data_error(_request, exc):
    return JSONResponse(status_code=422, content={"detail": str(exc)})


@app.get("/market-data/sources")
def research_sources():
    return market_data.catalog()


@app.post("/market-data/fetch")
def fetch_research(req: market_data.FetchRequest) -> JobStatus:
    jid = store.submit("market_data", store.fetch_research_data, req)
    return JobStatus(**store.job_status(jid))


@app.get("/market-data/snapshots")
def research_snapshots():
    return market_data.list_snapshots()


@app.get("/market-data/snapshots/{sid}")
def research_snapshot(sid: str, offset: int = 0, limit: int = 100):
    if offset < 0 or not 1 <= limit <= 1000:
        raise HTTPException(422, "offset must be nonnegative and limit between 1 and 1000")
    try:
        snap = market_data.get_snapshot(sid)
    except KeyError:
        raise HTTPException(404, "unknown research snapshot")
    from .forecast_data import metadata
    snap["forecast"] = metadata(snap)
    snap["observations"] = snap["observations"][offset:offset + limit]
    return snap | {"offset": offset, "limit": limit}


@app.get("/market-data/snapshots/{sid}/export")
def export_research_snapshot(sid: str):
    try:
        snap = market_data.get_snapshot(sid)
    except KeyError:
        raise HTTPException(404, "unknown research snapshot")
    return JSONResponse(snap, headers={"Content-Disposition": f'attachment; filename="research-{sid[:12]}.json"'})


@app.post("/market-data/import")
def import_research(req: market_data.ImportRequest):
    return market_data.summary(market_data.import_snapshot(req))


@app.put("/market-data/active-curve")
def apply_research_curve(req: market_data.ApplyRequest):
    try:
        return store.apply_research_curve(req.snapshot_id, req.expected_revision)
    except KeyError:
        raise HTTPException(404, "unknown research snapshot")
    except RuntimeError as exc:
        raise HTTPException(409, str(exc))


@app.post("/forecasts/preview")
def preview_forecast(req: ForecastRequest):
    return store.prepare_forecast(req)[2]


@app.post("/forecasts/run")
def run_forecast(req: ForecastRequest) -> JobStatus:
    state, plan, preview = store.prepare_forecast(req)
    jid = store.submit("forecast_nii", store.run_forecast, plan, preview, state=state)
    return JobStatus(**store.job_status(jid))


@app.exception_handler(store.QueueFull)
async def queue_full(_request, exc):
    return JSONResponse(status_code=429, content={"detail": str(exc)}, headers={"Retry-After": "5"})
