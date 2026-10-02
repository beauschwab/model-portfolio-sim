# Code review remediation — 2026-09-28

Implemented against the review of checkout `66fdb36`; engine version **0.18.0**.
The 17 priority findings and seven optimization items have corresponding code
changes and regression coverage. This document supersedes the original review's
description of the working tree; that report remains the pre-change evidence.

## Priority findings

| Review item | Resolution | Evidence |
|---|---|---|
| 1. Reversed MBS vegas | Up-minus-down vol differences | Complete batch/sequential driver parity, including custom HPI |
| 2. Optimizer purchase timing | One shifted/truncated coefficient convention for LP and evaluation | Controlled late-purchase fixture now selects month zero; total objective 2,800 includes 100 base income |
| 3. Stressed earnings/objective | Separate NII per scenario; total base-plus-overlay maximin objective | Adapter test checks distinct scenario earnings; solution replay |
| 4. Scenario OAS | Original-market calibration passed to product risk/stress/KPI drivers; spread shocks explicitly shift it | No-resolve tests, zero-scenario spot mark with distinct base/sensitivity counts, real HTTP scenario jobs |
| 5. Book saves | Typed dates, schedule reconstruction, finite/domain checks, atomic replacement | Six-product JSON round trips and browser save; malformed submissions rejected |
| 6. Strategy caches | Revision invalidation; complete library/KPI bundle published atomically | Failed/revision-obsolete builds cannot replace the previous valid bundle |
| 7. Ignored settings | Scoped actual path counts, base count, horizon and assumptions; exact odd CRN dimensions | Runtime probes; real 33-path, six-month API jobs; extended library grid test |
| 8. NSFR equity | Supplied equity or complete balance-sheet reconciliation including MM | Explicit/implicit equity equality fixture |
| 9. Net dashboard sensitivity | Liability signs subtracted; hedge risk included for full-book runs | Browser fixture proves equal asset/debt sensitivities cancel; API verifies hedge result |
| 10. Mutable queued inputs | Deep input snapshot and revision captured at submission | Queued seed 7 remains 7 after live state changes to 999 |
| 11. Thread reset | Apply supported maximum when zero is requested, on every job | Persistent-worker one-thread then all-thread regression |
| 12. Custom model loss | Suite propagated through setup and every path build | Full custom-model risk parity test |
| 13. Shortened long swap tenor | Full requested tenor with explicit terminal-forward extrapolation | Hand-computed 30-year par-rate fixture |
| 14. Editor remounts | Nested component types replaced with stable rendered subtrees | Multi-character focus and repeated position-slider edits in Edge |
| 15. Stale strategy response | Abort and discard obsolete requests with effect cleanup | Controlled delayed-response browser test |
| 16. KPI timing | Current ratios use active opening-month cohorts; horizon capital uses horizon income/RWA; DV01 scales with balance | Future, beyond-horizon and matured-cohort tests; partial-quarter capital test |
| 17. Separate panel jobs | Shared queue, revision-aware deduplication, telemetry and KPI publication | Real browser KPI run updates masthead and Morning Sheet |

## Optimizations and measurements

| Change | Result |
|---|---|
| Run-scoped market reuse | Content-keyed calibration, volatility features and rate paths; read-only arrays; 128 MiB LRU limit |
| Duplicate accounting work | Removed unused MBS OAS solve and second identical deposit cashflow pass |
| Library path reuse | Product passes share the same bounded run context |
| React updates | Separate stable data context from telemetry; memoized dashboard and position vectors |
| Loading | Lazy panels with loading/error boundaries and persisted layout compatibility |
| Job storage | Serialize once; retain serialized bytes with count/age/byte limits; bounded admission queue |
| Tables | At most 100 rendered rows per page, with accessible paging; position vectors reused |

Paired warm synthetic KPI benchmark: **1.250 s uncached → 0.581 s cached**
(medians of three samples, about **53% less time**). Both use four Numba threads,
128 sensitivity paths, 128 MBS calibration paths, and a 27-month horizon.
Calibration calls fall from **6 to 1** and rate simulations from **22 to 3**;
cached results match uncached serialized results **byte for byte**. Retained
run-cache arrays occupy approximately **6.38 MiB** in this fixture.

The same fixture's warm unit-library build took **0.571 s**. Synchronous two-row
strategy evaluation across 1,000 calls measured **17.0 µs median / 17.5 µs p95**.
These timings exclude HTTP transport and cold JIT compilation. The original
review's 1.002-second sample used the old computation path, so it is not the
paired uncached baseline above. Raw samples: `2026-09-28-benchmark.json`.

Production web build main JavaScript: **1,129.42 → 680.86 kB**; gzip
**296.30 → 173.27 kB**. This is the entry chunk, not total transferred JavaScript
or a measured page-load improvement. Default visible chart panels still load
their shared chart chunk. Vite continues to warn about the >500 kB entry chunk.

## Validation and reproduction

- `bun run test`: **49 engine tests, 27 API tests, 6 browser tests passed**.
- API integration uses real numerical drivers for base/named risk, named stress,
  NII, scenario forecast comparison, KPI calculation, library build, synchronous
  allocation evaluation and a two-market optimizer with independent replay.
- Browser tests use fresh local services and Edge; focus, popover persistence,
  layout restoration, table paging, book saves, response ordering, shared results,
  and signed risk aggregation are exercised.
- NumPy **2.0.0** lower-bound environment: **48 tests passed** at the compatibility
  checkpoint, before adding the extended-horizon grid regression.
- `bun run build`: TypeScript and production bundle passed.
- `git diff --check`: passed.
- Reproduce paired timings with
  `uv run --project apps/api python scripts/benchmark_review.py`.
- Embedded operator skill documentation is updated; regenerate its `.skill`
  archive with `python scripts/package_engine_skill.py`.

## Deliberate model boundaries

The funding default is zero external budget: new asset balances require matching
new liabilities throughout the horizon. An explicit positive `cash_budget` means
additional committed funding outside the base book; it does not release existing
reserve cash or automatically price borrowing. Its cost must already be in the
plan. Surplus funding earns no extra cash carry in this overlay model.

The optimizer enforces the exact Level 2A cap and replays its objective, monthly
liquidity/EVE/funding constraints, capital, commercial rows and asset cap. Base
regulatory components and template weights remain static approximations. Forward
DV01 scales a unit's initial sensitivity by outstanding balance; it is not a new
risk simulation at every future date. Terminal-forward extrapolation maintains
swap tenor without extending the stochastic grid.

The nine-quarter comparison is labeled as independent forecasts of the unchanged
book. It does not roll cohort state through a quarterly scenario path, and spread
does not change contractual NII. OAS solvers now reject unsupported convergence
failures; prepay edits still report `RESTART_REQUIRED`.

Validation used synthetic local data. No deployment, live portfolio, provider,
or production-model validation is claimed. Remaining build/test warnings concern
bundle size and existing FastAPI/Starlette deprecations. Existing unrelated
`.serena/project.yml` edits were preserved; TypeScript's existing build-info file
was regenerated by the build.
