"""Repository-shaped state facade and engine adapters.

Module dictionaries are a worker-local materialization of durable revisions.
The explicit memory execution mode is retained for isolated engine tests.
"""
from __future__ import annotations

import copy
from contextvars import ContextVar
import hashlib
import io
import json
import struct
import threading
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import numpy as np
import polars as pl

from .schemas import MarketScenario, RiskSettings

_LOCK = threading.RLock()
_POOL = ThreadPoolExecutor(max_workers=1)   # numba kernels saturate cores
_CUR_JID: str | None = None                 # job on the single worker thread
KRD_PILLARS = 10                             # curve pillars bumped for key-rate durations

BOOKS: dict[str, pl.DataFrame] = {}
TAPES: dict[str, dict] = {}
COHORT_PUBLICATIONS: dict[str, dict] = {}
MARKET: dict[str, Any] = {}
SCENARIOS: dict[str, MarketScenario] = {}
SETTINGS = RiskSettings()
JOBS: dict[str, dict] = {}
DEP_HIST: pl.DataFrame | None = None
MBS_HISTS = None
ASOF = None
EQUITY = 0.0
HEDGES = None
PROGRAMS: dict[str, dict] = {}
UNITLIB = None
BASE_KPIS = None  # legacy names retained for callers; evaluations use CACHE atomically
CACHE: dict = {}
PRICING: dict = {}  # bounded content-keyed engine cache, independent of library publication
DECISIONS: dict = {}  # opaque session id -> engine session and input revision
STATE_META = {"revision": 0}
ASSUMPTIONS: dict = {}
_RUN_STATE = ContextVar("application_run_state", default=None)
MAX_JOBS = 50
MAX_QUEUE = 32
MAX_RESULT_BYTES = 256 * 1024 * 1024
JOB_TTL_SECONDS = 3600


class QueueFull(RuntimeError):
    """The bounded compute queue cannot accept another job."""


def snapshot_local():
    with _LOCK:
        return {"books": {k: v.clone() for k, v in BOOKS.items()},
                "tapes": copy.deepcopy(TAPES), "cohort_publications": copy.deepcopy(COHORT_PUBLICATIONS),
                "market": copy.deepcopy(MARKET), "scenarios": copy.deepcopy(SCENARIOS),
                "settings": SETTINGS.model_copy(deep=True), "dep_hist": DEP_HIST,
                "mbs_hists": MBS_HISTS, "asof": ASOF, "equity": EQUITY,
                "hedges": HEDGES, "programs": copy.deepcopy(PROGRAMS),
                "assumptions": copy.deepcopy(ASSUMPTIONS), "revision": STATE_META["revision"]}


def snapshot():
    from . import persistence
    with _LOCK:
        persistence.refresh()
        return snapshot_local()


def current_state():
    return _RUN_STATE.get() or snapshot()


# ---- input fingerprints: the leaves of the client's recalculation graph ----------
# Each node is a content hash of one slice of the committed snapshot, so every API
# replica and the worker agree on what changed between two revisions without any
# extra bookkeeping. A result depends on a set of nodes; when none of them changed,
# the result is still current even though the revision moved.
_FINGERPRINTS: dict = {"revision": None, "inputs": {}}
# snapshot keys that only the cohort workflow reads; no base result depends on them
_COHORT_KEYS = ("tapes", "cohort_publications")
_NODE_KEYS = ("books", "market", "settings", "assumptions", "scenarios", "revision") + _COHORT_KEYS


def _digest(value, h) -> None:
    """Feed a canonical encoding of a snapshot value into hash ``h``."""
    if isinstance(value, pl.DataFrame):
        h.update(b"F" + repr(value.schema).encode())
        plain = [c for c, t in value.schema.items() if t != pl.Object]
        if plain:
            buf = io.BytesIO()
            value.select(plain).rechunk().write_ipc(buf, compression="uncompressed")
            h.update(hashlib.sha256(buf.getvalue()).digest())
        for c in (c for c, t in value.schema.items() if t == pl.Object):
            h.update(c.encode())
            for item in value[c].to_list():
                _digest(item, h)
    elif isinstance(value, np.ndarray):
        h.update(b"A" + str(value.dtype).encode() + str(value.shape).encode() + np.ascontiguousarray(value).tobytes())
    elif isinstance(value, dict):
        h.update(b"D")
        for k in sorted(value, key=repr):
            h.update(repr(k).encode())
            _digest(value[k], h)
    elif isinstance(value, (list, tuple)):
        h.update(b"L%d" % len(value))
        for item in value:
            _digest(item, h)
    elif hasattr(value, "model_dump"):
        _digest(value.model_dump(), h)
    else:
        h.update(repr(value).encode())
    h.update(b";")


def _fingerprint(value) -> str:
    h = hashlib.sha256()
    _digest(value, h)
    return h.hexdigest()[:16]


def input_fingerprints(state=None) -> dict:
    """``{"revision": r, "nodes": {...}}``: a content hash per input node of the
    snapshot at revision ``r``, memoised by revision. The revision travels with the
    hashes so a caller never files them under a revision they were not computed at.

    Nodes: ``books:<name>``, ``market``, ``settings``, ``assumptions:deposits``,
    ``assumptions:cds``, ``assumptions:other``, ``scenarios``, ``cohorts`` and
    ``context`` (every other snapshot key, so an input added later is covered)."""
    state = state or snapshot()
    with _LOCK:
        if _FINGERPRINTS["revision"] == state["revision"]:
            return {"revision": state["revision"], "nodes": dict(_FINGERPRINTS["inputs"])}
    assumptions = state["assumptions"]
    nodes = {f"books:{name}": _fingerprint(frame) for name, frame in state["books"].items()}
    nodes["market"] = _fingerprint(state["market"])
    nodes["settings"] = _fingerprint(state["settings"])
    nodes["assumptions:deposits"] = _fingerprint(assumptions.get("deposit_segments"))
    nodes["assumptions:cds"] = _fingerprint(assumptions.get("cd_ew_params"))
    nodes["assumptions:other"] = _fingerprint({k: v for k, v in assumptions.items() if k not in ("deposit_segments", "cd_ew_params")})
    nodes["scenarios"] = _fingerprint(state["scenarios"])
    nodes["cohorts"] = _fingerprint({k: state.get(k) for k in _COHORT_KEYS})
    nodes["context"] = _fingerprint({k: v for k, v in state.items() if k not in _NODE_KEYS and not k.startswith("_")})
    with _LOCK:
        _FINGERPRINTS.update(revision=state["revision"], inputs=nodes)
    return {"revision": state["revision"], "nodes": dict(nodes)}


