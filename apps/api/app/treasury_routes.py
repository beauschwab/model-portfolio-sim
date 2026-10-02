"""Durable capital/FTP reports. Policy and exposures are immutable job inputs."""
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, ConfigDict, Field
from . import persistence as db, store
from .schemas import JobStatus
from typing import Literal

router = APIRouter(prefix='/treasury')


class TreasuryRequest(BaseModel):
    model_config = ConfigDict(extra='forbid')
    expected_revision: int = Field(ge=0)
    specification: dict


class BridgeRequest(TreasuryRequest):
    source_job: str = Field(min_length=1,max_length=100)


@router.get('/example')
def example():
    from portfolio_risk.analytics.treasury import example as fixture
    return dict(specification=fixture(),revision=store.snapshot()['revision'],durable=db.REPO is not None)


@router.post('/runs')
def submit(request: TreasuryRequest):
    import json
    if db.REPO is None: raise HTTPException(409,'Capital/FTP reports require durable SQLite or PostgreSQL storage')
    try:
        size=len(json.dumps(request.specification,allow_nan=False).encode())
    except (ValueError,TypeError) as exc:
        raise HTTPException(422,'Capital/FTP inputs must contain finite JSON values') from exc
    if size>32*1024*1024:
        raise HTTPException(422,'Capital/FTP request exceeds 32 MiB')
    state=store.snapshot()
    if request.expected_revision!=state['revision']: raise db.Conflict('inputs changed; reload before running')
    return JobStatus(**store.job_status(store.submit('treasury',store.run_treasury,request.specification,state=state)))


@router.post('/bridges')
def bridge(request: BridgeRequest):
    from . import treasury_store
    if db.REPO is None: raise HTTPException(409,'Treasury bridges require durable storage')
    state=store.snapshot()
    if request.expected_revision!=state['revision']: raise db.Conflict('inputs changed; reload before running')
    if request.specification.get('kind') not in {'ledger','cashflows'}: raise HTTPException(422,'bridge kind must be ledger or cashflows')
    import json
    try:
        if len(json.dumps(request.specification,allow_nan=False).encode())>32*1024*1024: raise ValueError('bridge request exceeds 32 MiB')
        treasury_store._source(request.source_job,request.specification['kind'])
    except KeyError as exc: raise HTTPException(404,'unknown source job') from exc
    except (ValueError,TypeError) as exc: raise HTTPException(422,str(exc)) from exc
    return JobStatus(**store.job_status(store.submit('treasury',treasury_store.run,request.source_job,request.specification,state=state)))


@router.get('/sources/{jid}/template')
def source_template(jid:str,kind:Literal['ledger','cashflows']):
    from . import treasury_store
    if db.REPO is None: raise HTTPException(409,'Treasury bridges require durable storage')
    try:return treasury_store.template(jid,kind)
    except KeyError as exc: raise HTTPException(404,'unknown source job') from exc
    except (ValueError,TypeError) as exc: raise HTTPException(422,str(exc)) from exc
