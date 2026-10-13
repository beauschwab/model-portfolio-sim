# Rust mortgage execution and cache review — 0.27.6

The complete **project-wide Rust lifecycle goal is still open**. This delivery
moves built-in mortgage spot risk and forward stress from raw book/history inputs
through final position and portfolio results into Rust, and adds native market-stage
caching. The remaining work is tracked in the [full migration acceptance ledger](2026-09-30-rust-lifecycle-migration.md).
HiGHS remains C++, invoked and validated by the existing Rust strategy layer.

## Implemented ownership

| Stage | Owner in the native mortgage route |
|---|---|
| Table serialization, identifiers and DataFrame formatting | Python adapter |
| Natural-spline FICO/size factors, state/channel assumptions, original HPI and payment delays | Rust |
| Curve/PCA/current-coupon/PS/abcd calibration | Rust |
| Seeded shared draws and base OAS solve | Rust |
| All KRD/vega scenarios and dollar aggregation | Rust |
| Forward checkpoints, horizon/shock scheduling and fixed-OAS stress | Rust |
| Position stress P&L, portfolio totals and forward DV01 | Rust |
| Bounded immutable calibration/path cache | Rust |

The public risk and stress drivers each make one FFI call. Ownership tests prohibit
Python financial stages during these calls. The standalone `portfolio-mortgage-risk`
executable accepts `mortgage-risk-1` and `mortgage-stress-1` requests and requires no
Python runtime. Its named model-data inputs are immutable spline/LUT anchors and
categorical tables; callers do not supply precomputed prices or fitted parameters.

## Review findings resolved

- The first native mortgage implementation hardcoded categorical multipliers.
  It now transports the configured state/channel tables, including added categories.
- A wider fixed-OAS test exposed an existing curve-root precision discrepancy that
  straddled a float32 path boundary. The Python root tolerance changed from 2e-12
  to 2e-15; the reproduced maximum KRD difference fell from approximately $0.0000953
  to below $0.0000001. Acceptance tolerances were not relaxed.
- Warm native requests initially recomputed market stages. Native stage reuse now
  avoids that work. Complete length-tagged input bytes are retained in cache keys,
  so hash collisions cannot substitute another market or model configuration.
- Stress checkpoints no longer scale as a retained whole-book path tensor. Native
  execution processes at most 256 instruments per chunk with a 64 MiB scratch
  admission estimate and a separate 1 GiB result-buffer admission ceiling.

The process-local LRU retains at most 128 MiB of key/value accounting and 512
entries. Values are immutable. Eviction permits safe recomputation; failures do
not publish failed stages; clear prevents earlier in-flight work from repopulating
the cache. Instrument results, target prices and OAS solves are not cached. A
notional edit reuses market stages and still recomputes instrument output. This
is a market-stage cache, not the completed cross-book dependency graph.

## Measured comparison

Synthetic workload: **256 mortgage instruments, 16 base paths, 8 sensitivity paths,
360 simulated months, 3 forward-stress horizons, -100/0/+100 bp stress shocks,
4 worker threads**. Two public calls run risk and stress. Fresh processes isolate
backends. Imports/fixture creation are outside timing; first-call JIT is included.
Three warm calls share one run context, followed by a 1% first-position notional edit.

| Observed measure | Python | Rust |
|---|---:|---:|
| First risk + stress call | 3.478 s | 0.905 s |
| Warm median | 0.653 s | 0.335 s |
| Notional-edited call | 0.601 s | 0.500 s |
| Peak process working set | 205.6 MiB | 151.4 MiB |

Rust was approximately 1.95x faster at the observed warm median.
The prior pre-cache native median was 1.172 s; its measurements are preserved
[separately](2026-09-30-rust-mortgage-before-native-cache.json). These are sequential
local observations, not controlled confidence intervals or projected GSIB capacity.

The final native cache contained 63 entries (4,301,344 accounted bytes), with 63
misses and 282 hits. Warm and notional-edited calls introduced no further misses.
All timed outputs matched the independent reference at rtol=1e-7, atol=1e-5; the
largest absolute difference across retained outputs was 2.86102295e-06.
Peak process memory includes the Python host, imported libraries, JIT, returned
frames and transport; these numbers are not standalone-Rust allocator measurements.
See the [complete measurements and source/binary hashes](2026-09-30-rust-mortgage-lifecycle-benchmark.json).

Reproduce with:

```powershell
uv run --project apps/api python scripts/build_native.py
uv run --project apps/api python scripts/benchmark_mortgage_lifecycle.py --positions 256 --paths 8 --horizon 3 --threads 4 --repeats 3
```

## Validation

Final full engine run: **477 passed in 265.99 seconds**. Current-build API: 93 passed
in 100.93 seconds; Rust release:
16 passed; clippy with warnings denied and formatting: passed. Focused ownership,
parity and cache checks cover changed histories/quotes/seeds/paths/HPI/lag/precision,
cleared-cache equivalence, custom category tables, checkpoint chunk boundaries,
invalid/oversized inputs, mature/zero-face rows, fixed OAS, standalone execution,
thread determinism and independent portfolio aggregation. Documentation structure,
browser behavior and architecture checks passed separately from runtime validation.

The [machine-readable validation record](2026-09-30-rust-mortgage-lifecycle-validation.json)
binds these checks to source and native binary fingerprints.

## Remaining full-goal requirements

1. Native date/schedule normalization and complete risk/stress drivers for the other books.
2. Native cross-book income/accrual/KPI aggregation and direct unit-library pricing.
3. Full dependency resolution and selective instrument recomputation across edits,
   beyond the market-stage cache implemented here.
4. Saved-book/candidate mapping, accounting-category strategy constraints, complete
   daily journal reconciliation and final report assembly; resolve unsupported mappings.
5. One native job boundary with cancellation/deadlines, end-to-end publication/restart
   evidence, and equivalent complete 60,000-instrument cold/warm/edited performance
   and peak-memory measurements.

Existing financial approximations are retained: monthly MBS paths, fixed OAS,
stylized prepayment/HPI behavior, forward-shock templates and float32 checkpoints.
This work does not establish regulatory model validation or production deployment.
Local API tests exercise SQLite; no live PostgreSQL/S3/Iceberg deployment was tested.
