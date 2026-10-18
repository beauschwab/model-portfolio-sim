"""Explicit contract selection, finite bounded intake and durable worker admission."""
import json
from pathlib import Path
from fastapi import APIRouter,HTTPException,Request,Depends
from typing import Annotated
from pydantic import BaseModel,ConfigDict,Field
from . import persistence as db,store
from .schemas import JobStatus

router=APIRouter(prefix='/models')
EXAMPLES={
    "commercial-fee-journal-1":"commercial-fee-journal-1.json",
    "nonintegral-fee-accounting-1":"nonintegral-fee-accounting-1.json",
    "hedge-accounting-1":"hedge-accounting-1.json",
    "hedge-treasury-1":"hedge-treasury-1.json",
    "derivative-treasury-1":"derivative-treasury-1.json",
    "saved-derivative-risk-1":"saved-derivative-risk-1.json",
    "net-investment-hedge-1":"net-investment-hedge-1.json",
    'commercial-holding-journal-1':'commercial-holding-journal-1.json',
    'commercial-holding-continuation-1':'commercial-holding-continuation-1.json',
    'commercial-candidate-journal-1':'commercial-candidate-journal-1.json',
    'commercial-generated-exercise-1':'commercial-generated-exercise-1.json',
    'commercial-treasury-1':'commercial-treasury-1.json',
    'commercial-product-graph-1':'commercial-product-graph-1.json',
    'commercial-units-1':'commercial-units-1.json',
    'consumer-installment-1':'consumer-installment-1.json',
    'consumer-credit-accounting-1':'consumer-credit-accounting-1.json',
    'card-credit-accounting-1':'card-credit-accounting-1.json',
    'card-credit-accounting-2':'card-credit-accounting-2.json',
    'consumer-card-1':'consumer-card-1.json',
    'deposit-state-1':'deposit-state-1.json',
    'mortgage-state-1':'mortgage-state-1.json',
    'mortgage-credit-accounting-1':'mortgage-credit-accounting-1.json',
    'funding-contract-1':'funding-contract-1.json',
    'indexed-contract-1':'term-indexed.json',
    'conditional-exercise-1':'term-exercise.json',
    'cd-election-1':'term-cd-election.json',
    'dated-income-1':'dated-income-1.json',
    'model-market-1':'model-market-1.json',
    'market-driver-calibration-1':'market-driver-calibration-1.json',
    'market-quote-bootstrap-1':'market-quote-bootstrap-1.json',
    'observed-deposit-calibration-1':'observed-deposit-calibration-1.json',
    'market-indexed-adapter-1':'market-indexed-adapter-1.json',
    'market-indexed-books-1':'market-indexed-books-job-1.json',
    'aged-units-1':'aged-units-1.json',
    'credit-path-1':'credit-path-1.json',
    'joint-state-1':'joint-state-1.json',
    'joint-market-credit-1':'joint-market-credit-1.json',
    'credit-path-accounting-1':'credit-path-accounting-1.json',
    'credit-snapshot-calibration-1':'credit-snapshot-calibration-1.json',
    'derivative-contract-1':'derivative-contract-1.json',
    'derivative-accounting-1':'derivative-accounting-1.json',
    'credit-covariate-1':'credit-covariate-1.json',
    'advanced-product-graph-1':'advanced-product-graph-1.json',
    'card-product-graph-1':'card-product-graph-1.json',
    'deposit-product-graph-1':'deposit-product-graph-1.json',
    'mortgage-product-graph-1':'mortgage-product-graph-1.json',
    'funding-product-graph-1':'funding-product-graph-1.json',
    'graph-cohort-audit-1':'graph-cohort-audit-1.json',
    'term-product-graph-1':'term-product-graph-1.json',
    'term-product-graph-2':'term-product-graph-2.json',
    'term-product-reprice-2':'term-product-reprice-2.json',
    'product-units-1':'product-units-1.json',
    'strategy-product-1':'strategy-product-1.json',
    'product-candidate-journal-1':'product-candidate-journal-1.json',
    'product-mixed-dated-1':'product-mixed-dated-1.json',
    'product-mixed-consumer-credit-1':'product-mixed-consumer-credit-1.json',
    'product-mixed-product-credit-1':'product-mixed-product-credit-1.json',
}


class ModelRunRequest(BaseModel):
    model_config=ConfigDict(extra='forbid')
    expected_revision:int=Field(ge=0)
    schema_name:str=Field(min_length=1,max_length=100)
    request:dict
    timeout:float=Field(default=1800.,gt=0,le=1800,allow_inf_nan=False)


class PublishedHoldingRequest(BaseModel):
    model_config=ConfigDict(extra='forbid',strict=True)
    expected_revision:int=Field(ge=0)
    source_job_id:str=Field(min_length=1,max_length=100)
    cut_ordinal:int
    revised_sources:list[dict]=Field(min_length=1,max_length=512)
    timeout:float=Field(default=1800.,ge=.001,le=1800,allow_inf_nan=False)


