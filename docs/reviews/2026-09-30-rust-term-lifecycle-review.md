# Corporate and CD Rust lifecycle implementation review

Engine 0.27.7, native product ABI 7. This completes the raw corporate/CD
spot-risk migration. The full application migration remains in progress in the
[acceptance ledger](2026-09-30-rust-lifecycle-migration.md). HiGHS remains C++.

## Execution ownership

| Stage | Python reference | Selected Rust backend |
|---|---|---|
| Raw input transport and returned table formatting | Python | Python adapter, or standalone JSON executable |
| Dates, calendars, accruals, coupon schedules | Python conventions and product decks | Rust `conventions` and `term_deck` |
| Amortization, calls/puts, CD withdrawal terms | Python deck | Rust typed contract normalization |
| Curve/PCA/volatility calibration | Python/NumPy/SciPy | Rust, with shared immutable stage cache |
| Random draws and scenario paths | Python driver / compiled kernels | Rust shared seeded tape and market context |
| Base OAS, fixed-spread curve and volatility bumps | Python driver / compiled kernels | Rust `term_risk`, one call for the complete raw book |
| Final position price, dollar KRD/vega and DV01 | Python | Rust |
| Cross-book NII/KPIs, unit-library builds, final ledger reports | Existing mixed implementation | Still requires further lifecycle migration |

`run_corp_risk` and `run_cd_risk` pass raw contracts rather than Python-built
schedule arrays. Ownership tests forbid Python financial stages during execution.
Public `CorpDeck`/`CDDeck` also use Rust normalization when native computation is
selected, so accounting and hedge consumers share the same schedule implementation.
Their surrounding drivers are not thereby fully migrated.

The executable `portfolio-term-risk` accepts a single JSON envelope containing
`schema`, `threads` and `request`, with schemas `term-risk-1` and `term-deck-1`.
Python's synchronous FFI adapter uses the same raw JSON input and receives
contiguous numeric result buffers, avoiding bulk result JSON conversion. The
standalone executable needs no Python interpreter. Dates are ordinal integers, option schedules are
date/value pairs, and calendar extras are explicit inputs. Rust returns a typed
result or an error envelope. Unknown fields and malformed contracts fail;
custom Python calendar subclasses are rejected explicitly.

## Review findings and retained assumptions

- Fixed OAS and shared random numbers remain invariant across bump sides. Native
  rate-only paths retain float64 storage, matching the reference path.
- Complete schedules match the reference for all four day counts and all four
  business-day conventions, including stubs, leap days, holidays, extra holidays,
  duplicate option dates and terminal-grid clamping.
- The shared 128 MiB immutable cache now includes corporate/CD rate paths. Its
  identity includes rates, volatility parameters, factor loadings, seed and grid.
  Contract changes recompute cashflows/OAS/results while reusing market stages.
  Successful stages may survive a later request failure; no partial financial
  result is published. Warm and edited results are checked against cleared-cache
  rebuilds. Cache retention excludes active borrowers and transport copies.
- Cashflows use at most 256 contracts per risk chunk. There is no
  paths-by-instruments-by-months cube. Date/schedule construction admits at most
  4096 periods per contract and 1,048,576 per constructed deck. JSON input/output
  caps are 64/128 MiB for the standalone/deck protocols; FFI risk numeric output admission is capped at
  1 GiB. These are component budgets, not a total process memory guarantee.
- Corporate/CD kernels write directly into preallocated CSR buffers using
  disjoint mutable row slices. This removes three small allocations per
  instrument per scenario and the later assembly copies, while keeping the
  path reduction order unchanged. The same kernel serves risk and income users.
- Raw-input and standalone JSON transport adds serialization and copies. Python
  risk outputs use contiguous numeric buffers; failed requests leave all caller
  buffers untouched. Arrow input transport,
  unified cancellation/deadlines and large-workload cache-pressure optimization
  remain future work. When scenario paths exceed cache retention, later chunks
  may recompute evicted paths.
- Financial approximations are preserved, not upgraded: adjusted accrual dates,
  simplified US holiday rules, year-scoped New Year observation, linear-principal
  "annuity", heuristic call/put exercise and CD withdrawal, and long-maturity
  terminal clamping. A calendar provider and product-specific convention/model
  validation are still needed before production use.
- The review corrected integer 0/1 floating flags at serialization, reduced
  protocol enum stack size, and fixed a test assertion for Polars Object schedule
  columns. Numerical acceptance tolerances were not loosened.

## Verification and comparison

All **541 engine tests**, **93 API tests**, and **17 Rust unit tests** passed.
The 64 new schedule/ownership/cache tests are included in the engine total.
Rust formatting and all-target clippy with warnings denied also passed. API
durable-job validation ran with source and build identity held fixed. One existing
Starlette TestClient/httpx deprecation warning remains.

