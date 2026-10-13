"""Bounded native loan/fee journal transport; no financial calculation."""
import math
from .product_strategy_native import _run
SCHEMA='commercial-fee-journal-1'
def command(request,*,timeout=1800.):
    if request.get('schema')!=SCHEMA:raise ValueError('explicit commercial fee journal required')
    if type(timeout)not in(int,float)or not math.isfinite(timeout)or not .001<=timeout<=1800.:raise ValueError('invalid fee journal deadline')
    return dict(op='analyze_commercial_fee_journal',schema=SCHEMA,execution_timeout_ms=int(timeout*1000),request=request)
def run(request,*,threads=1,expected_binary=None,cancelled=None,timeout=1800.):
    prepared=command(request,timeout=timeout)
    result,execution=_run(request,schema=SCHEMA,command=prepared,threads=threads,expected_binary=expected_binary,cancelled=cancelled,timeout=timeout)
    execution['native_timeout_ms']=prepared['execution_timeout_ms'];return result,execution
