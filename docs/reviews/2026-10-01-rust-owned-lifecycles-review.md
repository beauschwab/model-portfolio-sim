# Rust lifecycle migration and optimization review

Engine 0.27.8; product ABI 8; daily ledger protocol 2. Local Windows validation.

**Status: migration is incomplete.** This change removes substantial Python numerical orchestration, but the incremental dependency graph, decision repricing coordinator and unified runtime remain open. It does not complete the full calculation-lifecycle goal.

## Ownership delivered

| Surface | Rust owns | Python retains |
|---|---|---|
| Deposits | Raw cohorts, history calibration, base OAS, fixed-OAS KRD/vega and checkpoint stress | Input/table transport; independent reference |
| Hedges | Raw swap schedules, cash-settled swaption valuation, carry and risk aggregation | Input/table transport; independent reference |
| Accounting | Product cashflows, effective yields, coupon/accrual timing, amortized basis, frozen anchors, NII/NIM and runoff | Raw snapshots and output frames |
| Conditional forecasts | Income-only path conditioning, rate/mortgage/HPI recurrences | Validated external forecast inputs |
| KPIs | Mixed-book parallel revaluation and current EVE/LCR/NSFR/CET1 calculations | Explicit model weights and output presentation |
| Unit library | Forward template construction, pricing, base OAS, DV01, monthly coefficient preparation | Template selection and table transport |
| Forward programs | Shared paths, reinvestment amounts, amortization, per-path coupon fixing, balances and DV01 reports | Raw program/runoff transport |
| Interactive evaluation | Numeric coefficient reduction and KPI deltas | Allocation lookup, contiguous buffers, output formatting |
| Strategy optimizer | Financial LP construction, C++ HiGHS invocation and allocation validation | Input/output transport |
| Saved-book/candidate mapping | Principal/basis scaling, explicit accounting mapping, monthly cashflow and origination timing | Schema validation, provenance/storage |
| Daily ledger reports | Independent posting replay, closing statements, currency consolidation and stress attribution | Independent persisted-Parquet replay/checksum gate before atomic publication |

Standalone `portfolio-lifecycle` accepts bounded raw requests for deposits, hedges, accounting, KPIs, unit libraries, coefficients, forward programs and ledger mapping. Existing mortgage and term executables remain available. No Python financial callback executes inside these migrated drivers. Multiple native entrypoints do not yet constitute one complete native worker.

## Correctness and compatibility findings

1. Independent deposit fitters now use analytic equilibrium and dynamic-recurrence derivatives. Finite-difference fitting had amplified parameter noise into final risk. Objectives, bounds and output tolerances were retained.
2. A zero-shock deposit restart now returns the base value exactly; rounded checkpoint state must not manufacture zero-shock P&L.
3. Conditional-forecast initial means now accumulate in float64 in the Python reference. Float32 accumulation had introduced avoidable conditioning differences. Forecasts remain NII/runoff-only and never feed OAS/EVE.
4. The raw mixed-risk boundary shares accounting input checks, including schedule grids, CD parameters and mortgage market/grid identity. Inconsistent requests fail instead of mixing markets.
5. Public coefficient evaluation accepts unprepared caller libraries through native coefficient construction. Production interactive libraries are prepared in the build job.
6. API interactive evaluation preserves the backend that built its library instead of falling back to the ambient Python default.
7. The ledger input reader enforces its byte limit while reading. Native replay retains dictionary-coded keys instead of allocating three strings per posting. Persisted journal verification and atomic publication remain independent gates.

## Measured optimization work

