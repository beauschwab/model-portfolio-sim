"""Actual T1000 experiment using unchanged prepared inputs and production Rust.

Run via uv --project apps/api --with nvidia-cuda-nvrtc-cu12==12.9.86.
The CUDA implementation is experimental; no production selection is changed.
"""
import argparse
import ctypes
from contextlib import redirect_stdout
import hashlib
import importlib.metadata
import io
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class Gpu:
    def __init__(self):
        import numpy as np
        from portfolio_risk.core.quant_native import Buffer
        self.np, self.Buffer = np, Buffer
        nvrtc = importlib.metadata.distribution('nvidia-cuda-nvrtc-cu12')
        self.nvrtc_version = nvrtc.version
        directory = nvrtc.locate_file('nvidia/cuda_nvrtc/bin')
        self.dll_directory = os.add_dll_directory(str(directory))
        self.builtins = ctypes.CDLL(str(directory / 'nvrtc-builtins64_129.dll'))
        self.nvrtc = ctypes.CDLL(str(directory / 'nvrtc64_120_0.dll'))
        self.path = HERE / 'target/release/portfolio_gpu_experiment.dll'
        self.lib = ctypes.CDLL(str(self.path))
        self.lib.portfolio_gpu_initialize.restype = ctypes.c_int
        self.lib.portfolio_gpu_cancel.argtypes = []
        self.lib.portfolio_gpu_reset_cancel.argtypes = []
        self.lib.portfolio_gpu_launches.argtypes = []
        self.lib.portfolio_gpu_launches.restype = ctypes.c_uint32
        self.lib.portfolio_gpu_error.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        self.lib.portfolio_gpu_error.restype = ctypes.c_size_t
        self.invoke = self.lib.portfolio_gpu_call
        self.invoke.argtypes = [ctypes.c_uint32, ctypes.POINTER(Buffer), ctypes.c_size_t,
                               ctypes.POINTER(Buffer), ctypes.c_size_t, ctypes.c_uint32,
                               ctypes.c_uint64, ctypes.POINTER(ctypes.c_double)]
        self.invoke.restype = ctypes.c_int
        start = time.perf_counter()
        if self.lib.portfolio_gpu_initialize():
            raise RuntimeError(self.error())
        self.initialization_seconds = time.perf_counter() - start

    def error(self):
        buf = ctypes.create_string_buffer(16384)
        self.lib.portfolio_gpu_error(buf, len(buf))
        return buf.value.decode(errors='replace')

    def descriptors(self, arrays):
        return (self.Buffer * len(arrays))(*(self.Buffer(
            a.ctypes.data_as(ctypes.POINTER(ctypes.c_double)), a.size,
            (ctypes.c_size_t * 3)(*a.shape, *([1] * (3-a.ndim)))) for a in arrays))

    def call(self, op, inputs, shapes, repeats=1, deadline_ms=30000, expect_error=False):
        np = self.np
        start = time.perf_counter()
        self.lib.portfolio_gpu_reset_cancel()
        arrays = [np.require(np.atleast_1d(v), dtype=np.float64, requirements=['C', 'A']) for v in inputs]
        outputs = [(np.full(s, -9876543., dtype=np.float64) if expect_error
                    else np.empty(s,dtype=np.float64)) for s in shapes]
        timing = np.full(6, -1., dtype=np.float64)
        status = self.invoke(op, self.descriptors(arrays), len(arrays),
                             self.descriptors(outputs), len(outputs), repeats, deadline_ms,
                             timing.ctypes.data_as(ctypes.POINTER(ctypes.c_double)))
        elapsed = time.perf_counter() - start
        if expect_error:
            assert status and all(np.all(a == -9876543.) for a in outputs)
            assert np.all(timing == -1.)
            return self.error()
        if status:
            raise RuntimeError(self.error())
        names = ['device_kernel_seconds', 'rust_call_seconds', 'input_bytes',
                 'scratch_bytes', 'compact_output_bytes', 'numeric_device_bytes']
        return tuple(outputs), dict(zip(names, timing.tolist())) | {'python_call_seconds': elapsed}


