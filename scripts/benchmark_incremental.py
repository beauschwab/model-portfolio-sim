# Historical Python/reference benchmark. Production execution requires Rust.
"""End-to-end synthetic pricing graph benchmark, excluding HTTP/serialization.

uv run --project apps/api python scripts/benchmark_incremental.py
Includes fingerprinting, dependency lookup, pricing, frames and totals. JIT is
warmed first. Each incremental result is checked against a fresh graph rebuild.
"""
from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import platform
import statistics
import time

import numba
import numpy as np
import polars as pl

from portfolio_risk.analytics.incremental import price_books, SUPPORTED_BOOKS
from portfolio_risk.core.dependency import DependencyCache
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.demo import model_balance_sheet, demo_market, demo_deposit_history


def run_benchmark():
    numba.set_num_threads(min(4, numba.config.NUMBA_NUM_THREADS))
    bs = model_balance_sheet(scale=.01)
    sr, vp = demo_market()
    books = {name: bs[name] for name in SUPPORTED_BOOKS}
    inputs = dict(books=books, asof=bs['asof'], swap_rates=sr, vol_pts=vp,
                  mbs_hists=bs['mbs_hists'], dep_hist=demo_deposit_history(),
                  config=RunConfig(128, 128, compute_backend='python'), seed=7)
    ident = books['loans']['id'][0]
    cache = DependencyCache()

    def run(args, cache):
        with redirect_stdout(io.StringIO()):
            start = time.perf_counter()
            out = price_books(**args, cache=cache)
            elapsed = (time.perf_counter() - start) * 1000
        return out, elapsed

    def compare(actual, reference):
        max_error = 0.
        for book in books:
            a = actual['positions'][book].drop('id').to_numpy()
            b = reference['positions'][book].drop('id').to_numpy()
            np.testing.assert_allclose(a, b, rtol=1e-10, atol=1e-6)
            max_error = max(max_error, float(np.max(np.abs(a - b))))
        return max_error

    # Warm readonly and writable numba specializations before timing.
    run(inputs, cache)
    run(inputs, DependencyCache())
    cases = {k: [] for k in ('unchanged', 'spread_edit', 'contract_edit', 'full_rebuild')}
    latest, max_error = {}, 0.
    for i in range(7):
        _, elapsed = run(inputs, cache)
        cases['unchanged'].append(elapsed)
        shift = 21. + i
        args = inputs | {'spread_overrides_bp': {'loans': {ident: shift}}}
        incremental, elapsed = run(args, cache)
        cases['spread_edit'].append(elapsed)
        reference, full = run(args, DependencyCache())
        cases['full_rebuild'].append(full)
        max_error = max(max_error, compare(incremental, reference))
        latest['spread_edit'] = incremental['graph']
        contracts = books['loans'].with_columns(
            pl.when(pl.col('id') == ident).then(pl.col('coupon_or_spread') + (i + 1) * .0001)
            .otherwise(pl.col('coupon_or_spread')).alias('coupon_or_spread'))
        args = inputs | {'books': books | {'loans': contracts}}
        edited, elapsed = run(args, cache)
        cases['contract_edit'].append(elapsed)
        rebuilt, _ = run(args, DependencyCache())
        max_error = max(max_error, compare(edited, rebuilt))
        latest['contract_edit'] = edited['graph']
    report = dict(platform=platform.platform(), python=platform.python_version(),
                  threads=numba.get_num_threads(), paths=128,
                  records={k: len(v) for k, v in books.items()}, samples_ms=cases,
                  median_ms={k: statistics.median(v) for k, v in cases.items()},
                  max_absolute_output_difference=max_error, latest_graph_work=latest,
                  cache=cache.info(),
                  scope='Full engine entry point including graph lookup and result construction; excludes HTTP and Arrow serialization. Synthetic inputs; no Rust backend.')
    target = Path(__file__).resolve().parents[1] / 'docs/reviews/2026-09-28-incremental-pricing.json'
    target.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report['median_ms'], indent=2))
    print(f"Maximum absolute output difference: {max_error:g}")


if __name__ == '__main__':
    run_benchmark()