- A cProfile probe of 2,000 prepared evaluations attributed 0.401 of 0.749 seconds to DLL path resolution. Resolving the default path once reduced that profiled workload to 0.204 seconds before the native scheduling change. These are attribution measurements, not latency percentiles.
- Serial coefficient reduction and cache diagnostics now run on the calling thread. They do not pay a Rayon worker handoff; heavy kernels retain bounded Rayon pools and ordered per-instrument reductions.
- The decision adapter caches the loaded DLL and its identity. File metadata changes reject reuse with a restart requirement. A prior constructor probe spent 0.121 of 0.140 seconds reading/hashing the DLL across 30 constructions.
- Effective-yield discounting uses a monthly recurrence instead of a transcendental power at every month/iteration. On 10,000 synthetic 360-month instruments and four threads, median warm time fell from **61.85 ms to 13.19 ms (4.69x)**. Maximum yield error against the independent Python reference was **7.23e-14**. [Before](2026-10-01-income-before.json), [after](2026-10-01-income-after.json).
- Product cashflow processing remains chunked; the immutable native market cache remains bounded. Inputs, output copies, active borrowers and interpreter memory remain additional to component budgets.

The accounting/KPI/unit/solver benchmark uses economically distinct instruments, equal seeds/path/thread budgets, fresh processes, cold/warm/edited reruns, final numerical comparisons and process peak RSS. The edit is a full rerun after a notional change, **not** native incremental invalidation. Eight paths measure throughput and parity; they do not demonstrate Monte Carlo convergence.

Validation: **620 engine tests, 94 API tests, 17 product Rust tests and 3 ledger Rust tests passed**. Strict clippy and formatting checks passed. API tests ran with fixed native binaries/source identity and temporary storage; one existing Starlette test-client deprecation warning remains. [Machine-readable validation](2026-10-01-owned-lifecycle-validation.json).

The initial API invocation from the monorepo root failed test collection because `app` was not on the import path; the complete API suite passed when run from `apps/api`, as required by that layer's instructions. This was a test-invocation error, not a product-code fix.

The pre-graph 60,000-instrument rerun compared **398,315 numeric values** at the unchanged `rtol=1e-7, atol=1e-5` gate.

| Raw accounting/KPI/unit/solver pipeline | Python | Rust |
|---|---:|---:|
| Cold total | 7.485 s | 5.261 s |
| Warm median total | 9.195 s | 6.741 s |
| Edited full rerun | 9.174 s | 6.762 s |
| Peak process memory | 660.4 MiB | 356.5 MiB |
| Interactive median | 0.0514 ms | 0.0922 ms |
| Interactive p99 | 0.0696 ms | 0.2519 ms |

[Raw results](2026-10-01-owned-lifecycle-benchmark-60000.json). This measured mixed pipeline is 1.36x faster with approximately 46% lower peak memory. The small Python coefficient evaluator remains faster than the Rust ABI call. Native interactive median improved from 0.4902 ms in the earlier run, but no improvement in overall warm pipeline time was demonstrated relative to that earlier Rust run.

The isolated effective-yield probe improved from 0.06185 s to 0.01319 s median (4.69x), with maximum yield error below 7.3e-14 against the reference. This does not translate directly into whole-book speedup: many benchmark instruments already supply book yields. [Before](2026-10-01-income-before.json), [after](2026-10-01-income-after.json).

## Performance practice and further work

