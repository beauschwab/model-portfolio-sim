"""Proposed per-job indexed actor transport; no financial calculation here."""
from __future__ import annotations
import hashlib
import json
import math
import threading
import time

SCHEMA = 'market-indexed-books-1'
MAX_RAW = 2 * 1024 * 1024
MAX_OUTPUT = 32 * 1024 * 1024
CACHE_BYTES = 8 * 1024 * 1024
CACHE_ENTRIES = 1024


class JobBudget:
    """Caller IO budget. Construct once, before capture; never renew per call."""
    def __init__(self, timeout, cancelled, *, started=None, deadline_unix=None, monotonic=time.monotonic, wall=time.time):
        if type(timeout) not in (int, float) or not math.isfinite(timeout) or not 0 < timeout <= 1800:
            raise ValueError('finite whole-job timeout in (0,1800] required')
        if not callable(cancelled):
            raise TypeError('bounded lease IO callback required')
        self._mono, self._wall = monotonic, wall
        now = monotonic(), wall()
        start = now if started is None else started
        if type(start) is not tuple or len(start)!=2 or any(type(x) not in (int,float) or not math.isfinite(x) for x in start) or start[0]>now[0] or start[1]>now[1]:
            raise ValueError('worker entry clocks cannot move the original deadline forward')
        deadline = start[1] + timeout if deadline_unix is None else deadline_unix
        if type(deadline) not in (int,float) or not math.isfinite(deadline) or deadline>start[1]+timeout:
            raise ValueError('immutable queued deadline cannot extend the accepted timeout')
        self.deadline_monotonic = min(start[0]+timeout,now[0]+deadline-now[1])
        self.publication_deadline_unix = deadline
        self._cancelled = cancelled
        self._read_lock = threading.Lock()
        self._last_checked = -math.inf

    def _deadline(self):
        remaining = min(self.deadline_monotonic - self._mono(),
                        self.publication_deadline_unix - self._wall())
        if remaining <= 0:
            raise TimeoutError('indexed book original job deadline exceeded')
        return remaining

    def _sample(self):
        # A mutex serializes concurrent bounded lease probes from the native
        # monitor and publication thread. No financial/global-cache lock here.
        if not self._read_lock.acquire(timeout=min(.25, self._deadline())):
            raise RuntimeError('INDEXED_JOB_LEASE_IO_NOT_BOUNDED')
        try:
            self._deadline()
            value = self._cancelled()
            if type(value) is not bool:
                raise ValueError('indexed job lease callback must return bool')
            if value:
                raise InterruptedError('indexed book cancelled or lease lost')
            self._deadline()
            self._last_checked=self._mono()
        finally:
            self._read_lock.release()
        return False

    def checkpoint(self):
        # Frequent pure/reference loops check the absolute deadline every time;
        # lease IO is refreshed at most 50 ms apart. Handoffs always use check(),
        # which performs a fresh read even immediately after a prior sample.
        self._deadline()
        if self._mono()-self._last_checked>=.05:return self.check()
        return False

    def check(self):
        self._deadline()
        done, errors = threading.Event(), []
        def read():
            try:
                self._sample()
            except BaseException as exc:
                errors.append(exc)
            finally:
                done.set()
        threading.Thread(target=read, name='indexed-job-lease', daemon=True).start()
        if not done.wait(min(.25, self._deadline())):
            self._deadline()
            raise RuntimeError('INDEXED_JOB_LEASE_IO_NOT_BOUNDED')
        if errors:
            raise errors[0]
        self._deadline()
        return False

    def runtime(self):
        self.check()
        milliseconds = math.floor(self._deadline() * 1000)
        if milliseconds < 1:
            raise TimeoutError('indexed book less than one millisecond remains')
        return dict(schema='graph-call-control-1', timeout_ms=milliseconds,
                    cancelled=self._sample)


