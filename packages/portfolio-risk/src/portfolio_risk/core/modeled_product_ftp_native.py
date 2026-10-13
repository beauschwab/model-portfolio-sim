"""Raw native source/FTP preparation transport; no financial runtime choices."""
import json,math
from .product_strategy_native import binary_path,binary_identity,_run
SCHEMA='modeled-product-ftp-preparation-1'
MAX_BODY=1048576
def command(request,*,timeout=1800.):
    if type(request)is not dict or request.get('schema')!=SCHEMA:raise ValueError('explicit raw modeled-product FTP source request required')
    if type(timeout)not in(int,float)or not math.isfinite(timeout)or not .001<=timeout<=1800.:raise ValueError('finite bounded original FTP deadline required')
    value=dict(op='prepare_modeled_product_ftp',schema=SCHEMA,execution_timeout_ms=int(timeout*1000),request=request)
    if len(json.dumps(value,allow_nan=False,separators=(',',':')).encode())>MAX_BODY:raise ValueError('selected FTP envelope exceeds1MiB')
    return value
def run(request,*,expected_binary=None,cancelled=None,timeout=1800.):
    return _run(request,schema=SCHEMA,command=command(request,timeout=timeout),threads=1,expected_binary=expected_binary,cancelled=cancelled,timeout=timeout)
