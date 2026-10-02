"""Rates Workbench API -- FastAPI wrapper over the portfolio-risk engine.

Run:  uvicorn app.main:app --reload --port 8000   (from apps/api)
Docs: http://localhost:8000/docs

Design notes:
- Long computations run on a single worker thread (numba kernels already
  saturate cores); clients poll GET /jobs/{id}.
- Books are Polars frames keyed by name; PUT replaces wholesale (the UI
  edits client-side and submits the full book -- simple and auditable).
- Assumption patches update per-run snapshot inputs; numba
  freezes constants at first compile, so prepay-vector changes require a
  process restart to affect the MBS kernel -- the endpoint says so.
"""
from __future__ import annotations
from contextlib import asynccontextmanager

import numpy as np
import polars as pl
from fastapi import FastAPI, HTTPException, Response, Depends, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from starlette.concurrency import run_in_threadpool

from . import store, market_data, decision_store, persistence
from .schemas import DecisionBuildRequest, DecisionUpdateRequest, DecisionEvalRequest, BalanceStressRequest, SavedBalanceStressRequest, StreamedBalanceStressRequest
from .schemas import (Allocation, OptimizeRequest, Program, AssumptionPatch, JobStatus, Market, MarketScenario,
                      RiskSettings, RunRequest, ForecastRequest, PythonBackendDeprecated)

async def request_context(request: Request):
    import hmac
    config = persistence.CONFIG
    if config and config.api_token and not hmac.compare_digest(
            request.headers.get('authorization', ''), f'Bearer {config.api_token}'):
        raise HTTPException(401, 'Authentication required')
    expected = request.headers.get('if-match')
    try:
        expected = int(expected.strip('"')) if expected is not None else None
    except ValueError:
        raise HTTPException(400, 'If-Match must contain a revision number')
    key = request.headers.get('idempotency-key')
    if key is not None and (not key or len(key) > 128):
        raise HTTPException(400, 'Idempotency-Key must contain 1 to 128 characters')
    token = persistence.EXPECTED_REVISION.set(expected)
    key_token = persistence.IDEMPOTENCY_KEY.set(key)
    try:
        await run_in_threadpool(persistence.refresh)
        yield
    finally:
        persistence.EXPECTED_REVISION.reset(token)
        persistence.IDEMPOTENCY_KEY.reset(key_token)


@asynccontextmanager
async def lifespan(_app):
    persistence.start()
    try:
        yield
    finally:
        persistence.stop()


app = FastAPI(title="Rates Workbench API", version="0.2.0", lifespan=lifespan, dependencies=[Depends(request_context)])
app.add_middleware(CORSMiddleware, allow_origins=["*"],
                   allow_methods=["*"], allow_headers=["*"])
from .cohort_routes import router as cohort_router
app.include_router(cohort_router)
from .treasury_routes import router as treasury_router
app.include_router(treasury_router)


@app.post('/decision/sessions')
def build_decision(req: DecisionBuildRequest) -> JobStatus:
    state = store.snapshot()
    if req.expected_revision != state['revision']:
        raise HTTPException(409, 'saved inputs changed; reload before building')
    if any(name not in state['scenarios'] for name in req.options.scenarios) or len(set(req.options.scenarios)) != len(req.options.scenarios):
        raise HTTPException(422, 'scenario names must be known and unique')
    jid = store.submit('decision_build', decision_store.build, req.options.model_dump(), state=state)
    return JobStatus(**store.job_status(jid))


@app.post('/decision/sessions/{sid}/update')
def update_decision(sid: str, req: DecisionUpdateRequest) -> JobStatus:
    if persistence.REPO is not None:
        persistence.remote(f'/decision/{sid}/check', req.model_dump())
        if req.constraints and req.constraints.scenarios:
            raise HTTPException(422, 'scenario changes require a new session')
        payload = req.model_dump()
        if payload['constraints']:
            payload['constraints'].pop('scenarios')
        jid = store.submit('decision_update', decision_store.update, sid, payload)
        return JobStatus(**store.job_status(jid))
    try:
        item = decision_store.entry(sid, req.expected_revision)
        if item['session'].version != req.version:
            raise RuntimeError('stale session version')
    except KeyError as exc:
        raise HTTPException(404, str(exc))
    except RuntimeError as exc:
        raise HTTPException(409, str(exc))
    if req.constraints and req.constraints.scenarios:
        raise HTTPException(422, 'scenario changes require a new session')
    payload = req.model_dump()
    if payload['constraints']:
        payload['constraints'].pop('scenarios')
    jid = store.submit('decision_update', decision_store.update, sid, payload)
    return JobStatus(**store.job_status(jid))


