# Historical Python/reference benchmark. Production execution requires Rust.
"""Reproducible scale, cache-pressure and real-FFI comparison. Synthetic inputs.

uv run --project apps/api python scripts/benchmark_comparison.py
Build Rust first with scripts/build_native.py. Records warm samples, numerical
errors and actual dependency work; never equates kernel speed with application speed.
"""
from contextlib import redirect_stdout
import ctypes
import gc
import hashlib
import io
import json
from pathlib import Path
import platform
import statistics
import time
import tracemalloc

import numba
import numpy as np
import polars as pl
from portfolio_risk.analytics.incremental import price_books, SUPPORTED_BOOKS
from portfolio_risk.analytics.whatif import compare_books
from portfolio_risk.core.batch import CashflowBatch, NumpyDiscountBackend
from portfolio_risk.core.dependency import DependencyCache
from portfolio_risk.core.native import NumbaDiscountBackend, RustDiscountBackend
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.demo import demo_market, model_balance_sheet, demo_deposit_history

OUT = Path(__file__).resolve().parents[1] / 'docs/reviews/2026-09-28-five-step-comparison.json'


def memory():
    if platform.system() != 'Windows':
        return {}
    class Counters(ctypes.Structure):
        _fields_ = [('cb', ctypes.c_ulong), ('faults', ctypes.c_ulong)] + [(k, ctypes.c_size_t) for k in
            ('peak_rss','rss','peak_paged','paged','peak_nonpaged','nonpaged','pagefile','peak_pagefile')]
    counters = Counters(); counters.cb = ctypes.sizeof(counters)
    fn = ctypes.windll.kernel32.GetCurrentProcess
    fn.restype = ctypes.c_void_p
    ctypes.windll.psapi.GetProcessMemoryInfo(ctypes.c_void_p(fn()), ctypes.byref(counters), counters.cb)
    return {'rss_mb': counters.rss/2**20, 'process_peak_rss_mb': counters.peak_rss/2**20}


def samples(fn, n=7):
    data = []
    for _ in range(n):
        start = time.perf_counter(); result = fn(); data.append((time.perf_counter()-start)*1000)
    return result, {'median_ms': statistics.median(data), 'p95_ms': float(np.percentile(data, 95)), 'samples_ms': data}


def parity(a, b):
    maximum = 0.
    for book in a['positions']:
        x, y = a['positions'][book].drop('id').to_numpy(), b['positions'][book].drop('id').to_numpy()
        np.testing.assert_allclose(x, y, rtol=1e-10, atol=1e-5, equal_nan=True)
        maximum = max(maximum, float(np.nanmax(np.abs(x-y))))
    if 'nii' in a:
        np.testing.assert_allclose(a['nii']['monthly']['nii'], b['nii']['monthly']['nii'], rtol=1e-10, atol=1e-5)
    return maximum