def fully_bound(raw):
    """Ownership-only gate; Rust still owns full domain/financial admission."""
    if type(raw) is not dict or raw.get('schema') != SCHEMA:
        raise ValueError('explicit market-indexed-books-1 request required')
    if set(raw) != {'schema','identity','balance_sheet_extras','legacy','bindings'}:
        raise ValueError('unknown or missing indexed book request field')
    bindings = raw['bindings']
    if type(bindings) is not list or not 1 <= len(bindings) <= 16:
        raise ValueError('complete bounded indexed loan bindings required')
    keys = [(b.get('book'), b.get('position_id')) for b in bindings if type(b) is dict]
    if len(keys) != len(bindings) or any(type(a) is not str or type(b) is not str for a,b in keys) or len(set(keys)) != len(keys):
        raise ValueError('unique typed indexed book binding keys required')
    for side in ['base','current']:
        legacy = raw['legacy'][side]
        if type(legacy['terms']) is not list or len(legacy['terms']) > 2:
            raise ValueError('bounded native term book ownership required')
        ordered = []
        seen = set()
        for book in legacy['terms']:
            if type(book) is not dict or book['key'] not in ['loans','debt'] or book['key'] in seen:
                raise ValueError('unique loans/debt ownership required')
            seen.add(book['key'])
            if type(book['ids']) is not list or len(book['ids']) != len(book['deck']['contracts']):
                raise ValueError('complete ordered native row ownership required')
            if book['key'] == 'debt' and book['ids']:
                raise ValueError('fully-bound job does not support nonempty debt')
            ordered.extend((book['key'], ident) for ident in book['ids'])
        if ordered != keys:
            raise ValueError('every native monetary row must be explicitly bound in order')


def _encode(value, budget, maximum):
    budget.check()
    encoder = json.JSONEncoder(allow_nan=False, ensure_ascii=False, separators=(',',':'))
    chunks, total = [], 0
    for chunk in encoder.iterencode(value):
        encoded = chunk.encode()
        total += len(encoded)
        if total > maximum:
            raise ValueError('indexed job JSON capacity exceeded')
        chunks.append(encoded)
        # CPU formatting admission is bounded; periodically refresh lease.
        if len(chunks) % 1024 == 0:
            budget.checkpoint()
    budget.check()
    return b''.join(chunks)


def _library_hash(path, budget):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        while block := stream.read(1024 * 1024):
            budget.check()
            digest.update(block)
    budget.check()
    return digest.hexdigest()


def run(raw, *, expected_library_sha256, budget, threads=None):
    """One job-local actor, no shared Python cache or uncontrolled fallback."""
    # Import readiness is inside the original job budget. Synchronous OS file
    # reads/imports can be checked before/after, not forcibly interrupted; the
    # worker/process watchdog remains required for blocked OS operations.
    budget.check()
    from .graph_native import NativeMarketIndexedBooks, library_path
    from numba import get_num_threads
    budget.check()
    actual_threads=get_num_threads()
    if threads is not None and (type(threads) is not int or threads!=actual_threads):
        raise ValueError('queued threads must match the worker computation context')
    path = library_path()
    if _library_hash(path, budget) != expected_library_sha256:
        raise RuntimeError('MODEL_VERSION_CHANGED: queued product DLL differs')
    payload = _encode(raw, budget, MAX_RAW)
    immutable = json.loads(payload)
    fully_bound(immutable)
    budget.check()
    actor = None
    try:
        actor = NativeMarketIndexedBooks(immutable, CACHE_BYTES, CACHE_ENTRIES,
                                        runtime=budget.runtime())
        budget.check()
        result = actor.initial
        output = _encode(result, budget, MAX_OUTPUT)
        # Actor retains its captured library; re-resolved current identity must
        # match the queued file before returning a source, even after cleanup.
        if library_path() != path or _library_hash(path, budget) != expected_library_sha256:
            raise RuntimeError('MODEL_VERSION_CHANGED: product DLL changed during job')
        command=dict(schema='graph-1',threads=actual_threads,request=dict(op='create_market_indexed_books',max_bytes=CACHE_BYTES,max_entries=CACHE_ENTRIES,request=immutable))
        executed=json.dumps(command,allow_nan=False,separators=(',',':')).encode()
        return json.loads(output), dict(schema=SCHEMA, backend='rust',threads=actual_threads,
            artifact_kind='product_library', binary_sha256=expected_library_sha256,
            input_sha256=hashlib.sha256(payload).hexdigest(),
            executed_input_sha256=hashlib.sha256(executed).hexdigest(),
            publication_deadline_unix=budget.publication_deadline_unix,
            runtime_control='graph-call-control-1', job_local_actor=True,
            fully_bound=True, saved_workspace_adoption=False,
            source_verification_required=True)
    finally:
        if actor is not None:
            actor.close()
        # Late cancellation/cleanup failure cannot return a publishable result.
        budget.check()
