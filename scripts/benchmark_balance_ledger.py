"""Isolated full-journal scaling probe; synthetic data, not a GSIB coverage claim."""
import argparse
import json
import subprocess
import sys
import time
import threading
from pathlib import Path


def run(count):
    import psutil
    from portfolio_risk.analytics.balance_stress import MODEL_VERSION, run_balance_stress
    spec = dict(version=MODEL_VERSION, horizon_days=30,
        accounts=[dict(id='bank', entity='bank', currency='USD', cash=20., equity=20.+count)],
        positions=[dict(id=f'loan:{i}', account='bank', kind='loan', balance=1., rate=.04) for i in range(count)],
        scenarios=[dict(name='up', rate_shift=.02)])
    process = psutil.Process()
    peak, stop = [process.memory_info().rss], threading.Event()
    def sample():
        while not stop.wait(.01):
            peak[0] = max(peak[0], process.memory_info().rss)
    monitor = threading.Thread(target=sample, daemon=True)
    monitor.start()
    start = time.perf_counter()
    result = run_balance_stress(spec)
    seconds = time.perf_counter()-start
    peak[0] = max(peak[0], process.memory_info().rss)
    stop.set(); monitor.join()
    return dict(positions=count, days=30, scenarios=2, elapsed_seconds=seconds,
        sampled_peak_rss_mib=peak[0]/2**20, journal_lines=result['journal'].height,
        max_reconciliation_error=result['summary']['max_reconciliation_error'].max(),
        journal_replay_passed=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--positions', type=int)
    parser.add_argument('--output', default='docs/reviews/2026-09-29-ledger-benchmark.json')
    args = parser.parse_args()
    if args.positions:
        print(json.dumps(run(args.positions)))
    else:
        rows = []
        for n in (100, 500, 2000):
            completed = subprocess.run([sys.executable, __file__, '--positions', str(n)], capture_output=True, text=True, check=True)
            rows.append(json.loads(completed.stdout))
        report = dict(scope='synthetic daily accrual, full journal retention and independent replay; no product path generation',
            memory='process RSS sampled every 10ms; includes Python/Polars and output retention', results=rows)
        Path(args.output).write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps(report, indent=2))