def capture(fn):
    captured = []
    fn(captured)
    assert len(captured) == 1
    return tuple(captured[0])


def shapes_for(op, a):
    p, t = a[0].shape
    n = len(a[13 if op == 4 else 6])
    h = len(a[22 if op == 4 else 17])
    return ([(n,t),(n,h),(n,h),(n,p,h),(n,p,h),(n,t),(n,t)] if op == 4
            else [(n,t),(n,t),(n,h),(n,h),(n,p,h),(n,t)])


def prepare(paths, positions, shocks=(0, -1, 1, -200, 200), seed=7):
    import numpy as np
    from portfolio_risk import demo
    from portfolio_risk.core import scenarios
    from portfolio_risk.core.runtime import RunConfig, run_context
    from portfolio_risk.products import deposits
    with run_context(RunConfig(paths, paths, 27, compute_backend='rust')):
        port = demo.demo_portfolio(positions)
        sr, vp = demo.demo_market()
        cc, ps = demo.demo_histories()
        models, b, abcd, sec, target, balance = scenarios.setup(port, sr, vp, cc, ps)
        crn = scenarios.CRN(paths, seed)
        dbook = demo.demo_deposit_book(positions)
        deck = deposits.DepositDeck(dbook)
        model = deposits.LogisticBetaECM()
        params = model.fit(demo.demo_deposit_history())
        prepared = {'mortgage': {}, 'deposit': {}}
        r0 = None
        for shock in shocks:
            market = scenarios.build_paths(sr + shock / 10000., vp, abcd, b, models, crn)
            if r0 is None:
                r0 = float(model.equilibrium(params, market['short'][:, 0].mean()))
            dep = model.paths(market['short'].astype(float), params, r0)
            def mortgage(captured):
                with patch.object(scenarios, 'engine', side_effect=lambda *a: captured.append(a)):
                    scenarios.run_engine(market, sec)
            def deposit(captured):
                with patch.object(deposits, 'deposit_engine', side_effect=lambda *a: captured.append(a)):
                    deposits._deposit_A(deck, market, dep, r0)
            prepared['mortgage'][str(shock)] = capture(mortgage)
            prepared['deposit'][str(shock)] = capture(deposit)
        identities = dict(seed=seed, rate_draw_sha256=hashlib.sha256(crn.Z.tobytes()).hexdigest(),
                          spread_draw_sha256=hashlib.sha256(crn.eps_ps.tobytes()).hexdigest(),
                          hpi_draw_sha256=hashlib.sha256(crn.eps_h.tobytes()).hexdigest())
    return prepared, {'mortgage': (target, balance), 'deposit': (deck.tgt, deck.bal)}, identities


def compare(a, b, rtol=1e-10, atol=1e-10):
    import numpy as np
    maximum = 0.
    cells = 0
    for x, y in zip(a, b):
        assert x.shape == y.shape
        np.testing.assert_allclose(x, y, rtol=rtol, atol=atol)
        if x.size:
            maximum = max(maximum, float(np.max(np.abs(x-y))))
        cells += x.size
    return dict(cells=cells, max_absolute_error=maximum, rtol=rtol, atol=atol, passed=True)


def finance(op, a, outputs, target, balance, held_oas=None, held_yield=None):
    """Use production Rust OAS/PV and existing frozen-yield accounting conventions."""
    import numpy as np
    from portfolio_risk.core import quant_native
    n, t = outputs[0].shape
    p = a[0].shape[0]
    offsets = np.arange(n+1)*t
    times = np.broadcast_to((np.arange(t)+1)/12., (n,t)).reshape(-1)
    if held_oas is None:
        held_oas, px = quant_native.csr_solve(offsets, times, outputs[0].ravel(), target, p,
                                            lo=-.15 if op == 6 else -.05)
    else:
        px = quant_native.csr_price(offsets, times, outputs[0].ravel(), held_oas, p)
    interest = outputs[5]
    principal = outputs[6] if op == 4 else outputs[1]
    cf = (interest + principal) / p
    if op == 4:
        if held_yield is None:
            held_yield = quant_native.call(14, [cf, target, 0, 60], [(n,0),(n,0),(n,)])[2]
        bv = target.copy()
        income = np.empty((n,27))
        for m in range(27):
            income[:,m] = bv*held_yield/12.
            bv = bv+income[:,m]-cf[:,m]
    else:
        income = -interest[:,:27]/p
    return {'pv_$': px*balance, 'nii_$': income*balance[:,None],
            'runoff_$': principal[:,:27]/p*balance[:,None]}, held_oas, held_yield


