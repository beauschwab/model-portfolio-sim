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
    if request.specification.get('kind') not in {'ledger','credit_ledger','cashflows'}: raise HTTPException(422,'bridge kind must be ledger, credit_ledger or cashflows')
    import json
    try:
        if len(json.dumps(request.specification,allow_nan=False).encode())>32*1024*1024: raise ValueError('bridge request exceeds 32 MiB')
        treasury_store._source(request.source_job,request.specification['kind'])
    except KeyError as exc: raise HTTPException(404,'unknown source job') from exc
    except (ValueError,TypeError) as exc: raise HTTPException(422,str(exc)) from exc
    return JobStatus(**store.job_status(store.submit('treasury',treasury_store.run,request.source_job,request.specification,state=state)))


@router.get('/sources/{jid}/template')
def source_template(jid:str,kind:Literal['ledger','credit_ledger','cashflows']):
    from . import treasury_store
    if db.REPO is None: raise HTTPException(409,'Treasury bridges require durable storage')
    try:return treasury_store.template(jid,kind)
    except KeyError as exc: raise HTTPException(404,'unknown source job') from exc
    except (ValueError,TypeError) as exc: raise HTTPException(422,str(exc)) from exc


class MortgageClosingRequest(BridgeRequest):
    expected_revision: int = Field(ge=0,strict=True)
    closing_review: dict
    timeout: float = Field(default=1800.,ge=.001,le=1800.,strict=True)


@router.post('/mortgage-closing-capital')
def mortgage_closing_capital(request:MortgageClosingRequest):
    from . import mortgage_capital
    if db.REPO is None:raise HTTPException(409,'Mortgage closing capital requires durable storage')
    state=store.snapshot()
    if request.expected_revision!=state['revision']:raise db.Conflict('inputs changed; reload before running')
    try:
        queued=mortgage_capital.queue_input(request.source_job,request.specification,request.closing_review,timeout=request.timeout)
    except KeyError as exc:raise HTTPException(404,'unknown completed mortgage source') from exc
    except (ValueError,TypeError) as exc:raise HTTPException(422,str(exc)) from exc
    jid=store.submit('mortgage_closing_capital',mortgage_capital.run,queued,state=state)
    return JobStatus(**store.job_status(jid))


from fastapi import Depends,Request as HTTPRequest


async def _modeled_ftp_original_value(request:HTTPRequest):
    """New-route original bytes: refuse duplicates before the collapsed model is authority.

    FastAPI has already buffered JSON; this is a selected admission cap, not a
    preparse RSS guarantee. Native execution independently reparses its selected bytes.
    """
    import json
    body=await request.body()
    if len(body)>1048576:raise HTTPException(422,'selected FTP HTTP envelope exceeds1MiB')
    def pairs(values):
        result={}
        for key,value in values:
            if key in result:raise ValueError('duplicate FTP HTTP field: '+key)
            result[key]=value
        return result
    def nonfinite(value):raise ValueError('nonfinite FTP JSON value: '+value)
    try:return json.loads(body,object_pairs_hook=pairs,parse_constant=nonfinite)
    except (ValueError,TypeError,RecursionError)as exc:raise HTTPException(422,str(exc))from exc


class ModeledProductFTPRequest(TreasuryRequest):
    expected_revision:int=Field(ge=0,strict=True)
    timeout:float=Field(default=1800.,ge=.001,le=1800.,strict=True)


@router.post('/modeled-product-ftp')
def modeled_product_ftp(request:ModeledProductFTPRequest,original:dict=Depends(_modeled_ftp_original_value)):
    from . import modeled_product_ftp_jobs
    if db.REPO is None:raise HTTPException(409,'Modeled-product FTP requires durable storage')
    state=store.snapshot()
    if request.expected_revision!=state['revision']:raise db.Conflict('inputs changed; reload before running')
    from portfolio_risk.analytics.mortgage_mixed_identity import exact_json_identity
    if not exact_json_identity(original.get('specification'),request.specification):raise HTTPException(422,'FTP HTTP raw Value changed during validation')
    try:queued=modeled_product_ftp_jobs.queue_input(original['specification'],timeout=request.timeout)
    except (ValueError,TypeError)as exc:raise HTTPException(422,str(exc))from exc
    jid=store.submit('modeled_product_ftp',modeled_product_ftp_jobs.run,queued,state=state)
    return JobStatus(**store.job_status(jid))


@router.get('/modeled-product-ftp/example')
def modeled_product_ftp_example():
    import json
    from pathlib import Path
    root=Path(__file__).resolve().parents[3]
    raw=json.loads((root/'examples/models/modeled-product-ftp-preparation-1.json').read_text())
    return dict(specification=raw,revision=store.snapshot()['revision'],durable=db.REPO is not None)