The [machine-readable validation](2026-09-30-rust-term-lifecycle-validation.json)
records suite results and source/binary identities. The
[reproducible benchmark](2026-09-30-rust-term-lifecycle-benchmark.json) compares
fresh-process, warm and single-notional-edit runs with every timed output
parity-gated. Imports are outside timing; first-call Numba compilation/cache load
is inside timing. Peak memory includes the Python host, imports, transport and
retained outputs for both backends.

Measured on this Windows host with 8 paths, 360 monthly steps and 4 worker
threads. Warm times are medians of three repetitions; the edit changes one
instrument's notional by 1%. Corporate books repeat the demo loan templates;
CD books use the synthetic CD generator. These are product-driver benchmarks,
not a diversified full balance sheet or regulatory model validation.

### 256 instruments per product

| Product/backend | First call (ms) | Warm median (ms) | Edited (ms) | Peak process memory |
|---|---:|---:|---:|---:|
| Corporate Python | 544.801 | 31.895 | 31.309 | 179.5 MiB |
| Corporate Rust | 86.927 | 6.294 | 6.294 | 143.0 MiB |
| Cd Python | 530.032 | 24.612 | 24.787 | 169.3 MiB |
| Cd Rust | 85.395 | 4.275 | 4.212 | 132.6 MiB |

Warm Python/Rust runtime ratios: corporate: 5.07x, cd: 5.76x.

### 60,000 instruments per product

| Product/backend | First call (s) | Warm median (s) | Edited (s) | Peak process memory |
|---|---:|---:|---:|---:|
| Corporate Python | 3.775 | 4.845 | 4.809 | 504.3 MiB |
| Corporate Rust | 1.372 | 2.964 | 3.033 | 302.1 MiB |
| Cd Python | 1.244 | 1.416 | 1.381 | 318.1 MiB |
| Cd Rust | 0.925 | 1.930 | 1.975 | 321.6 MiB |

Warm Python/Rust runtime ratios: corporate: 1.64x, cd: 0.73x. Every timed final output
passed rtol=1e-7/atol=1e-5. The largest absolute difference across columns was
1.17e-06, in the corresponding column's units.
Native warm and notional-edit runs added no market-stage misses. Full data is
in the [60,000-instrument comparison](2026-09-30-rust-term-lifecycle-60000-benchmark.json).
These are observations for this fixture/host/path budget, not universal speed
or memory guarantees. Cold and warm timing includes allocation/runtime effects;
cache hits alone do not guarantee that a repeated run is faster.

### Scale review corrections

The first scale run exposed a CD regression despite a small-book speedup. It is
preserved in [the initial result](2026-09-30-rust-term-before-transport-review.json).
Profiling motivated flat CSR output buffers (removing millions of small kernel
allocations) and a contiguous numeric FFI response (removing bulk result JSON).
The benchmark now retains compact numeric results between samples, avoiding
unrelated GC scans of millions of earlier Python list objects. The initial run
therefore is not a controlled single-variable optimization comparison.

The [pre-numeric-output measurement](2026-09-30-rust-term-before-numeric-output.json)
uses the corrected compact retention and flat CSR kernel, but still serializes
bulk risk results as JSON. It is retained for comparison with the final numeric
response. Further optimization opportunities include columnar raw input and
cache-pressure-aware scenario/chunk scheduling at larger path counts.

Run the comparison with:

```powershell
uv run --project apps/api python scripts/build_native.py
$env:NUMBA_NUM_THREADS='4'
uv run --project apps/api python scripts/benchmark_term_lifecycle.py --positions 256 --paths 8 --threads 4 --repeats 3
uv run --project apps/api python scripts/benchmark_term_lifecycle.py --positions 60000 --paths 8 --threads 4 --repeats 3 --output docs/reviews/2026-09-30-rust-term-lifecycle-60000-benchmark.json
```

This is a synthetic corporate/CD spot-risk comparison, not the 60,000-instrument
mixed-book, strategy and daily-ledger acceptance run. Local API tests do not
constitute live PostgreSQL, S3 or Iceberg deployment validation.

## Remaining lifecycle order

1. Complete deposit/hedge and remaining raw-product drivers and scenario loops.
2. Move cross-book NII/accrual/KPI aggregation and unit-library pricing into Rust.
3. Connect the global dependency graph to instrument edits, pricing, Rust-owned
   strategy construction and the existing C++ HiGHS solve.
4. Complete saved-book mapping, candidate replay, daily journal reconciliation
   and final report assembly, with unified cancellation/deadline controls.
5. Run equivalent full 60,000-instrument cold/warm/edited workloads, including
   solver and ledger results, peak memory and durable publication/restart gates.
