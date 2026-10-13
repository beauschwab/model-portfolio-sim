"""Durable storage adapter behind the existing module repository functions."""
from __future__ import annotations

from contextvars import ContextVar
import hashlib
import copy
from pathlib import Path

from .artifacts import Codec, Objects
from .repository import Conflict, Repository
from .storage_config import StorageConfig
from .contracts import WorkspacePort, WorkspaceSnapshot, ModelIdentity, DurableRequest
from .result_transport import to_arrow_envelope

_WORKSPACE: WorkspacePort | None = None


def bind_workspace(workspace: WorkspacePort) -> None:
    """Application startup supplies its sole materialization explicitly."""
    global _WORKSPACE
    _WORKSPACE = workspace


def workspace() -> WorkspacePort:
    if _WORKSPACE is None:
        raise RuntimeError('Bind the workspace state port before starting storage')
    return _WORKSPACE

CONFIG = None
REPO = None
CODEC = None
WORKER = None
_IDENTITY = None
EXPECTED_REVISION = ContextVar("expected_revision", default=None)
IDEMPOTENCY_KEY = ContextVar("idempotency_key", default=None)
HTTP_REQUEST_SHA256 = ContextVar('http_request_sha256', default=None)
ACTIVE_JOB = ContextVar('active_job', default=None)


def identity() -> ModelIdentity:
    global _IDENTITY
    if _IDENTITY is None:
        _IDENTITY = current_identity()
    return copy.deepcopy(_IDENTITY)


def current_identity() -> ModelIdentity:
    """Read current code/artifacts without changing the startup-pinned identity."""
    import portfolio_risk
    import importlib.metadata
    root = Path(portfolio_risk.__file__).parent
    digest = hashlib.sha256()
    for folder in (root, Path(__file__).parent):
        for path in sorted(folder.rglob("*.py")):
            digest.update(path.relative_to(folder).as_posix().encode())
            digest.update(path.read_bytes().replace(b"\r\n", b"\n"))
    # Reading a historical workspace needs no binary. Production execution
    # independently requires matching native artifacts; identity pins those present.
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
    from portfolio_risk.analytics.ledger_native import library_path as ledger_library_path
    ledger_library = ledger_library_path()
    ledger_validation_native = _hash(ledger_library) if ledger_library.is_file() else None
    from portfolio_risk.analytics.owned_workflow import binary_path as workflow_binary_path
    workflow_binary = workflow_binary_path()
    workflow_native = _hash(workflow_binary) if workflow_binary.is_file() else None
    return {"engine_version": portfolio_risk.__version__, "source_sha256": digest.hexdigest(),
            "python": platform.python_version(), "platform": platform.system(), "machine": platform.machine(),
            "native_sha256": native, "risk_native_sha256": risk_native, "balance_native_sha256": balance_native,
            "ledger_validation_native_sha256": ledger_validation_native,
            "workflow_native_sha256": workflow_native, "packages": {name: importlib.metadata.version(name)
                for name in ("numpy", "numba", "polars", "scipy")}}


def start(config=None):
    global CONFIG, REPO, CODEC
    store = workspace()
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


def restore(state: WorkspaceSnapshot):
    workspace().restore_snapshot(state)


def refresh(force=False):
    store = workspace()
    if REPO is None:
        return
    with store._LOCK:
        revision = REPO.head()
        if force or revision != store.STATE_META["revision"]:
            restore(CODEC.load(REPO.revision(revision)["manifest"]))


def changed():
    store = workspace()
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


def submit(kind, operation: str, args, plan, state: WorkspaceSnapshot):
    if EXPECTED_REVISION.get() not in (None, state['revision']):
        raise Conflict('saved inputs changed before job admission; reload before executing')
    payload: DurableRequest = {"operation": operation, "args": args, "state": state, "identity": identity()}
    fingerprint = HTTP_REQUEST_SHA256.get()
    key = IDEMPOTENCY_KEY.get()
    if fingerprint and key:
        payload['http_request_sha256'] = fingerprint
        prior = REPO.list_jobs(idempotency_key=key, limit=1)
        if prior:
            existing = CODEC.metadata(REPO.job(prior[0]['id'])['request'],
                                      {'http_request_sha256', 'identity', 'operation'})
            # Admission may derive fresh deadlines/identities. An exact HTTP retry
            # returns the original immutable job instead of refreshing those values.
            if (existing.get('http_request_sha256') == fingerprint and
                    existing['identity'] == payload['identity'] and
                    prior[0]['revision'] == state['revision'] and existing['operation'] == operation):
                return prior[0]['id']
            raise Conflict('idempotency key already used for a different request or model/revision')
    request = CODEC.dump(payload)
    progress = {"stage": "queued", "pct": 0., "plan": plan or {}, "stats": {}, "elapsed_s": 0., "log": [], "nodes": []}
    store = workspace()
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
    return to_arrow_envelope(CODEC.load(result_ref(jid)))


def remote(path, payload=None, method="POST"):
    """Only internal interactive requests travel to the session-owning worker."""
    import httpx
    from fastapi import HTTPException
    try:
        response = httpx.request(method, CONFIG.worker_url + path, json=payload,
            headers={"Authorization": f"Bearer {CONFIG.worker_token}"} if CONFIG.worker_token else {}, timeout=10.)
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
    store = workspace()
    if REPO is not None:
        if REPO.head() != revision:
            raise Conflict("inputs changed during calculation; candidate discarded")
        if WORKER is not None and not REPO.owns(WORKER):
            raise Conflict("worker lease lost; candidate discarded")
    elif store.STATE_META["revision"] != revision:
        raise RuntimeError("inputs changed during calculation; candidate discarded")