def changed():
    """Caller holds the repository lock; invalidate the published cache bundle."""
    global UNITLIB, BASE_KPIS
    from . import persistence
    persistence.changed()
    STATE_META["revision"] += 1
    CACHE.clear()
    UNITLIB = BASE_KPIS = None


def replace_book(name, rows):
    from .books import normalize_book
    with _LOCK:
        frame = normalize_book(name, rows, BOOKS[name], ASOF)
        BOOKS[name] = frame
        COHORT_PUBLICATIONS.pop(name, None)
        changed()
    return len(frame)


def market_view():
    with _LOCK:
        return {"swap_tenors": [1, 2, 3, 4, 5, 7, 10, 15, 20, 30],
                "swap_rates": MARKET["swap_rates"].tolist(), "vol_pts": MARKET["vol_pts"].tolist(),
                "source": MARKET.get("source", ""), "revision": STATE_META["revision"],
                "provenance": copy.deepcopy(MARKET.get("provenance", {}))}


def replace_market(m):
    with _LOCK:
        MARKET.update(swap_rates=np.array(m.swap_rates), vol_pts=np.array(m.vol_pts),
                      source="Manually supplied research market", provenance={
                          "curve": "assumed", "volatility": "assumed",
                          "warnings": ["Manually supplied inputs; source snapshot association cleared."]})
        changed()


def fetch_research_data(req):
    from .market_data import fetch_snapshot, summary
    report("Downloading research data", 0.1)
    result = fetch_snapshot(req)
    report("Research snapshot saved", 1.0)
    return summary(result)


def apply_research_curve(sid, expected_revision):
    from .market_data import DataError, get_snapshot
    from .schemas import Market
    snap = get_snapshot(sid)
    curve = snap.get("curve")
    if snap["dataset"] != "eris_sofr" or not curve:
        raise DataError("only an Eris discount-curve snapshot can replace the active curve")
    with _LOCK:
        if STATE_META["revision"] != expected_revision:
            raise RuntimeError("inputs changed while reviewing this snapshot; reload before applying")
        checked = Market(swap_rates=curve["swap_rates"], vol_pts=MARKET["vol_pts"].tolist())
        prior_vol = MARKET.get("provenance", {}).get("volatility", "assumed")
        warnings = list(snap["warnings"])
        warnings.append(f"Book valuation date remains {ASOF}; market date is {curve['as_of']}. This is a research repricing of the existing book, not an aged portfolio.")
        MARKET.update(swap_rates=np.array(checked.swap_rates), source=f"Eris SOFR {curve['as_of']} · derived research curve",
                      provenance={"snapshot_id": sid, "curve": "derived", "curve_as_of": curve["as_of"],
                                  "volatility": prior_vol, "warnings": warnings,
                                  "projection": copy.deepcopy(curve)})
        changed()
        return market_view()


def prune_jobs():
    """Bound completed results by age, count and bytes; never evict active work."""
    with _LOCK:
        finished = sorted((j for j in JOBS.values() if j["status"] in ("done", "error")),
                          key=lambda j: j.get("finished_at", 0))
        total = sum(len(j.get("result") or b"") for j in finished)
        for j in finished:
            expired = time.time() - j.get("finished_at", time.time()) > JOB_TTL_SECONDS
            if expired or len(JOBS) > MAX_JOBS or total > MAX_RESULT_BYTES:
                total -= len(j.get("result") or b"")
                JOBS.pop(j["id"], None)


def job_status(jid):
    from . import persistence
    if persistence.REPO is not None:
        return persistence.job_status(jid)
    with _LOCK:
        prune_jobs()
        j = JOBS[jid]
        return copy.deepcopy({k: v for k, v in j.items() if k != "result"})


def job_result(jid):
    from . import persistence
    if persistence.REPO is not None:
        return persistence.job_result(jid)
    with _LOCK:
        prune_jobs()
        j = JOBS[jid]
        if j["status"] != "done":
            raise RuntimeError(f"job {j['status']}")
        return j["result"]


def balance_sheet(state):
    bs = {k: v for k, v in state['books'].items() if len(v)}
    bs.update(mbs_hists=state['mbs_hists'], asof=state['asof'],
              equity=state['equity'], hedges=state['hedges'])
    return bs


def base_calibration(state):
    from portfolio_risk.analytics.calibration import calibrate_books
    if "_base_oas" not in state:
        state["_base_oas"] = calibrate_books(balance_sheet(state), state['market']["swap_rates"],
                           state['market']["vol_pts"], state['dep_hist'], state['settings'].seed)
    return state["_base_oas"]



def seed_demo():
    """Load the WFC-proportional model balance sheet + demo market."""
    from portfolio_risk.demo import (demo_deposit_history, demo_market,
                               model_balance_sheet)
    PRICING.clear()
    TAPES.clear()
    COHORT_PUBLICATIONS.clear()
    global DEP_HIST, MBS_HISTS, ASOF
    bs = model_balance_sheet(scale=0.01, basis="amortized_cost",
                             include_markets_bs=True)
    for k in ("mbs", "loans", "debt", "deposits", "cds", "mm"):
        if bs.get(k) is not None:
            BOOKS[k] = bs[k]
    MBS_HISTS = bs["mbs_hists"]
    ASOF = bs["asof"]
    global EQUITY, HEDGES
    EQUITY = bs.get("equity", 0.0)
    from portfolio_risk.demo import demo_hedge_book
    HEDGES = demo_hedge_book(scale=0.01)
    DEP_HIST = demo_deposit_history()
    from portfolio_risk.products.deposits import SEGMENTS
    from portfolio_risk.products.cds import CD_EW_PARAMS
    ASSUMPTIONS.update(deposit_segments=copy.deepcopy(SEGMENTS), cd_ew_params=CD_EW_PARAMS.tolist())
    sr, vp = demo_market()
    MARKET.clear()
    MARKET["swap_rates"] = sr
    MARKET["vol_pts"] = vp
    MARKET["source"] = "Synthetic research market"
    MARKET["provenance"] = {"curve": "assumed", "volatility": "assumed",
                            "warnings": ["Curve, volatility and behavioral histories are synthetic demo inputs."]}