@router.post('/modeled-product-ftp/sources')
def modeled_product_ftp_sources(request:ModeledProductFTPRequest,original:dict=Depends(_modeled_ftp_original_value)):
    from . import modeled_product_ftp_jobs,modeled_product_ftp_selection
    if db.REPO is None:raise HTTPException(409,'Selected modeled-product FTP requires durable storage')
    state=store.snapshot()
    if request.expected_revision!=state['revision']:raise db.Conflict('inputs changed; reload before running')
    from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
    if not exact_json_identity(original.get('specification'),request.specification):raise HTTPException(422,'Selected FTP raw Value changed during validation')
    try:queued=modeled_product_ftp_selection.queue_input(original['specification'],timeout=request.timeout)
    except KeyError as exc:raise HTTPException(404,'unknown completed source job')from exc
    except(ValueError,TypeError)as exc:raise HTTPException(422,str(exc))from exc
    jid=store.submit('modeled_product_ftp',modeled_product_ftp_jobs.run,queued,state=state)
    return JobStatus(**store.job_status(jid))


class ModeledFTPPathMetadata(BaseModel):
    model_config = ConfigDict(extra='forbid')
    owner_index: int
    run_index: int
    path_index: int
    position_id: str
    run_id: str


class ModeledFTPSourceMetadata(BaseModel):
    model_config = ConfigDict(extra='forbid')
    job_id: str
    kind: Literal['card_v2','deposit_performing','deposit_portfolio']
    status: Literal['done']
    revision: int
    model_schema: str
    scope: str
    paths: list[ModeledFTPPathMetadata]
    compatibility_metadata_checked: Literal[True]
    source_journal_verified: Literal[False]
    capital_ftp_eligible: Literal[False]
    accounting_standard_approval: Literal[False]


@router.get('/modeled-product-ftp/sources/{jid}',response_model=ModeledFTPSourceMetadata)
def modeled_product_ftp_source_metadata(jid:str,kind:Literal['card_v2','deposit_performing','deposit_portfolio']):
    """Bounded compatibility metadata only; the NEW worker independently replays sources."""
    import time
    from . import modeled_product_ftp_source_binding as binding
    if db.REPO is None:raise HTTPException(409,'Completed-source selection requires durable storage')
    if not 1<=len(jid)<=100:raise HTTPException(422,'bounded source job ID required')
    deadline=time.monotonic()+5.
    def check():
        if time.monotonic()>=deadline:raise TimeoutError('completed-source metadata deadline expired')
        return False
    try:
        captured=binding.capture(db.REPO,db.CODEC,jid,kind,check)
        raw=captured['raw'];paths=[]
        def add(oi,ri,pi,position,run):
            check()
            if len(paths)>=256 or any(type(v)is not str or not 1<=len(v)<=256 for v in(position,run)):
                raise ValueError('bounded source path metadata required')
            paths.append(dict(owner_index=oi,run_index=ri,path_index=pi,position_id=position,run_id=run))
        if kind=='card_v2':
            for oi,owner in enumerate(raw['product_owners']):
                for ri,run in enumerate(owner['runs']):
                    for pi,path in enumerate(run['paths']):add(oi,ri,pi,owner['position_id'],run['ledger_run_id'])
        elif kind=='deposit_portfolio':
            for oi,owner in enumerate(raw['positions']):
                for ri,run in enumerate(raw['runs']):add(oi,ri,0,owner['position_id'],run['run_id'])
        else:
            for ri,run in enumerate(raw['runs']):add(0,ri,0,raw['position_id'],run['run_id'])
        if not paths:raise ValueError('explicit owner/run/path metadata required')
        response=dict(job_id=jid,kind=kind,status='done',revision=captured['binding']['revision'],
            model_schema=binding.KINDS[kind][2],scope={
                'card_v2':'exclusive_product_owners_with_inherited_dated_sources_retained_not_funded-1',
                'deposit_performing':'whole_isolated_continuation_deposit_owner-1','deposit_portfolio':'whole_performing_deposit_portfolio-2'}[kind],paths=paths,
            compatibility_metadata_checked=True,source_journal_verified=False,
            capital_ftp_eligible=False,accounting_standard_approval=False)
        if len(binding.encoded(response))>65536:raise ValueError('source metadata response exceeds64KiB')
        # Repeat exact SQL and bounded request/result object reads after extraction.
        # No financial tables or existing verification receipt grant permission here.
        fresh=binding.capture(db.REPO,db.CODEC,jid,kind,check)
        binding.exact(fresh,captured,'completed source changed during metadata capture')
        check();return response
    except KeyError as exc:raise HTTPException(404,'unknown compatible completed source job')from exc
    except TimeoutError as exc:raise HTTPException(408,str(exc))from exc
    except(ValueError,TypeError,IndexError)as exc:raise HTTPException(422,str(exc))from exc
