# Historical Python/reference benchmark. Production execution requires Rust.
"""Actual mixed-product pricing at scale; no precomputed position contributions.

uv run --project apps/api python scripts/benchmark_balance_sheet.py --key-rates
Results are checkpointed after each stage. Windows process memory uses OS counters.
"""
from __future__ import annotations

import argparse
import copy
import ctypes
from ctypes import wintypes
import gc
import hashlib
import json
import os
import platform
from pathlib import Path
import statistics
import threading
import time
import traceback
import winreg

import numba
import numpy as np
import polars as pl

from portfolio_risk.demo import model_balance_sheet, demo_market, demo_deposit_history, demo_hedge_book
from portfolio_risk.analytics.incremental import price_books, SUPPORTED_BOOKS
from portfolio_risk.core.dependency import DependencyCache, fingerprint
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.strategy import decision
from portfolio_risk.strategy.optimizer import optimize_balance_sheet
from portfolio_risk.strategy.unitlib import build_unit_library, evaluate_strategy

ROOT = Path(__file__).resolve().parents[1]


class MemoryCounters(ctypes.Structure):
    _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD)] + [
        (name, ctypes.c_size_t) for name in ('PeakWorkingSetSize', 'WorkingSetSize',
        'QuotaPeakPagedPoolUsage', 'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage',
        'QuotaNonPagedPoolUsage', 'PagefileUsage', 'PeakPagefileUsage', 'PrivateUsage')]


class MemoryStatus(ctypes.Structure):
    _fields_ = [('length', wintypes.DWORD), ('load', wintypes.DWORD)] + [
        (name, ctypes.c_ulonglong) for name in ('total_phys', 'avail_phys', 'total_page',
        'avail_page', 'total_virtual', 'avail_virtual', 'avail_extended')]


def memory():
    kernel, psapi = ctypes.WinDLL('kernel32'), ctypes.WinDLL('psapi')
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(MemoryCounters), wintypes.DWORD]
    m = MemoryCounters()
    m.cb = ctypes.sizeof(m)
    if not psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(), ctypes.byref(m), m.cb):
        raise ctypes.WinError()
    system = MemoryStatus()
    system.length = ctypes.sizeof(system)
    if not kernel.GlobalMemoryStatusEx(ctypes.byref(system)):
        raise ctypes.WinError()
    return dict(rss_bytes=m.WorkingSetSize, peak_rss_process_bytes=m.PeakWorkingSetSize,
                private_bytes=m.PrivateUsage, peak_commit_process_bytes=m.PeakPagefileUsage,
                system_available_bytes=system.avail_phys, system_total_bytes=system.total_phys)


class Monitor:
    def __init__(self):
        self.stop = threading.Event()
        self.samples = []
        self.thread = threading.Thread(target=self.run, daemon=True)

    def run(self):
        while not self.stop.is_set():
            self.samples.append(memory())
            self.stop.wait(.1)

    def __enter__(self):
        self.before = memory()
        self.wall, self.cpu = time.perf_counter(), time.process_time()
        self.thread.start()
        return self

    def __exit__(self, *_):
        elapsed, cpu = time.perf_counter()-self.wall, time.process_time()-self.cpu
        self.stop.set()
        self.thread.join()
        after = memory()
        samples = self.samples + [self.before, after]
        self.result = dict(wall_seconds=elapsed, process_cpu_seconds=cpu,
            average_cpu_cores=cpu/elapsed, before=self.before, after=after,
            sampled_peak_rss_bytes=max(s['rss_bytes'] for s in samples),
            sampled_peak_private_bytes=max(s['private_bytes'] for s in samples),
            minimum_system_available_bytes=min(s['system_available_bytes'] for s in samples))