def apply_scenario(sc: MarketScenario, quarter: int, state=None
                   ) -> tuple[np.ndarray, np.ndarray, float]:
    """Map trader-space legs onto engine inputs at a given quarter:
    10y leg shifts all pillars in parallel; 2s10s leg twists linearly
    around the 5y pivot (2y -x/2, 10y +x/2); vol leg shifts the surface;
    spread leg is returned for OAS-level application by the caller."""
    def leg(xs: list[float]) -> float:
        if not xs:
            return 0.0
        return xs[min(quarter, len(xs) - 1)] * 1e-4

    state = state or current_state()
    sr = state['market']["swap_rates"].copy()
    tens = np.array([1, 2, 3, 4, 5, 7, 10, 15, 20, 30], dtype=float)
    sr = sr + leg(sc.ust10y_bp)
    tw = leg(sc.twos_tens_bp)
    if tw:
        sr = sr + tw * (np.clip(tens, 2.0, 10.0) - 5.0) / 8.0  # 2s10s pivot 5y
    vp = state['market']["vol_pts"].copy()
    vleg = leg(sc.vol_bp)
    if vleg:
        vp = vp.copy()
        vp[:, 2] = np.maximum(vp[:, 2] + vleg, 1e-4)
    return sr, vp, leg(sc.spread_bp)


# ---- Arrow envelope serialization -------------------------------------------
# Computed polars frames cross the wire as Apache Arrow IPC, not JSON. A result
# tree is split into a JSON "skeleton" (every pl.DataFrame leaf replaced by a
# {"__arrow__": i} marker) plus N concatenated Arrow IPC blobs. Scalars, lists,
# and numpy-derived arrays (e.g. runoff_vectors) stay inline in the skeleton.
ARROW_ENVELOPE_MIME = "application/vnd.arrow-envelope"
_IPC_COMPRESSION = None      # Arrow-JS IPC compression is version-fragile; these
                             # frames are small -- uncompressed is the safe wire.
# polars defaults string columns to the Arrow Utf8View layout (type 24), which
# apache-arrow JS cannot read. oldest() emits plain Utf8 the JS reader accepts.
_IPC_COMPAT = pl.CompatLevel.oldest()


def _arrow_safe(df: pl.DataFrame) -> pl.DataFrame:
    """Cast columns Arrow IPC cannot encode (pl.Object, e.g. the demo
    `call_schedule` list column) to JSON-encoded Utf8. Anything else
    unencodable surfaces loudly from write_ipc -- intentionally."""
    obj_cols = [c for c, dt in zip(df.columns, df.dtypes) if dt == pl.Object]
    if not obj_cols:
        return df
    return df.with_columns([
        pl.col(c).map_elements(
            lambda v: None if v is None else json.dumps(v, default=str),
            return_dtype=pl.Utf8).alias(c)
        for c in obj_cols
    ])


def _frames_to_arrow(obj):
    """Walk a result tree, returning (skeleton, blobs). Each pl.DataFrame leaf
    becomes a {"__arrow__": idx} marker and an Arrow IPC blob at that index."""
    blobs: list[bytes] = []

    def walk(o):
        import datetime
        if isinstance(o, (datetime.date, datetime.datetime)):
            return o.isoformat()
        if isinstance(o, bytes):
            import base64
            return {'__binary_base64__': base64.b64encode(o).decode('ascii')}
        if isinstance(o, pl.DataFrame):
            buf = io.BytesIO()
            _arrow_safe(o).write_ipc(buf, compression=_IPC_COMPRESSION,
                                     compat_level=_IPC_COMPAT)
            blobs.append(buf.getvalue())
            return {"__arrow__": len(blobs) - 1}
        if isinstance(o, dict):
            return {k: walk(v) for k, v in o.items()}
        if isinstance(o, (list, tuple)):
            return [walk(v) for v in o]
        if isinstance(o, np.ndarray):
            return walk(o.tolist())
        if isinstance(o, np.generic):
            return walk(o.item())
        if isinstance(o, float) and not np.isfinite(o):
            return None
        return o

    return walk(obj), blobs


def _pack_envelope(skeleton, blobs: list[bytes]) -> bytes:
    """Frame skeleton + blobs into the ARW1 binary envelope (little-endian)."""
    sj = json.dumps(skeleton, allow_nan=False).encode("utf-8")
    parts = [b"ARW1", struct.pack("<I", len(sj)), sj,
             struct.pack("<I", len(blobs))]
    parts += [struct.pack("<I", len(b)) for b in blobs]
    parts += blobs
    return b"".join(parts)


def to_arrow_envelope(obj) -> bytes:
    """Serialize a result object (frames + scalars) to an ARW1 envelope."""
    return _pack_envelope(*_frames_to_arrow(obj))


def report(stage: str | None = None, pct: float | None = None,
           log: str | None = None, **inc: float) -> None:
    """Update the running job's progress telemetry. Stage/pct are set;
    keyword counters in **inc are accumulated (records, revaluations,
    reductions, path_evaluations, scenario_paths, books_done, ...).
    Safe no-op when no job is bound to the worker thread."""
    jid = _CUR_JID
    if jid is None:
        return
    with _LOCK:
        if jid not in JOBS:
            return
        p = JOBS[jid].setdefault("progress", {})
        if stage is not None:
            p["stage"] = stage
        if pct is not None:
            # monotonic: nested adapters (run_nii inside kpis/optimize) report
            # their own band; never let the bar run backward within a job
            nxt = round(max(0.0, min(100.0, float(pct))), 1)
            p["pct"] = max(p.get("pct", 0.0), nxt)
        t0 = JOBS[jid].get("_t0")
        if t0 is not None:
            p["elapsed_s"] = round(time.perf_counter() - t0, 3)
        stats = p.setdefault("stats", {})
        for k, v in inc.items():
            stats[k] = stats.get(k, 0) + v
        if log:
            p.setdefault("log", []).append(
                {"t": p.get("elapsed_s", 0.0), "msg": log})
            p["log"] = p["log"][-60:]      # keep the tail bounded


def node(nid: str, *, parent: str | None = None, label: str | None = None,
         kind: str | None = None, status: str | None = None,
         detail: str | None = None, **stat: float) -> None:
    """Upsert a pipeline node on the running job's progress telemetry.

    Nodes form a flat list with `parent` pointers (rendered as a tree on the
    client). `kind` in {build, branch, paths, cashflow, oas, reduce, solve};
    `status` in {pending, running, done, error}. `t0` is stamped the first time
    a node goes `running`, `t1` when it reaches `done`/`error`. **stat carries
    per-node counters (e.g. records, paths, calcs). Safe no-op off-thread."""
    jid = _CUR_JID
    if jid is None:
        return
    with _LOCK:
        if jid not in JOBS:
            return
        p = JOBS[jid].setdefault("progress", {})
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
        t0 = JOBS[jid].get("_t0")
        now = round(time.perf_counter() - t0, 3) if t0 is not None else 0.0
        if status is not None:
            if status == "running" and n["t0"] is None:
                n["t0"] = now
            if status in ("done", "error"):
                n["t1"] = now
            n["status"] = status
        for k, v in stat.items():
            n["stat"][k] = n["stat"].get(k, 0) + v


