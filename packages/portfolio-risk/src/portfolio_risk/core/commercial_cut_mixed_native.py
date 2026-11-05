"""Transport raw completed contracts into a native cut-started commercial Sim."""
import json
import math
import time
from .product_strategy_native import binary_path, binary_identity, _run

SCHEMA = 'commercial-cut-mixed-journal-1'


def command(request, *, timeout=1800.):
    if request.get('schema') != SCHEMA:
        raise ValueError('explicit commercial cut mixed contract required')
    if type(timeout) not in (int,float) or not math.isfinite(timeout) or not .001 <= timeout <= 1800.:
        raise ValueError('invalid commercial cut mixed deadline')
    return dict(op='analyze_commercial_cut_mixed_journal',schema=SCHEMA,
        execution_timeout_ms=int(timeout*1000),request=request)


def run(request, *, threads=1, expected_binary=None, cancelled=None, timeout=1800.):
    started = time.monotonic()
    prepared = command(request,timeout=timeout)
    if len(json.dumps(prepared,allow_nan=False,separators=(',',':')).encode()) > 2*1024*1024:
        raise ValueError('commercial cut mixed envelope exceeds2MiB')
    result, execution = _run(request,schema=SCHEMA,command=prepared,threads=threads,
        expected_binary=expected_binary,cancelled=cancelled,timeout=timeout)
    execution['native_timeout_ms'] = prepared['execution_timeout_ms']
    if result.get('schema') == 'commercial-fixed-sequential-cut-1':
        # Protocol decoding only. Raw independent source/resource verification
        # remains the worker's pre-publication and persisted-publication gate.
        from ..analytics.commercial_cut_stream_transport import decode
        def check():
            if cancelled and cancelled():
                raise InterruptedError('commercial cut decode cancelled or worker lease lost')
            if time.monotonic() - started >= timeout:
                raise TimeoutError('commercial cut original deadline exceeded during decode')
        tables, completion = decode(result['cut_stream'], check)
        check()
        result = dict(schema=SCHEMA, transport='fixed-sequential-cut-1',
            raw_source=request, native_packet=result, cut_tables=tables,
            producer_completion=completion)
    return result, execution
