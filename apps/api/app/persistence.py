"""Durable storage adapter behind the existing module repository functions."""
from __future__ import annotations

from contextvars import ContextVar
import hashlib
import copy
from pathlib import Path

from .artifacts import Codec, Objects
from .repository import Conflict, Repository
from .storage_config import StorageConfig

CONFIG = None
REPO = None
CODEC = None
WORKER = None
_IDENTITY = None
EXPECTED_REVISION = ContextVar("expected_revision", default=None)
IDEMPOTENCY_KEY = ContextVar("idempotency_key", default=None)
ACTIVE_JOB = ContextVar('active_job', default=None)


def identity():
    global _IDENTITY
    if _IDENTITY is not None:
        return copy.deepcopy(_IDENTITY)
    import portfolio_risk
    import importlib.metadata
    root = Path(portfolio_risk.__file__).parent
    digest = hashlib.sha256()
    for folder in (root, Path(__file__).parent):
        for path in sorted(folder.rglob("*.py")):
            digest.update(path.relative_to(folder).as_posix().encode())
            digest.update(path.read_bytes().replace(b"\r\n", b"\n"))
    # Native availability is optional; the adapter includes its own DLL digest
    # in decision contexts. Record the selected binary when present as well.
    from .storage_config import ROOT
    import os
    candidates = [Path(os.environ["PORTFOLIO_DECISION_LIBRARY"])] if os.getenv("PORTFOLIO_DECISION_LIBRARY") else [
        ROOT / "packages/portfolio-decision-native/target/release" / name
        for name in ("portfolio_decision_native.dll", "libportfolio_decision_native.so", "libportfolio_decision_native.dylib")]
    native = next((hashlib.sha256(p.read_bytes()).hexdigest() for p in candidates if p.is_file()), None)
    risk_candidates = [Path(os.environ['PORTFOLIO_RISK_RUST_LIB'])] if os.getenv('PORTFOLIO_RISK_RUST_LIB') else [
        ROOT / 'packages/portfolio-risk-native/target/release' / name
        for name in ('portfolio_risk_native.dll', 'libportfolio_risk_native.so', 'libportfolio_risk_native.dylib')]
    risk_native = next((hashlib.sha256(p.read_bytes()).hexdigest() for p in risk_candidates if p.is_file()), None)
    import platform
    from portfolio_risk.analytics.balance_stream import binary_path, _hash
    balance_native = _hash(binary_path()) if binary_path().is_file() else None
    from portfolio_risk.analytics.owned_workflow import binary_path as workflow_binary_path
    workflow_binary = workflow_binary_path()
    workflow_native = _hash(workflow_binary) if workflow_binary.is_file() else None
    _IDENTITY = {"engine_version": portfolio_risk.__version__, "source_sha256": digest.hexdigest(),
            "python": platform.python_version(), "platform": platform.system(), "machine": platform.machine(),
            "native_sha256": native, "risk_native_sha256": risk_native, "balance_native_sha256": balance_native,
            "workflow_native_sha256": workflow_native, "packages": {name: importlib.metadata.version(name)
                for name in ("numpy", "numba", "polars", "scipy")}}
    return copy.deepcopy(_IDENTITY)


def start(config=None):
    global CONFIG, REPO, CODEC
    from . import store
    CONFIG = config or StorageConfig.from_env()
    if CONFIG.execution == "memory":
        REPO = CODEC = None
        store.seed_demo()
        return
    REPO = Repository(CONFIG.database_url, CONFIG.tenant, CONFIG.workspace)
    REPO.migrate()
    CODEC = Codec(Objects(CONFIG.artifact_url, CONFIG.tenant, CONFIG.workspace))
    identity()  # Pin identity at process initialization, not after a source edit.
    if REPO.head() is None:
        if not CONFIG.seed_demo:
            raise RuntimeError("Workspace is empty. Import a snapshot with python -m app.storage_admin before serving.")
        store.seed_demo()
        store.STATE_META["revision"] = 0
        REPO.initialize(CODEC.dump(store.snapshot_local()))
    refresh(force=True)


def stop():
    global REPO, CODEC, CONFIG, _IDENTITY
    if REPO is not None:
        REPO.engine.dispose()
    REPO = CODEC = CONFIG = None
    _IDENTITY = None