def compute_run_plan(kind: str, books: list[str], state=None) -> dict:
    """Model the workload a run will dispatch, from current settings and
    book sizes. These are the quantities a quant developer reaches for:
    records in scope, Monte-Carlo paths, scenario path-steps simulated,
    full revaluations, path evaluations, and mean reductions. Modeled
    (settings x book sizes) -- actual kernel work may differ."""
    state = state or current_state()
    s = state['settings']
    per_book = {b: int(len(state['books'][b])) for b in books if b in state['books']}
    positions = sum(per_book.values())
    paths = int(s.n_paths)
    horizon = int(s.horizon_months)
    shocks = list(map(float, s.shocks_bp))
    n_shocks = len(shocks)
    if kind == "risk":
        scope = positions
        revals_per = 1 + 2 + 2 * KRD_PILLARS          # base + parallel dv01 + KRD pillars
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


def submit(kind: str, fn, *args, plan: dict | None = None, state=None) -> str:
    state = state or snapshot()
    state['settings'].require_production_backend()
    args = copy.deepcopy(args)
    from . import persistence
    if persistence.REPO is not None:
        return persistence.submit(kind, fn, args, plan, state)
    with _LOCK:
        prune_jobs()
        if sum(j["status"] in ("queued", "running") for j in JOBS.values()) >= MAX_QUEUE:
            raise QueueFull("compute queue is full; wait for a job to finish")
        jid = uuid.uuid4().hex[:12]
        JOBS[jid] = {"id": jid, "kind": kind, "status": "queued", "revision": state['revision'],
                    "market_provenance": copy.deepcopy(state["market"].get("provenance", {})),
                     "detail": None, "result": None,
                     "progress": {"stage": "queued", "pct": 0.0, "plan": plan or {},
                                  "stats": {}, "elapsed_s": 0.0, "log": [], "nodes": []}}

    def run():
        global _CUR_JID
        from portfolio_risk.core.runtime import RunConfig, run_context
        import numba
        _CUR_JID = jid
        token = _RUN_STATE.set(state)
        JOBS[jid]["status"] = "running"
        JOBS[jid]["_t0"] = time.perf_counter()
        report(stage="starting", pct=1.0, log=f"{kind} run started")
        try:
            settings = state['settings']
            numba.set_num_threads(settings.n_threads or numba.config.NUMBA_NUM_THREADS)
            config = RunConfig(settings.n_paths, settings.n_paths_base, settings.horizon_months,
                               state['assumptions'].get("deposit_segments"),
                               tuple(state['assumptions'].get("cd_ew_params", [])) or None, compute_backend=settings.compute_backend)
            with run_context(config) as context:
                encoded = to_arrow_envelope(fn(*args))
                report(cache_hits=context.hits, cache_misses=context.misses)
            if len(encoded) > MAX_RESULT_BYTES:
                raise RuntimeError("result exceeds the retention byte limit; reduce book size")
            with _LOCK:
                JOBS[jid]["result"] = encoded
                JOBS[jid]["status"] = "done"
                JOBS[jid]["finished_at"] = time.time()
            report(stage="done", pct=100.0, log="run complete")
        except Exception as e:
            with _LOCK:
                JOBS[jid]["status"] = "error"
                JOBS[jid]["finished_at"] = time.time()
                JOBS[jid]["detail"] = f"{type(e).__name__}: {e}"
            report(stage="error", log=f"{type(e).__name__}: {e}")
        finally:
            with _LOCK:
                if jid in JOBS:
                    JOBS[jid].pop("_t0", None)
                prune_jobs()
            _RUN_STATE.reset(token)
            _CUR_JID = None

    _POOL.submit(run)
    return jid


# ---- engine adapters (each returns JSON-able frames) -------------------------

def pricing_scope(state, books, overrides):
    """Validate scope/IDs before queueing; numerical validation is engine-owned."""
    from portfolio_risk.analytics.incremental import SUPPORTED_BOOKS
    selected = list(SUPPORTED_BOOKS) if books is None else books
    if set(selected) - set(SUPPORTED_BOOKS):
        raise ValueError("pricing supports mbs, loans, debt, deposits and cds; money markets and hedges are not included")
    if set(overrides) - set(selected):
        raise ValueError("spread overrides must refer to selected books")
    for name, values in overrides.items():
        frame = state['books'][name]
        ids = frame['cusip' if name == 'mbs' else 'id'].to_list()
        if set(values) - set(ids):
            raise ValueError(f"{name}: spread override refers to an unknown position")
    return selected


def run_pricing(books, sr, vp, spread_shift=0.0, overrides=None, options=None):
    from portfolio_risk.analytics.incremental import price_books
    from portfolio_risk.analytics.whatif import compare_books
    from portfolio_risk.core.dependency import DependencyCache
    from portfolio_risk.core.runtime import RunConfig
    state = current_state()
    options = options or {}
    state['settings'].require_production_backend()
    if options.get('backend', 'rust') != 'rust':
        raise ValueError('PYTHON_BACKEND_DEPRECATED: production pricing requires the native graph')
    with _LOCK:
        if "cache" not in PRICING:
            # Mixed-book curve risk retains ~179 MiB at 128 paths. Keep the
            # interactive working set, with explicit byte AND node ceilings.
            PRICING["cache"] = DependencyCache(max_bytes=512 * 1024 * 1024, max_entries=500_000)
        cache = PRICING["cache"]
    settings = state['settings']
    config = RunConfig(settings.n_paths, settings.n_paths_base, settings.horizon_months,
                       state['assumptions'].get('deposit_segments'),
                       tuple(state['assumptions'].get('cd_ew_params', [])) or None, compute_backend=settings.compute_backend)
    report(stage="resolving pricing dependencies", pct=5.0)
    compare = options.get('compare', False)
    runner = compare_books if compare else price_books
    extra = ({'assumption_overrides': options.get('assumption_overrides', {}),
              'calibration_mode': options.get('calibration_mode', 'hold')} if compare else {})
    result = runner({b: state['books'][b] for b in books}, asof=state['asof'],
        swap_rates=state['market']['swap_rates'], vol_pts=state['market']['vol_pts'],
        cache=cache, config=config, seed=settings.seed, mbs_hists=state['mbs_hists'],
        dep_hist=state['dep_hist'], scenario_market=(sr, vp), spread_shift=spread_shift,
        spread_overrides_bp=overrides, backend=None,
        include_analytics=options.get('include_analytics', False),
        balance_sheet_extras=({'mm': state['books'].get('mm'), 'hedges': state['hedges'], 'equity': state['equity']}
                              if set(books) == {'mbs', 'loans', 'debt', 'deposits', 'cds'} else None), **extra)
    result['revision'] = state['revision']
    graphs = [result['baseline']['graph'], result['revised']['graph']] if compare else [result['graph']]
    report(stage="priced selected books", pct=99.0,
           pricing_nodes_computed=sum(s['computed'] for g in graphs for s in g.values()),
           pricing_nodes_reused=sum(s['reused'] for g in graphs for s in g.values()))
    return result


