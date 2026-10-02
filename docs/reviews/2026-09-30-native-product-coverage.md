# Native product and strategy coverage comparison

30 September 2026 · Engine 0.27.0 · Windows local validation

The built-in product computation path now has a native implementation and passes
the Python reference comparisons. The application remains hybrid: Python owns
input preparation, calibration controllers, orchestration, reporting and
independent verification. This is not a standalone Rust rewrite or proof of
equivalence for every possible input.

## Implemented coverage

| Component | Python reference | Native selection | Validation |
|---|---|---|---|
| Shared LMM rate paths | Numba | Rust/Rayon, same supplied CRN | Odd path counts, unequal base/risk counts, final risk outputs |
| CC, primary spread, HPI, deposit ECM paths | NumPy/Python recurrence | Native batch recurrence | Path and final-output comparisons |
| Rebonato volatility | NumPy contractions | Native values and analytic abcd derivatives | Central-difference derivative check, fitted parameter parity |
| Mortgage/whole-loan cashflows | Numba A-matrix kernel | Native base, scenario batch and checkpoint restart | All seven outputs, interest/principal, balances, float32 checkpoints, forward stress |
| Corporate/commercial loans and debt | Numba exact-time CSR | Native CSR with fixing interpolation, calls/puts and amortization | Cashflows, prices, KRD and vega outputs |
| CDs | Numba | Native withdrawal/call/maturity cashflows | Cashflows and final risk |
| Non-maturity deposits | Numba | Native attrition/runoff/interest and checkpoint stress | Cashflows, risk and stress |
| OAS/PV | NumPy/Numba | Native monthly and CSR batch solvers | Roundtrip, nonconvergence rejection, fixed-OAS scenarios |
| Swaps/swaptions and money markets | Existing drivers | Native corporate legs, swaption payoff and MM income batches | Final hedge risk and aggregate accounting/KPIs |
| Effective-interest income and CSR accrual | Python/NumPy | Native IRR, income, smear/bucket operations | Final NII and captured product-flow comparisons |
| Forward programs | Python/NumPy | Native fixed/floating income, balances and DV01 | Reference comparisons |
| Unit library | Python orchestration, reference products | Same orchestration, native product batches | Complete library output comparisons |
| Public robust optimizer | SciPy/HiGHS | Rust owns C++ HiGHS solve | Feasible/infeasible cases, monthly funding, late purchases, binding limits, independent replay |
| Decision sessions | Optional native graph/actor | Native graph with native product rebuilds | Edits, constraints-only updates, reset, rollback, isolation, full repricing equivalence |
| Saved-book/candidate daily replay | Python state loop | Native product capture followed by native daily state loop | Named financial columns in all output frames and persisted journal replay |
| Settings and durable jobs | Python API/worker | Backend snapshot selects native work | Real separate processes, restart/reload, SQLite and temporary local PostgreSQL |

The fast interactive strategy evaluator is intentionally still coefficient-only
Python; it never invokes a pricer. Independent Python replay is retained as a
correctness check. The optimizer's algorithm is HiGHS (C++), not a newly written
Rust LP solver. Static typed schedules, RNG/CRN generation, curve bootstrap, PCA,
OLS/nonlinear fit controllers, shock construction, some vector aggregation and
accounting loops, JSON/Polars/reporting and storage remain shared.

## Financial and operational boundaries

- Fixed base OAS and shared draws remain required. Calibration uses analytic
  abcd derivatives in **both** backends: finite-difference noise otherwise
  amplified tiny native volatility differences into failing final risk outputs.
- LCR composition-cap branches, NSFR, funding, EVE, horizon CET1, total-asset and
  commercial rows remain covered by the same optimizer contract. HTM category
  rules remain part of daily stress/candidate acceptance; the unit-template LP
  does not gain an independent accounting-category HTM constraint in this port.
- Monthly captured cashflows, proportional surviving balances, duration-based
  scenario marks, simplified regulatory weights and NII-based capital projection
  remain model approximations. Native parity does not establish regulatory or
  full real-world GSIB coverage. Saved-book hedge mapping remains unsupported even
  though standalone hedge analytics have native pricers.
- Custom Python CC/PS/HPI/prepayment suites and custom deposit-rate models are
  rejected explicitly in native mode. They remain supported by the Python path.
- ABI version/shape checks, bounded thread selection, panic containment and
  all-output validation precede publication. Buffers must remain live, aligned
  and nonoverlapping. Float32-to-f64 conversion and owned native result buffers
  can copy; this is not a zero-copy Arrow boundary.
- PostgreSQL and SQLite retain their current roles. Parquet/artifact publishing,
  persisted replay and reports remain shared Python. No live AWS or Iceberg
  catalog rollout was performed in this task.
- Source changes require a native rebuild and worker restart. Native-specific
  tests skip when libraries are absent; the local native release gates reported
  here ran with the libraries present. Linux/Windows CI is added but remote CI
  execution is not claimed.

## Selecting and reproducing the implementation

```powershell
uv run --project apps/api python scripts/build_native.py
uv run --project apps/api python scripts/build_ledger_native.py
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py
```

