"""Bounded standalone product allocation transport; Rust/HiGHS own economics."""
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


def binary_path():
    name='portfolio-strategy.exe' if sys.platform=='win32' else 'portfolio-strategy'
    return Path(__file__).resolve().parents[4]/'portfolio-decision-native'/'target'/'release'/name


def binary_identity():
    path=binary_path()
    if not path.is_file():raise RuntimeError('Rust product strategy runtime requires rebuilding; no fallback')
    digest=hashlib.sha256()
    with path.open('rb') as handle:
        for block in iter(lambda:handle.read(1024*1024),b''):digest.update(block)
    return digest.hexdigest()


def run(request,*,threads=1,expected_binary=None,cancelled=None,timeout=1800.):
    if request.get('op')!='optimize_product' or request.get('schema')!='strategy-product-1':
        raise ValueError('explicit native product strategy contract required')
    return _run(request,schema='strategy-product-1',command=request,threads=threads,
        expected_binary=expected_binary,cancelled=cancelled,timeout=timeout)


def _run(request,*,schema,command,threads=1,expected_binary=None,cancelled=None,timeout=1800.):
    if schema not in {'strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1','mortgage-closing-capital-1','modeled-product-ftp-preparation-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1'}:
        raise ValueError('explicit bounded strategy process contract required')
    if type(threads) is not int or not 1<=threads<=256:raise ValueError('invalid strategy thread count')
    if not math.isfinite(timeout) or not .001<=timeout<=1800.:raise ValueError('invalid strategy deadline')
    envelope=json.dumps(dict(schema=schema,threads=threads,request=request),allow_nan=False,separators=(',',':')).encode()
    data=json.dumps(command,allow_nan=False,separators=(',',':')).encode()
    if max(len(envelope),len(data))>MAX_INPUT:raise ValueError('product strategy request exceeds 32 MiB')
    if schema in {'commercial-holding-journal-1','commercial-fee-journal-1'} and max(len(envelope),len(data))>2*1024*1024:raise ValueError('holding journal request exceeds2MiB')
    output_limit=32*1024*1024 if schema in {'commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1'} else MAX_OUTPUT
    identity=binary_identity()
    if expected_binary is not None and identity!=expected_binary:
        raise RuntimeError('MODEL_VERSION_CHANGED: product strategy executable differs from queued identity')
    deadline=time.monotonic()+timeout
    def check():
        if cancelled and cancelled():raise InterruptedError('product strategy cancelled or worker lease lost')
        if time.monotonic()>=deadline:raise TimeoutError('product strategy deadline exceeded')
    check()
    with scratch_directory(prefix='portfolio-product-strategy-') as scratch:
        folder=Path(scratch);input_path=folder/'input.json';output_path=folder/'output.json';error_path=folder/'error.txt'
        input_path.write_bytes(data)
        with input_path.open('rb') as stdin,output_path.open('w+b') as stdout,error_path.open('w+b') as stderr:
            child=subprocess.Popen([str(binary_path())],stdin=stdin,stdout=stdout,stderr=stderr)
            try:
                while child.poll() is None:
                    check()
                    if output_path.stat().st_size>output_limit or error_path.stat().st_size>65536:
                        raise ValueError('product strategy output exceeded bounded transport')
                    try:child.wait(timeout=min(.1,max(.001,deadline-time.monotonic())))
                    except subprocess.TimeoutExpired:pass
                check()
                if error_path.stat().st_size>65536:raise ValueError('product strategy error output exceeded bounded transport')
                stdout.seek(0);raw=stdout.read(output_limit+1)
                if len(raw)>output_limit:raise ValueError('product strategy output exceeded bounded transport')
                if child.returncode not in {0,2}:raise RuntimeError('product strategy process failed; no fallback')
            finally:
                if child.poll() is None:child.kill()
                child.wait()
    if binary_identity()!=identity:raise RuntimeError('MODEL_VERSION_CHANGED: product strategy executable changed during execution')
    response=json.loads(raw)
    if 'error' in response:raise ValueError(f"native product strategy rejected input: {response['error']}")
    if not isinstance(response.get('ok'),dict):raise ValueError('invalid native product strategy response')
    check()
    return response['ok'],dict(binary_sha256=identity,input_sha256=hashlib.sha256(envelope).hexdigest(),
        executed_input_sha256=hashlib.sha256(data).hexdigest(),backend='rust',schema=schema)
