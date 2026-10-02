# Historical Python/reference benchmark. Production execution requires Rust.
"""Reproducible local synthetic benchmark; run with the API environment.

uv run --project apps/api python scripts/benchmark_review.py
Uses four Numba threads; includes warmup but reports only warm samples.
"""
from pathlib import Path
import cProfile
import json
import statistics
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "apps/api"))
import numba
import numpy as np
from app import store
from portfolio_risk.core.runtime import RunConfig, run_context

numba.set_num_threads(min(4, numba.config.NUMBA_NUM_THREADS))
store.seed_demo()
config = RunConfig(n_paths=128, n_paths_base=128, horizon=27, compute_backend='python')


def kpis(cached):
    snapshot = store.snapshot()
    token = store._RUN_STATE.set(snapshot)
    try:
        with run_context(config) as context:
            if not cached:
                context.max_bytes = 0
            profile = cProfile.Profile()
            start = time.perf_counter()
            profile.enable()
            result = store.run_kpis(snapshot["market"]["swap_rates"], snapshot["market"]["vol_pts"])
            profile.disable()
            elapsed = time.perf_counter() - start
            counts = {}
            for entry in profile.getstats():
                if not isinstance(entry.code, str) and entry.code.co_name in ("simulate_rates", "calibrate_abcd"):
                    counts[entry.code.co_name] = counts.get(entry.code.co_name, 0) + entry.callcount
            return store.to_arrow_envelope(result), dict(seconds=elapsed, cache_hits=context.hits,
                                                         cache_bytes=context.bytes, calls=counts)
    finally:
        store._RUN_STATE.reset(token)


# Compile both writable and readonly signatures before the paired comparison.
kpis(False); kpis(True)
samples = {"uncached": [], "cached": []}
for _ in range(3):
    old, before = kpis(False)
    new, after = kpis(True)
    assert old == new, "caching changed numerical or serialized KPI results"
    samples["uncached"].append(before); samples["cached"].append(after)

snapshot = store.snapshot()
token = store._RUN_STATE.set(snapshot)
try:
    with run_context(config):
        start = time.perf_counter()
        store.build_unitlib_job(snapshot["market"]["swap_rates"], snapshot["market"]["vol_pts"])
        build_s = time.perf_counter() - start
    allocations = [{"template": "agency_mbs", "purchase_m": 6, "notional": 1e8},
                   {"template": "cd_2y", "purchase_m": 0, "notional": 1e8}]
    times = []
    for _ in range(1000):
        start = time.perf_counter()
        store.eval_strategy_sync(allocations)
        times.append((time.perf_counter() - start) * 1e6)
finally:
    store._RUN_STATE.reset(token)

summary = {"threads": numba.get_num_threads(), "paths": 128, "base_paths": 128,
           "horizon_months": 27, "samples": samples, "kpis_equal_byte_for_byte": True,
           "unit_build_s": build_s, "strategy_eval_median_us": statistics.median(times),
           "strategy_eval_p95_us": float(np.percentile(times, 95))}
print(json.dumps(summary, indent=2))
output = Path(__file__).resolve().parents[1] / "docs/reviews/2026-09-28-benchmark.json"
output.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
