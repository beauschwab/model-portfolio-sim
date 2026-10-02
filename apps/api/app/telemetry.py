"""Telemetry; the caller supplies the sole state owner.

Functions transport/format engine outputs. Financial calculations stay in Rust.
"""
from __future__ import annotations

import time

def report(owner, stage: str | None = None, pct: float | None = None,
           log: str | None = None, **inc: float) -> None:
    """Update the running job's progress telemetry. Stage/pct are set;
    keyword counters in **inc are accumulated (records, revaluations,
    reductions, path_evaluations, scenario_paths, books_done, ...).
    Safe no-op when no job is bound to the worker thread."""
    jid = owner._CUR_JID
    if jid is None:
        return
    with owner._LOCK:
        if jid not in owner.JOBS:
            return
        p = owner.JOBS[jid].setdefault("progress", {})
        if stage is not None:
            p["stage"] = stage
        if pct is not None:
            # monotonic: nested adapters (run_nii inside kpis/optimize) report
            # their own band; never let the bar run backward within a job
            nxt = round(max(0.0, min(100.0, float(pct))), 1)
            p["pct"] = max(p.get("pct", 0.0), nxt)
        t0 = owner.JOBS[jid].get("_t0")
        if t0 is not None:
            p["elapsed_s"] = round(time.perf_counter() - t0, 3)
        stats = p.setdefault("stats", {})
        for k, v in inc.items():
            stats[k] = stats.get(k, 0) + v
        if log:
            p.setdefault("log", []).append(
                {"t": p.get("elapsed_s", 0.0), "msg": log})
            p["log"] = p["log"][-60:]      # keep the tail bounded


def node(owner, nid: str, *, parent: str | None = None, label: str | None = None,
         kind: str | None = None, status: str | None = None,
         detail: str | None = None, **stat: float) -> None:
    """Upsert a pipeline node on the running job's progress telemetry.

    Nodes form a flat list with `parent` pointers (rendered as a tree on the
    client). `kind` in {build, branch, paths, cashflow, oas, reduce, solve};
    `status` in {pending, running, done, error}. `t0` is stamped the first time
    a node goes `running`, `t1` when it reaches `done`/`error`. **stat carries
    per-node counters (e.g. records, paths, calcs). Safe no-op off-thread."""
    jid = owner._CUR_JID
    if jid is None:
        return
    with owner._LOCK:
        if jid not in owner.JOBS:
            return
        p = owner.JOBS[jid].setdefault("progress", {})
        nodes = p.setdefault("nodes", [])
        n = next((x for x in nodes if x["id"] == nid), None)
        if n is None:
            n = {"id": nid, "parent": parent, "label": label or nid,
                 "kind": kind or "branch", "status": "pending",
                 "detail": None, "stat": {}, "t0": None, "t1": None}
            nodes.append(n)
        if parent is not None:
            n["parent"] = parent
        if label is not None:
            n["label"] = label
        if kind is not None:
            n["kind"] = kind
        if detail is not None:
            n["detail"] = detail
        t0 = owner.JOBS[jid].get("_t0")
        now = round(time.perf_counter() - t0, 3) if t0 is not None else 0.0
        if status is not None:
            if status == "running" and n["t0"] is None:
                n["t0"] = now
            if status in ("done", "error"):
                n["t1"] = now
            n["status"] = status
        for k, v in stat.items():
            n["stat"][k] = n["stat"].get(k, 0) + v


def compute_run_plan(owner, kind: str, books: list[str], state=None) -> dict:
    """Model the workload a run will dispatch, from current settings and
    book sizes. These are the quantities a quant developer reaches for:
    records in scope, Monte-Carlo paths, scenario path-steps simulated,
    full revaluations, path evaluations, and mean reductions. Modeled
    (settings x book sizes) -- actual kernel work may differ."""
    state = state or owner.current_state()
    s = state['settings']
    per_book = {b: int(len(state['books'][b])) for b in books if b in state['books']}
    positions = sum(per_book.values())
    paths = int(s.n_paths)
    horizon = int(s.horizon_months)
    shocks = list(map(float, s.shocks_bp))
    n_shocks = len(shocks)
    if kind == "risk":
        scope = positions
        revals_per = 1 + 2 + 2 * owner.KRD_PILLARS          # base + parallel dv01 + KRD pillars
        path_steps = paths
    elif kind in ("stress", "deposit_stress"):
        scope = sum(per_book.get(b, 0) for b in ("mbs", "deposits"))
        revals_per = 1 + n_shocks
        path_steps = paths * horizon
    elif kind in ("nii", "kpis"):
        scope = positions
        revals_per = 1
        path_steps = paths * horizon
    elif kind == "scenario_nii":
        scope = positions
        revals_per = 9
        path_steps = paths * horizon * 9
    else:                                              # unitlib, strategy, optimize
        scope = positions
        revals_per = 1
        path_steps = paths * horizon
    revaluations = scope * revals_per
    return {
        "kind": kind,
        "records": positions,
        "records_by_book": per_book,
        "in_scope": scope,
        "monte_carlo_paths": paths,
        "base_calibration_paths": s.n_paths_base,
        "horizon_months": horizon,
        "rate_shocks_bp": shocks,
        "scenario_path_steps": path_steps,
        "revaluations": revaluations,
        "path_evaluations": revaluations * paths,
        "reductions": revaluations,
        "crn_seed": int(s.seed),
        "note": "workload modeled from settings x book sizes",
    }