@app.post('/decision/sessions/{sid}/eval')
def evaluate_decision(sid: str, req: DecisionEvalRequest):
    if persistence.REPO is not None:
        return persistence.remote(f'/decision/{sid}/eval', req.model_dump()).json()
    try:
        return decision_store.evaluate(sid, req)
    except KeyError as exc:
        raise HTTPException(404, str(exc))
    except RuntimeError as exc:
        raise HTTPException(409, str(exc))
    except ValueError as exc:
        raise HTTPException(422, str(exc))


@app.delete('/decision/sessions/{sid}')
def close_decision(sid: str):
    if persistence.REPO is not None:
        # Durable close succeeds even while the worker is unavailable.
        return {'closed': persistence.REPO.close_session(sid)}
    return decision_store.close(sid)


@app.exception_handler(store.QueueFull)
async def queue_full(_request, exc):
    return JSONResponse(status_code=429, content={"detail": str(exc)}, headers={"Retry-After": "5"})


@app.exception_handler(persistence.Conflict)
async def revision_conflict(_request, exc):
    return JSONResponse(status_code=409, content={"detail": str(exc)})


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
        store.replace_book(name, rows)
    except persistence.Conflict:
        raise
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


# ---- settings + assumptions -------------------------------------------------
@app.post("/forecasts/preview")
def preview_forecast(req: ForecastRequest):
    return store.prepare_forecast(req)[2]


@app.get("/balance-stress/example")
def balance_stress_example():
    from portfolio_risk.analytics.balance_stress import example_specification, contract
    return {"specification": example_specification(), "contract": contract(), "revision": store.snapshot()["revision"]}


@app.post("/balance-stress/run")
def run_balance_stress(req: BalanceStressRequest) -> JobStatus:
    state = store.snapshot()
    if req.expected_revision != state["revision"]:
        raise HTTPException(409, "Inputs changed. Reload before submitting balance-sheet stress.")
    jid = store.submit("balance_stress", store.run_balance_stress, req.specification, state=state)
    return JobStatus(**store.job_status(jid))


@app.post('/balance-stress/stream')
def streamed_balance_stress(req: StreamedBalanceStressRequest) -> JobStatus:
    import os
    from portfolio_risk.analytics.balance_stream import binary_path, _hash
    if persistence.REPO is None:
        raise HTTPException(409, 'Partitioned simulation requires durable execution')
    if req.large_book and os.getenv('WORKBENCH_BALANCE_LARGE_BOOK', '0') != '1':
        raise HTTPException(422, 'Large-book capacity is disabled on this deployment')
    state = store.snapshot()
    if req.expected_revision != state['revision']:
        raise HTTPException(409, 'Inputs changed. Reload before submitting balance-sheet stress.')
    expected = persistence.identity()['balance_native_sha256'] if req.backend == 'rust' else None
    if req.backend == 'rust' and (expected is None or not binary_path().is_file() or _hash(binary_path()) != expected):
        raise HTTPException(409, 'Rust state engine is unavailable or changed; build it and restart API and worker')
    request = req.model_dump(exclude={'expected_revision'}) | {'binary_sha256': expected}
    jid = store.submit('streamed_balance_stress', store.run_streamed_balance_stress, request, state=state)
    return store.job_status(jid)


@app.get('/balance-stress/capabilities')
def balance_stress_capabilities():
    import os
    from portfolio_risk.analytics.balance_stream import binary_path, _hash
    durable = persistence.REPO is not None
    expected = persistence.identity()['balance_native_sha256'] if durable else None
    return {'durable': durable,
            'rust': bool(expected and binary_path().is_file() and _hash(binary_path()) == expected),
            'large_book': durable and os.getenv('WORKBENCH_BALANCE_LARGE_BOOK', '0') == '1',
            'scope_id': persistence.CODEC.objects.prefix.split('/')[-1] if durable else None}


@app.get('/balance-stress/inventory')
def balance_stress_inventory():
    from portfolio_risk.analytics.balance_workflow import inventory, REQUIRED
    state = store.snapshot()
    return dict(revision=state['revision'], instruments=inventory(store.balance_sheet(state)),
                required_mapping=sorted(REQUIRED))


@app.post('/balance-stress/saved-book')
def saved_balance_stress(req: SavedBalanceStressRequest) -> JobStatus:
    from portfolio_risk.analytics.balance_workflow import check_mapping
    state = store.snapshot()
    if req.expected_revision != state['revision']:
        raise HTTPException(409, 'Inputs changed. Reload before submitting saved-book stress.')
    try:
        check_mapping(store.balance_sheet(state), req.position_mapping)
    except ValueError as exc:
        raise HTTPException(422, str(exc)) from exc
    jid = store.submit('saved_balance_stress', store.run_saved_balance_stress,
                       req.model_dump(exclude={'expected_revision'}), state=state)
    return JobStatus(**store.job_status(jid))


