"""Bounded, durable tape-to-position endpoints."""
from fastapi import APIRouter, HTTPException, Query
from pydantic import BaseModel, ConfigDict, Field, model_validator
import io
import polars as pl

from . import cohort_store, persistence as db, store
from .schemas import JobStatus

router = APIRouter(prefix='/cohorts')

class Strict(BaseModel):
    model_config = ConfigDict(extra='forbid')

class TapeImport(Strict):
    name: str = Field(min_length=1, max_length=200)
    format: str = Field(pattern='^(csv|parquet)$')
    uri: str | None = Field(None, max_length=2000)
    content_base64: str | None = Field(None, max_length=44739244)

    @model_validator(mode='after')
    def source(self):
        if bool(self.uri) == bool(self.content_base64): raise ValueError('provide exactly one URI or file payload')
        return self

class Adopt(Strict):
    job_id: str
    expected_revision: int

class Build(Strict):
    tape_id: str
    config: dict
    baseline_job: str | None = None
    previous_job: str | None = None
    expected_revision: int

class Publish(Strict):
    products: list[str] = Field(min_length=1,max_length=5)
    expected_revision: int
    mode: str = Field('replace_books',pattern='^(replace_books|replace_tape)$')

class Analyze(Strict):
    products: list[str] = Field(min_length=1,max_length=5)
    loan_ids: list[str] | None = Field(None,min_length=1,max_length=256)

class Attribution(Strict):
    analytics_job: str

class Audit(Strict):
    products: list[str] = Field(min_length=1, max_length=5)
    shocks: list[float] = Field(default_factory=lambda:[0., -200., 200.], min_length=1, max_length=9)
    tolerances: dict = Field(default_factory=dict)
    timeout: float = Field(1800., gt=0, le=3600)


def durable():
    if db.REPO is None: raise HTTPException(409, 'Tape workflows require durable SQLite or PostgreSQL storage')


def submit(kind, fn, *args, state=None):
    durable()
    return JobStatus(**store.job_status(store.submit(kind, fn, *args, state=state)))


@router.get('/presets')
def presets():
    from portfolio_risk.analytics.cohorts import presets as defaults
    state = store.snapshot()
    return dict(config=defaults(), revision=state['revision'], durable=db.REPO is not None,
        tapes={k:{f:v[f] for f in ('sha256','source','rows','columns')} for k,v in state['tapes'].items()},
        publications=state['cohort_publications'])


@router.get('/example')
def example():
    from portfolio_risk.analytics.cohorts import example as tape
    return {'csv':tape()}


@router.post('/imports')
def import_tape(req: TapeImport):
    return submit('tape_import', cohort_store.import_tape, req.model_dump())


@router.put('/tapes/{tape_id}')
def adopt(tape_id: str, req: Adopt):
    durable()
    if not tape_id or len(tape_id)>128: raise HTTPException(422, 'tape name must contain 1..128 characters')
    try: return cohort_store.adopt(tape_id, req.job_id, req.expected_revision)
    except db.Conflict: raise
    except KeyError: raise HTTPException(404, 'unknown import job')
    except (ValueError, RuntimeError) as exc: raise HTTPException(422, str(exc))


@router.post('/builds')
def build(req: Build):
    durable()
    state = store.snapshot()
    if req.expected_revision != state['revision']: raise db.Conflict('inputs changed; reload before building cohorts')
    if req.tape_id not in state['tapes']: raise HTTPException(404, 'unknown tape')
    return submit('cohort_build', cohort_store.build, req.tape_id, req.config, req.baseline_job, req.previous_job, state=state)


@router.get('/builds/{jid}/summary')
def summary(jid: str):
    durable()
    try:
        return cohort_store.metadata(jid)
    except KeyError: raise HTTPException(404, 'unknown build')
    except (ValueError, RuntimeError) as exc: raise HTTPException(422, str(exc))


@router.get('/builds/{jid}/lineage')
def lineage(jid: str, loan_id: str | None=None, cohort_id: str | None=None, offset: int=Query(0,ge=0), limit: int=Query(100,ge=1,le=1000)):
    durable()
    try:
        if db.REPO.job(jid)['kind'] != 'cohort_build': raise ValueError('expected cohort build')
        ref = db.CODEC.tables(db.result_ref(jid))['/lineage']
        frame = pl.read_parquet(io.BytesIO(db.CODEC.objects.get(ref)))
        if loan_id is not None: frame=frame.filter(pl.col('loan_id')==loan_id)
        if cohort_id is not None: frame=frame.filter(pl.col('cohort_id')==cohort_id)
        return dict(total=len(frame), rows=frame.slice(offset,limit).to_dicts())
    except KeyError: raise HTTPException(404, 'unknown build')
    except (ValueError, RuntimeError) as exc: raise HTTPException(422, str(exc))


@router.put('/builds/{jid}/publish')
def publish(jid: str, req: Publish):
    durable()
    try: return cohort_store.publish(jid, req.products, req.expected_revision, req.mode)
    except db.Conflict: raise
    except KeyError: raise HTTPException(404, 'unknown build')
    except (ValueError, RuntimeError) as exc: raise HTTPException(422, str(exc))


@router.post('/builds/{jid}/analytics')
def analytics(jid: str, req: Analyze):
    return submit('cohort_analytics', cohort_store.analytics, jid, req.products, req.loan_ids)


@router.post('/builds/{jid}/attribution')
def attribution(jid: str, req: Attribution):
    return submit('cohort_attribution', cohort_store.attribution, jid, req.analytics_job)


@router.post('/builds/{jid}/audit')
def audit(jid: str, req: Audit):
    return submit('cohort_audit', cohort_store.audit, jid, req.products, req.model_dump(exclude={'products'}))


@router.get('/audits/{jid}/summary')
def audit_summary(jid: str):
    durable()
    import json
    try:
        if db.REPO.job(jid)['kind'] != 'cohort_audit': raise ValueError('expected cohort audit')
        manifest = json.loads(db.CODEC.objects.get(db.result_ref(jid)))
        return {key:db.CODEC.decode(value) for key,value in manifest['value']['items']
                if key in {'build_id','revision','source_sha256','summary'}}
    except KeyError: raise HTTPException(404, 'unknown audit')
    except (ValueError, RuntimeError) as exc: raise HTTPException(422, str(exc))
