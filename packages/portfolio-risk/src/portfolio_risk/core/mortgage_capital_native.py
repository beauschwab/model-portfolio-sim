"""Transport to the existing native strategy process; no financial backend choice."""
import json
import math
from .product_strategy_native import binary_path, binary_identity, _run

SCHEMA='mortgage-closing-capital-1'
MAX_BODY=2*1024*1024

def command(request,*,timeout=1800.):
    if type(request) is not dict or request.get('schema')!=SCHEMA:
        raise ValueError('explicit raw mortgage closing capital request required')
    if type(timeout) not in (int,float) or not math.isfinite(timeout) or not .001<=timeout<=1800.:
        raise ValueError('finite bounded original capital deadline required')
    if len(json.dumps(request,allow_nan=False,separators=(',',':')).encode())>MAX_BODY:
        raise ValueError('mortgage capital body exceeds2MiB')
    return dict(op='analyze_mortgage_closing_capital',schema=SCHEMA,
        execution_timeout_ms=int(timeout*1000),request=request)

def run(request,*,expected_binary=None,cancelled=None,timeout=1800.):
    return _run(request,schema=SCHEMA,command=command(request,timeout=timeout),threads=1,
        expected_binary=expected_binary,cancelled=cancelled,timeout=timeout)
