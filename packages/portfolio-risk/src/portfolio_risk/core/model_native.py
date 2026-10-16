"""Bounded standalone model transport with process cancellation/deadline fences."""
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys
import time

from .native_scratch import scratch_directory

MAX_INPUT=32*1024*1024
MAX_OUTPUT=128*1024*1024

def is_generated_oas_calibration(schema,request):
    """Select explicit bounded transport controls; performs no finance."""
    if type(request)is not dict:return False
    if schema=='commercial-product-graph-1':graphs=[request]
    elif schema=='commercial-units-1' and type(request.get('units'))is list:graphs=[u.get('graph',{}) for u in request['units'] if type(u)is dict]
    else:return False
    for graph in graphs:
        if type(graph)is not dict or type(graph.get('positions'))is not list:continue
        for position in graph['positions']:
            if type(position)is dict and type(position.get('generated_policy'))is dict and position['generated_policy'].get('oas_calibration') is not None:return True
    return False


def binary_path():
    name='portfolio-lifecycle.exe' if sys.platform=='win32' else 'portfolio-lifecycle'
    return Path(__file__).resolve().parents[4]/'portfolio-risk-native'/'target'/'release'/name


def binary_identity():
    path=binary_path()
    if not path.is_file():raise RuntimeError('Rust model runtime not built; rebuild native artifacts; no fallback')
    digest=hashlib.sha256()
    with path.open('rb') as handle:
        for block in iter(lambda:handle.read(1024*1024),b''):digest.update(block)
    return digest.hexdigest()


def run(schema,request,*,threads=1,expected_binary=None,cancelled=None,timeout=1800.):
    """Transport one admitted analysis; native code owns all financial calculations."""
    if type(threads) is not int or not 1<=threads<=256:raise ValueError('invalid model thread count')
    if not math.isfinite(timeout) or not .001<=timeout<=1800.:raise ValueError('invalid model deadline')
    envelope=dict(schema=schema,threads=threads,request=request)
    controlled=schema in {'commercial-generated-exercise-1','commercial-holding-continuation-1','nonintegral-fee-accounting-1','hedge-accounting-1'} or is_generated_oas_calibration(schema,request)
    if controlled:envelope['execution_timeout_ms']=max(1,int(timeout*1000))
    data=json.dumps(envelope,allow_nan=False,separators=(',',':')).encode()
    input_limit=1024*1024 if schema in {'commercial-generated-exercise-1','commercial-holding-continuation-1'} else 2*1024*1024 if schema=='nonintegral-fee-accounting-1' else MAX_INPUT
    output_limit=32*1024*1024 if controlled else MAX_OUTPUT
    if len(data)>input_limit:raise ValueError('model request exceeds bounded native input')
    identity=binary_identity()
    if expected_binary is not None and identity!=expected_binary:
        raise RuntimeError('MODEL_VERSION_CHANGED: model executable differs from queued identity')
    deadline=time.monotonic()+timeout
    def check():
        if cancelled and cancelled():raise InterruptedError('model analysis cancelled or worker lease lost')
        if time.monotonic()>=deadline:raise TimeoutError('model analysis deadline exceeded')
    check()
    with scratch_directory(prefix='portfolio-model-') as scratch:
        folder=Path(scratch);input_path=folder/'input.json';output_path=folder/'output.json';error_path=folder/'error.txt'
        input_path.write_bytes(data)
        with input_path.open('rb') as stdin,output_path.open('w+b') as stdout,error_path.open('w+b') as stderr:
            child=subprocess.Popen([str(binary_path())],stdin=stdin,stdout=stdout,stderr=stderr)
            try:
                while child.poll() is None:
                    check()
                    if output_path.stat().st_size>output_limit or error_path.stat().st_size>65536:
                        raise ValueError('native model output exceeded bounded transport')
                    try:child.wait(timeout=min(.1,max(.001,deadline-time.monotonic())))
                    except subprocess.TimeoutExpired:pass
                check()
                if error_path.stat().st_size>65536:raise ValueError('native model error output exceeded bounded transport')
                stdout.seek(0);raw=stdout.read(output_limit+1)
                if len(raw)>output_limit:raise ValueError('native model output exceeded bounded transport')
                if child.returncode not in {0,1}:raise RuntimeError('native model process failed; no fallback')
            finally:
                if child.poll() is None:child.kill()
                child.wait()
    if binary_identity()!=identity:raise RuntimeError('MODEL_VERSION_CHANGED: model executable changed during execution')
    response=json.loads(raw)
    if not response.get('ok'):raise ValueError(f"native model rejected input: {response.get('error','invalid request')}")
    check()
    execution=dict(binary_sha256=identity,input_sha256=hashlib.sha256(data).hexdigest(),backend='rust',schema=schema)
    if controlled:
        public=dict(schema=schema,threads=threads,request=request)
        execution.update(input_sha256=hashlib.sha256(json.dumps(public,allow_nan=False,separators=(',',':')).encode()).hexdigest(),executed_input_sha256=hashlib.sha256(data).hexdigest(),native_timeout_ms=envelope['execution_timeout_ms'])
    return response['result'],execution