@app.post("/forecasts/run")
def run_forecast(req: ForecastRequest) -> JobStatus:
    state, plan, preview = store.prepare_forecast(req)
    jid = store.submit("forecast_nii", store.run_forecast, plan, preview, state=state)
    return JobStatus(**store.job_status(jid))


@app.get("/settings")
def get_settings() -> RiskSettings:
    return store.SETTINGS


@app.exception_handler(PythonBackendDeprecated)
async def deprecated_backend(request, exc):
    return JSONResponse(status_code=409, content={'detail': str(exc)})


@app.put("/settings")
def put_settings(s: RiskSettings):
    try:
        s.require_production_backend()
    except ValueError as exc:
        raise HTTPException(422, str(exc)) from exc
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
@app.get('/pricing/assumptions')
def pricing_assumptions():
    from portfolio_risk.analytics.whatif import FIELDS, DEFAULTS
    return {'fields': FIELDS, 'defaults': DEFAULTS,
            'calibration': 'hold baseline OAS unless explicitly recalibrated'}


@app.post("/run")
def run(req: RunRequest) -> JobStatus:
    state = store.snapshot()
    if req.expected_revision is not None and req.expected_revision != state['revision']:
        raise HTTPException(409, 'Inputs changed. Reload the instruments before comparing.')
    sr, vp, spr = state["market"]["swap_rates"], state["market"]["vol_pts"], 0.0
    books = req.books if req.books is not None else list(state["books"])
    if req.kind in ("pricing", "whatif"):
        try:
            books = store.pricing_scope(state, req.books, req.spread_overrides_bp)
            if req.kind == 'whatif':
                from portfolio_risk.analytics.whatif import apply_overrides
                apply_overrides({b: state['books'][b] for b in books}, req.assumption_overrides)
        except ValueError as exc:
            raise HTTPException(422, str(exc))
    plan = store.compute_run_plan(req.kind, books, state)
    if req.scenario:
        sc = state["scenarios"].get(req.scenario)
        if sc is None:
            raise HTTPException(404, f"unknown scenario {req.scenario}")
        if req.kind == "unitlib":
            raise HTTPException(422, "interactive library must use the base market; use optimizer for robust scenarios")
        if req.kind == "nii":
            jid = store.submit("scenario_nii", store.run_scenario_grid, sc,
                               plan=store.compute_run_plan("scenario_nii", books, state), state=state)
            return JobStatus(**store.job_status(jid))
        sr, vp, spr = store.apply_scenario(sc, 0, state)
    if req.kind in ("pricing", "whatif"):
        options = dict(compare=req.kind == 'whatif', backend=req.backend, include_analytics=req.include_analytics,
                       assumption_overrides=req.assumption_overrides, calibration_mode=req.calibration_mode)
        jid = store.submit(req.kind, store.run_pricing, books, sr, vp, spr,
                           req.spread_overrides_bp, options, plan=plan, state=state)
    elif req.kind == "risk":
        jid = store.submit("risk", store.run_risk_all, books, sr, vp, spr, bool(req.scenario), plan=plan, state=state)
    elif req.kind in ("stress", "deposit_stress"):
        jid = store.submit("stress", store.run_stress_all, books, sr, vp, bool(req.scenario), spr, plan=plan, state=state)
    elif req.kind == "kpis":
        jid = store.submit("kpis", store.run_kpis_scenario, sr, vp, bool(req.scenario), spr, plan=plan, state=state)
    else:
        runner = {"nii": store.run_nii, "unitlib": store.build_unitlib_job, "strategy": store.run_strategy_job}[req.kind]
        jid = store.submit(req.kind, runner, sr, vp, plan=plan, state=state)
    return JobStatus(**store.job_status(jid))


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
def optimize(request: OptimizeRequest):
    """Robust balance-sheet optimization (job): base market + named
    MarketScenarios; absolute ratio floors + commercial plan rows; LP
    with worst-case-NII objective; returns allocation + binding
    constraints with shadow prices."""
    opt = request.model_dump()
    state = store.snapshot()
    missing = set(opt["scenarios"]) - state["scenarios"].keys()
    if missing:
        raise HTTPException(422, f"unknown scenarios: {sorted(missing)}")
    sr, vp = state["market"]["swap_rates"], state["market"]["vol_pts"]
    plan = store.compute_run_plan("optimize", list(state["books"]), state)
    plan["scenario_markets"] = 1 + len(opt["scenarios"])
    jid = store.submit("optimize", store.run_optimize_job, sr, vp, opt, plan=plan, state=state)
    return JobStatus(**store.job_status(jid))


