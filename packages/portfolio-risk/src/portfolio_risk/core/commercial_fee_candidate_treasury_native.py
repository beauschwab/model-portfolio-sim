"""Transport source-owned candidate fee capital/FTP; no financial fallback."""
import json
import math
from .product_strategy_native import binary_path, binary_identity, _run

SCHEMA = 'commercial-candidate-fee-treasury-1'


def command(request, *, timeout=1800.):
    if request.get('schema') != SCHEMA:
        raise ValueError('explicit acquired commercial fee treasury contract required')
    if type(timeout) not in (int,float) or not math.isfinite(timeout) or not .001 <= timeout <= 1800.:
        raise ValueError('invalid acquired commercial fee treasury deadline')
    return dict(op='analyze_commercial_candidate_fee_treasury',schema=SCHEMA,
        execution_timeout_ms=int(timeout*1000),request=request)


def run(request, *, threads=1, expected_binary=None, cancelled=None, timeout=1800.):
    prepared = command(request,timeout=timeout)
    if len(json.dumps(prepared,allow_nan=False,separators=(',',':')).encode()) > 2*1024*1024:
        raise ValueError('acquired commercial fee treasury envelope exceeds2MiB')
    result, execution = _run(request,schema=SCHEMA,command=prepared,threads=threads,
        expected_binary=expected_binary,cancelled=cancelled,timeout=timeout)
    execution['native_timeout_ms'] = prepared['execution_timeout_ms']
    return result, execution
