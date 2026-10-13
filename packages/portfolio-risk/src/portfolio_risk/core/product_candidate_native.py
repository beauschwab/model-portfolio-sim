"""Bounded standalone candidate-journal transport; no financial calculation."""
import math
from .product_strategy_native import binary_path,binary_identity,_run

SCHEMA='product-candidate-journal-1'

def command(request,*,timeout=1800.):
    if request.get('schema')!=SCHEMA:raise ValueError('explicit native candidate journal contract required')
    if not math.isfinite(timeout) or not .001<=timeout<=1800.:raise ValueError('invalid candidate deadline')
    return dict(op='analyze_product_candidates',schema=SCHEMA,execution_timeout_ms=int(timeout*1000),request=request)

def run(request,*,threads=1,expected_binary=None,cancelled=None,timeout=1800.):
    return _run(request,schema=SCHEMA,command=command(request,timeout=timeout),threads=threads,
        expected_binary=expected_binary,cancelled=cancelled,timeout=timeout)