async def _published_holding_body(http:Request):
    import time
    from .completed_source_reader import loads
    started=time.time();body=await http.body()
    if len(body)>2*1024*1024:raise HTTPException(422,'bounded published holding input')
    try:value=loads(body)
    except (ValueError,TypeError)as exc:raise HTTPException(422,str(exc))from exc
    return value,started


@router.post('/holding-journals',response_model=JobStatus)
def published_holding(request:PublishedHoldingRequest,raw_body:Annotated[tuple|None,Depends(_published_holding_body)]=None):
    import time
    from . import commercial_prior_source as source
    started=time.time()if raw_body is None else raw_body[1]
    if raw_body is not None:
        from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
        if not exact_json_identity(raw_body[0].get('revised_sources'),request.revised_sources):
            raise HTTPException(422,'original published holding sources changed during parsing')
    if db.REPO is None:raise HTTPException(409,'durable repository required')
    deadline=started+request.timeout
    def check():
        if time.time()>=deadline:raise TimeoutError('original published holding queue deadline expired')
        return False
    try:
        captured=source.capture(db.REPO,db.CODEC,request.source_job_id,request.cut_ordinal,check)
        raw=source.materialize(captured,request.revised_sources,request.cut_ordinal)
    except KeyError as exc:raise HTTPException(404,'unknown completed holding source')from exc
    except (ValueError,TypeError)as exc:raise HTTPException(422,str(exc))from exc
    except TimeoutError as exc:raise HTTPException(408,str(exc))from exc
    selected=ModelRunRequest(expected_revision=request.expected_revision,schema_name=source.SCHEMA,request=raw,timeout=request.timeout)
    return enqueue(selected,'model_analysis',store.run_model_analysis,accepted_start=started,prior_publication=captured)


@router.get('/contracts')
def contracts():
    return dict(schemas=list(EXAMPLES),revision=store.snapshot()['revision'],durable=db.REPO is not None,
        scope='Explicit analytical contracts; synthetic examples do not establish empirical accuracy or upgrade saved economics')


@router.get('/examples/{schema}')
def example(schema:str):
    if schema not in EXAMPLES:raise HTTPException(404,'unsupported model contract')
    root=Path(__file__).resolve().parents[3]
    raw=json.loads((root/'examples/models'/EXAMPLES[schema]).read_text(encoding='utf-8'))
    request=raw['request'] if 'request' in raw else raw
    return dict(schema_name=schema,request=request,revision=store.snapshot()['revision'],synthetic=True)


async def _selected_cd_body(http:Request):
    import time
    started=time.time()
    body=await http.body();duplicates=[];schemas=set();commercial_kind=False
    def object_pairs(pairs):
        nonlocal commercial_kind
        row={}
        for key,value in pairs:
            if key=='schema_name' and type(value)is str:schemas.add(value)
            if key=='kind' and value=='commercial':commercial_kind=True
            if key in row:duplicates.append(key)
            row[key]=value
        return row
    # Historical selections retain their decoder behavior. Selected CD and
    # commercial operations retain original scalar categories and reject duplicates.
    value=json.loads(body,object_pairs_hook=object_pairs)
    if type(value)is not dict:return None
    selected={'term-product-graph-2','term-product-reprice-2','commercial-product-graph-1','commercial-units-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1','commercial-generated-exercise-1','commercial-holding-continuation-1','nonintegral-fee-accounting-1','hedge-accounting-1','hedge-treasury-1','derivative-treasury-1','saved-derivative-risk-1','net-investment-hedge-1'}
    cohort='graph-cohort-audit-1'in schemas and commercial_kind
    if not schemas.intersection(selected) and not cohort:return None
    bound=1024*1024 if schemas.intersection({'commercial-generated-exercise-1','commercial-holding-continuation-1'}) else 2*1024*1024 if schemas.intersection({'commercial-treasury-1','commercial-holding-journal-1','commercial-fee-journal-1'}) else 32*1024*1024 if cohort or schemas.intersection({'commercial-product-graph-1','commercial-units-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1','hedge-accounting-1','hedge-treasury-1','derivative-treasury-1','saved-derivative-risk-1','net-investment-hedge-1'}) else 2*1024*1024
    if len(body)>bound:raise HTTPException(422,'bounded selected HTTP raw bytes')
    if duplicates:raise HTTPException(422,'duplicate field `'+duplicates[0]+'`')
    return (value.get('request'),started)

@router.post('/runs',response_model=JobStatus)
def submit(request:ModelRunRequest, raw_body:Annotated[tuple|None,Depends(_selected_cd_body)]=None):
    accepted_start=None
    if raw_body is not None:
        raw_body,accepted_start=raw_body
        from portfolio_risk.analytics.cd_sequence_graph_reference import exact
        try:exact(raw_body,request.request,'original accepted CD JSON')
        except ValueError as exc:raise HTTPException(422,str(exc)) from exc
    return enqueue(request,'model_analysis',store.run_model_analysis,accepted_start=accepted_start)