def run_risk_all(books: list[str], sr, vp, spread_shift: float = 0.0, scenario=False):
    state = current_state()
    from portfolio_risk import run_cd_risk, run_corp_risk, run_deposit_risk
    from portfolio_risk.risk import run_risk
    from portfolio_risk.analytics.calibration import spread_oas
    fixed = spread_oas(base_calibration(state), spread_shift) if scenario else {}
    books = [b for b in books if len(state['books'].get(b, []))]
    out = {}
    order = [b for b in ("mbs", "loans", "debt", "deposits", "cds") if b in books]
    total = max(len(order), 1)
    done = 0
    factor = 1 + 2 + 2 * KRD_PILLARS              # base + parallel dv01 + KRD pillars
    _oas_books = {"mbs", "loans", "debt", "cds"}  # books priced via an OAS solve
    npaths = state['settings'].n_paths

    # declare the pipeline skeleton: build -> branch per book -> paths/cashflow/oas
    node("build", parent=None, kind="build", status="running",
         label="Position build", detail=f"{total} books in scope")
    for b in order:
        node(f"book:{b}", parent="build", kind="branch", label=b.upper(),
             detail=f"{int(len(state['books'][b]))} positions \u00b7 {factor} bump scenarios")
        node(f"{b}:paths", parent=f"book:{b}", kind="paths", label="rate paths")
        node(f"{b}:cf", parent=f"book:{b}", kind="cashflow", label="cashflow engine")
        if b in _oas_books:
            node(f"{b}:oas", parent=f"book:{b}", kind="oas", label="OAS solve")

    report(stage="seeding CRN", pct=3.0,
           log=f"CRN draws: {npaths} paths, seed {state['settings'].seed}")
    node("build", status="done", records=sum(int(len(state['books'][b])) for b in order))

    def _start(book: str):
        n = int(len(state['books'][book]))
        node(f"book:{book}", status="running")
        node(f"{book}:paths", status="running")
        node(f"{book}:paths", status="done", paths=npaths)
        node(f"{book}:cf", status="running")
        report(paths_generated=npaths)

    def _did(book: str):
        nonlocal done
        done += 1
        n = int(len(state['books'][book]))
        rv = n * factor
        cf = n * factor * npaths
        node(f"{book}:cf", status="done", calcs=cf)
        report(cashflow_calcs=cf)
        if book in _oas_books:
            node(f"{book}:oas", status="running")
            node(f"{book}:oas", status="done", calcs=n * factor)
            report(oas_calcs=n * factor)
        node(f"book:{book}", status="done")
        report(stage=f"revalued {book}", pct=3 + 94 * done / total,
               records=n, revaluations=rv,
               path_evaluations=rv * npaths, reductions=rv,
               books_done=1, log=f"{book}: {n} positions \u2192 {rv} revaluations")

    if "mbs" in books:
        report(stage="revaluing mbs", log="mbs: OAS-held dv01 + KRD by pillar")
        _start("mbs")
        out["mbs"] = run_risk(state['books']["mbs"], sr, vp, *state['mbs_hists'],
                              seed=state['settings'].seed, oas=fixed.get("mbs"))
        _did("mbs")
    if "loans" in books:
        report(stage="revaluing loans")
        _start("loans")
        out["loans"] = run_corp_risk(state['books']["loans"], state['asof'], sr, vp,
                                     *state['mbs_hists'], seed=state['settings'].seed, oas=fixed.get("loans"))
        _did("loans")
    if "debt" in books:
        report(stage="revaluing debt")
        _start("debt")
        out["debt"] = run_corp_risk(state['books']["debt"], state['asof'], sr, vp,
                                    *state['mbs_hists'], seed=state['settings'].seed, oas=fixed.get("debt"))
        _did("debt")
    if "deposits" in books:
        report(stage="revaluing deposits")
        _start("deposits")
        out["deposits"] = run_deposit_risk(state['books']["deposits"], sr, vp,
                                           state['dep_hist'], seed=state['settings'].seed, oas=fixed.get("deposits"))
        _did("deposits")
    if "cds" in books:
        report(stage="revaluing cds")
        _start("cds")
        out["cds"] = run_cd_risk(state['books']["cds"], state['asof'], sr, vp,
                                 seed=state['settings'].seed, oas=fixed.get("cds"))
        _did("cds")
    if state['hedges'] is not None and {"mbs", "loans", "debt", "deposits", "cds"}.issubset(books):
        from portfolio_risk.products.hedges import run_hedge_risk
        swaps, swaptions = state['hedges']
        hedge = run_hedge_risk(swaps, swaptions, state['asof'], sr, vp,
                               seed=state['settings'].seed, horizon=state['settings'].horizon_months)
        out["hedges"] = pl.DataFrame([{ "dv01": hedge["book_dv01_$"], **hedge["book_krd"] }])
    return out


