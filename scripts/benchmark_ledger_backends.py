"""Fresh-process ledger comparison, including local Parquet round trips.

Run: uv run --project apps/api --with psutil python scripts/benchmark_ledger_backends.py
Three rotated-order samples per workload/backend. No API, S3 or product paths.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import threading
import time

import numpy as np
import polars as pl
import psutil

from portfolio_risk.analytics.balance_stress import run_balance_stress
from portfolio_risk.analytics.ledger_native import RustLedgerKernel, library_path, python_apply
from profile_balance_ledger import fixture, source_identity


def fingerprint(result):
    digest = hashlib.sha256()
    for key, value in sorted(result.items()):
        if key == 'execution':
            continue
        digest.update(key.encode())
        if isinstance(value, pl.DataFrame):
            digest.update(str(value.schema).encode())
            digest.update(value.hash_rows(seed=173).to_numpy().tobytes())
        else:
            digest.update(json.dumps(value, sort_keys=True).encode())
    return digest.hexdigest()


def sample(backend, count, days):
    spec = fixture(count, days)
    proc = psutil.Process()
    peak = [proc.memory_info().rss]
    baseline = peak[0]
    stop = threading.Event()
    def monitor():
        while not stop.wait(.005):
            peak[0] = max(peak[0], proc.memory_info().rss)
    thread = threading.Thread(target=monitor, daemon=True)
    thread.start()
    try:
        start = time.perf_counter()
        result = run_balance_stress(spec, journal_backend=backend)
        compute = time.perf_counter()-start
        with tempfile.TemporaryDirectory(prefix='ledger-bench-') as directory:
            artifacts = []
            io_start = time.perf_counter()
            for name, frame in result.items():
                if isinstance(frame, pl.DataFrame):
                    path = Path(directory)/(name+'.parquet')
                    frame.write_parquet(path)
                    artifacts.append(path)
                    restored = pl.read_parquet(path)
                    if not restored.equals(frame):
                        raise AssertionError(f'Parquet round-trip mismatch: {name}')
                    del restored
            io_seconds = time.perf_counter()-io_start
            artifact_bytes = sum(path.stat().st_size for path in artifacts)
        peak[0] = max(peak[0], proc.memory_info().rss)
    finally:
        stop.set(); thread.join()
    return dict(backend=backend, positions=count, days=days, scenarios=2,
                compute_seconds=compute, parquet_roundtrip_seconds=io_seconds,
                compute_plus_io_seconds=compute+io_seconds,
                baseline_rss_mib=baseline/2**20, peak_rss_mib=peak[0]/2**20,
                journal_rows=result['journal'].height, journal_mib=result['journal'].estimated_size()/2**20,
                parquet_mib=artifact_bytes/2**20, output_sha256=fingerprint(result),
                execution=result['execution'])


def kernel_samples():
    n = 100_000
    offsets = np.arange(n+1, dtype=np.int64)*2
    keys = np.tile(np.array([0, 1], dtype=np.int64), n)
    values = np.tile(np.array([1., -1.]), n)
    expected = np.array([float(n), -float(n)])
    previous = np.zeros(2)
    kernels = {'columnar': python_apply, 'rust': RustLedgerKernel()}
    result = {}
    for name, kernel in kernels.items():
        samples = []
        for _ in range(3):
            start = time.perf_counter()
            actual = kernel(previous, offsets, keys, values, expected)
            samples.append(time.perf_counter()-start)
            assert np.array_equal(actual, expected)
        result[name] = dict(samples_seconds=samples, median_seconds=statistics.median(samples))
    return dict(transactions=n, lines=2*n, scope='prepared buffers; Rust adapter copies included; no event construction/output conversion', backends=result)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--child', action='store_true')
    parser.add_argument('--backend', choices=['python', 'columnar', 'rust'])
    parser.add_argument('--positions', type=int, default=2000)
    parser.add_argument('--days', type=int, default=30)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--output', default='docs/reviews/2026-09-29-ledger-native-benchmark.json')
    args = parser.parse_args()
    if args.child:
        print(json.dumps(sample(args.backend, args.positions, args.days)))
        return
    initial_source = source_identity()
    initial_binary = hashlib.sha256(library_path().read_bytes()).hexdigest()
    matrix = [(500, 30), (2000, 30), (2000, 180)]
    raw = []
    names = ['python', 'columnar', 'rust']
    for count, days in matrix:
        for repeat in range(args.repeats):
            for backend in names[repeat % 3:]+names[:repeat % 3]:
                completed = subprocess.run([sys.executable, __file__, '--child', '--backend', backend,
                                            '--positions', str(count), '--days', str(days)],
                                           capture_output=True, text=True, check=True)
                row = json.loads(completed.stdout)
                row['repeat'] = repeat
                raw.append(row)
                print(f'{count} positions / {days} days / {backend}: {row["compute_plus_io_seconds"]:.3f}s, {row["peak_rss_mib"]:.1f} MiB', flush=True)
    summaries = []
    for count, days in matrix:
        group = [r for r in raw if (r['positions'], r['days']) == (count, days)]
        assert len({r['output_sha256'] for r in group}) == 1, 'backend output mismatch'
        for backend in names:
            subset = [r for r in group if r['backend'] == backend]
            summaries.append(dict(positions=count, days=days, backend=backend,
                **{key: statistics.median(r[key] for r in subset) for key in
                   ['compute_seconds', 'parquet_roundtrip_seconds', 'compute_plus_io_seconds', 'peak_rss_mib', 'journal_rows', 'journal_mib', 'parquet_mib']},
                min_seconds=min(r['compute_plus_io_seconds'] for r in subset),
                max_seconds=max(r['compute_plus_io_seconds'] for r in subset)))
    kernels = kernel_samples()
    assert source_identity() == initial_source, 'engine source changed during benchmark'
    assert hashlib.sha256(library_path().read_bytes()).hexdigest() == initial_binary, 'native binary changed during benchmark'
    report = dict(scope='synthetic accrual ledger; daily checks, closing replay, conversion and local Parquet write/read verification; excludes product paths, durable API publication, S3, process startup',
        python=platform.python_version(), machine=platform.platform(), source_sha256=initial_source,
        binary_sha256=initial_binary, source_unchanged=True, repeats=args.repeats,
        output_parity='exact frame-schema and ordered-row hash equality, metadata excluding execution identity',
        summary=summaries, kernel=kernels, samples=raw)
    Path(args.output).write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(summaries, indent=2))


if __name__ == '__main__':
    main()
