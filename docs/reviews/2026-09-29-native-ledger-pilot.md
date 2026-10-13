# Native ledger pilot: implementation and measured comparison

29 September 2026 · Engine 0.25.0 · Local Windows validation

> Follow-up: [0.26 full daily state and partitioned journals](2026-09-29-native-state-streaming.md)
> extends this reduction-only pilot. The measurements below retain their original scope.

## Result and decision

The optional Rust journal pilot is implemented and integrated into the full
balance-sheet stress engine. At 2,000 positions, 180 days and two scenarios,
simulation plus local Parquet round-trip validation took **5.105 seconds versus
8.830 seconds** with the original Python journal: **1.73× faster**. Median sampled
peak process RSS fell from **1,246 MiB to 827 MiB**, a **33.6% reduction**.

This does not meet the earlier proposed promotion gate of 2× complete-request
throughput or 50% lower peak RSS. The pilot remains **opt-in through the engine**;
the API/UI and default engine path retain the original Python journal. It
demonstrates a useful native computation boundary, not a completed Rust daily
financial state machine or evidence for rewriting the backend.

The columnar Python control is essential to interpreting the result: it used
820 MiB at the same size. Almost all measured memory improvement comes from
compact storage rather than Rust itself. Rust adds a **1.51× speedup over that
columnar control** on the 180-day workload.

## Delivered scope

`packages/portfolio-ledger-native` is a dependency-free Rust crate exposed through
a versioned C ABI. `analytics.ledger_native` provides its Python adapter and a
columnar Python control. `run_balance_stress` accepts a keyword-only
`journal_backend` of `python`, `columnar` or `rust`.

| Responsibility | Owner in this pilot |
|---|---|
| Product cashflows, duration marks, credit/default behavior | Existing Python/Numba engines and Python event generator |
| Policy decisions, timing and collateral/funding state transitions | Existing Python daily simulation |
| Posting labels, stable event order, transaction boundaries | Python event generator and compact buffer builder |
| Transaction balance checks and ordered GL reduction | Rust for the native path; Python in both controls |
| Daily GL versus expected-subledger checkpoint | Rust for the native path |
| Complete closing journal replay from zero | Rust for the native path |
| Closing statements, limits, breaches and reverse stress | Existing Python engine |
| Table conversion and local Parquet output | Polars through Python |
| Durable worker, revision fencing, SQLite/Postgres, S3 | Existing architecture; API continues using the default Python path |

The native kernel is stateless between calls. It borrows dense numeric arrays
and CSR transaction boundaries, stages changes privately, checks every
transaction and checkpoint, then returns a new GL vector. Python retains the
returned vector and the account/GL/instrument label mapping. There is one call
per daily checkpoint, including opening day, plus a full replay at close. This
avoids per-position/day FFI calls without pretending financial event generation
has already moved to Rust.

The buffer builder retains compact key/value arrays and per-transaction metadata
instead of a Python dictionary per journal line. Columnar output expansion occurs
once at close. Both the Rust and columnar controls use this layout. The original
Python path now constructs each scenario's journal frame before concatenation,
rather than retaining a second aggregate list of all scenario dictionaries.
The benchmark compares all backends on this same 0.25 source snapshot.

## Safety and accounting contracts

- Native inputs are owned copies during the GIL-releasing call. These copies are
  included in timing; this is not a zero-copy Arrow C Data implementation.
- No global mutable native state, retained pointer or native session lifecycle
  exists. Independent calls can run concurrently; each simulation's Python
  journal object remains private to that simulation.
- Invalid dimensions, offsets, GL keys, nonfinite values, unbalanced transactions,
  arithmetic overflow and mismatched subledgers fail explicitly. Native output
  buffers are unchanged on failure; no silent backend fallback is allowed.
- Transaction sums retain cancellation terms. GL balance updates preserve the
  reference's sequential floating-point order. Existing reconciliation tolerances
  were not loosened. This remains simulation arithmetic, not rounded production
  accounting.
- A pilot posting error can be detected at the next daily checkpoint instead of
  at the individual `post` call. The containing simulation aborts before returning
  results. Pending buffers are not a recoverable or published audit log.
- Tests compare native and reference journal rows, trial balances, daily paths,
  closing statements, consolidated balances, actions, breaches, reverse grids and
  other public frames exactly. They then independently replay native-produced
  lines using the original Python implementation.
- Native runs use native closing replay internally. They do not also run the
  slower Python replay during normal execution; that independent implementation
  is a parity-test gate. Timing includes all checks performed by the selected
  backend, with this distinction made explicit.
- `execution` records the selected backend and native binary SHA-256. The Rust
  option is engine-only; this metadata is not a replacement for durable worker
  model/binary admission checks if the pilot is later exposed through the API.

## Measured workload

Synthetic loans with daily floating-rate accrual, baseline and +200 bp scenario,
full journal retention, every daily subledger check, full closing replay, output
conversion, then local Parquet write/read/equality checks for every output table.
No product path generation, API/network request, durable manifest publication,
S3/Iceberg or process startup time is included. This is a controlled engine/I/O
slice, not production request latency.

Each cell is the median of three separate fresh processes. Backend order rotates
between repetitions. RSS is sampled every 5 ms, includes Python/native/Polars
memory and retained output, and can miss briefer peaks. Output fingerprints match
across all backends and repetitions at each workload, excluding only backend
identity metadata. Source and DLL hashes were unchanged during measurement.

