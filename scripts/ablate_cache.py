# Historical Python/reference benchmark. Production execution requires Rust.
"""Replay the exact pre-change cache sources for a same-host timing comparison.

Source SHA-256 must match the recorded 60k baseline before anything executes.
Only benchmark-local modules are created; installed engine modules stay unchanged.
"""
import argparse
import gc
import hashlib
import json
import time
import types
from pathlib import Path

import numba
import numpy as np

from benchmark_balance_sheet import fixture
from portfolio_risk.analytics import incremental
from portfolio_risk.core import dependency
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.demo import demo_market, demo_deposit_history, demo_hedge_book

ROOT = Path(__file__).resolve().parents[1]


def original_sources():
    pricing = Path(incremental.__file__).read_text(encoding='utf-8')
    begin = pricing.index('            # Polars equality can report False')
    end = pricing.index('            n_paths =', begin)
    pricing = pricing[:begin] + pricing[end:]
    pricing = pricing.replace('            if unchanged:',
        '            if fingerprint(base_market) == fingerprint(current_market) and valued_frame.equals(frame):')
    pricing = pricing.replace('                        del cf  # A completed sensitivity leg need not stay live through NII.\n', '')
    begin = pricing.index('                # These immutable cashflows')
    end = pricing.index('                income_keys =', begin)
    pricing = pricing[:begin] + '''                nk, ncf = cashflows(book, valued_frame, valued_rows, current_market, config.n_paths)
                bk, bcf = cashflows(book, frame, rows, base_market, config.n_paths)
''' + pricing[end:]
    pricing = pricing.replace('                del ncf, bcf, monthly, amounts\n            del base_cf, mark_cf\n', '')
    caching = Path(dependency.__file__).read_text(encoding='utf-8')
    begin, end = caching.index('class DependencyCache:'), caching.index('class Evaluation:')
    caching = caching[:begin] + '''class DependencyCache:
    """Process-local LRU, bounded by retained result bytes and node count.

    Computation is serialized by the reentrant lock. This matches the API's
    single quant worker and permits nested parent resolution without races.
    The budget excludes temporary batches, output frames and in-flight values.
    """

    def __init__(self, max_bytes=128 * 1024 * 1024, max_entries=50_000):
        if max_bytes < 0 or max_entries < 0:
            raise ValueError("cache limits must be nonnegative")
        self.max_bytes = max_bytes
        self.max_entries = max_entries
        self.bytes = 0
        self._entries = OrderedDict()
        self._lock = RLock()

    def clear(self):
        with self._lock:
            self._entries.clear()
            self.bytes = 0

    def info(self):
        with self._lock:
            return {"entries": len(self._entries), "bytes": self.bytes,
                    "max_bytes": self.max_bytes, "max_entries": self.max_entries}


''' + caching[end:]
    caching = caching.replace('entry = cache._get((stage, key))', 'entry = cache._entries.get((stage, key))')
    caching = caching.replace('                    output[i] = entry[0]\n',
        '                    output[i] = entry[0]\n                    cache._entries.move_to_end((stage, key))\n')
    caching = caching.replace('                    cache._put(key, value, size)\n', '''                    if cache.max_entries and size <= cache.max_bytes:
                        old = cache._entries.pop(key, None)
                        if old is not None:
                            cache.bytes -= old[1]
                        while cache._entries and (cache.bytes + size > cache.max_bytes
                                                  or len(cache._entries) >= cache.max_entries):
                            _, (_, removed) = cache._entries.popitem(last=False)
                            cache.bytes -= removed
                        cache._entries[key] = (value, size)
                        cache.bytes += size
''')
    recorded = json.loads((ROOT/'docs/reviews/2026-09-28-balance-sheet-60000.json').read_text(encoding='utf-8'))['source_sha256']
    for path, text in [(Path(incremental.__file__), pricing), (Path(dependency.__file__), caching)]:
        key = str(path.resolve().relative_to(ROOT))
        candidates = [text.encode(), text.replace('\n','\r\n').encode()]
        assert recorded[key] in [hashlib.sha256(x).hexdigest() for x in candidates], f'Baseline reconstruction differs: {key}'
    return pricing, caching


def main(args):
    numba.set_num_threads(4)
    pricing, caching = original_sources()
    old_cache = types.ModuleType('cache_before')
    exec(compile(caching, 'verified-baseline-dependency.py', 'exec'), old_cache.__dict__)
    old_price = types.ModuleType('portfolio_risk.analytics.cache_before')
    old_price.__package__ = 'portfolio_risk.analytics'
    exec(compile(pricing, 'verified-baseline-incremental.py', 'exec'), old_price.__dict__)
    old_price.Evaluation = old_cache.Evaluation
    bs, _ = fixture(args.positions)
    sr, vp = demo_market()
    inputs = dict(asof=bs['asof'],swap_rates=sr,vol_pts=vp,config=RunConfig(128,128,27, compute_backend='python'),
        mbs_hists=bs['mbs_hists'],dep_hist=demo_deposit_history(),include_analytics=True,include_key_rates=False,
        balance_sheet_extras={'mm':bs['mm'],'hedges':demo_hedge_book(),'equity':bs['equity']})
    books = {k:bs[k] for k in incremental.SUPPORTED_BOOKS}
    # Small warmup outside measurement, shared compiled kernels for both variants.
    incremental.price_books({k:f.head(1) for k,f in books.items()}, **inputs,cache=dependency.DependencyCache())
    report = {'positions':args.positions,'baseline_sources_sha256_verified':True,'measurements':[]}
    previous = None
    for variant in args.order.split(','):
        assert variant in ('before','after')
        module, cache_module = (old_price,old_cache) if variant == 'before' else (incremental,dependency)
        gc.collect()
        cache = cache_module.DependencyCache(max_bytes=512*2**20,max_entries=500_000)
        started, cpu = time.perf_counter(), time.process_time()
        result = module.price_books(books, **inputs, cache=cache)
        record = dict(variant=variant,wall_seconds=time.perf_counter()-started,cpu_seconds=time.process_time()-cpu,
                      graph=result['graph'],cache=cache.info())
        report['measurements'].append(record)
        if previous is not None:
            for book in books:
                np.testing.assert_allclose(result['positions'][book].drop('id').to_numpy(),
                    previous['positions'][book].drop('id').to_numpy(),rtol=1e-12,atol=1e-5)
            np.testing.assert_allclose(result['nii']['total'],previous['nii']['total'],rtol=1e-12,atol=1e-5)
        previous = result
        cache.clear()
        print('ABLATION',variant,record['wall_seconds'],record['cpu_seconds'],flush=True)
    report['status']='passed'
    (ROOT/args.output).write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--positions',type=int,default=6000)
    parser.add_argument('--order',default='before,after,after,before')
    parser.add_argument('--output',default='docs/reviews/2026-09-28-cache-ablation.json')
    main(parser.parse_args())