def run_stress_all(books, sr, vp, scenario=False, spread_shift=0.0):
    state = current_state()
    from portfolio_risk import run_deposit_stress
    from portfolio_risk.stress import run_stress
    out = {}
    from portfolio_risk.analytics.calibration import spread_oas
    fixed = spread_oas(base_calibration(state), spread_shift) if scenario else {}
    books = [b for b in books if len(state['books'].get(b, []))]
    shocks = tuple(state['settings'].shocks_bp)
    factor = 1 + len(shocks)
    npaths = state['settings'].n_paths
    node("build", parent=None, kind="build", status="running",
         label="Position build", detail=f"{len(shocks)} forward shocks")
    report(stage="seeding CRN", pct=4.0,
           log=f"{len(shocks)} forward shocks: {list(shocks)} bp")
    node("build", status="done")

    def _stress_book(book: str, runner, p_run: float, p_done: float):
        n = int(len(state['books'][book]))
        node(f"book:{book}", parent="build", kind="branch", label=book.upper(),
             detail=f"{n} positions \u00d7 {factor} states", status="running")
        node(f"{book}:paths", parent=f"book:{book}", kind="paths",
             label="forward shock paths", status="running")
        node(f"{book}:paths", status="done", paths=npaths)
        report(paths_generated=npaths)
        node(f"{book}:cf", parent=f"book:{book}", kind="cashflow",
             label="stress cashflow engine", status="running")
        report(stage=f"stressing {book}", pct=p_run)
        res = runner(n)
        cf = n * factor * npaths
        node(f"{book}:cf", status="done", calcs=cf)
        node(f"book:{book}", status="done")
        report(stage=f"stressed {book}", pct=p_done, records=n,
               revaluations=n * factor, path_evaluations=cf,
               reductions=n * factor, books_done=1, cashflow_calcs=cf,
               log=f"{book}: {n} positions \u00d7 {factor} states")
        return res

    if "mbs" in books:
        def _run_mbs(_n):
            pos, agg, prof = run_stress(state['books']["mbs"], sr, vp, *state['mbs_hists'],
                                        shocks_bp=shocks, seed=state['settings'].seed, oas=fixed.get("mbs"))
            out["mbs"] = {"agg": agg, "profile": prof}
        _stress_book("mbs", _run_mbs, 20.0, 60.0)
    if "deposits" in books:
        def _run_dep(_n):
            pos, agg, prof = run_deposit_stress(
                state['books']["deposits"], sr, vp, state['dep_hist'],
                shocks_bp=shocks, seed=state['settings'].seed, oas=fixed.get("deposits"))
            out["deposits"] = {"agg": agg, "profile": prof}
        _stress_book("deposits", _run_dep, 70.0, 96.0)
    return out


def run_nii(sr, vp, *, node_parent: str | None = None, node_prefix: str = "nii"):
    state = current_state()
    """Forward balance-sheet NII. When `node_parent` is given, attaches its
    paths/cashflow/reduce nodes under that branch (so nested callers -- kpis,
    scenario_grid, optimize -- can place it in their own tree without id
    clashes) and stays quiet on stage/pct so the caller owns the bar."""
    from portfolio_risk.accounting import run_balance_sheet_nii
    npaths = state['settings'].n_paths
    nested = node_parent is not None
    pre = node_prefix
    if not nested:                       # standalone run: own the root stages
        node("build", parent=None, kind="build", status="running",
             label="Position build")
        node(pre, parent="build", kind="branch", label="Base market",
             status="running")
        node("build", status="done")
        parent = pre
    else:
        parent = node_parent

    node(f"{pre}:paths", parent=parent, kind="paths", label="LMM paths",
         status="running", detail=f"{npaths} paths \u00d7 {state['settings'].horizon_months}m")
    if not nested:
        report(stage="simulating LMM paths", pct=8.0,
               log=f"LMM: {npaths} paths \u00d7 {state['settings'].horizon_months}m")
    report(scenario_paths=npaths, paths_generated=npaths)
    node(f"{pre}:paths", status="done", paths=npaths)

    bs = balance_sheet(state)
    node(f"{pre}:cf", parent=parent, kind="cashflow",
         label="forward balance & NII", status="running")
    if not nested:
        report(stage="forward balance & NII", pct=35.0)
    out = run_balance_sheet_nii(bs, sr, vp, state['dep_hist'],
                                horizon=state['settings'].horizon_months,
                                seed=state['settings'].seed, asof=state['asof'])
    n = sum(int(len(state['books'][k])) for k in bs if isinstance(state['books'].get(k), pl.DataFrame))
    cf = n * npaths
    node(f"{pre}:cf", status="done", calcs=cf)
    node(f"{pre}:reduce", parent=parent, kind="reduce",
         label="reduce to monthly NII", status="running")
    if not nested:
        report(stage="reducing to monthly NII", pct=55.0)
    report(records=n, path_evaluations=cf, reductions=n, cashflow_calcs=cf,
           log="reduced path NII to monthly means")
    node(f"{pre}:reduce", status="done")
    if not nested:
        node(pre, status="done")
    return out


def run_kpis_scenario(sr, vp, scenario=False, spread_shift=0.0):
    from portfolio_risk.analytics.calibration import spread_oas
    state = current_state()
    fixed = spread_oas(base_calibration(state), spread_shift)
    return run_kpis(sr, vp, fixed)


def run_kpis(sr, vp, oas_by_book=None):
    state = current_state()
    from portfolio_risk.kpis import compute_kpis
    bs = balance_sheet(state)
    nii = run_nii(sr, vp)
    oas_by_book = base_calibration(state) if oas_by_book is None else oas_by_book
    node("kpi:eve", parent="build", kind="solve", label="EVE & parallel dv01",
         status="running")
    report(stage="EVE & parallel dv01", pct=65.0, log="full-reval EVE + dv01")
    out = compute_kpis(bs, sr, vp, state['dep_hist'],
                       nii_monthly=nii["monthly"], seed=state['settings'].seed, oas_by_book=oas_by_book)
    node("kpi:eve", status="done")
    node("kpi:ratios", parent="build", kind="solve", label="LCR / NSFR / CET1",
         status="running")
    report(stage="LCR / NSFR / CET1", pct=90.0, log="liquidity + capital ratios")
    node("kpi:ratios", status="done")
    out["nii_summary"] = nii["summary"]
    return out


def build_unitlib_job(sr, vp):
    state = current_state()
    from portfolio_risk.unitlib import build_unit_library
    global UNITLIB, BASE_KPIS
    npaths = state['settings'].n_paths
    node("build", parent=None, kind="build", status="running",
         label="Position build")
    node("unit:lib", parent="build", kind="cashflow", label="unit library",
         detail="template unit tensor", status="running")
    report(stage="building unit library", pct=4.0, scenario_paths=npaths,
           paths_generated=npaths, log="template unit tensor")
    lib = build_unit_library(sr, vp, state['mbs_hists'], state['dep_hist'],
                             seed=state['settings'].seed, horizon=state['settings'].horizon_months,
                             asof=state['asof'])
    n_units = len(lib["units"])
    node("unit:lib", status="done", calcs=n_units * npaths, units=n_units)
    report(unit_columns=n_units, cashflow_calcs=n_units * npaths)
    base = run_kpis(sr, vp)
    with _LOCK:
        from . import persistence
        persistence.assert_current(state['revision'])
        if STATE_META["revision"] != state['revision']:
            raise RuntimeError("inputs changed while building; rebuild the strategy library")
        CACHE.update(revision=state['revision'], library=lib, base=base, compute_backend=state['settings'].compute_backend)
        UNITLIB, BASE_KPIS = lib, base
    return {"units": n_units, "templates": list(lib["templates"]), "grid_m": lib["grid_m"],
            "horizon_months": lib["horizon"], "revision": state['revision']}


