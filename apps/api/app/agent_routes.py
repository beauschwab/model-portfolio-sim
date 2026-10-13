"""Bounded agent conveniences over the existing revision and publication contracts."""
from typing import Any, Literal

from fastapi import APIRouter, HTTPException, Query
from pydantic import BaseModel, ConfigDict, Field

from . import persistence as db, store
from .books import normalize_book

router = APIRouter()


def durable():
    if db.REPO is None:
        raise HTTPException(409, 'Durable storage is required for discovery')


@router.get('/jobs')
def list_jobs(status: Literal['queued', 'running', 'done', 'error'] | None = None,
              kind: str | None = None, idempotency_key: str | None = Query(None, min_length=1, max_length=128),
              offset: int = Query(0, ge=0),
              limit: int = Query(100, ge=1, le=1000)):
    """Discover prior jobs after reconnecting; returns metadata without result hydration."""
    durable()
    return dict(rows=db.REPO.list_jobs(status=status, kind=kind, idempotency_key=idempotency_key, offset=offset, limit=limit),
                offset=offset, limit=limit)


@router.get('/decision/sessions')
def list_sessions(offset: int = Query(0, ge=0), limit: int = Query(100, ge=1, le=1000)):
    """Discover persisted sessions, including stale and closed sessions."""
    durable()
    revision = db.REPO.head()
    rows = db.REPO.list_sessions(offset=offset, limit=limit)
    return dict(rows=[r | {'rebuild_required': r['closed'] or r['revision'] != revision} for r in rows],
                revision=revision, offset=offset, limit=limit)


@router.get('/decision/sessions/{sid}')
def get_session(sid: str):
    """Inspect durable identity/version without recovering or repricing the session."""
    durable()
    row = db.REPO.session(sid)
    if row is None:
        raise HTTPException(404, 'unknown decision session')
    revision = db.REPO.head()
    return {k: row[k] for k in ('id', 'revision', 'version', 'closed')} | {
        'current_revision': revision, 'rebuild_required': row['closed'] or row['revision'] != revision}


@router.get('/books/{name}/rows')
def book_rows(name: str, offset: int = Query(0, ge=0), limit: int = Query(100, ge=1, le=1000),
              columns: list[str] | None = Query(None), position_id: str | None = None):
    """Read a bounded JSON book page and schema at one input revision."""
    with store._LOCK:
        if name not in store.BOOKS:
            raise HTTPException(404, 'unknown book')
        frame = store.BOOKS[name]
        schema = {k: str(v) for k, v in frame.schema.items()}
        if position_id is not None:
            import polars as pl
            frame = frame.filter(pl.col('cusip' if name == 'mbs' else 'id') == position_id)
        if columns is not None:
            if not columns or len(set(columns)) != len(columns) or set(columns) - set(schema):
                raise HTTPException(422, 'select valid unique columns')
            frame = frame.select(columns)
        return dict(revision=store.STATE_META['revision'], schema=schema, total=len(frame),
                    offset=offset, limit=limit, rows=frame.slice(offset, limit).to_dicts())


class BookEdit(BaseModel):
    model_config = ConfigDict(extra='forbid')
    expected_revision: int = Field(ge=0, strict=True)
    updates: dict[str, dict[str, Any]] = Field(min_length=1, max_length=1000)


def prepare_edit(name, request):
    if name not in store.BOOKS:
        raise HTTPException(404, 'unknown book')
    if request.expected_revision != store.STATE_META['revision']:
        raise db.Conflict('saved inputs changed; reload before editing')
    frame = store.BOOKS[name]
    key = 'cusip' if name == 'mbs' else 'id'
    remaining = set(request.updates)
    rows, changes = [], []
    for original in frame.to_dicts():
        identity = str(original[key])
        patch = request.updates.get(identity)
        if patch is not None:
            if not patch or key in patch or set(patch) - set(frame.columns):
                raise HTTPException(422, 'updates must contain known fields and cannot change position identity')
            remaining.discard(identity)
            changes.append(dict(position_id=identity, before={k: original[k] for k in patch}, after=patch))
            original = original | patch
        rows.append(original)
    if remaining:
        raise HTTPException(422, 'unknown position IDs: ' + ', '.join(sorted(remaining)[:10]))
    try:
        prepared = normalize_book(name, rows, frame, store.ASOF)
    except (ValueError, TypeError, KeyError) as exc:
        raise HTTPException(422, str(exc)) from exc
    return prepared, changes


@router.post('/books/{name}/preview')
def preview_book_edit(name: str, request: BookEdit):
    """Validate targeted edits and show the diff without committing or pricing."""
    with store._LOCK:
        _, changes = prepare_edit(name, request)
        return dict(revision=store.STATE_META['revision'], changes=changes, committed=False)


@router.patch('/books/{name}/rows')
def patch_book_rows(name: str, request: BookEdit):
    """Commit validated targeted edits atomically; invalidate prepared coefficients."""
    with store._LOCK:
        prepared, changes = prepare_edit(name, request)
        store.BOOKS[name] = prepared
        store.COHORT_PUBLICATIONS.pop(name, None)
        store.changed()
        return dict(revision=store.STATE_META['revision'], changes=changes, committed=True,
                    rebuild_required=True)