Save `compute_backend: "rust"` in `/settings`, or choose **Rust** under
**Assumptions & Settings → Product simulation backend**. Queued jobs retain their
saved setting even if the current setting changes. Rebuild existing unit libraries
after switching. Python remains the default. The older pricing request `backend`
option chooses only prepared-cashflow discount reduction; it is a separate switch.

```python
from portfolio_risk.core.runtime import RunConfig, run_context
with run_context(RunConfig(n_paths=128, n_paths_base=512, compute_backend="rust")):
    # Call the existing product/risk/accounting/strategy drivers here.
    pass
```

## Measurements and validation

Fresh-process comparison uses five synthetic product books in their original
proportions, 128 paths, a 27-month accounting horizon and four compute threads.
The timed graph includes spot valuation, curve sensitivities and monthly NII.
Each backend runs in its own process: one cold invocation, three uncached repeats,
then a cache-reuse attempt (which can miss under pressure). Unit-library/base-KPI build and optimizer solve are
timed separately. Memory is Windows peak resident working set over that whole
child process, including imports, all repeats and strategy work, not isolated
kernel memory. Cold timing can use an existing on-disk Numba compilation cache.
Warm native timing includes FFI conversions and output copies. No speedup claim
is inferred from language choice.

Benchmark results and validation counts are recorded below. The prior
60,000-instrument daily-ledger measurement is a
different workload and must not be treated as a measurement of these new product
kernels.

Sources: [native batch kernels](../../packages/portfolio-risk-native/src/quant.rs),
[Python dispatch](../../packages/portfolio-risk/src/portfolio_risk/core/quant_native.py),
[parity gates](../../packages/portfolio-risk/tests/test_native_products.py),
[benchmark harness](../../scripts/benchmark_native_products.py),
[durable worker tests](../../apps/api/tests/test_durable_processes.py),
[previous daily-ledger capacity report](2026-09-29-operations-and-parity.md).


### Measured comparison

| Instruments | Backend | Cold graph (s) | Warm uncached median (s) | Cache-reuse attempt (s) | Recomputed nodes | Peak RSS (MiB) | Library + base KPIs (s) | LP call (ms) |
|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 375 | python | 2.657 | 2.538 | 0.112 | 0 | 391.6 | 0.477 | 3.51 |
| 375 | rust | 2.982 | 3.046 | 0.102 | 0 | 357.9 | 0.604 | 13.76 |
| 6,000 | python | 18.618 | 18.871 | 20.199 | 288,070 | 1035.3 | 2.146 | 4.12 |
| 6,000 | rust | 30.320 | 30.275 | 32.062 | 288,070 | 1023.6 | 4.094 | 16.65 |

Rust was approximately **20% slower at 375 instruments and 60% slower at 6,000**
for warm uncached graph analytics. Peak RSS was about 9% lower at 375 and only
1% lower at 6,000. These measurements do not justify changing the default.
The strategy timing includes library/base calculations using the selected
products; its LP measurement includes bridge overhead and first library loading,
so it is not an isolated steady-state HiGHS kernel comparison.

At 375 instruments the reused graph computed zero nodes. At 6,000 the 512 MiB
retained-cache budget caused eviction and 288,070 recomputed nodes in both
backends. Its repeat timings must not be advertised as cache-hit latency.
Accounted cache bytes are not process RSS. Repeated input/output allocation,
float32 conversion, native loop efficiency, graph hashing and cache pressure are
optimization candidates; this benchmark does not isolate their individual cost.

Both comparisons passed with `rtol=1e-7, atol=1e-4`. Maximum instrument-output
absolute difference was below 8.6e-8; aggregate monthly NII differed by at most
3.28e-7 and optimizer objective by 2.33e-10. The financial columns mix units;
the JSON keeps errors separated by book/NII/objective. Regression gates retain
their own stricter kernel and final-output tolerances.

Reproduce with `scripts/benchmark_native_products.py --positions 375 --paths 128`
or `--positions 6000 --paths 128`, using `uv run --project apps/api python`.
Raw samples, binary identity and harness hash: [375 instruments](2026-09-30-native-products-375.json),
[6,000 instruments](2026-09-30-native-products-6000.json).

### Validation completed

- Full engine suite: **311 passed in 124.97s (0:02:04)**, with native libraries present and no skips.
- Full API suite: **123 passed, 1 warning in 153.16s (0:02:33)**, including SQLite and temporary local PostgreSQL, separate API/worker processes, queued-backend immutability and recovered libraries.
- Product Rust crate: 3 unit tests; decision crate: 4 unit tests; formatting and Clippy with warnings denied passed. The new product FFI is exercised through the Python native parity suite, including malformed shapes and atomic failure.
- Browser: backend selection/save/reload passed, no page exceptions or Vite error overlay. Screenshot reviewed. TypeScript/Vite production build passed; existing chunk-size advisory remains.
- Guide: structure/references, 25 chapters, 263 rendered equations, desktop/mobile/print behavior and architecture diagrams passed. These are documentation checks, separate from numerical validation.
- Operator skill mirrors refreshed and `.skill` archive rebuilt/verified. Engine version and lockfiles updated to 0.27.0.

The justified next optimization step is measured profiling of this mixed workload,
followed by reducing native copies and retained graph pressure while preserving
the independent Python oracle. A full Rust rewrite, production throughput or
60,000-instrument product-risk advantage is **not** established by this work.
