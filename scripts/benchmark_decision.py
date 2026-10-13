# Historical Python/reference benchmark. Production execution requires Rust.
"""Reproducible complete workflow and separately labeled synthetic graph probe.

uv run --project apps/api python scripts/benchmark_decision.py
"""
from pathlib import Path
import contextlib
import io
import json
import platform
import statistics
import time

import numba
import numpy as np
from portfolio_risk.demo import model_balance_sheet, demo_market, demo_deposit_history, demo_hedge_book
from portfolio_risk.analytics.incremental import price_books, SUPPORTED_BOOKS
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.core.dependency import DependencyCache
from portfolio_risk.strategy.decision import DecisionSession, NativeDecision, _base, _vectors
from portfolio_risk.strategy.optimizer import optimize_balance_sheet
from portfolio_risk.strategy.unitlib import build_unit_library, evaluate_strategy


def measure(fn, repetitions=5):
    samples = []
    for i in range(repetitions):
        start = time.perf_counter()
        out = fn(i)
        samples.append((time.perf_counter()-start)*1000)
    return {'median_ms': statistics.median(samples), 'max_ms': max(samples), 'samples_ms': samples}, out


def main():
    numba.set_num_threads(4)
    bs = model_balance_sheet(scale=.01, basis='amortized_cost', include_markets_bs=True)
    sr, vp = demo_market()
    config = RunConfig(128, 128, 27, compute_backend='python')
    markets = [('base', sr, vp, 0.), ('up25bp', sr+.0025, vp, 0.), ('down25bp', sr-.0025, vp, 0.)]
    limits = dict(lcr_min=1.10, nsfr_min=1.05, cet1_min=.10, eve_limit=.15,
                  max_total_assets=3e10, cash_budget=0., commercial=[])
    inputs = dict(books={b: bs[b] for b in SUPPORTED_BOOKS}, asof=bs['asof'], swap_rates=sr, vol_pts=vp,
                  config=config, mbs_hists=bs['mbs_hists'], dep_hist=demo_deposit_history(),
                  extras={'mm': bs['mm'], 'hedges': demo_hedge_book(), 'equity': bs['equity']},
                  constraints=limits, markets=markets)
    report = {'environment': {'platform': platform.platform(), 'processor': platform.processor(),
              'python': platform.python_version(), 'paths':128, 'threads':4, 'horizon':27,
              'positions': sum(len(v) for v in inputs['books'].values()), 'scenarios':3},
              'scope': 'Real mixed-book engine plus Rust graph/HiGHS/independent replay. Synthetic 100k graph probe is separate. Existing Numba disk caches may be warm; initialization is not cold compilation.'}
    start = time.perf_counter()
    s = DecisionSession(**inputs)
    try:
        initial = s.update(version=0)
        report['initialization_and_first_solve_ms'] = (time.perf_counter()-start)*1000
        report['initial_feasible'] = initial['feasible']
        assert initial['feasible'], initial
        report['constraint_update'], out = measure(lambda i: s.update(version=s.version, constraints=limits | {'max_total_assets': 3e10-i*1e8}))
        report['constraint_work'] = out['work']
        ident = bs['loans']['id'][0]
        original = float(bs['loans']['coupon_or_spread'][0])
        report['single_instrument_update'], out = measure(lambda i: s.update(version=s.version,
            edits={f'loans:{ident}': {'coupon_or_spread': original+.0001*(i+1)}}))
        report['instrument_work'] = out['work']
        report['template_update'], out = measure(lambda i: s.update(version=s.version, templates={'cml_fixed_5y': {'spread_bp': 191.+i}}))
        report['template_work'] = out['work']
        report['native_allocation_replay'], _ = measure(lambda _: s.evaluate(out['allocation'], s.version), 30)
        with run_context(config):
            report['python_allocation_replay'], _ = measure(lambda _: [evaluate_strategy(lib,out['allocation'],base) for lib,base in zip(s.libraries,s.bases)], 30)
        final_limits = limits | {'max_total_assets':3e10-4e8}
        with run_context(config):
            report['fresh_scipy_solve_same_coefficients'], ref = measure(lambda _: optimize_balance_sheet(list(zip(s.libraries,s.bases)), **final_limits))
        report['objective_difference_$'] = ref['worst_case_nii_$'] - out['worst_case_nii_$']
        np.testing.assert_allclose(ref['worst_case_nii_$'], out['worst_case_nii_$'], rtol=1e-7, atol=.01)
        report['persistent_native_solve_same_coefficients'], out = measure(lambda _: s.update(version=s.version))
        report['final_solver'] = out['solver']
        report['cache'] = s.cache.info()
        # Full fresh pricing, full unit library and SciPy from baseline snapshot.
        def full_reference(_):
            pairs=[]
            reference_cache=DependencyCache(max_bytes=512*1024*1024,max_entries=500_000)
            with run_context(config):
                for _, rates, vols, spread in markets:
                    p = price_books(inputs['books'], **(s.pricing | {'cache':reference_cache}),
                        scenario_market=(rates,vols), spread_shift=spread, balance_sheet_extras=inputs['extras'])
                    lib = build_unit_library(rates,vols,inputs['mbs_hists'],inputs['dep_hist'],horizon=27,seed=7,asof=bs['asof'])
                    pairs.append((lib,p['kpis'] | {'nii_total_$':p['nii']['total']}))
            with run_context(config):
                return optimize_balance_sheet(pairs,**limits)
        report['full_python_workflow_rebuild'], full = measure(full_reference, 3)
        report['full_rebuild_initial_objective_difference_$'] = full['worst_case_nii_$']-initial['worst_case_nii_$']
        np.testing.assert_allclose(full['worst_case_nii_$'], initial['worst_case_nii_$'],rtol=1e-7,atol=.01)
        # 100k synthetic contribution records; NO product cashflow/pricing claim.
        native=NativeDecision()
        count=100_000
        start=time.perf_counter()
        handle=native.call(op='create',units=s.libraries[0]['units'],
            scenarios=[{'base':_base(lib,base),'vectors':_vectors(lib)} for lib,base in zip(s.libraries,s.bases)],
            records=[{'key':f'loans:{i}','metrics':[[1.,0.,0.,0.,1.]]*3} for i in range(count)],constraints=final_limits)['handle']
        init_ms=(time.perf_counter()-start)*1000
        version=0
        try:
            def delta(i):
                nonlocal version
                p=native.call(op='plan',handle=handle,version=version,edits={'loans:99999':{'coupon_or_spread':.01+i*.0001}},templates={})
                r=native.call(op='stage',handle=handle,version=version,token=p['token'],records=[{'key':'loans:99999','metrics':[[1.+i,0.,0.,0.,1.+i]]*3}],columns={})
                result=native.call(op='publish',handle=handle,version=version,token=p['token'])
                version=result['version']
                assert len(result['changed_positions'])==1
                return result
            delta(0)
            sent=native.bytes_sent
            timing, _ = measure(lambda i:delta(i+1),30)
            report['synthetic_100k_graph']={'scope':'Synthetic precomputed contributions, no product engines, no Python independent replay, 3 real-sized scenario/unit coefficient sets',
                'initialization_ms':init_ms,'single_record_update':timing,'average_request_bytes':(native.bytes_sent-sent)/30}
        finally:
            native.call(op='close',handle=handle)
    finally:
        s.close()
    return report


if __name__ == '__main__':
    with contextlib.redirect_stdout(io.StringIO()):
        report=main()
    path=Path(__file__).resolve().parents[1]/'docs/reviews/2026-09-28-decision-prototype.json'
    path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))
