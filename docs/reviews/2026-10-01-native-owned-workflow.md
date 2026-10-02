# Native calculation migration and optimization, 0.28.0

The built-in Rust execution migration is complete for the existing supported
contracts. This report separates execution ownership, financial parity, scale measurements,
and model limitations. Rust owns the built-in financial runtime; HiGHS remains
C++ and is invoked by Rust. Python remains the application adapter, artifact
writer, independent verifier, and reference implementation.

## Execution boundary

```text
Raw books + markets + histories + configuration + edits + ledger mapping
  -> Rust calibration / shared seeded CRN / bounded dependency graph
  -> Rust scenario pricing / NII / KPI contributions / unit library
  -> Rust transactional decision state / LP construction
  -> C++ HiGHS / Rust allocation replay and publication staging
  -> Rust bounded accounting capture / explicit candidate and saved-book mapping
  -> Rust daily subledgers / balanced postings / constraints / closing reports
  -> streamed tables / Python Parquet writer / independent persisted replay
  -> atomic immutable manifest publication
```

The standalone `portfolio-workflow` executable executes all financial stages in
one native process, with no Python callbacks. The Python wrapper sends one raw
request, writes bounded output partitions, verifies the persisted journal, and
publishes only after identity checks. `portfolio-strategy` also accepts raw books
and a sequence of native edits without requiring a Python interpreter.

The public Rust decision-session route now invokes the native coordinator.
Constraint-only updates skip pricing; contract edits select affected saved rows;
template edits rebuild selected templates. Published state is unchanged until
the transaction token and external snapshot guard pass. Aborts discard pending
state. Base OAS and effective-yield anchors remain tied to original contracts and
markets. Unit coefficients are matched by template and purchase month.

The public Rust what-if route computes baseline, edits, repricing, and differences
inside its native graph. The existing saved-book stress route invokes the native
accounting/mapping/daily coordinator. The materialized compatibility route still
has its 250,000-cashflow limit; the streaming workflow uses bounded numeric
schedules instead of enlarging that JSON limit.

Public saved-book base calibration, spread application, raw published-forecast
compilation, anchored conditional replay, and empirical joint-driver fitting and
scenario selection also dispatch to native controllers. Custom Python model
extensions, API/storage operations, and independent verification remain Python.
These are distinct from the built-in native simulation runtime. Existing custom
extensions do not silently become native code.

## Optimization changes

* Retain compact instrument results ahead of transient cashflow buffers within
  the same cache budget. Complete contract and parent identities still gate reuse.
  Notional scaling reuses per-unit results, with original calibration anchors.
* Capture saved-book accounting in batches of at most 256 contracts. Store
  generated schedules as dictionary position indices and numeric values, bounded
  by 128 MiB allocated capacity. No instrument-by-path-by-time cube is retained.
* Consume the owned raw request during typed decoding instead of duplicating its
  entire JSON tree. Validate typed ledger rows incrementally rather than building
  another full-book JSON tree.
* Share immutable position labels across daily snapshots. Use integer dictionary
  keys for daily GL and independent subledger reconciliation instead of allocating
  repeated account/instrument strings. Lexical final table ordering is preserved.
* Write OAS/PV results directly into flat buffers instead of allocating nested
  vectors per instrument and bump. Resolve small immutable term-market grids once
  per request under a 32 MiB retention bound; larger grids use the existing cache.
* Release private Python transport dictionaries after writing the immutable request
  file, before receiving large output tables. Caller books/mappings are preserved.
* Keep deterministic financial reduction and posting order. Do not parallelize
  cash, collateral, and policy state transitions whose order can change outcomes.
* Carry cooperative deadlines through native stages and Rayon pool boundaries.
  The outer watchdog can terminate the standalone process on cancellation,
  timeout, output failure, or a broken pipe. HiGHS retains its bounded solve time.

The native graph and generated-schedule budgets are component limits, not a
global RSS limit. The scale harness separately enforces a 4 GiB process-tree
ceiling. The generated-schedule workflow admits 62,000 total ledger positions and
120 million daily work units; existing raw-spec API admission is unchanged.

