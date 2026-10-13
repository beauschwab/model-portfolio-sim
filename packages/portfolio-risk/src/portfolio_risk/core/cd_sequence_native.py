"""Selected graph2 process transport. No financial backend or fallback."""
import hashlib,json,subprocess,time
from pathlib import Path
from . import model_native
from .native_scratch import scratch_directory
SCHEMAS=frozenset(('term-product-graph-2','term-product-reprice-2'))
MAX_INPUT=2*1024*1024
MAX_OUTPUT=32*1024*1024

def encoded(value,check=lambda:False,maximum=MAX_INPUT):
    parts=[];size=0
    for piece in json.JSONEncoder(allow_nan=False,separators=(',',':')).iterencode(value):
        if check():raise InterruptedError('CD transport cancelled')
        b=piece.encode();size+=len(b)
        if size>maximum:raise ValueError('CD selected transport byte cap')
        parts.append(b)
    if check():raise InterruptedError('CD transport cancelled')
    return b''.join(parts)

def _hash(path,check):
    h=hashlib.sha256()
    with path.open('rb')as f:
        while b:=f.read(1024*1024):check();h.update(b);check()
    return h.hexdigest()

def run(schema,request,*,budget,expected_binary,threads=1):
    if schema not in SCHEMAS:raise ValueError('explicit CD graph2 operation required')
    if type(threads)is not int or not 1<=threads<=64:raise ValueError('CD threads must be in1..64')
    budget.check();captured=model_native.binary_path().resolve()
    if not captured.is_file():raise RuntimeError('Rust CD graph2 runtime unavailable; rebuild; no fallback')
    identity=_hash(captured,budget.check)
    if identity!=expected_binary:raise RuntimeError('MODEL_VERSION_CHANGED: queued CD executable differs')
    data=encoded(dict(schema=schema,threads=threads,request=request),budget.checkpoint)
    budget.check()
    with scratch_directory(prefix='cd-sequence-model-')as directory:
        folder=Path(directory);source=folder/'input.json';out=folder/'output.json';error=folder/'error.txt'
        source.write_bytes(data);budget.check()
        with source.open('rb')as stdin,out.open('w+b')as stdout,error.open('w+b')as stderr:
            child=subprocess.Popen([str(captured)],stdin=stdin,stdout=stdout,stderr=stderr)
            try:
                while child.poll()is None:
                    budget.check()
                    if out.stat().st_size>MAX_OUTPUT or error.stat().st_size>65536:raise ValueError('CD native transport output cap')
                    try:child.wait(timeout=min(.1,max(.001,budget._deadline())))
                    except subprocess.TimeoutExpired:pass
                budget.check()
                if error.stat().st_size>65536:raise ValueError('CD native error cap')
                stdout.seek(0);raw=stdout.read(MAX_OUTPUT+1);budget.check()
                if len(raw)>MAX_OUTPUT:raise ValueError('CD native result cap')
                if child.returncode not in(0,1):raise RuntimeError('CD native process failed; no fallback')
            finally:
                if child.poll()is None:child.kill()
                child.wait()
    budget.check()
    if model_native.binary_path().resolve()!=captured or _hash(captured,budget.check)!=identity:raise RuntimeError('MODEL_VERSION_CHANGED: captured CD executable/path changed')
    response=json.loads(raw);budget.check()
    if type(response)is not dict or response.get('ok')is not True:raise ValueError('native CD rejected input: '+str(response.get('error','invalid response')))
    return response['result'],dict(schema=schema,backend='rust',binary_sha256=identity,input_sha256=hashlib.sha256(data).hexdigest(),runtime_control='process-watchdog-1',caller_cooperative_native_control=False)
