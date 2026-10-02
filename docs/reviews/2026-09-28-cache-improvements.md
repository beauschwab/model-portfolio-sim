# Pricing cache improvements: 60,000-instrument validation

Implemented in portfolio-risk 0.21.1. Product models, discounting, fixed-OAS
scenario treatment, common random numbers, input fixture and the configured
512 MiB retained-result budget are unchanged.

## Changes

- Income reuses the request's live base/current-market cashflows even after LRU
  eviction. Different mortgage calibration and sensitivity path counts still
  produce separate cashflows; edited contracts retain their original accounting anchors.
- A bounded protected tier retains shared fits and market/deposit paths. Its
  maximum is the lesser of 64 MiB and one quarter of the existing byte budget,
  with at most 1024 nodes or one quarter of the entry budget. Both tiers share
  the original total limits. Ordinary nodes borrow unused space.
- Completed risk legs and book-local cashflow references are released promptly.
- Identical CD input objects reuse base cashflows even when Polars equality
  returns false for their object-valued call schedules. Persistent keys remain
  based on content, never object identity.

## Measurements

### Back-to-back comparison of the actual old and new implementations

The original cache and pricing source files were reconstructed in benchmark-local
modules and their SHA-256 hashes verified against the original 60k manifest.
No installed engine code was reverted. With the same 60,000-contract base market,
128 paths, four threads and empty 512 MiB caches, measured consecutively after a
small kernel warmup:

| Measure | Original implementation | Improved implementation | Reduction |
|---|---:|---:|---:|
| Base pricing, parallel DV01, NII and KPIs | 55.362 s | 48.049 s | 13.2% |
| Process CPU time | 148.781 s | 124.219 s | 16.5% |

All instrument prices, OAS, parallel DV01, position NII and aggregate NII passed
parity in this comparison. This is one before/after pair, not a latency distribution.

### Historical full-run comparison

Same workstation, 60,000 distinct core contracts plus 11 auxiliary positions,
128 paths, four pricing threads, three markets and 27 income months. The key-rate
sweep covers ten tenors in the base market. Positive change below means reduced
elapsed time or memory. Full phases have one measurement each; interactive
operations have the repetitions preserved in the raw files.

The original implementation itself now takes longer than the earlier recorded
31.565-second base pass. Thus the historical wall-time columns below are not a
controlled estimate of the cache change's effect. Use the back-to-back comparison
above for that purpose; keep these full-run figures for transparency, validation,
work counts and memory observations. Cache-independent operations, including
same-coefficient solver calls, also vary substantially between the historical
and current runs, so the wall-time drift is not specific to the pricing cache.

| Measurement | Before | After | Reduction |
|---|---:|---:|---:|
| Three-market initialization and first solve | 122.057 s | 171.040 s | -40.1% |
| Base pricing, DV01, income and KPIs | 31.565 s | 47.004 s | -48.9% |
| Fresh full Python rebuild and solve | 123.487 s | 170.446 s | -38.0% |
| Full ten-tenor key-rate analytics, base market | 188.959 s | 315.928 s | -67.2% |
| Single mortgage edit, median | 30.110 ms | 39.816 ms | -32.2% |
| Single loan edit, median | 15.646 ms | 21.424 ms | -36.9% |
| 100 loan edits, median | 104.548 ms | 155.853 ms | -49.1% |
| Constraint update, median | 3.620 ms | 6.696 ms | -85.0% |
| First mortgage edit | 644.449 ms | 45.390 ms | +93.0% |
| Peak resident process memory | 2.073 GiB | 2.054 GiB | +0.9% |
| Peak process commit | 3.659 GiB | 3.640 GiB | +0.5% |

Initial three-market cashflow computations fell from **800,470 to
660,000** (17.5% fewer). The base-market
pass now performs exactly 180,000 instrument cashflow calculations: one spot
and two parallel sensitivity legs per instrument. Income adds no repeated legs.
These work counts are stronger evidence than small timing differences on a
shared workstation. The retained-result budget was not increased.

An earlier after-change timing attempt overlapped an external test suite.
Only the benchmark process was interrupted; the external work was untouched.
That partial run is preserved as `2026-09-28-balance-sheet-60000-cache-contended`
and excluded from this comparison. The reported after-run began after that
test process exited. Neither run is a dedicated-host service-level guarantee.
The subsequent profiling checkpoint is also retained separately. It prompted
the source-verified old/new comparison rather than accepting a presumed speedup.

## Validation

- **103 engine tests passed**, including eight new eviction, budget, shared
  retention, failed-batch, unequal-path, edited-anchor and full-key-rate cases.
- **55 API tests passed** (three pre-existing framework deprecation warnings).
- Before/after book values, NII, full base KPI trees, all three initial scenario
  KPI trees, optimized objective and ten-tenor risk totals pass numerical parity
  at relative tolerance 1e-12 / absolute tolerance 1e-5.
- The fixture fingerprints and native library identity match exactly. Engine
  sources did not change during the completed after-run.
- Both runs independently replay native allocations and compare against a fresh
  full pricing rebuild and SciPy solve. These are synthetic-model checks.

## Scope and remaining work

The cache byte counter measures retained results, not a hard process-memory
ceiling. Temporary arrays, interpreters, native libraries and allocator overhead
remain additional. The protected-tier size is an initial bounded policy, not a
claim that 64 MiB is optimal for every path/scenario configuration. Nonlinear
stress, vega, distributed workers and concurrent-user capacity were not benchmarked.

No product kernel or Rust solver was rewritten. This change applies to existing
Python pricing/what-if requests and the hybrid Rust decision workflow alike.

## Evidence and reproduction

- [Before measurements](2026-09-28-balance-sheet-60000.json)
- [After measurements](2026-09-28-balance-sheet-60000-cache-after.json)
- [After execution log](2026-09-28-balance-sheet-60000-cache-after.log)
- [Engine tests](2026-09-28-cache-engine-tests.log)
- [API tests](2026-09-28-cache-api-tests.log)
- [Source-verified 60k comparison](2026-09-28-cache-ablation-60000.json)

```powershell
uv run --project apps/api python scripts/benchmark_balance_sheet.py --positions 60000 --paths 128 --threads 4 --key-rates --output docs/reviews/2026-09-28-balance-sheet-60000-cache-after.json
uv run --project apps/api python scripts/compare_cache_benchmarks.py
uv run --project apps/api python scripts/ablate_cache.py --positions 60000 --order before,after --output docs/reviews/2026-09-28-cache-ablation-60000.json
```
