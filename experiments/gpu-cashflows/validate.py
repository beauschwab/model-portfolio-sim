"""Focused real-device gates for the isolated prototype, including in-flight cancel."""
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time

import numpy as np
import numba
from portfolio_risk.core import quant_native

from benchmark import Gpu, ROOT, prepare, shapes_for, compare, sha


def watchdog_child(directory):
    gpu=Gpu()
    numba.set_num_threads(4)
    with redirect_stdout(io.StringIO()):
        prepared, _, _=prepare(512,256,shocks=(0,))
    a=prepared['mortgage']['0']
    gpu.lib.portfolio_gpu_reset_cancel()
    def mark_launched():
        end=time.monotonic()+10
        while gpu.lib.portfolio_gpu_launches()==0 and time.monotonic()<end:
            time.sleep(.001)
        if gpu.lib.portfolio_gpu_launches()>0:
            (directory/'launched').write_text('actual CUDA launch',encoding='utf-8')
    worker=threading.Thread(target=mark_launched)
    worker.start()
    gpu.call(4,a,shapes_for(4,a),repeats=20)
    worker.join(timeout=10)
    (directory/'result.json').write_text('completed',encoding='utf-8')


def watchdog_check():
    with tempfile.TemporaryDirectory(prefix='gpu-watchdog-') as tmp:
        directory=Path(tmp)
        process=subprocess.Popen([sys.executable,__file__,'--watchdog-child',tmp],
                                 stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,
                                 creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            end=time.monotonic()+30
            while not (directory/'launched').exists():
                if process.poll() is not None or time.monotonic()>=end:
                    raise RuntimeError('watchdog child did not reach CUDA launch')
                time.sleep(.005)
            process.kill()
            process.communicate(timeout=15)
            assert process.returncode != 0 and not (directory/'result.json').exists()
            return dict(gate='process_watchdog_after_actual_launch',passed=True,no_result_published=True)
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=15)


def main():
    gpu = Gpu()
    numba.set_num_threads(4)
    checks = []
    with redirect_stdout(io.StringIO()):
        prepared, _, _ = prepare(7, 8, shocks=(0,), seed=19)
    for product, op in [('mortgage',4),('deposit',6)]:
        a = prepared[product]['0']
        shapes = shapes_for(op,a)
        actual, _ = gpu.call(op,a,shapes)
        checks.append({'product':product,'gate':'odd_paths_separate_seed',
                       **compare(quant_native.call(op,a,shapes),actual)})
        again, _ = gpu.call(op,a,shapes)
        checks.append({'product':product,'gate':'repeat_exact',**compare(actual,again,rtol=0,atol=0)})
        if op == 4:
            exact = list(a); exact[24] = False
            output, _ = gpu.call(op,exact,shapes)
            checks.append({'product':product,'gate':'exact_sigmoid',
                           **compare(quant_native.call(op,exact,shapes),output)})
        checks.append({'product':product,'gate':'wrong_operation',
                       'error':gpu.call(99,a,shapes,expect_error=True)})
        checks.append({'product':product,'gate':'bad_output_shape',
                       'error':gpu.call(op,a,[(1,)]*len(shapes),expect_error=True)})
        oversized = list(a)
        oversized[13 if op == 4 else 6] = np.zeros(257)
        checks.append({'product':product,'gate':'contract_admission',
                       'error':gpu.call(op,oversized,shapes,expect_error=True)})
        oversized = list(a)
        oversized[:4] = [np.zeros((2049,1))]*4
        checks.append({'product':product,'gate':'path_admission',
                       'error':gpu.call(op,oversized,shapes,expect_error=True)})
        malformed = list(a)
        malformed[0] = np.zeros((7,359))
        checks.append({'product':product,'gate':'path_shape',
                       'error':gpu.call(op,malformed,shapes,expect_error=True)})
        if op == 4:
            malformed = list(a); malformed[4] = np.full(360,999.)
            checks.append({'product':product,'gate':'seasonality_index',
                           'error':gpu.call(op,malformed,shapes,expect_error=True)})

    with redirect_stdout(io.StringIO()):
        large, _, _ = prepare(512,256,shocks=(0,))
    a = large['mortgage']['0']; shapes = shapes_for(4,a)
    gpu.lib.portfolio_gpu_reset_cancel()
    cancellation_errors = []
    def cancel_after_launch():
        end = time.monotonic()+10
        while gpu.lib.portfolio_gpu_launches() == 0:
            if time.monotonic() >= end:
                cancellation_errors.append('no kernel launched')
                return
            time.sleep(.001)
        gpu.lib.portfolio_gpu_cancel()
    worker = threading.Thread(target=cancel_after_launch)
    worker.start()
    error = gpu.call(4,a,shapes,repeats=20,expect_error=True)
    worker.join(timeout=10)
    assert not worker.is_alive() and not cancellation_errors
    launches = gpu.lib.portfolio_gpu_launches()
    assert error == 'cancelled; output unpublished' and 0 < launches < 20
    checks.append(dict(gate='cancel_after_actual_launch',launches=launches,error=error))
    checks.append(watchdog_check())
    output, stats = gpu.call(4,a,shapes)
    checks.append({'gate':'healthy_after_cancel',**compare(quant_native.call(4,a,shapes),output)})
    assert stats['numeric_device_bytes'] < 128*1024*1024
    checks.append(dict(gate='bounded_numeric_device_allocation',bytes=stats['numeric_device_bytes']))
    report=dict(passed=True,checks=checks,gpu_binary_sha256=sha(gpu.path),
                caveat='Cancellation observed after a real launch; cooperative checks wait for the current bounded kernel. No production manifest or API integration.')
    out=ROOT/'docs/reviews/2026-10-01-gpu-cashflows-validation.json'
    out.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(dict(passed=True,checks=len(checks),report=str(out))))


if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='--watchdog-child':
        watchdog_child(Path(sys.argv[2]))
    else:
        main()