def child(args):
    import numba
    import numpy as np
    from portfolio_risk.core import quant_native
    gpu = Gpu()
    numba.set_num_threads(args.threads)
    start = time.perf_counter()
    with redirect_stdout(io.StringIO()):
        prepared, books, identity = prepare(args.paths, args.positions)
    setup_seconds = time.perf_counter() - start
    report = dict(paths=args.paths, positions_per_product=args.positions, months=360,
                  threads=args.threads, initialization_seconds=gpu.initialization_seconds,
                  shared_preparation_seconds=setup_seconds, shared_draws=identity, results=[])
    for product, op in [('mortgage', 4), ('deposit', 6)]:
        a = prepared[product]['0']
        shapes = shapes_for(op,a)
        expected = quant_native.call(op,a,shapes)
        actual, _ = gpu.call(op,a,shapes)
        parity = compare(expected,actual)
        # Independent Python kernel, same inputs; compilation excluded from timings.
        from portfolio_risk.core import kernels
        from portfolio_risk.products import deposits
        reference = kernels.engine.__wrapped__ if op == 4 else deposits.deposit_engine.__wrapped__
        python_reference = reference(*a)
        independent_parity = compare(python_reference,actual)
        cpu, cuda = [], []
        for repetition in range(args.repeats):
            # Alternate order to reduce temperature/order bias.
            order = ('cpu','gpu') if repetition % 2 == 0 else ('gpu','cpu')
            for backend in order:
                start = time.perf_counter()
                if backend == 'cpu':
                    quant_native.call(op,a,shapes)
                    cpu.append(time.perf_counter()-start)
                else:
                    _, stats = gpu.call(op,a,shapes)
                    cuda.append(stats)
        resident_out, resident = gpu.call(op,a,shapes,repeats=args.repeats)
        compare(actual,resident_out,rtol=0,atol=0)
        target, balance = books[product]
        financial, pipeline_samples = {}, {'cpu': [], 'gpu': []}
        for repetition in range(3):
            order = ('cpu','gpu') if repetition % 2 == 0 else ('gpu','cpu')
            for backend in order:
                start = time.perf_counter()
                values = {}; held_oas = held_yield = None
                for shock, inputs in prepared[product].items():
                    output = (quant_native.call(op,inputs,shapes) if backend == 'cpu'
                              else gpu.call(op,inputs,shapes)[0])
                    values[shock], held_oas, held_yield = finance(
                        op,inputs,output,target,balance,held_oas,held_yield)
                values['dv01_$'] = (values['-1']['pv_$']-values['1']['pv_$'])/2.
                values['base_oas'] = held_oas
                financial[backend] = values
                pipeline_samples[backend].append(time.perf_counter()-start)
        pipeline = {key: statistics.median(values) for key, values in pipeline_samples.items()}
        finance_a, finance_b = [], []
        for key in financial['cpu']:
            x, y = financial['cpu'][key], financial['gpu'][key]
            if isinstance(x,dict):
                for field in x:
                    finance_a.append(x[field]); finance_b.append(y[field])
            else:
                finance_a.append(x); finance_b.append(y)
        finance_parity = compare(finance_a,finance_b,rtol=1e-7,atol=1e-5)
        failures = {'expired_deadline': gpu.call(op,a,shapes,deadline_ms=0,expect_error=True)}
        invalid = list(a); invalid[23 if op == 4 else 18] = True
        failures['unsupported_forward'] = gpu.call(op,invalid,shapes,expect_error=True)
        invalid = list(a); invalid[0] = np.array(a[0],dtype=float,copy=True); invalid[0][0,0] = np.nan
        failures['nonfinite_input'] = gpu.call(op,invalid,shapes,expect_error=True)
        cpu_median = statistics.median(cpu)
        gpu_median = statistics.median(s['python_call_seconds'] for s in cuda)
        result = dict(product=product,cashflow_parity=parity,independent_python_parity=independent_parity,
                      final_financial_parity=finance_parity,cpu_seconds=cpu,gpu_samples=cuda,
                      cpu_median_seconds=cpu_median,gpu_median_seconds=gpu_median,
                      call_speedup=cpu_median/gpu_median,resident=resident,
                      prepared_five_scenario_pipeline_seconds=pipeline,
                      pipeline_samples_seconds=pipeline_samples,
                      pipeline_speedup=pipeline['cpu']/pipeline['gpu'],atomic_failure_checks=failures)
        report['results'].append(result)
        print(json.dumps(dict(paths=args.paths,threads=args.threads,product=product,
                              speedup=result['call_speedup'],pipeline_speedup=result['pipeline_speedup'],
                              parity=True)),flush=True)
    Path(args.output).write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--child',action='store_true')
    parser.add_argument('--paths',type=int,default=128)
    parser.add_argument('--positions',type=int,default=256)
    parser.add_argument('--threads',type=int,default=4)
    parser.add_argument('--repeats',type=int,default=3)
    parser.add_argument('--timeout',type=float,default=240)
    parser.add_argument('--matrix-paths',nargs='+',type=int,default=[128,512,2048])
    parser.add_argument('--matrix-threads',nargs='+',type=int,default=[4])
    parser.add_argument('--output',default=str(ROOT/'docs/reviews/2026-10-01-gpu-cashflows.json'))
    args=parser.parse_args()
    if args.child:
        child(args)
        return
    from portfolio_risk.core.native import library_path
    device = subprocess.check_output(['nvidia-smi','--query-gpu=name,memory.total,driver_version,compute_cap',
                                      '--format=csv,noheader'],text=True).strip()
    report = dict(synthetic=True,device=device,platform=platform.platform(),results=[],
                  cpu_logical_processors=os.cpu_count(),
                  nvrtc_version=importlib.metadata.version('nvidia-cuda-nvrtc-cu12'),
                  source_sha256={str(p.relative_to(ROOT)):sha(p) for p in
                                 [Path(__file__),HERE/'src/lib.rs',HERE/'src/cashflows.cu',HERE/'Cargo.lock']},
                  gpu_binary_sha256=sha(HERE/'target/release/portfolio_gpu_experiment.dll'),
                  cpu_binary_sha256=sha(library_path()),
                  scope='Prepared base mortgage/deposit cashflows and five-scenario PV/DV01/frozen-yield NII stage; excludes API, ledger, artifacts, full KRD/vega and forward capture',
                  cpu_timing='Python adapter plus production Rust FFI, allocations and output copy',
                  gpu_timing='Python adapter plus Rust validation, transpose/packing, device allocation, upload, kernel, download and output copy; initialization separate',
                  resident_timing='CUDA events for repeated kernels on retained buffers; not end-to-end speedup')
    with tempfile.TemporaryDirectory(prefix='portfolio-gpu-experiment-') as tmp:
        for threads in args.matrix_threads:
            for paths in args.matrix_paths:
                out = Path(tmp)/f'{threads}-{paths}.json'
                command=[sys.executable,__file__,'--child','--threads',str(threads),'--paths',str(paths),
                         '--positions',str(args.positions),'--repeats',str(args.repeats),'--output',str(out)]
                completed=subprocess.run(command,capture_output=True,text=True,timeout=args.timeout,
                                         creationflags=subprocess.CREATE_NO_WINDOW)
                print(completed.stdout,flush=True,end='')
                if completed.returncode:
                    raise RuntimeError(completed.stderr)
                report['results'].append(json.loads(out.read_text()))
    destination = Path(args.output)
    destination.parent.mkdir(parents=True,exist_ok=True)
    staging = destination.with_suffix('.json.tmp')
    staging.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')
    staging.replace(destination)
    print(destination,flush=True)


if __name__=='__main__':
    main()
