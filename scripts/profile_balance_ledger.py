"""Profile the full daily ledger to choose a native boundary from measured work.

Run with: uv run --project apps/api --with psutil python scripts/profile_balance_ledger.py
Timings are synthetic research measurements, not production latency promises.
"""
import argparse
import cProfile
import gc
import hashlib
import json
from pathlib import Path
import platform
import pstats
import statistics
import threading
import time

import psutil
from portfolio_risk.analytics import balance_stress


def fixture(count, days):
    return dict(version=balance_stress.MODEL_VERSION, horizon_days=days,
        accounts=[dict(id='bank', entity='bank', currency='USD', cash=20., equity=20.+count)],
        positions=[dict(id=f'loan:{i}', account='bank', kind='loan', balance=1., rate=.04, floating_beta=1.) for i in range(count)],
        scenarios=[dict(name='rates_up', rate_shift=.02)])


def source_identity():
    engine_root = Path(balance_stress.__file__).resolve().parents[1]
    digest = hashlib.sha256()
    for path in sorted(engine_root.rglob('*.py')):
        digest.update(path.relative_to(engine_root).as_posix().encode())
        digest.update(path.read_bytes().replace(b'\r\n', b'\n'))
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--positions', type=int, default=2000)
    parser.add_argument('--days', type=int, default=30)
    parser.add_argument('--output', default='docs/reviews/2026-09-29-ledger-profile.json')
    args = parser.parse_args()
    initial_source = source_identity()
    spec = fixture(args.positions, args.days)
    process = psutil.Process()
    samples, rows, frame_bytes = [], 0, 0
    stop = threading.Event()
    baseline = process.memory_info().rss
    peak = [baseline]
    def sample():
        while not stop.wait(.01):
            peak[0] = max(peak[0], process.memory_info().rss)
    monitor = threading.Thread(target=sample, daemon=True)
    monitor.start()
    try:
        for _ in range(3):
            gc.collect()
            start = time.perf_counter()
            result = balance_stress.run_balance_stress(spec)
            samples.append(time.perf_counter()-start)
            rows = result['journal'].height
            frame_bytes = result['journal'].estimated_size()
            assert result['summary']['max_reconciliation_error'].max() < 1e-7
            del result
    finally:
        peak[0] = max(peak[0], process.memory_info().rss)
        stop.set(); monitor.join()
    gc.collect()
    profiler = cProfile.Profile()
    profiler.enable()
    result = balance_stress.run_balance_stress(spec)
    profiler.disable()
    stats = pstats.Stats(profiler)
    functions = []
    groups = {}
    for (file, line, function), (primitive, calls, own, cumulative, callers) in stats.stats.items():
        filename = Path(file).name
        group = ('journal' if filename == 'journal.py' else
                 'daily_simulation' if filename == 'balance_stress.py' else
                 'columnar_conversion' if 'polars' in file else 'other')
        groups[group] = groups.get(group, 0.)+own
        functions.append(dict(file=filename, line=line, function=function, calls=calls,
                              self_seconds=own, cumulative_seconds=cumulative))
    if source_identity() != initial_source:
        raise RuntimeError('Engine changed during measurement; rerun on a stable source snapshot')
    report = dict(scope='2 scenarios; synthetic daily accrual, full journal retention, all daily checks and independent closing replay; no product path generation or persistence',
        positions=args.positions, days=args.days, scenarios=2, python=platform.python_version(),
        source_sha256=initial_source, source_unchanged=True, wall_samples_seconds=samples,
        wall_median_seconds=statistics.median(samples), journal_rows=rows,
        journal_columnar_mib=frame_bytes/2**20, baseline_rss_mib=baseline/2**20,
        sampled_peak_rss_mib=peak[0]/2**20,
        profile_total_seconds=stats.total_tt,
        self_time_groups={k: dict(seconds=v, percent=100*v/stats.total_tt) for k,v in groups.items()},
        top_self=sorted(functions, key=lambda r:r['self_seconds'], reverse=True)[:20],
        top_cumulative=sorted(functions, key=lambda r:r['cumulative_seconds'], reverse=True)[:20],
        cautions=['cProfile adds overhead; use unprofiled samples for elapsed time',
                  'Module self-time groups exclude builtin time attributed to other; cumulative times overlap',
                  'RSS spans three sequential samples and includes allocator retention; not a cold-process peak',
                  'No Rust ledger implementation is measured; this identifies candidate work only'])
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps({k:v for k,v in report.items() if k not in {'top_self','top_cumulative'}}, indent=2))


if __name__ == '__main__':
    main()
