"""Per-call execution IO only. No financial callback or alternate backend."""
import ctypes
import json
import threading
import time
import math

def _options(runtime):
    if type(runtime) is not dict or not {'schema','timeout_ms'}<=set(runtime)<= {'schema','timeout_ms','cancelled'}:
        raise ValueError('exact graph-call-control-1 runtime fields required')
    if runtime['schema']!='graph-call-control-1' or type(runtime['timeout_ms']) is not int or not 1<=runtime['timeout_ms']<=3600000:
        raise ValueError('graph-call-control-1 timeout_ms integer in [1,3600000] required')
    callback=runtime.get('cancelled')
    if callback is not None and not callable(callback):raise ValueError('cancelled must be a bounded IO callback')
    return callback

def _symbols(lib):
    names=['portfolio_graph_runtime_version','portfolio_graph_cancel_new','portfolio_graph_cancel_signal','portfolio_graph_cancel_release','portfolio_term_graph_controlled']
    try:version,new,signal,release,invoke=(getattr(lib,n) for n in names)
    except AttributeError as exc:raise RuntimeError('NATIVE_GRAPH_CONTROL_REQUIRED: rebuild and restart; uncontrolled fallback forbidden') from exc
    version.argtypes=[];version.restype=ctypes.c_uint32
    if version()!=1:raise RuntimeError('NATIVE_GRAPH_CONTROL_REQUIRED: unsupported runtime capability; rebuild and restart')
    new.argtypes=[];new.restype=ctypes.c_uint64
    for f in [signal,release]:f.argtypes=[ctypes.c_uint64];f.restype=ctypes.c_int32
    invoke.argtypes=[ctypes.c_char_p,ctypes.c_size_t,ctypes.c_uint64,ctypes.c_uint64];invoke.restype=ctypes.c_void_p
    return new,signal,release,invoke

def cleanup_created(lib,payload,response):
    """Retire only this invocation's unpublished new owner using captured library."""
    raw=json.loads(payload)
    if raw.get('schema')!='graph-1' or raw.get('request',{}).get('op') not in ['create_market_indexed','create_market_indexed_books'] or response.get('ok') is not True:return
    handle=response.get('result',{}).get('handle')
    if type(handle) is not int or handle<=0:raise RuntimeError('invalid newly created graph cleanup identity')
    data=json.dumps(dict(schema='graph-1',threads=raw['threads'],request=dict(op='drop',handle=handle)),separators=(',',':')).encode()
    invoke=lib.portfolio_term_request;invoke.argtypes=[ctypes.c_char_p,ctypes.c_size_t];invoke.restype=ctypes.c_void_p
    pointer=invoke(data,len(data))
    if not pointer:raise RuntimeError('unpublished graph cleanup allocation failed')
    try:
        if json.loads(ctypes.string_at(pointer)).get('ok') is not True:raise RuntimeError('unpublished graph cleanup failed')
    finally:lib.portfolio_term_free(pointer)

def invoke_controlled(lib,payload,runtime):
    callback=_options(runtime)
    new,signal,release,invoke=_symbols(lib)
    token=new()
    if not token:raise RuntimeError('GRAPH_CONTROL_TOKEN_LIMIT: no uncontrolled fallback')
    deadline=time.monotonic()+runtime['timeout_ms']/1000.
    stop=threading.Event();ready=threading.Event();errors=[];pointer=None
    def probe():
        if errors:raise errors[0]
        if time.monotonic()>=deadline:raise TimeoutError('controlled graph transport deadline exceeded')
    def sample_callback():
        if callback is not None:
            value=callback()
            if type(value) is not bool:raise ValueError('cancelled IO callback must return bool')
            if value:raise InterruptedError('controlled graph cancelled or caller lease lost')
    def check():
        # A fresh bounded lease read at each handoff/decode boundary. The monitor
        # samples directly; it never recursively creates readers.
        probe()
        if callback is not None:
            completed=threading.Event();fault=[]
            def read():
                try:sample_callback()
                except BaseException as exc:fault.append(exc)
                finally:completed.set()
            reader=threading.Thread(target=read,name='native-graph-final-lease-io',daemon=True)
            reader.start()
            if not completed.wait(min(.25,max(0.,deadline-time.monotonic()))):
                probe()
                raise RuntimeError('GRAPH_CONTROL_CALLBACK_NOT_BOUNDED')
            if fault:raise fault[0]
        probe()
    def poll():
        while not stop.is_set():
            try:
                sample_callback()
                probe()
            except BaseException as exc:
                errors.append(exc)
                if not stop.is_set():signal(token)
                ready.set();return
            ready.set()
            if stop.wait(.05):return
    thread=threading.Thread(target=poll,name='native-graph-cancel-io',daemon=True)
    started=False;released=False
    try:
        thread.start();started=True
        if not ready.wait(.25):raise RuntimeError('GRAPH_CONTROL_CALLBACK_NOT_BOUNDED')
        if errors:raise errors[0]
        probe()
        remaining_ms=math.floor((deadline-time.monotonic())*1000.)
        if remaining_ms<1:raise TimeoutError('controlled graph transport deadline exceeded')
        pointer=invoke(payload,len(payload),token,remaining_ms)
        stop.set();thread.join(.25)
        if thread.is_alive():raise RuntimeError('GRAPH_CONTROL_CALLBACK_NOT_BOUNDED')
        check()
        if release(token)!=1:raise RuntimeError('graph control token cleanup failed')
        released=True
        return pointer,check
    except BaseException:
        if pointer:
            try:cleanup_created(lib,payload,json.loads(ctypes.string_at(pointer)))
            finally:lib.portfolio_term_free(pointer)
        raise
    finally:
        stop.set()
        if started:thread.join(.25)
        if not released and release(token)!=1:raise RuntimeError('graph control token cleanup failed')
