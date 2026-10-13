"""Explicit durable saved-derivative mixed source route; no request-side finance."""
from fastapi import APIRouter,HTTPException,Request
from pydantic import BaseModel,ConfigDict,Field
from typing import Literal
from . import persistence as db,store
from .schemas import JobStatus
router=APIRouter(prefix='/derivatives')
class SavedDerivativeDepositPortfolioRequest(BaseModel):
    model_config=ConfigDict(extra='forbid')
    expected_revision:int=Field(ge=0,strict=True)
    specification:dict
    timeout:float=Field(default=1800.,gt=0,le=1800.,strict=True)
@router.post('/deposit-portfolio/runs',response_model=JobStatus)
async def submit(request:SavedDerivativeDepositPortfolioRequest,http:Request):
    from . import joint_deposit_derivative_jobs as jobs
    from portfolio_risk.analytics.saved_derivative_identity import loads
    if db.REPO is None:raise HTTPException(409,'Saved derivative sources require durable storage')
    try:
        # FastAPI parsing is not source authority; preserve/reject duplicates from
        # the original request body before the server captures immutable values.
        original=SavedDerivativeDepositPortfolioRequest.model_validate(loads(await http.body()))
        state=store.snapshot()
        if original.expected_revision!=state['revision']:raise db.Conflict('inputs changed; reload before running')
        queued=jobs.queue_input(original.specification,timeout=original.timeout)
        jid=store.submit('saved_derivative_deposit_portfolio',jobs.run,queued,state=state)
        return JobStatus(**store.job_status(jid))
    except (ValueError,ArithmeticError,KeyError)as e:raise HTTPException(422,str(e))from e

@router.get('/deposit-portfolio/example')
def example():
    from .storage_config import ROOT
    from portfolio_risk.analytics.saved_derivative_identity import loads
    return loads((ROOT/'examples/models/saved-derivative-deposit-portfolio-mixed-1.json').read_bytes())