This pass follows measurement-first optimization and targets observed allocation/transport costs and repeated hot-loop work. The Rust Performance Book recommends profiling before changing hot paths and reducing observed heap-allocation costs: [profiling](https://nnethercote.github.io/perf-book/profiling.html), [heap allocation](https://nnethercote.github.io/perf-book/heap-allocations.html).

Release builds already use optimization level 3, thin LTO and one codegen unit. These are measured choices, not guarantees that every workload becomes faster. [Cargo profile documentation](https://doc.rust-lang.org/cargo/reference/profiles.html). This change does not enable fast-math, change financial tolerances, or force a machine-specific CPU target.

Safe disjoint output slices and bounded chunk parallelism preserve the financial reduction order. More threads can lose on small batches; Rayon supplies disjoint chunk APIs but the workload must justify their use. [Rayon parallel slice documentation](https://docs.rs/rayon/latest/rayon/slice/trait.ParallelSliceMut.html).

Profile-guided optimization remains an experiment after the entire native lifecycle is available and representative training workloads can include cold/warm runs, edits, callable/withdrawable products, credit/ledger events and solver constraints. No PGO speedup is claimed. [Rust PGO documentation](https://doc.rust-lang.org/rustc/profile-guided-optimization.html).

## Requirements open at the 0.27.9 checkpoint

- Cross-book incremental resolution and native assumption validation are implemented in 0.27.9; the latest validation and scale results below supersede the earlier Python-ownership status.
- Remove Python numerical work from `strategy/decision.py` and what-if override/repricing coordination, including complete edit-to-solve-to-ledger replay.
- Raw daily specification defaulting/validation is implemented and has focused independent-reference tests. Review remaining public helper orchestration. Preserve explicit rejection of unmapped saved hedge trades until a validated trade/netting adapter exists.
- Unify native stage execution behind cancellation/deadline handling and bounded intermediate transport. The current per-entrypoint byte limits are not a global memory limit.
- Validate the complete 60,000-instrument graph/solver/daily-ledger workload, including cold/warm/edited cases and real concurrency/restart boundaries. The saved-book route still has a 250,000 cashflow-row budget; large monthly captures need streaming, not a larger unbounded JSON document.
- Profile and resolve the previously recorded raw-CD performance regression with a comparable workload. It is not declared fixed by the mixed-book benchmark.

Research approximations persist: heuristic regulatory weights, proportional survival of base expected cashflows in daily overlays, fixed 30-day reporting months, simplified forward-unit pricing, balance-scaled unit DV01 and current synthetic calibration data. HTM restrictions remain in daily replay and are not yet a category-aware unit-template LP constraint. Native parity does not validate a production GSIB model.

No deployment, live PostgreSQL, live S3/Iceberg or real-market model validation is claimed.

## Incremental graph follow-through (0.27.9)

`incremental.rs`, `graph_cache.rs` and `whatif.rs` now own the cross-book instrument
graph and temporary assumption domains/defaults. The public Rust path sends raw
contracts once and receives final position/portfolio results, without Python
financial callbacks. Base OAS and income yields remain anchored to the original
contracts under temporary edits. Explicitly supplied custom discount backends
retain their reference extension path.

Shared identity interning avoids repeated retained histories/model tables per
position; hash collisions still compare complete input and parent identities.
Dictionary/B-tree eviction avoids scanning all retained entries for each removal.
Mortgage missing-row batches borrow model inputs and copy only the selected
contracts. There is no full-book cashflow cube. Instrument retention is separate
from the existing bounded native market cache and request/transport memory.

The initial 45 incremental/what-if tests passed before the final cache interning
and native override additions. Final follow-through validation is recorded below.

## Native graph scale baseline before compact-result retention

The 60,000-position, 8-path, 27-month, four-thread graph/unit/solver comparison passed **1,696,660 numeric comparisons** at the unchanged combined relative/absolute tolerance. Maximum absolute numeric difference was 0.00341. The temporary edit changes one loan coupon while retaining original OAS; both backends used the same 128 MiB instrument-cache limit and fresh processes.

| Graph plus unit library and HiGHS | Python | Rust |
|---|---:|---:|
| Cold total | 24.016 s | 13.436 s |
| Warm median total | 25.078 s | 15.136 s |
| Temporary contract edit | 24.976 s | 15.378 s |
| Peak process memory | 1,280.2 MiB | 592.7 MiB |

[Raw results before compact retention](2026-10-01-native-graph-benchmark-60000.json). Rust was 1.66x faster warm and used approximately 54% less peak memory. This benchmark includes parallel risk and income, but excludes the full key-rate sweep and daily ledger. Eight paths are a throughput/parity fixture, not Monte Carlo convergence evidence.

Neither implementation reused instrument cashflow/mark/income nodes between full-book passes at this budget. The native optimization therefore retains compact per-instrument results ahead of transient cashflow arrays, within the same budget. It excludes notional from retained result identity and re-scales outputs when notionals change. Updated measurements follow after validation.

## Allocation and concurrency ablations

Opt-in `compute-profile` builds instrument native stages; profiling hooks compile out of normal builds. Inclusive nested timings cannot be added. Disabling Numba pool initialization did not remove the observed same-process slowdown. An isolated `allocator-mimalloc` feature improved the profiling probe modestly but did not remove it. The normal build remains on the system allocator; no allocator production speedup or memory improvement is claimed. [System FFI](2026-10-01-native-ffi-profile.json), [without Numba pool](2026-10-01-native-ffi-no-numba-profile.json), [optional allocator](2026-10-01-native-ffi-mimalloc-profile.json).

The optional allocator is pinned and its upstream MIT notices are included. Global allocator experiments require both throughput and retained-memory evidence, particularly in a long-lived worker. [mimalloc Rust wrapper](https://docs.rs/mimalloc/latest/mimalloc/index.html), [Microsoft mimalloc](https://github.com/microsoft/mimalloc).

## Native raw daily-spec boundary

Rust now owns required fields, typed defaults, numeric domains, IDs/references, collateral links, scheduled cashflow coverage, work admission and opening equity reconciliation. The stream adapter only transports raw JSON and formats typed output. The native process independently validates again before running. The existing explicit 60,000-position/90-million-work-unit large-book tier and 250,000 captured-cashflow-row bound are retained.

Focused raw-spec and streamed-ledger tests passed **71 tests**. The final combined regression evidence follows below. No hedge mapping, HTM optimizer-category support, production regulatory certification, or unified runtime completion is implied.

## Compact graph retention results

The optimized 60,000-position graph/unit/HiGHS run retained 60,000 compact position results inside the same 128 MiB instrument-cache budget. Transient cashflows cannot evict these results. Complete input and parent identities still determine reuse; notionals rescale the retained per-unit outputs.

| Same graph/unit/HiGHS workload | Python | Rust |
|---|---:|---:|
| Cold total | 24.530 s | 15.532 s |
| Warm median total | 24.722 s | 2.443 s |
| Temporary one-loan coupon edit | 25.169 s | 2.394 s |
| Peak process memory | 1,274.4 MiB | 570.8 MiB |

[Raw optimized results](2026-10-01-native-graph-optimized-60000.json). Warm Rust execution is **10.12x faster**, edited execution **10.51x faster**, with **55.2% lower peak memory** in these fresh-process measurements. All **1,696,660 numeric comparisons** passed the unchanged rtol=1e-7/atol=1e-5 gate. Maximum absolute numeric difference was 0.0034098625. Eight paths are a throughput/parity fixture, not a convergence result.

Warm instrument work was empty. The edit recomputed one loan's original calibration when its transient OAS node had been evicted, four cashflow nodes, three marks and one income node. That calibration still used the original contract and original market. It did not fit the edited contract to its old quote. Cold Rust time increased from 13.44 s before compact retention to 15.53 s in the later run; these are separate process measurements, not a controlled causal ablation.

This table excludes full key-rate sweeps, daily ledger execution and HTTP transport. The new native workflow introduced subsequently must be measured separately.

The 0.27.9 regression checkpoint covered all 642 engine cases across a full invocation (641 passed, one obsolete Python-cache ownership assertion failed) and a corrected single-test rerun (passed). No numerical tolerance was changed. The API suite passed 94 tests with fixed source/native identity; product Rust tests passed 19 and ledger tests 3, with strict default clippy passing. The 0.28.0 orchestration work has additional validation gates; these checkpoint counts do not certify that newer implementation.


## Superseding 0.28.0 evidence

[Native workflow migration and optimization report](2026-10-01-native-owned-workflow.md)
records the completed native decision/what-if/public-controller orchestration,
unified graph-to-daily-ledger executable, independent final-output gates, complete
60,000-instrument runs, and subsequent allocation/scheduling improvements. The
open implementation items in the historical checkpoint above are not the current
ownership inventory. Model limitations remain separately disclosed.