def fixture(count):
    if count < 375 or count % 375:
        raise ValueError('positions must be a positive multiple of the 375-position seed book')
    bs = model_balance_sheet(scale=.01, basis='amortized_cost', include_markets_bs=True)
    copies = count // 375
    fields = dict(mbs='wac', loans='coupon_or_spread', debt='coupon_or_spread',
                  deposits='avg_account_size', cds='rate')
    metadata = {}
    for book in SUPPORTED_BOOKS:
        original = bs[book]
        ident = 'cusip' if book == 'mbs' else 'id'
        amount = dict(mbs='current_face', loans='face', debt='face', deposits='balance', cds='balance')[book]
        field = fields[book]
        frames = []
        for i in range(copies):
            # Centered, modest economic variation. Preserve quoted balance-sheet
            # value and mix by splitting notionals, but actually change cashflows.
            offset = (i - (copies-1)/2) / max(copies-1, 1)
            term = pl.col(field) * (1 + offset*.10) if book == 'deposits' else pl.col(field)+offset*.002
            frames.append(original.with_columns(
                (pl.col(ident)+pl.lit(f'_{i:04d}')).alias(ident),
                (pl.col(amount)/copies).alias(amount), term.alias(field)))
        bs[book] = pl.concat(frames)
        assert bs[book][ident].n_unique() == len(bs[book])
        economic = [c for c in bs[book].columns if c not in
                    {ident, 'price', 'face', 'current_face', 'balance', 'book_yield'}]
        assert bs[book].select(economic).unique().height == len(bs[book])
        np.testing.assert_allclose(bs[book][amount].sum(), original[amount].sum(), rtol=1e-12)
        metadata[book] = dict(positions=len(bs[book]), distinct_economic_contracts=len(bs[book]),
                             notional=float(bs[book][amount].sum()), varied_field=field,
                             terms_fingerprint=fingerprint(bs[book]))
    return bs, metadata