@app.post("/strategy/eval")
def strategy_eval(allocations: list[Allocation]):
    """SYNCHRONOUS interactive evaluation (~sub-ms): time-shifted unit
    tensor dot product + closed-form KPI recalc. Requires the unit
    library (POST /run kind='unitlib', ~20s one-time)."""
    if persistence.REPO is not None:
        response = persistence.remote('/strategy/eval', [a.model_dump() for a in allocations])
        return Response(response.content, media_type=store.ARROW_ENVELOPE_MIME)
    try:
        return Response(store.to_arrow_envelope(
            store.eval_strategy_sync([a.model_dump() for a in allocations])),
            media_type=store.ARROW_ENVELOPE_MIME)
    except RuntimeError as e:
        raise HTTPException(409, str(e))
    except ValueError as e:
        raise HTTPException(422, str(e))


@app.get("/programs")
def list_programs():
    return store.PROGRAMS


@app.put("/programs/{name}")
def put_program(name: str, prog: Program):
    if prog.name != name:
        raise HTTPException(422, "program name must match its URL")
    with store._LOCK:
        store.PROGRAMS[name] = prog.model_dump(exclude_none=True)
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


@app.get("/state")
def state_status():
    if persistence.REPO is not None:
        try:
            return persistence.remote('/state', method='GET').json()
        except HTTPException:
            return {"revision": persistence.REPO.head(), "library_ready": False,
                    "library_horizon": None, "worker_ready": False}
    with store._LOCK:
        return {"revision": store.STATE_META["revision"],
                "library_ready": bool(store.CACHE),
                "library_horizon": store.CACHE.get("library", {}).get("horizon")}


@app.get('/revisions')
def revision_history():
    if persistence.REPO is None:
        return []
    return [{k: r[k] for k in ('revision', 'created_at', 'actor', 'reason')} for r in persistence.REPO.history()]


@app.get('/jobs/{jid}/manifest')
def run_manifest(jid: str):
    if persistence.REPO is None:
        raise HTTPException(409, 'Durable storage is disabled')
    try:
        job = persistence.REPO.job(jid)
        request = persistence.CODEC.load(job['request'])
        execution = None
        if job['result'] and request['operation'] == 'store.run_streamed_balance_stress':
            saved = persistence.CODEC.load(job['result'])['execution']
            execution = {k: v for k,v in saved.items() if k != 'specification'}
        return {"id": jid, "revision": job['revision'], "identity": request['identity'],
                "execution": execution,
                "catalog_publications": [dict(destination=r['destination'], manifest=r['manifest'])
                                         for r in persistence.REPO.publication_receipts(jid)],
                "settings": request['state']['settings'].model_dump(),
                "request": job['request'], "result": job['result'],
                "tables": persistence.CODEC.tables(job['result']) if job['result'] else {}}
    except KeyError:
        raise HTTPException(404, 'Unknown job')


@app.delete('/jobs/{jid}')
def cancel_job(jid: str):
    if persistence.REPO is None:
        raise HTTPException(409, 'Durable storage is disabled')
    try:
        return {'cancelled': persistence.REPO.cancel(jid)}
    except KeyError:
        raise HTTPException(404, 'Unknown job')


@app.get('/jobs/{jid}/table')
def result_table(jid: str, path: str, offset: int = 0, limit: int = 100):
    if persistence.REPO is None:
        raise HTTPException(409, 'Durable storage is disabled')
    from .reporting import table
    try:
        return table(jid, path, offset=offset, limit=limit)
    except KeyError:
        raise HTTPException(404, 'Unknown job or result table')
    except (ValueError, RuntimeError) as exc:
        raise HTTPException(409 if isinstance(exc, RuntimeError) else 422, str(exc))


@app.get('/jobs/{jid}/parquet')
def result_parquet(jid: str, path: str, partition: int | None = None):
    if persistence.REPO is None:
        raise HTTPException(409, 'Durable storage is disabled')
    try:
        ref = persistence.CODEC.tables(persistence.result_ref(jid))[path]
        if ref.get('format') == 'partitioned-parquet':
            if partition is None or not 0 <= partition < len(ref['parts']):
                raise HTTPException(422, 'Select a partition index from the table manifest')
            ref = ref['parts'][partition]['ref']
        elif partition is not None:
            raise HTTPException(422, 'This table is not partitioned')
        return Response(persistence.CODEC.objects.get(ref), media_type='application/vnd.apache.parquet',
                        headers={'Content-Disposition': 'attachment; filename="result.parquet"'})
    except KeyError:
        raise HTTPException(404, 'Unknown job or result table')
    except RuntimeError as exc:
        raise HTTPException(409, str(exc))