def main():
    numba.set_num_threads(min(4, numba.config.NUMBA_NUM_THREADS))
    backends = {'numpy': NumpyDiscountBackend(), 'numba': NumbaDiscountBackend(), 'rust': RustDiscountBackend()}
    report = {'platform': platform.platform(), 'python': platform.python_version(), 'numpy': np.__version__,
              'numba': numba.__version__, 'threads': numba.get_num_threads(),
              'script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'backend_identities': {k: b.identity for k, b in backends.items()}, 'kernels': [], 'graphs': [], 'analytics': []}
    def save():
        OUT.write_text(json.dumps(report, indent=2, allow_nan=False)+'\n', encoding='utf-8')

    for count in (1_000, 10_000, 100_000):
        n = count*60
        times = np.tile(np.arange(1,61)/2, count)
        values = np.tile(np.r_[np.full(59,.025),1.025], count) * np.exp(-.04*times) * 128
        offsets = np.arange(count+1, dtype=np.int64)*60
        batch = CashflowBatch(offsets, times, values, 128)
        rows = [(values[i*60:(i+1)*60], times[i*60:(i+1)*60]) for i in range(count)]
        oas = np.linspace(-.01,.03,count)
        expected = backends['numpy'].price(batch, oas)
        for name, backend in backends.items():
            for _ in range(3): backend.price(batch, oas)
            actual, timing = samples(lambda: backend.price(batch, oas))
            np.testing.assert_allclose(actual, expected, rtol=1e-12, atol=1e-12)
            _, packed = samples(lambda: backend.price(CashflowBatch.from_rows(rows, 128), oas), n=3)
            tracemalloc.start(); backend.price(batch, oas); _, peak = tracemalloc.get_traced_memory(); tracemalloc.stop()
            report['kernels'].append(dict(positions=count, cashflows=n, backend=name, prepared_call=timing,
                pack_and_call=packed, traced_call_peak_mb=peak/2**20,
                max_abs_error=float(np.max(np.abs(actual-expected))), **memory()))
        save(); print('kernel comparison complete:', count, flush=True)
        del batch, rows, times, values, expected, actual; gc.collect()

    bs = model_balance_sheet(scale=.01, basis='amortized_cost', include_markets_bs=True)
    sr, vp = demo_market()
    inputs = dict(asof=bs['asof'], swap_rates=sr, vol_pts=vp, seed=7, config=RunConfig(128,128,27, compute_backend='python'),
                  mbs_hists=bs['mbs_hists'], dep_hist=demo_deposit_history())
    mixed = {k: bs[k] for k in SUPPORTED_BOOKS}
    for count in (375, 10_000, 100_000):
        if count == 375:
            books = mixed
        else:
            frame = pl.concat([bs['loans']] * ((count+89)//90)).head(count).with_columns(
                pl.Series('id', [f'CML{i:07d}' for i in range(count)]))
            books = {'loans': frame}
        ident = books['loans']['id'][0]
        args = inputs | {'books': books}
        baseline = None
        for name, backend in backends.items():
            cache = DependencyCache(max_bytes=512*1024*1024, max_entries=500_000)
            with redirect_stdout(io.StringIO()):
                start = time.perf_counter(); initial = price_books(**args, cache=cache, backend=backend); cold = (time.perf_counter()-start)*1000
                unchanged, warm = samples(lambda: price_books(**args, cache=cache, backend=backend), n=3)
                edits, times_ms = [], []
                for shift in range(21, 26):
                    start = time.perf_counter()
                    result = price_books(**args, cache=cache, backend=backend, spread_overrides_bp={'loans': {ident: shift}})
                    times_ms.append((time.perf_counter()-start)*1000)
                    edits.append(sum(s['computed'] for s in result['graph'].values()))
                fresh, rebuild = samples(lambda: price_books(**args, cache=DependencyCache(max_bytes=512*1024*1024, max_entries=500_000),
                    backend=backend, spread_overrides_bp={'loans': {ident: 25}}), n=1)
            err = parity(result, fresh)
            backend_error = 0 if baseline is None else parity(result, baseline)
            if baseline is None: baseline = result
            report['graphs'].append(dict(positions=count, composition='five-product mixed' if count==375 else 'corporate loans',
                backend=name, initial_ms=cold, unchanged=warm, edit_samples_ms=times_ms, edit_median_ms=statistics.median(times_ms),
                fresh_rebuild=rebuild, computed_nodes_per_edit=edits, cache=cache.info(), max_rebuild_error=err,
                max_numpy_error=backend_error, **memory()))
            del cache, initial, unchanged, result, fresh; gc.collect(); save()
            print('graph comparison complete:', count, name, flush=True)
        del baseline; gc.collect()

    # Whole mixed book analytics, including all curve pillars and 27-month NII.
    for name, backend in backends.items():
        cache = DependencyCache(max_bytes=512*1024*1024, max_entries=100_000)
        args = inputs | {'books': mixed, 'include_analytics': True, 'backend': backend}
        ident = mixed['loans']['id'][0]
        patch = {'loans': {ident: {'coupon_or_spread': float(mixed['loans']['coupon_or_spread'][0])+.001}}}
        with redirect_stdout(io.StringIO()):
            start = time.perf_counter(); price_books(**args, cache=cache); cold = (time.perf_counter()-start)*1000
            edit_times = []
            for step in range(1, 6):
                patch['loans'][ident]['coupon_or_spread'] = float(mixed['loans']['coupon_or_spread'][0]) + step * .001
                start = time.perf_counter()
                out = compare_books(**args, cache=cache, assumption_overrides=patch)
                edit_times.append((time.perf_counter()-start)*1000)
            timing = {'median_ms': statistics.median(edit_times), 'p95_ms': float(np.percentile(edit_times,95)), 'samples_ms': edit_times}
            fresh = compare_books(**args, cache=DependencyCache(max_bytes=512*1024*1024), assumption_overrides=patch)
        err = parity(out['revised'], fresh['revised'])
        report['analytics'].append(dict(backend=name, positions=375, paths=128, horizon=27, initial_ms=cold,
            comparison=timing, revised_graph=out['revised']['graph'], max_rebuild_error=err, cache=cache.info(), **memory()))
        del cache, out, fresh; gc.collect(); save(); print('analytics complete:', name, flush=True)

    # A broad market shock changes paths and all valuation flows, but holds OAS.
    frame = pl.concat([bs['loans']] * 112).head(10_000).with_columns(pl.Series('id', [f'SHOCK{i:07d}' for i in range(10_000)]))
    args = inputs | {'books': {'loans': frame}}
    cache = DependencyCache(max_bytes=512*1024*1024, max_entries=500_000)
    with redirect_stdout(io.StringIO()):
        base = price_books(**args, cache=cache)
        start = time.perf_counter()
        shocked = price_books(**args, cache=cache, scenario_market=(sr+.01, vp))
        elapsed = (time.perf_counter()-start)*1000
        fresh = price_books(**args, cache=DependencyCache(), scenario_market=(sr+.01, vp))
    np.testing.assert_array_equal(base['positions']['loans']['base_oas_bp'], shocked['positions']['loans']['base_oas_bp'])
    report['broad_market_shock'] = dict(positions=10_000, parallel_bp=100, elapsed_ms=elapsed,
        graph=shocked['graph'], max_rebuild_error=parity(shocked, fresh), cache=cache.info())
    del cache, base, shocked, fresh; gc.collect()

    # Pressure: a deliberately undersized cache must remain correct and bounded.
    args = inputs | {'books': {'loans': bs['loans']}}
    tiny = DependencyCache(max_bytes=64*1024, max_entries=64)
    with redirect_stdout(io.StringIO()):
        expected = price_books(**args, cache=DependencyCache())
        measured, timing = samples(lambda: price_books(**args, cache=tiny), n=3)
    report['cache_pressure'] = dict(max_error=parity(measured, expected), timing=timing,
                                    cache=tiny.info(), computed=sum(v['computed'] for v in measured['graph'].values()))
    report['limits'] = ['Synthetic instruments, not live market validation.',
        'Large 10k/100k graph tests use corporate loans; mixed 375-position analytics covers all five products.',
        'Engine timings exclude HTTP/Arrow and queueing; kernel calls include FFI, validation and output allocation.',
        'Prepared-kernel input packing is separately measured. Process peak RSS is cumulative, not backend-attributable.',
        'Traced allocation peak does not capture all Rust/native allocations.']
    save(); print(OUT, flush=True)


if __name__ == '__main__': main()