These choices follow measurement-first guidance in the
[Rust Performance Book](https://nnethercote.github.io/perf-book/profiling.html),
particularly [heap allocation](https://nnethercote.github.io/perf-book/heap-allocations.html).
The product release profile retains opt-level 3, thin LTO, and one codegen unit;
[Cargo documents these controls](https://doc.rust-lang.org/cargo/reference/profiles.html).
No fast-math, reduced financial tolerances, or machine-specific CPU target was
introduced. Optional allocator experiments did not justify changing the default.
[PGO](https://doc.rust-lang.org/rustc/profile-guided-optimization.html) remains an
experiment requiring representative training and held-out workload measurements.

The decision actor reserves session capacity before pricing, then releases the
global registry lock. Expensive initialization no longer blocks unrelated session
evaluation or close operations. Failed initialization releases its reservation.

Durable identity now includes the workflow executable, in addition to the product
and decision libraries and raw ledger binary. Saved-book jobs verify it before
execution and verify the executed hash before publication. Queued jobs from a
different build fail explicitly. The API/source/native build remained fixed
during durable-worker verification.

## Complete workflow scale measurement

Workload: 60,000 primary instruments plus money-market/candidate positions,
8 paths, 27 reporting months, four compute threads. It executes initial pricing,
solve, a constraint-only warm step, and a one-loan coupon edit; it then runs the
final edited ledger for 810 days over baseline and funding-stress scenarios.
Immutable Parquet writing and independent journal replay are included in total
time. This is not three complete repeated ledger runs. The scale fixture uses
synthetic amortized-cost loan/funding ledger mappings, without active credit
defaults, collateral/netting or management-action policies. Those features have
separate functional tests; this scale result does not measure an active GSIB
stress-event mix. Product pricing/cashflow generation still uses the five
underlying product families.

The pre-allocation-optimization baseline completed in **793.275 seconds**, using
**903.836 MiB peak native RSS** and **1,554.539 MiB peak process-tree RSS**. It
generated **10,497,266 journal rows**, **3,360,392 exposure rows**, and **364,834
closing GL keys**. Numeric cashflow capacity was **64,972,800 bytes** for 1,620,189
rows. The persisted journal reconciled. Two dynamic constraint breaches were
reported, so `dynamic_validated` is false; accounting reconciliation does not
make an economically infeasible scenario feasible.

[Baseline machine-readable evidence](2026-10-01-native-workflow-60000.json).
The first optimized rerun completed in **373.183 seconds** (2.13x faster), with
**620.801 MiB native peak RSS** (31.3% lower) and **1,436.695 MiB process-tree
peak RSS** (7.6% lower). Row counts and independent journal validation matched.
The Python artifact writer remains a material part of total RSS. The run also
includes the compatibility fix restoring reference book/posting order, so this
is a combined change comparison, not an isolated allocation ablation.
[Optimized workflow evidence](2026-10-01-native-workflow-optimized-60000.json).
The final build includes the controller/actor corrections, flat result buffers,
bounded term-market retention and Python request release. It completed in
**372.824 seconds**, effectively unchanged from the first optimized run, with
**620.734 MiB native peak RSS** and **1,319.449 MiB process-tree peak RSS**.
The additional request-lifetime work reduced tree peak by a further 8.2%; no
additional full-workflow speedup is inferred from a 0.1% timing difference.

| Complete native workflow | Before allocation optimization | Final build | Change |
|---|---:|---:|---:|
| Total including Parquet and independent replay | 793.275 s | 372.824 s | 2.13x faster |
| Peak native RSS | 903.836 MiB | 620.734 MiB | 31.3% lower |
| Peak process-tree RSS | 1,554.539 MiB | 1,319.449 MiB | 15.1% lower |
| Journal rows | 10,497,266 | 10,497,266 | Identical |
| Exposure rows | 3,360,392 | 3,360,392 | Identical |
| Closing GL keys | 364,834 | 364,834 | Identical |

Final computation and partition writing took 337.812 s; independent replay and
finalization took 34.912 s. Geometric FlowStore growth trades some spare capacity
for fewer reallocations: final allocated schedule capacity is 70,778,880 bytes,
still below the 128 MiB bound. RSS was sampled every 20 ms and is a process metric,
not a sum of configured cache budgets.

All 515 retained financial/validation numeric comparisons passed against the
original complete-workflow run, with maximum absolute difference 2.387e-10 at
rtol=1e-7 and atol=1e-5. Table counts match exactly. Every persisted journal was
independently replayed in its own run; the large journals were not retained for a
rowwise before/after comparison. This **2.13x is a Rust before/after comparison**,
not a comparison with Python's complete 60,000-instrument daily pipeline. The
legacy materialized Python saved-book route has a 250,000-flow admission limit;
complete small-book cross-backend parity and the separate scale probes establish
the tested boundaries without bypassing that limit.

[Final workflow evidence](2026-10-01-native-workflow-final-60000.json) and
[reproducible comparison](2026-10-01-native-workflow-final-comparison.json).

## Separate Python/Rust graph comparison

The earlier same-size graph/unit/HiGHS comparison used fresh processes and the
same 128 MiB instrument cache. It excludes daily ledger and full key-rate sweeps.

| 60,000-instrument graph, units and solve | Python | Rust |
|---|---:|---:|
| Cold | 24.530 s | 15.532 s |
| Warm median | 24.722 s | 2.443 s |
| One-loan coupon edit | 25.169 s | 2.394 s |
| Peak process RSS | 1,274.4 MiB | 570.8 MiB |

All 1,696,660 numeric comparisons passed at rtol=1e-7 and atol=1e-5. The warm
speedup was 10.12x, edit speedup 10.51x, and peak memory reduction 55.2% in this
workload. These ratios do **not** describe the complete daily workflow above.
[Graph evidence](2026-10-01-native-graph-optimized-60000.json).

## Corporate and CD full spot-risk comparison

Fresh-process 60,000-instrument probes include complete OAS, model price, DV01,
key-rate and volatility risk outputs. Both backends use eight paths, four threads,
three warm repetitions and a contract edit. This is a different workload from the
graph and complete daily pipeline above; the measured processes include Python
imports, transport and retained output arrays.

| Product / backend | Cold | Warm median | Edited | Peak process RSS |
|---|---:|---:|---:|---:|
| Corporate / Python | 3.654 s | 4.855 s | 4.776 s | 503.6 MiB |
| Corporate / Rust | 1.280 s | 2.355 s | 2.410 s | 302.5 MiB |
| CD / Python | 1.250 s | 1.518 s | 1.375 s | 319.9 MiB |
| CD / Rust | 0.750 s | 1.341 s | 1.366 s | 321.3 MiB |

Corporate warm computation is **2.06x faster**, with about **40% lower peak RSS**.
CD warm computation is **1.13x faster** in this run. Its edited timings are
practically tied and peak RSS is unchanged within 0.5%; this is not evidence of a
large CD advantage. All cold/warm/edited output comparisons pass at rtol=1e-7 and
atol=1e-5. Maximum absolute differences are 1.163e-7 for corporate and 1.174e-6
for CD. The report retains individual samples; it is a local benchmark, not a
cross-machine statistical guarantee.
[Final spot-risk evidence](2026-10-01-term-lifecycle-optimized-60000.json).

The earlier 0.28.0 probe measured Rust CD warm time at **1.858 s**, versus Python
**1.431 s**. That regression was real. Boundary profiling showed the native call,
including JSON decoding, dominated rather than thread oversubscription: reducing
to one thread increased total median time from 1.891 s to 2.898 s. Writing OAS/PV
results into flat buffers reduced native-call median time from 1.456 s to 1.045 s
and total median from 1.891 s to 1.464 s in the isolated follow-up. Bounded
term-market retention followed. The final parity benchmark above uses both
changes; its warm Rust CD time is about 28% below the earlier Rust probe.

[Earlier term comparison](2026-10-01-term-lifecycle-60000.json),
[boundary profile](2026-10-01-term-boundary-profile.json),
[single-thread check](2026-10-01-term-boundary-single-thread.json), and
[flat-buffer follow-up](2026-10-01-term-boundary-flat-results.json).

## Validation scope and model limits

Focused ownership tests forbid Python pricing, overrides, mapping, unit-building,
and daily simulation callbacks on the native routes. Independent Python replay
compares the complete small-book workflow, including candidates and closing
reports. Failure tests require no published artifacts after invalid mapping,
deadline, cancellation, or partition-write failure. The new full-suite checkpoint
is recorded below; earlier 0.27.9 test counts do not certify this implementation.
The ownership audit also gates raw base calibration, forecast compilation/replay,
joint-driver fitting, holdout isolation, stable ordering, and nonconvergent inputs.

| Gate | Result |
|---|---|
| Full engine regression checkpoint | 665 passed, 2 expected skips, 290.51 s |
| Subsequent calibration-boundary and forecast/controller regression | 21 passed, 2.06 s; includes three new empty/irrelevant-book cases |
| Final full API suite | 99 passed, 84.72 s |
| Rust release unit tests | 32 passed: product 19, decision 7, ledger 4, compute control 2 |
| Strict release clippy | Product, decision and ledger crates passed with warnings denied |
| Formatting | All four Rust crates passed `cargo fmt --check` |

[Machine-readable validation and binary/source identities](2026-10-01-native-owned-validation.json).

The two engine skips are Python parameterizations of Rust-only ownership tests,
not missing native binaries. The full engine checkpoint preceded the small public
calibration transport correction; its affected tests were rerun afterward. API
tests ran after that correction and the executable-identity fix. The API suite has
one existing Starlette/httpx deprecation warning. Storage tests used isolated
SQLite and mocked object-store protocols; no live PostgreSQL/S3/Iceberg result is
claimed. No engine or native rebuild occurred during durable-job tests.

Commands (from the repository root unless indicated):

```powershell
$env:NUMBA_NUM_THREADS='4'
uv run --project apps/api python -m pytest packages/portfolio-risk/tests -q --tb=short
uv run --project apps/api python -m pytest packages/portfolio-risk/tests/test_native_controllers.py packages/portfolio-risk/tests/test_forecast.py -q --tb=short
# From apps/api:
uv run python -m pytest tests -q --tb=short
# From the repository root:
cargo test --release --locked --manifest-path packages/portfolio-risk-native/Cargo.toml
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py --test
cargo test --release --locked --manifest-path packages/portfolio-ledger-native/Cargo.toml
cargo test --release --locked --manifest-path packages/portfolio-compute-control/Cargo.toml
```

The framework still uses research regulatory weights, synthetic calibration data,
30-day reporting months for mapped accounting, expected-cashflow survival overlays,
and approximate forward-unit pricing. These are inherited model assumptions,
not changes introduced by the Rust migration. LCR, NSFR, EVE, CET1, funding and
commercial constraints are in the strategy LP. HTM restrictions remain in daily
replay, not a category-aware strategy constraint. Saved hedge trades require an
explicit trade-to-netting adapter and are rejected by this saved-book workflow;
the standalone hedge pricer itself is native and parity-tested.

Eight paths provide a throughput/parity fixture, not Monte Carlo convergence
evidence. There is no claim of production GSIB model certification, live database
or S3/Iceberg validation, deployment, or HTTP coverage for a newly added workflow
endpoint. The existing API routes retain their persistence and publication
contracts; this change does not introduce a second mutable source of truth.

## Enable and reproduce

Select `compute_backend="rust"` in `RunConfig` or API `RiskSettings`. The default
remains Python for existing workspaces; no persisted settings were rewritten.
Rebuild all native components before restarting the API and worker together:

```powershell
uv run --project apps/api python scripts/build_native.py
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py
uv run --project apps/api python scripts/build_ledger_native.py
```

Reproduce the final scale probes one at a time, with no concurrent builds, tests
or other compute-heavy jobs:

```powershell
$env:NUMBA_NUM_THREADS='4'
uv run --project apps/api python scripts/benchmark_term_lifecycle.py --positions 60000 --paths 8 --threads 4 --repeats 3 --output docs/reviews/2026-10-01-term-lifecycle-optimized-60000.json
uv run --project apps/api --with psutil python scripts/benchmark_native_workflow.py --positions 60000 --paths 8 --horizon 27 --output docs/reviews/2026-10-01-native-workflow-final-60000.json
python scripts/compare_native_workflows.py docs/reviews/2026-10-01-native-workflow-60000.json docs/reviews/2026-10-01-native-workflow-final-60000.json --output docs/reviews/2026-10-01-native-workflow-final-comparison.json
```

The full workflow fixture writes and independently replays temporary Parquet
partitions, then retains compact benchmark evidence. It does not retain the large
journal files after successful completion. The comparison script checks retained
financial summaries, allocation/replay results, validation and exact table counts;
it does not claim a rowwise comparison against a retained baseline journal.
