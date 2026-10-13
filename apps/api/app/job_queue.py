"""Job queue; the caller supplies the sole state owner.

Functions transport/format engine outputs. Financial calculations stay in Rust.
"""
from __future__ import annotations

import copy
import heapq
import time
import uuid

def prune_jobs(owner):
    """Bound completed results by age, count and bytes; never evict active work."""
    with owner._LOCK:
        finished = sorted((j for j in owner.JOBS.values() if j["status"] in ("done", "error")),
                          key=lambda j: j.get("finished_at", 0))
        total = sum(len(j.get("result") or b"") for j in finished)
        for j in finished:
            expired = time.time() - j.get("finished_at", time.time()) > owner.JOB_TTL_SECONDS
            if expired or len(owner.JOBS) > owner.MAX_JOBS or total > owner.MAX_RESULT_BYTES:
                total -= len(j.get("result") or b"")
                owner.JOBS.pop(j["id"], None)


def submit(owner, kind: str, fn, *args, plan: dict | None = None, state=None, priority="interactive") -> str:
    if priority not in owner.PRIORITIES:
        raise ValueError(f"unknown priority {priority}")
    state = state or owner.snapshot()
    state['settings'].require_production_backend()
    args = copy.deepcopy(args)
    from . import persistence
    if persistence.REPO is not None:
        from .operations import operation
        return persistence.submit(kind, operation(fn), args, plan, state, priority=priority)
    with owner._LOCK:
        owner.prune_jobs()
        if sum(j["status"] in ("queued", "running") for j in owner.JOBS.values()) >= owner.MAX_QUEUE:
            raise owner.QueueFull("compute queue is full; wait for a job to finish")
        jid = uuid.uuid4().hex[:12]
        owner.JOBS[jid] = {"id": jid, "kind": kind, "status": "queued", "revision": state['revision'],
                    "market_provenance": copy.deepcopy(state["market"].get("provenance", {})),
                     "detail": None, "result": None,
                     "progress": {"stage": "queued", "pct": 0.0, "plan": plan or {}, "priority": priority,
                                  "stats": {}, "elapsed_s": 0.0, "log": [], "nodes": []}}

    def run():
        with owner._LOCK:
            if owner.JOBS.get(jid, {}).get("status") != "queued":
                return
            owner.JOBS[jid]["status"] = "running"
        from portfolio_risk.core.runtime import RunConfig, run_context
        import numba
        owner._CUR_JID = jid
        token = owner._RUN_STATE.set(state)
        owner.JOBS[jid]["_t0"] = time.perf_counter()
        owner.report(stage="starting", pct=1.0, log=f"{kind} run started")
        try:
            settings = state['settings']
            numba.set_num_threads(settings.n_threads or numba.config.NUMBA_NUM_THREADS)
            config = RunConfig(settings.n_paths, settings.n_paths_base, settings.horizon_months,
                               state['assumptions'].get("deposit_segments"),
                               tuple(state['assumptions'].get("cd_ew_params", [])) or None, compute_backend=settings.compute_backend)
            with run_context(config) as context:
                encoded = owner.to_arrow_envelope(fn(*args))
                owner.report(cache_hits=context.hits, cache_misses=context.misses)
            if len(encoded) > owner.MAX_RESULT_BYTES:
                raise RuntimeError("result exceeds the retention byte limit; reduce book size")
            with owner._LOCK:
                if owner.JOBS[jid].get("_cancelled"):
                    return
                owner.JOBS[jid]["result"] = encoded
                owner.JOBS[jid]["status"] = "done"
                owner.JOBS[jid]["finished_at"] = time.time()
            owner.report(stage="done", pct=100.0, log="run complete")
        except Exception as e:
            with owner._LOCK:
                if owner.JOBS[jid].get("_cancelled"):
                    return
                owner.JOBS[jid]["status"] = "error"
                owner.JOBS[jid]["finished_at"] = time.time()
                owner.JOBS[jid]["detail"] = f"{type(e).__name__}: {e}"
            owner.report(stage="error", log=f"{type(e).__name__}: {e}")
        finally:
            with owner._LOCK:
                if jid in owner.JOBS:
                    owner.JOBS[jid].pop("_t0", None)
                owner.prune_jobs()
            owner._RUN_STATE.reset(token)
            owner._CUR_JID = None

    with owner._LOCK:
        heapq.heappush(owner._PENDING, (owner.PRIORITIES.index(priority), next(owner._SEQ), jid, run))
    owner._POOL.submit(owner._run_next)
    return jid