def eval_strategy_sync(allocations: list[dict]):
    from portfolio_risk.unitlib import evaluate_strategy
    from portfolio_risk.core.runtime import RunConfig, run_context
    with _LOCK:
        if not CACHE or CACHE["revision"] != STATE_META["revision"]:
            raise RuntimeError("REBUILD_REQUIRED: build the unit library for the current inputs")
        lib, base, revision = CACHE["library"], CACHE["base"], CACHE["revision"]
        backend = CACHE.get('compute_backend', SETTINGS.compute_backend)
        if backend != 'rust':
            raise RuntimeError('PYTHON_BACKEND_DEPRECATED: rebuild the unit library with Rust')
    with run_context(RunConfig(compute_backend=backend)):
        out = evaluate_strategy(lib, allocations, base_kpis=base)
    out["revision"] = revision
    return out


def run_strategy_job(sr, vp):
    state = current_state()
    from portfolio_risk.strategies import run_strategies
    nii = run_nii(sr, vp)
    return run_strategies(list(state['programs'].values()), sr, vp,
                          runoff_by_book=nii["runoff_vectors"],
                          horizon=state['settings'].horizon_months,
                          seed=state['settings'].seed)


def run_optimize_job(sr, vp, opt: dict):
    state = current_state()
    from portfolio_risk.optimizer import optimize_balance_sheet
    from portfolio_risk.unitlib import build_unit_library
    from portfolio_risk.kpis import compute_kpis
    npaths = state['settings'].n_paths
    node("build", parent=None, kind="build", status="running",
         label="Position build")
    node("opt:base", parent="build", kind="branch", label="Base NII objective",
         status="running")
    report(stage="base NII path", pct=4.0,
           scenario_paths=npaths,
           log="base market NII path for the objective")
    nii = run_nii(sr, vp, node_parent="opt:base", node_prefix="opt:base")
    node("opt:base", status="done")
    node("build", status="done")
    scen_libs = []
    markets = [("base", sr, vp, 0.0)]
    for name in opt.get("scenarios", []):
        sc = state['scenarios'].get(name)
        if sc:
            s2, v2, spread = apply_scenario(sc, 0)
            markets.append((name, s2, v2, spread))
    bs = balance_sheet(state)
    fixed = base_calibration(state)
    total = max(len(markets), 1)
    span = 84.0 / total                               # 8% .. 92% across markets
    for i, (name, s2, v2, spread) in enumerate(markets):
        base = 8.0 + span * i
        node(f"mkt:{name}", parent="build", kind="branch",
             label=f"market: {name}", detail=f"unit tensor ({i + 1}/{total})",
             status="running")
        node(f"mkt:{name}:paths", parent=f"mkt:{name}", kind="paths",
             label="LMM paths", status="running")
        node(f"mkt:{name}:paths", status="done", paths=npaths)
        node(f"mkt:{name}:cf", parent=f"mkt:{name}", kind="cashflow",
             label="unit library", status="running")
        report(stage=f"unit library: {name}", pct=base,
               scenario_paths=npaths, paths_generated=npaths,
               log=f"building unit tensor under {name} market ({i + 1}/{total})")
        lib = build_unit_library(s2, v2, state['mbs_hists'], state['dep_hist'],
                                 seed=state['settings'].seed, horizon=state['settings'].horizon_months, asof=state['asof'])
        n_units = len(lib.get("units", [])) if isinstance(lib, dict) else 0
        node(f"mkt:{name}:cf", status="done", calcs=n_units * npaths)
        node(f"mkt:{name}:kpi", parent=f"mkt:{name}", kind="solve",
             label="KPIs", status="running")
        report(stage=f"KPIs: {name}", pct=base + span * 0.6,
               revaluations=n_units, reductions=n_units,
               unit_columns=n_units, cashflow_calcs=n_units * npaths,
               oas_calcs=n_units,
               log=f"{name}: {n_units} unit columns priced")
        scenario_nii = nii if name == "base" else run_nii(s2, v2, node_parent=f"mkt:{name}", node_prefix=f"opt:{name}:nii")
        from portfolio_risk.analytics.calibration import spread_oas
        base = compute_kpis(bs, s2, v2, state['dep_hist'], nii_monthly=scenario_nii["monthly"],
                            seed=state['settings'].seed, oas_by_book=spread_oas(fixed, spread))
        scen_libs.append((lib, base))
        node(f"mkt:{name}:kpi", status="done", calcs=n_units)
        node(f"mkt:{name}", status="done")
    node("opt:lp", parent="build", kind="solve", label="LP maximin solve",
         detail=f"{total} markets", status="running")
    report(stage="solving LP", pct=94.0,
           log=f"maximin LP across {total} markets")
    out = optimize_balance_sheet(
        scen_libs, lcr_min=opt.get("lcr_min", 1.10),
        nsfr_min=opt.get("nsfr_min", 1.05),
        cet1_min=opt.get("cet1_min", 0.10),
        eve_limit=opt.get("eve_limit", 0.15),
        commercial=opt.get("commercial"),
        max_total_assets=opt.get("max_total_assets"), cash_budget=opt.get("cash_budget", 0.0),
        capital_limits=opt.get("capital_limits", []))
    nb = len(out.get("binding_constraints", []) or []) if isinstance(out, dict) else 0
    node("opt:lp", status="done", detail=f"{nb} binding constraints")
    report(stage="LP solved", pct=99.0, binding_constraints=nb,
           log=f"{'feasible' if isinstance(out, dict) and out.get('feasible') else 'infeasible'} \u00b7 {nb} binding")
    return out