def main(args):
    numba.set_num_threads(args.threads)
    path = Path(args.output).resolve() if args.output else ROOT / f'docs/reviews/2026-09-28-balance-sheet-{args.positions}.json'
    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r'HARDWARE\DESCRIPTION\System\CentralProcessor\0') as key:
        processor_name = winreg.QueryValueEx(key, 'ProcessorNameString')[0].strip()
    report = dict(status='running', configuration=vars(args), environment=dict(
        platform=platform.platform(), python=platform.python_version(), numba=numba.__version__,
        cpu=processor_name, logical_cores=os.cpu_count(), memory=memory()), stages={})

    def save():
        path.write_text(json.dumps(report, indent=2, allow_nan=False, default=str)+'\n', encoding='utf-8')

    def stage(name, fn):
        print(f'BENCH START {name}', flush=True)
        with Monitor() as monitor:
            value = fn()
        report['stages'][name] = monitor.result
        save()
        print(f'BENCH DONE {name}: {monitor.result["wall_seconds"]:.3f}s; '
              f'RSS {monitor.result["sampled_peak_rss_bytes"]/2**30:.3f} GiB', flush=True)
        return value

    def repeated(name, fn, repetitions=5):
        samples, values = [], []
        def run():
            for i in range(repetitions):
                start = time.perf_counter()
                values.append(fn(i))
                samples.append((time.perf_counter()-start)*1000)
        stage(name, run)
        report[name] = dict(samples_ms=samples, median_ms=statistics.median(samples), max_ms=max(samples))
        if isinstance(values[-1], dict) and 'work' in values[-1]:
            report[name]['work'] = values[-1]['work']
            report[name]['last_timings_ms'] = values[-1]['timings_ms']
            assert all(v['validated'] and v['feasible'] for v in values)
        save()
        return values[-1]

    original_price = decision.price_books
    session = None
    try:
        bs, report['fixture'] = stage('fixture', lambda: fixture(args.positions))
        sr, vp = demo_market()
        config = RunConfig(args.paths, args.paths, 27, compute_backend='python')
        markets = [('base', sr, vp, 0.), ('up25bp', sr+.0025, vp, 0.), ('down25bp', sr-.0025, vp, 0.)]
        limits = dict(lcr_min=1.10, nsfr_min=1.05, cet1_min=.10, eve_limit=.15,
                      max_total_assets=3e10, cash_budget=0., commercial=[])
        inputs = dict(books={b: bs[b] for b in SUPPORTED_BOOKS}, asof=bs['asof'], swap_rates=sr, vol_pts=vp,
                      config=config, mbs_hists=bs['mbs_hists'], dep_hist=demo_deposit_history(),
                      extras={'mm': bs['mm'], 'hedges': demo_hedge_book(), 'equity': bs['equity']},
                      constraints=limits, markets=markets)
        report['scope'] = dict(core_positions=args.positions, money_market_rows=len(bs['mm']),
            swap_rows=len(inputs['extras']['hedges'][0]), swaption_rows=len(inputs['extras']['hedges'][1]),
            scenarios=[m[0] for m in markets], units_per_scenario=35, horizon_months=27,
            synthetic=True, notional_policy='Preserve seed book balances; subdivide and vary economic terms',
            decision_metrics='Spot/OAS, parallel DV01, NII/runoff, EVE/LCR/NSFR/CET1, robust LP, allocation replay',
            excluded='Nonlinear forward stress, vega, forecast-conditioned NII, dynamic reinvestment, API/network/UI',
            initialization='Empty pricing cache; existing Numba disk compilation caches may be warm')
        sources = list((ROOT/'packages/portfolio-risk/src/portfolio_risk').rglob('*.py'))
        report['source_sha256'] = {str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
        report['pricing_calls'] = []
        def instrumented(books, **kw):
            start = time.perf_counter()
            result = original_price(books, **kw)
            item = dict(positions=sum(len(f) for f in books.values()), wall_seconds=time.perf_counter()-start,
                        graph=result['graph'], cache=result['cache'])
            report['pricing_calls'].append(item)
            if item['positions'] == args.positions:
                # No position is skipped; scenarios preserve its calibrated OAS.
                assert sum(len(f) for f in result['positions'].values()) == args.positions
                if len(report['pricing_calls']) == 1:
                    report['initial_book_totals'] = result['totals']
                    report['initial_kpis'] = result['kpis']
                    report['initial_nii'] = result['nii']['total']
                    report['base_repricing_max_error_price_points'] = {
                        b:float(np.max(np.abs(result['positions'][b]['model_price'].to_numpy()-books[b]['price'].to_numpy())))
                        for b in books}
                    assert max(report['base_repricing_max_error_price_points'].values()) < 1e-5
                print(f'BENCH PRICED {item["positions"]} positions in {item["wall_seconds"]:.3f}s', flush=True)
                save()
            return result
        decision.price_books = instrumented
        session = stage('initialize_three_market_session', lambda: decision.DecisionSession(**inputs))
        decision.price_books = original_price
        initial = stage('first_solve_and_independent_replay', lambda: session.update(version=0))
        assert initial['feasible'] and initial['validated']
        report['initial_objective_$'] = initial['worst_case_nii_$']
        report['native_identity'] = session.native.identity
        report['initial_native_request_bytes'] = session.native.bytes_sent
        report['solver'] = initial['solver']
        report['initial_cache'] = session.cache.info()
        report['initial_bases'] = copy.deepcopy(session.bases)
        repeated('constraint_update', lambda i: session.update(version=session.version,
            constraints=limits | {'max_total_assets':3e10-i*1e8}))
        session.update(version=session.version, constraints=limits)
        fields = dict(mbs='wac', loans='coupon_or_spread', debt='coupon_or_spread', deposits='rate_paid', cds='rate')
        for book, field in fields.items():
            frame = bs[book]
            ident = frame['cusip' if book == 'mbs' else 'id'][0]
            original = float(frame[field][0])
            out = repeated(f'single_{book}_update', lambda i, b=book, f=field, k=ident, v=original:
                session.update(version=session.version, edits={f'{b}:{k}': {f:v+(i+1)*.0001}}))
            assert out['work']['positions_repriced'] == 1
            session.update(version=session.version, edits={f'{book}:{ident}': {field:None}})
        out = repeated('hundred_loan_update', lambda i: session.update(version=session.version,
            edits={f'loans:{r["id"]}': {'coupon_or_spread':r['coupon_or_spread']+(i+1)*.0001}
                   for r in bs['loans'].head(100).iter_rows(named=True)}), 3)
        assert out['work']['positions_repriced'] == min(100, len(bs['loans']))
        session.update(version=session.version, edits={f'loans:{k}': {'coupon_or_spread':None} for k in bs['loans']['id'].head(100)})
        out = repeated('template_update', lambda i: session.update(version=session.version,
            templates={'cml_fixed_5y': {'spread_bp':191.+i}}), 3)
        repeated('native_allocation_replay', lambda _: session.evaluate(out['allocation'], session.version), 30)
        repeated('python_allocation_replay', lambda _: [evaluate_strategy(lib,out['allocation'],base)
            for lib,base in zip(session.libraries,session.bases)], 30)
        ref = repeated('fresh_scipy_same_coefficients', lambda _: optimize_balance_sheet(list(zip(session.libraries,session.bases)), **limits))
        report['same_coefficients_objective_difference_$'] = ref['worst_case_nii_$']-out['worst_case_nii_$']
        np.testing.assert_allclose(ref['worst_case_nii_$'],out['worst_case_nii_$'],rtol=1e-7,atol=.01)
        repeated('persistent_native_same_coefficients', lambda _: session.update(version=session.version))
        pricing = dict(session.pricing)
        session.close()
        session.cache.clear()
        session = None
        gc.collect()
        # A genuinely fresh full pricing rebuild, then the SciPy optimizer.
        def full_reference():
            pairs = []
            reference_cache = DependencyCache(max_bytes=512*2**20,max_entries=500_000)
            with run_context(config):
                for name, rates, vols, spread in markets:
                    p = stage(f'full_rebuild_price_{name}', lambda: price_books(inputs['books'],
                        **(pricing | {'cache':reference_cache}), scenario_market=(rates,vols),
                        spread_shift=spread,balance_sheet_extras=inputs['extras']))
                    lib = build_unit_library(rates,vols,inputs['mbs_hists'],inputs['dep_hist'],horizon=27,seed=7,asof=bs['asof'])
                    pairs.append((lib,p['kpis'] | {'nii_total_$':p['nii']['total']}))
            report['full_rebuild_cache'] = reference_cache.info()
            result = optimize_balance_sheet(pairs, **limits)
            reference_cache.clear()
            return result
        full = stage('full_python_rebuild_and_solve', full_reference)
        assert full['validated']
        report['full_rebuild_objective_difference_$'] = full['worst_case_nii_$']-initial['worst_case_nii_$']
        np.testing.assert_allclose(full['worst_case_nii_$'], initial['worst_case_nii_$'], rtol=1e-7,atol=.01)
        if args.key_rates:
            gc.collect()
            cache = DependencyCache(max_bytes=512*2**20,max_entries=500_000)
            risk = stage('full_key_rate_analytics_base_market', lambda: price_books(inputs['books'],
                **(pricing | {'cache':cache,'include_key_rates':True}), balance_sheet_extras=inputs['extras']))
            report['key_rate_graph'] = risk['graph']
            report['key_rate_cache'] = cache.info()
            report['key_rate_totals'] = {b:{c:float(f[c].sum()) for c in f.columns if c.startswith('krd01_') or c=='dv01'}
                                       for b,f in risk['positions'].items()}
            np.testing.assert_allclose(risk['nii']['total'], report['initial_nii'],rtol=1e-10,atol=.01)
            for b, value in report['initial_book_totals'].items():
                np.testing.assert_allclose(risk['totals'][b],value,rtol=1e-10,atol=.01)
            assert all(len(v)==11 and all(np.isfinite(list(v.values()))) for v in report['key_rate_totals'].values())
            cache.clear()
        changed = [str(p.relative_to(ROOT)) for p in sources if hashlib.sha256(p.read_bytes()).hexdigest()!=report['source_sha256'][str(p.relative_to(ROOT))]]
        report['sources_changed_during_run'] = changed
        assert not changed, changed
        report['status'] = 'passed'
    except BaseException:
        report['status'] = 'failed'
        report['error'] = traceback.format_exc()
        raise
    finally:
        decision.price_books = original_price
        if session is not None:
            session.close()
        report['final_memory'] = memory()
        save()
        print(f'BENCH REPORT {path}', flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--positions',type=int,default=60_000)
    parser.add_argument('--paths',type=int,default=128)
    parser.add_argument('--threads',type=int,default=4)
    parser.add_argument('--key-rates',action='store_true')
    parser.add_argument('--output',help='Separate result file for comparison runs; preserves earlier evidence')
    main(parser.parse_args())
