"""Bounded acquisition-aware mixed dated transport; Rust owns all finance."""
import math
from .product_strategy_native import binary_path,binary_identity,_run

SCHEMA='product-mixed-dated-1'


def command(request,*,timeout=1800.):
    if request.get('schema')!=SCHEMA:raise ValueError('explicit native mixed dated contract required')
    if not math.isfinite(timeout) or not .001<=timeout<=1800.:raise ValueError('invalid mixed dated deadline')
    return dict(op='analyze_product_mixed_dated',schema=SCHEMA,execution_timeout_ms=int(timeout*1000),request=request)


def run(request,*,threads=1,expected_binary=None,cancelled=None,timeout=1800.):
    return _run(request,schema=SCHEMA,command=command(request,timeout=timeout),threads=threads,
        expected_binary=expected_binary,cancelled=cancelled,timeout=timeout)