| Positions × days; two scenarios | Original Python time | Columnar Python time | Rust time | Original / columnar / Rust peak RSS |
|---|---:|---:|---:|---:|
| 500 × 30 | 0.477 s | 0.416 s | 0.302 s | 217 / 194 / 195 MiB |
| 2,000 × 30 | 1.611 s | 1.478 s | 1.050 s | 405 / 320 / 319 MiB |
| 2,000 × 180 | 8.830 s | 7.728 s | 5.105 s | 1,246 / 820 / 827 MiB |

Times include local Parquet round trips. For the largest workload, engine-only
medians were 8.606 / 7.541 / 4.920 seconds. Corresponding round-trip medians were
0.214 / 0.192 / 0.190 seconds. Medians of components need not sum to the median
combined time. The three largest-workload combined samples ranged from
8.545–8.852 s (original), 7.708–7.772 s (columnar), and 5.080–5.113 s (Rust).

The largest workload produced **1,492,004 journal lines**, **129.99 MiB** of
columnar journal data, and **3.23 MiB** of total compressed Parquet tables.
The fixture is highly repetitive and compressible; those storage ratios are
not forecasts for a bank's heterogeneous journal. Hardware is the same local
i7-14700 workstation used in earlier comparisons, not a reserved benchmark host.

[Raw results, all samples, hashes and prepared-kernel timings](2026-09-29-ledger-native-benchmark.json).
The prepared reduction comparison excludes financial event construction and
output conversion, so its speedup must not be presented as application speedup.

## Validation

Final source-frozen validation:

| Check | Result |
|---|---|
| Complete engine suite, including existing native integrations and 33 new ledger cases | **199 passed**, zero skipped, 49.47 s |
| Complete API/storage suite with isolated SQLite and PostgreSQL 17.6 | **94 passed**, 63.62 s |
| New Rust crate unit tests | **3 passed** |
| Rust formatting and clippy with warnings denied | Passed |
| Fresh-process backend benchmark | 27 full runs, matching output fingerprints at every workload |
| Guide structure, links, numerical examples and browser checks | Passed: 25 chapters, 13 feature panels, 263 MathML equations, three diagrams |
| Version locks and operator skill archive | Updated to 0.25.0; archive integrity verified |

The API suite includes durable process restart tests for the default Python job
path and retains one existing Starlette/httpx deprecation warning. No UI source
changed; application browser tests and the web build were not rerun. Guide
rendering tests are separate from application validation. No S3/Iceberg service
or production deployment was tested.

Coverage includes mixed bank/dealer stress, AFS/trading premium amortization and
sales, tax ordering, forward purchases, collateral funding/maturity, default and
recovery, negative rate inputs, zero accounts, reverse stress, randomized postings,
accurate cancellation, invalid input, atomic rejection and concurrent isolation.
The original financial models, fixed-OAS and CRN contracts are unchanged.

A new GitHub Actions workflow builds/tests the pilot on Ubuntu and Windows.
That workflow has been added but has not been run remotely in this local task.

## Next native boundary and production admission

The next useful experiment is to move a **complete daily financial event/state
loop** behind the batch boundary, together with bounded journal partitions. The
current pilot leaves position traversal, policy evaluation, dictionary-based
expected-state construction and label encoding in Python. Porting only another
small reduction is unlikely to eliminate that remaining work.

Before broadening the production workload:

1. Profile the remaining pipeline and port one complete state/event family with
   the same independent reference and exact output checks. Include default,
   maturity, funding and policy-order interactions rather than timing isolated
   arithmetic.
2. Implement partitioned journal output plus rolling checkpoints and a reconciled
   manifest, with failure/partial-write tests. Avoid retaining all lines just to
   run final replay; stream an independent reconstruction across partitions.
3. Extend the dedicated harness to 10k/60k and longer horizons only under an
   explicit memory budget. The public 2,000-position guard remains unchanged;
   this pilot does not establish 60k daily-ledger capacity.
4. If native execution meets the agreed performance/ownership goals, then wire
   explicit backend identity into queued job snapshots, worker admission and
   publication, and test cancellation/restart/failure with native jobs before
   exposing an API choice. Existing storage tests currently exercise the default
   Python job path.

Exact product calendars and scenario-specific cashflows, hedge/netting/XVA,
FX/consolidated capital, jurisdictional mappings, bank-calibrated behavior and
nonlinear policy search remain separate modeling work. This pilot closes none
of those gaps merely by changing language.

## Reproduction

```powershell
uv run --project apps/api python scripts/build_ledger_native.py
cargo test --locked --manifest-path packages/portfolio-ledger-native/Cargo.toml
cargo fmt --manifest-path packages/portfolio-ledger-native/Cargo.toml -- --check
cargo clippy --locked --manifest-path packages/portfolio-ledger-native/Cargo.toml --all-targets -- -D warnings
uv run --project apps/api python -m pytest packages/portfolio-risk/tests/test_ledger_native.py -q
uv run --project apps/api --with psutil python scripts/benchmark_ledger_backends.py
```

For engine use:

```python
from portfolio_risk.analytics.balance_stress import run_balance_stress

reference = run_balance_stress(specification)  # original Python journal
control = run_balance_stress(specification, journal_backend="columnar")
native = run_balance_stress(specification, journal_backend="rust")
```