def run_scenario_grid(sc: MarketScenario):
    state = current_state()
    """9Q scenario: revalue the books at each quarter's market and report
    value/NII drift along the named path (base OAS held fixed -- the
    engine's global invariant)."""
    rows = []
    node("build", parent=None, kind="build", status="running",
         label="Position build", detail=f"{sc.name}: 9 quarters")
    for q in range(9):
        node(f"q{q}", parent="build", kind="branch", label=f"Q{q + 1}",
             detail="revalue at scenario market")
    node("build", status="done")
    for q in range(9):
        node(f"q{q}", status="running")
        report(stage=f"quarter {q + 1}/9", pct=4 + 92 * q / 9,
               scenario_paths=state['settings'].n_paths,
               log=f"Q{q + 1}: revalue at scenario market")
        sr, vp, spr = apply_scenario(sc, q)
        nii = run_nii(sr, vp, node_parent=f"q{q}", node_prefix=f"q{q}")
        node(f"q{q}", status="done")
        s = nii["summary"]
        rows.append({"quarter": q + 1,
                     "ust10y_pct": float(sr[6] * 100),
                     "twos_tens_bp": float((sr[6] - sr[1]) * 1e4),
                     "nii_annualized": float(
                         s.filter(pl.col("metric") == "nii_annualized_$")
                          ["value"][0]),
                     "nim_pct": float(
                         s.filter(pl.col("metric") == "nim_model_%")
                          ["value"][0])})
    return {"scenario": sc.name, "path": rows, "method": "independent instantaneous forecasts on the unchanged book; spread does not alter contractual NII"}


def prepare_forecast(req):
    from fastapi import HTTPException
    from .market_data import get_snapshot
    from .forecast_data import DATASETS, normalized_rows
    from portfolio_risk.analytics.forecast import compile_forecast
    state = snapshot()
    if state["revision"] != req.expected_revision:
        raise HTTPException(409, "Inputs changed. Reload before previewing or running a forecast.")
    try:
        source = get_snapshot(req.snapshot_id)
    except KeyError:
        raise HTTPException(404, "unknown research snapshot")
    if source["dataset"] not in DATASETS:
        raise HTTPException(422, "snapshot is not a supported forecast source")
    try:
        plan = compile_forecast(normalized_rows(source), req.scenario, str(req.start_period), req.horizon_months)
    except ValueError as exc:
        raise HTTPException(422, str(exc))
    preview = {"snapshot_id": source["id"], "dataset": source["dataset"],
               "source_as_of": source["as_of"], "book_as_of": str(state["asof"]),
               "revision": state["revision"], "scenario": req.scenario, "start_period": str(req.start_period),
               "alignment": req.alignment, "horizon_months": req.horizon_months,
               "warnings": source["warnings"] + plan["warnings"], "coverage": plan["coverage"],
               "unused_variables": plan["unused_variables"], "sources": source["sources"],
               "drivers": [{"month": m + 1, **{k: float(v[m]) for k, v in plan["targets"].items()}}
                           for m in range(req.horizon_months)]}
    return state, plan, preview


def run_forecast(plan, provenance):
    from portfolio_risk.analytics.forecast import run_forecast_nii
    state = current_state()
    report(stage="base and conditional monthly cashflows", pct=10)
    result = run_forecast_nii(balance_sheet(state), state["market"]["swap_rates"],
        state["market"]["vol_pts"], state["dep_hist"], plan, seed=state["settings"].seed)
    result["provenance"] = provenance
    return result


def run_balance_stress(specification):
    from portfolio_risk.analytics.balance_stress import run_balance_stress as simulate
    result = simulate(specification, progress=lambda stage, pct: report(stage=stage, pct=pct))
    result["revision"] = current_state()["revision"]
    return result


def run_treasury(specification):
    from portfolio_risk.analytics.treasury import evaluate
    report('Calculating capital ratios and FTP reconciliation', 0.1)
    result = evaluate(specification)
    result['revision'] = current_state()['revision']
    report('Capital and FTP report complete', 1.)
    return result


def run_saved_balance_stress(request):
    from portfolio_risk.analytics.balance_workflow import run_saved_book_stress
    from . import persistence as db
    state = current_state()
    expected = None
    if db.ACTIVE_JOB.get() is not None and state['settings'].compute_backend == 'rust':
        from portfolio_risk.analytics.owned_workflow import binary_path
        from portfolio_risk.analytics.balance_stream import _hash
        expected = db.identity()['workflow_native_sha256']
        binary = binary_path()
        if expected is None or not binary.is_file() or _hash(binary) != expected:
            raise RuntimeError('MODEL_VERSION_CHANGED: Rust workflow binary differs from queued identity')
    result = run_saved_book_stress(balance_sheet(state), state['market']['swap_rates'],
        state['market']['vol_pts'], state['dep_hist'], request, seed=state['settings'].seed,
        asof=state['asof'], progress=lambda stage, pct: report(stage=stage, pct=pct))
    if expected is not None and result['execution']['journal_identity'] != expected:
        raise RuntimeError('MODEL_VERSION_CHANGED: executed workflow binary differs from queued identity')
    result['revision'] = state['revision']
    return result


def run_streamed_balance_stress(request):
    """Worker-only engine adapter; persist partitions without materializing tables."""
    if request['backend'] != 'rust':
        raise ValueError('PYTHON_BACKEND_DEPRECATED: submit a new Rust simulation')
    import tempfile
    from portfolio_risk.analytics.balance_stream import run_streamed_balance_stress as simulate, binary_path, _hash
    from . import persistence as db, worker
    job = db.ACTIVE_JOB.get()
    if db.REPO is None or job is None:
        raise RuntimeError('Partitioned simulation requires a durable worker')
    expected = request['binary_sha256']
    if request['backend'] == 'rust' and (not binary_path().is_file() or _hash(binary_path()) != expected):
        raise RuntimeError('MODEL_VERSION_CHANGED: Rust state binary differs from queued identity')
    checked = [0., False]
    def cancelled():
        # Native watchdog and writer both call this; duplicate read-only probes
        # are harmless. SQL complete() remains the authoritative final fence.
        if worker.STOP.is_set():
            return True
        now = time.monotonic()
        if now - checked[0] >= .2:
            row = db.REPO.job(job['id'])
            checked[1] = (row['status'] != 'running' or row['token'] != job['token']
                          or row['owner'] != job['owner'] or not db.REPO.owns(job['owner']))
            checked[0] = now
        return checked[1]
    from .maintenance import scratch_root
    scratch = scratch_root(db.CONFIG, db.CODEC.objects)
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='balance-', dir=scratch) as directory:
        manifest = simulate(request['specification'], directory, backend=request['backend'],
                            large_book=request['large_book'], cancelled=cancelled,
                            progress=lambda stage: report(stage=stage))
        result = db.CODEC.import_balance_partitions(manifest, cancelled)
        if request['backend'] == 'rust' and result['execution']['binary_sha256'] != expected:
            raise RuntimeError('MODEL_VERSION_CHANGED: executed binary differs from queued identity')
    # The immutable queued request already owns the complete input. Keeping it
    # inline here makes every table page re-read a portfolio-sized manifest.
    result['execution'].pop('specification')
    result['execution']['request_manifest'] = job['request']
    result['revision'] = current_state()['revision']
    return result