@router.post('/aged-library',response_model=JobStatus)
def aged_library(request:ModelRunRequest,raw_body:Annotated[tuple|None,Depends(_selected_cd_body)]=None):
    if request.schema_name not in {'aged-units-1','product-units-1','commercial-units-1'}:raise HTTPException(422,'dated unit contract required')
    accepted_start=None
    if raw_body is not None:
        raw,accepted_start=raw_body
        from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
        if not exact_json_identity(raw,request.request):raise HTTPException(422,'original commercial library scalar categories changed')
    return enqueue(request,'aged_unitlib',store.build_aged_unitlib_job,accepted_start=accepted_start)


def enqueue(request,kind,operation,*,accepted_start=None,prior_publication=None):
    from .commercial_selection import is_commercial_cohort
    indexed_deadline=None
    if is_commercial_cohort(request.schema_name,request.request) or request.schema_name in {'market-indexed-books-1','term-product-graph-2','term-product-reprice-2','commercial-product-graph-1','commercial-units-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1','commercial-generated-exercise-1','commercial-holding-continuation-1','nonintegral-fee-accounting-1','hedge-accounting-1','hedge-treasury-1','derivative-treasury-1','saved-derivative-risk-1','net-investment-hedge-1'}:
        import time
        indexed_deadline=(time.time() if accepted_start is None else accepted_start)+request.timeout
    if db.REPO is None:raise HTTPException(409,'model analyses require durable storage')
    if request.schema_name not in EXAMPLES:raise HTTPException(422,'unsupported model analysis contract')
    try:
        size=len(json.dumps(request.request,allow_nan=False,separators=(',',':')).encode())
        bound=1024*1024 if request.schema_name in {'commercial-generated-exercise-1','commercial-holding-continuation-1'} else 2*1024*1024 if request.schema_name in {'commercial-treasury-1','commercial-holding-journal-1','commercial-fee-journal-1','nonintegral-fee-accounting-1'} else 32*1024*1024
        if size>bound-1024:raise ValueError('model request exceeds selected byte bound')
    except (ValueError,TypeError) as exc:raise HTTPException(422,str(exc)) from exc
    state=store.snapshot()
    if state['revision']!=request.expected_revision:raise db.Conflict('inputs changed; reload before running')
    from numba import get_num_threads
    from portfolio_risk.core.model_native import binary_identity
    if request.schema_name in {'strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1'}:
        from portfolio_risk.core.product_strategy_native import binary_identity
    from portfolio_risk.analytics.balance_stream import source_identity
    try:
        if request.schema_name=='market-indexed-books-1':
            from .indexed_book_jobs import queued_identity
            identity=queued_identity()['binary_sha256']
        elif request.schema_name in {'term-product-graph-2','term-product-reprice-2'}:
            from .cd_sequence_jobs import queued_identity
            bundle=queued_identity();identity=bundle['binary_sha256']
        else:identity=binary_identity()
    except RuntimeError as exc:raise HTTPException(409,str(exc)) from exc
    payload=dict(schema=request.schema_name,request=request.request,timeout=request.timeout,
        threads=state['settings'].n_threads or get_num_threads(),binary_sha256=identity,source_sha256=source_identity())
    if prior_publication is not None:
        if request.schema_name!='commercial-holding-journal-1':raise HTTPException(422,'holding prior binding requires holding native contract')
        from copy import deepcopy
        payload['prior_publication']=deepcopy(prior_publication)
    if is_commercial_cohort(request.schema_name,request.request) or request.schema_name in {'commercial-product-graph-1','commercial-units-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1','commercial-generated-exercise-1','commercial-holding-continuation-1','nonintegral-fee-accounting-1','hedge-accounting-1','hedge-treasury-1','derivative-treasury-1','saved-derivative-risk-1','net-investment-hedge-1'}:
        payload.update(publication_deadline_unix=indexed_deadline)
        if time.time()>=indexed_deadline:raise HTTPException(408,'original commercial queue deadline expired')
    if request.schema_name=='market-indexed-books-1':
        from portfolio_risk.core.indexed_book_job_native import fully_bound
        try:fully_bound(request.request)
        except (ValueError,TypeError,KeyError) as exc:raise HTTPException(422,str(exc)) from exc
        payload.update(artifact_kind='product_library',publication_deadline_unix=indexed_deadline)
        if time.time()>=indexed_deadline:raise HTTPException(408,'original indexed queue deadline expired')
    if request.schema_name in {'term-product-graph-2','term-product-reprice-2'}:
        from portfolio_risk.core.cd_sequence_native import encoded
        try:encoded(dict(schema=request.schema_name,threads=payload['threads'],request=request.request))
        except (ValueError,TypeError) as exc:raise HTTPException(422,str(exc)) from exc
        payload.update(artifact_kind='model_executable',native_bundle=bundle,publication_deadline_unix=indexed_deadline)
        if time.time()>=indexed_deadline:raise HTTPException(408,'original CD queue deadline expired')
    jid=store.submit(kind,operation,payload,state=state)
    return JobStatus(**store.job_status(jid))