def restore(state):
    from . import store
    store.TAPES.clear()
    store.TAPES.update(state.get('tapes', {}))
    store.COHORT_PUBLICATIONS.clear()
    store.COHORT_PUBLICATIONS.update(state.get('cohort_publications', {}))
    for field, name in (("books", "BOOKS"), ("market", "MARKET"), ("scenarios", "SCENARIOS"),
                        ("programs", "PROGRAMS"), ("assumptions", "ASSUMPTIONS")):
        target = getattr(store, name)
        target.clear()
        target.update(state[field])
    for field, name in (("settings", "SETTINGS"), ("dep_hist", "DEP_HIST"), ("mbs_hists", "MBS_HISTS"),
                        ("asof", "ASOF"), ("equity", "EQUITY"), ("hedges", "HEDGES")):
        setattr(store, name, state[field])
    store.STATE_META["revision"] = state["revision"]
    store.CACHE.clear()
    store.UNITLIB = store.BASE_KPIS = None


def refresh(force=False):
    from . import store
    if REPO is None:
        return
    with store._LOCK:
        revision = REPO.head()
        if force or revision != store.STATE_META["revision"]:
            restore(CODEC.load(REPO.revision(revision)["manifest"]))


def changed():
    from . import store
    if REPO is None:
        return
    expected = store.STATE_META["revision"]
    try:
        if EXPECTED_REVISION.get() not in (None, expected):
            raise Conflict("saved inputs changed; reload before editing")
        state = store.snapshot_local() | {"revision": expected + 1}
        manifest = CODEC.dump(state)
        REPO.save_revision(expected, manifest)
    except BaseException:
        refresh(force=True)
        raise


def operation(fn):
    from . import store, decision_store
    from . import cohort_store
    from . import treasury_store
    allowed = {getattr(store, name): "store." + name for name in (
        "run_pricing", "run_risk_all", "run_stress_all", "run_kpis_scenario", "run_nii",
        "build_unitlib_job", "run_strategy_job", "run_optimize_job", "run_scenario_grid",
        "run_forecast", "run_balance_stress", "run_streamed_balance_stress", "run_saved_balance_stress", "fetch_research_data", "run_treasury")}
    allowed.update({decision_store.build: "decision.build", decision_store.update: "decision.update"})
    allowed[treasury_store.run]='treasury.run'
    allowed.update({getattr(cohort_store, name): 'cohort.'+name for name in ('import_tape','build','analytics','attribution','audit')})
    if fn not in allowed:
        raise ValueError("function is not registered for durable execution")
    return allowed[fn]


def resolve(name):
    from . import store, decision_store
    from . import cohort_store
    from . import treasury_store
    module, attr = name.split(".", 1)
    fn = getattr({"store": store, "decision": decision_store, "cohort":cohort_store, "treasury":treasury_store}[module], attr)
    if operation(fn) != name:
        raise ValueError("unregistered operation")
    return fn


def submit(kind, fn, args, plan, state):
    request = CODEC.dump({"operation": operation(fn), "args": args, "state": state, "identity": identity()})
    progress = {"stage": "queued", "pct": 0., "plan": plan or {}, "stats": {}, "elapsed_s": 0., "log": [], "nodes": []}
    from . import store
    return REPO.enqueue(kind, state["revision"], request, progress,
                        key=IDEMPOTENCY_KEY.get(), max_queue=store.MAX_QUEUE)


def job_status(jid):
    row = REPO.job(jid)
    return {k: row[k] for k in ("id", "kind", "status", "revision", "progress", "detail", "attempt", "created_at", "finished_at")}


def result_ref(jid):
    row = REPO.job(jid)
    if row["status"] != "done":
        raise RuntimeError(f"job {row['status']}")
    return row["result"]


def job_result(jid):
    from . import store
    return store.to_arrow_envelope(CODEC.load(result_ref(jid)))


def remote(path, payload=None, method="POST"):
    """Only internal interactive requests travel to the session-owning worker."""
    import httpx
    from fastapi import HTTPException
    try:
        response = httpx.request(method, CONFIG.worker_url + path, json=payload,
            headers={"Authorization": f"Bearer {CONFIG.worker_token}"}, timeout=10.)
    except httpx.RequestError as exc:
        raise HTTPException(503, "Calculation worker is unavailable") from exc
    if response.status_code >= 400:
        try:
            detail = response.json()["detail"]
        except (ValueError, KeyError):
            detail = "Calculation worker request failed"
        raise HTTPException(response.status_code, detail)
    return response


def assert_current(revision):
    from . import store
    if REPO is not None:
        if REPO.head() != revision:
            raise Conflict("inputs changed during calculation; candidate discarded")
        if WORKER is not None and not REPO.owns(WORKER):
            raise Conflict("worker lease lost; candidate discarded")
    elif store.STATE_META["revision"] != revision:
        raise RuntimeError("inputs changed during calculation; candidate discarded")
