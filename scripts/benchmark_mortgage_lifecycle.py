"""Compare complete mortgage spot-risk and forward-stress drivers in fresh processes.

Synthetic mortgage workload only, not the full mixed-book/solver/ledger benchmark.
Imports and fixture generation are excluded from timings; first-call JIT is included.
Same-process warm calls share one run context. A final notional-only edit exposes
recomputation costs; neither backend is assumed to own a persistent result cache.
"""
import argparse
from contextlib import redirect_stdout
import hashlib
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

ROOT = Path(__file__).resolve().parents[1]


def child(args):
    import numba
    import numpy as np
    import polars as pl
    from benchmark_comparison import memory
    from portfolio_risk import __version__, demo
    from portfolio_risk.analytics.risk import run_risk
    from portfolio_risk.analytics.stress import run_stress
    from portfolio_risk.core.native import library_path
    from portfolio_risk.core.runtime import RunConfig, run_context

    numba.set_num_threads(args.threads)
    book = demo.demo_portfolio(args.positions)
    market = (*demo.demo_market(), *demo.demo_histories())
    edited = book.with_columns(pl.when(pl.col('cusip') == book['cusip'][0])
        .then(pl.col('current_face')*1.01).otherwise(pl.col('current_face')).alias('current_face'))
    config = RunConfig(args.paths, args.paths*2, args.horizon, compute_backend=args.child)
    baseline_memory = memory()
    timings, financial = [], {}
    with redirect_stdout(io.StringIO()), run_context(config) as ctx:
        for index in range(args.repeats+2):
            phase = 'cold' if index == 0 else ('edited' if index == args.repeats+1 else f'warm_{index}')
            positions = edited if phase == 'edited' else book
            start = time.perf_counter()
            risk = run_risk(positions, *market, seed=27)
            middle = time.perf_counter()
            stress = run_stress(positions, *market, shocks_bp=[-100.,0.,100.], seed=27)
            end = time.perf_counter()
            timings.append(dict(phase=phase, risk_seconds=middle-start, stress_seconds=end-middle,
                total_seconds=end-start, python_market_cache_hits=ctx.hits, python_market_cache_misses=ctx.misses))
            if args.child == 'rust':
                from portfolio_risk.core.quant_native import mortgage_cache_statistics
                timings[-1]['native_market_cache'] = mortgage_cache_statistics()
            # Retain every timed result so warm and edited outputs are parity-gated.
            for name, frame in [('risk', risk), ('positions',stress[0]), ('aggregate',stress[1]), ('profile',stress[2])]:
                columns = [c for c in frame.columns if c not in ('cusip','state','channel')]
                financial[f'{phase}/{name}'] = frame[columns].to_numpy().tolist()
    result = dict(backend=args.child, version=__version__, positions=args.positions,
        sensitivity_paths=args.paths, base_paths=args.paths*2, horizon=args.horizon,
        threads=args.threads, timings=timings,
        warm_median_seconds=statistics.median(t['total_seconds'] for t in timings if t['phase'].startswith('warm_')),
        baseline_memory=baseline_memory, final_memory=memory(), financial=financial)
    if args.child == 'rust':
        result['native_library_sha256'] = hashlib.sha256(library_path().read_bytes()).hexdigest()
    Path(args.output).write_text(json.dumps(result,allow_nan=False),encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--positions',type=int,default=256)
    parser.add_argument('--paths',type=int,default=8)
    parser.add_argument('--horizon',type=int,default=3)
    parser.add_argument('--threads',type=int,default=4)
    parser.add_argument('--repeats',type=int,default=3)
    parser.add_argument('--child',choices=['python','rust'])
    parser.add_argument('--output')
    args = parser.parse_args()
    if min(args.positions,args.paths,args.threads,args.repeats) < 1 or not 1 <= args.horizon < 360:
        parser.error('positive dimensions/repeats and horizon 1..359 required')
    if args.child:
        return child(args)
    results = []
    with tempfile.TemporaryDirectory(prefix='mortgage-lifecycle-bench-') as tmp:
        for backend in ('python','rust'):
            out = Path(tmp)/f'{backend}.json'
            command = [sys.executable,__file__,'--child',backend,'--output',str(out)]
            for name in ('positions','paths','horizon','threads','repeats'):
                command += ['--'+name,str(getattr(args,name))]
            process = subprocess.run(command,capture_output=True,text=True,
                env=os.environ|{'NUMBA_NUM_THREADS':str(args.threads)},
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
            if process.returncode:
                raise RuntimeError(process.stdout+process.stderr)
            results.append(json.loads(out.read_text(encoding='utf-8')))
            print(backend,'completed',flush=True)
    import numpy as np
    reference, native = [r.pop('financial') for r in results]
    errors = {}
    for name, values in reference.items():
        a,b = np.asarray(values),np.asarray(native[name])
        np.testing.assert_allclose(a,b,rtol=1e-7,atol=1e-5,err_msg=name)
        errors[name] = float(np.max(np.abs(a-b)))
    report = dict(scope='Mortgage risk and forward stress only; excludes other books, strategy solver and daily ledger',
        synthetic=True,platform=platform.platform(),python=platform.python_version(),results=results,
        tolerance=dict(rtol=1e-7,atol=1e-5),parity=True,max_absolute_errors=errors,
        script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        memory_note='OS peak process memory includes imports, JIT, output retention and transport, not just Rust allocation; not a standalone-Rust memory benchmark')
    output = Path(args.output) if args.output else ROOT/'docs/reviews/2026-09-30-rust-mortgage-lifecycle-benchmark.json'
    output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(output)


if __name__ == '__main__':
    main()
