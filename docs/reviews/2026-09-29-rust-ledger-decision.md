# Rust engine decision and ledger continuation

29 September 2026 · Engine 0.24.0 · Local working-tree evidence

> Follow-up: the [0.25 native journal pilot](2026-09-29-native-ledger-pilot.md)
> implements and measures the first batch boundary. This document retains the
> pre-pilot evidence and proposed promotion criteria.

## Recommendation

**Revisit Rust now with a bounded daily-ledger pilot, before attempting a
60,000-instrument full-journal simulation. Keep the existing Python/Numba product
engines and durable API while measuring that pilot.** The new ledger adds a
different workload from the previously measured pricing kernels: typed state
updates, event ordering, journal allocation, collateral links and reconciliation.

The project already has a Rust discount kernel and a Rust decision runtime with
a persistent native HiGHS solver. It does not yet have Rust product models or a
Rust daily ledger. The decision is where to extend native ownership, rather than
whether to introduce Rust for the first time. A full backend rewrite is not
supported by the current performance evidence.

## What was completed in this continuation

- Separated security opening cost from clean fair value. AFS/trading positions
  can now carry premiums and discounts. Opening AFS OCI is initialized without
  manufacturing current-period income. Supplied equity must include that OCI.
- Added consistent coupon/amortization, partial-sale, forward-purchase and
  redemption accounting. AFS reclassification removes only the sold share of
  accumulated OCI; trading avoids booking the basis loss a second time.
- Captured the engine's opening market quote in saved-book accounting output;
  the adapter uses it separately from historical cost and rejects quote overrides.
- Retained zero-balance accounts in GL-derived closing statements even when no
  journal entries exist.
- Cached static dataclass type annotations. Previously, validation of 2,000
  positions repeatedly compiled annotations: the initial profile recorded
  86,030 `compile` calls. This cache holds schema metadata, not financial state.
- Replaced repeated whole-book collateral scans with indexes by collateral
  instrument and claim maturity day; opening pledge validation now makes a
  single allocation pass instead of scanning all debts for every position.
- Added 19 regression cases, a reproducible ledger profiler, and updated the
  operator skill and model guide. Fixed-OAS, CRN and durable publication contracts
  are unchanged.

Hand-check fixture: principal 100, cost 110, clean FV 105, cash 20 and liabilities
80 gives assets 125, equity 45 and AFS OCI -5. Selling half for 52.5 leaves equity
45, realizes a 2.5 cost-basis loss and releases 2.5 of OCI. Redemption at par leaves
cash 120 and equity 40 with no residual principal, premium or FV adjustment.
Tests also exercise discount accretion and the corresponding trading treatment.

## Measurements and their boundaries

| Workload | Existing Python path | Rust/native path | Interpretation |
|---|---:|---:|---|
| Prepared discount reduction, 100k instruments | Numba 6.687 ms | Rust 17.112 ms | Rust was slower for this particular kernel |
| Same operation including packing | Numba 89.513 ms | Rust 94.397 ms | Boundary/packing cost matters |
| 60k backbook, three markets, build plus first validated allocation | Fresh full rebuild + SciPy 123.487 s | Rust decision runtime + Python/Numba products 122.057 s | Similar full-build time; this is not a Rust product-engine comparison |
| 60k backbook, allocation coefficient replay | Python 0.111 ms | Native 0.213 ms | Both already fast; no reason to port this for latency |
| Same 35-candidate LP coefficients | Fresh SciPy solve 6.29 ms | Persistent native solve 3.84 ms | Solver/session reuse helps; HiGHS itself is C++ |
| New daily ledger: 2k positions, 30 days, two scenarios | 1.497 / 2.670 / 3.662 s | Not implemented or measured | Candidate for the next native comparison |

Earlier numbers are historical September 28 measurements, not reruns against
0.24. Sources: [discount/backend comparison](2026-09-28-five-step-comparison.md),
[decision prototype](2026-09-28-decision-prototype.md), and
[60k benchmark](2026-09-28-balance-sheet-60000.md). The 60k workload optimized
**35 candidate cohorts**, not 60,000 independent decision variables. It excluded
the new daily ledger, full dealer/XVA calculations, nonlinear policy search and
distributed operation. Its peak process RSS was 2.07 GiB; this cannot be assigned
to Rust or Python separately because both lived in the same process.

The current [ledger profile](2026-09-29-ledger-profile.json) measures synthetic
daily loan accrual, every daily accounting check, full journal retention and
independent closing replay. It excludes product path generation and persistence.
Three unprofiled samples give a **2.670 s median**, but the 1.497–3.662 s range
shows workstation/allocator variability. Do not treat that median as an SLO.
The source fingerprint was unchanged throughout the final measurement.

- Journal output: **252,004 lines**, **21.70 MiB** of final columnar journal data.
- Process RSS: **114.82 MiB baseline**, **391.25 MiB sampled peak** across all three
  samples. This includes interpreter, other outputs, temporary objects and
  allocator retention; it is not a cold-process memory measurement.
- Separate instrumented profile: 7.234 s. Journal code contributes **34.10%** of
  self time; daily simulation code **26.31%**. Together these are **60.42%** of
  profiled self time. Builtin/other work is separate, and instrumentation adds
  overhead. This is hotspot evidence, not a forecast of achievable Rust speedup.
- Prominent operations include journal posting (124,002 calls), daily observation,
  subledger verification and closing replay.

The [initial profile](2026-09-29-ledger-profile-before.json) recorded a 3.970 s
median and repeated annotation compilation. Its samples were also variable;
the experiments were sequential on a shared workstation, not randomized,
controlled A/B runs. The code changes remove identifiable redundant work, but
these samples do not establish a durable percentage speedup. The initial source
fingerprint was captured at the end; only the final profiler enforces unchanged
source before and after the run.

The 21.70 MiB journal versus 391.25 MiB process RSS motivates investigation of
columnar construction and streaming in either language. It does not prove that
the difference consists entirely of Python row objects. Raising the 2,000-position
guard without a different retention strategy would be premature.

## Native ownership worth testing

| Layer | Next ownership choice | Reason |
|---|---|---|
| Product models and calibration | Keep Python/Numba reference implementations | Existing compiled kernels are competitive; economic coverage is still evolving |
| Daily position/account state | Pilot Rust typed arrays | Frequent updates and object traversal are now measurable costs |
| Dated event queue and collateral graph | Pilot Rust, explicit stable ordering | State/event ownership and dependency invalidation belong together |
| Journal construction and reductions | Pilot batched Rust/Arrow output | Reduce per-line object allocation and support bounded output buffers |
| Scenario execution | Add bounded independent batches after parity | Avoid duplicating large mutable Python books; measure actual peak memory |
| LP/strategy decisions | Extend existing Rust decision runtime as needed | Keep persistent HiGHS reuse; measure larger decision dimensions separately |
| API, jobs, revisions and storage | Keep current Python/SQLAlchemy architecture | No profile evidence that replacing these improves quant throughput |
| Calibration research and model authoring | Keep Python | Preserve a readable, independently executable reference |

Use one coarse request per scenario batch, with a versioned schema and contiguous
buffers; do not cross the language boundary for each position/day. Arrow's C Data
Interface supports ABI-stable, zero-copy exchange **within one process**. For a
separate worker service, use Arrow IPC or persisted Parquet instead of assuming
the C interface is an interprocess transport.
[Apache Arrow specification](https://arrow.apache.org/docs/format/CDataInterface.html).

Rust's ownership/type system can support safer concurrent native state, but it
does not establish financial correctness, faster algorithms or smaller retained
output by itself. [Rust concurrency documentation](https://doc.rust-lang.org/book/ch16-00-concurrency.html).

## Promotion gates

These are proposed engineering gates, not previously agreed business SLOs or
measured Rust gains.

1. **Freeze a small, complete event contract.** Include opening state, dated
   cash/accrual/principal flows, cost/FV/OCI, defaults/recoveries, funding claims,
   policy priority, tax ordering and GL account mapping. Preserve the Python
   implementation as an independently executable reference. Product calendars
   and new models can evolve behind the same event boundary.
2. **Match every observable output.** Compare all account/day cash, principal,
   basis, accrual, P&L, OCI, collateral and funding balances; journal postings;
   closing statements; breach dates and policy order. Include zero/negative-rate
   cases, partial repayment/sale, default/maturity collisions, invalid inputs,
   deterministic thread counts and randomized balanced portfolios. Keep fixed-OAS
   and CRN invariants for any product work. Do not relax tolerances to gain parity.
3. **Measure whole requests.** Include input packing, compute, validation, output
   conversion, Parquet publication and failed-publication recovery. Benchmark
   full journals and streamed/aggregate modes separately. Streaming must retain
   auditable event partitions and a reconciled manifest, not discard required
   evidence. Preserve revision checks, cancellation and worker attempt fencing.
4. **Expand capacity deliberately.** Start with 2k/30-day parity, then test 10k
   and 60k positions, 360/1,080-day horizons and 1/5/20 scenarios in the dedicated
   harness. Larger combinations exceed today's admission limits and require
   partitioning/streaming before production admission. Test the matrix in stages
   under a memory cap rather than allocating its largest dense output at once.
   Distinguish large backbooks from large LP/MIP decision spaces.
5. **Promote for an actual benefit.** Define request latency and worker RSS budgets.
   If Python event/state processing remains over half of wall time or misses
   those budgets after batching/streaming, prefer the native pilot if it delivers
   at least **2× end-to-end throughput or 50% lower peak RSS**, with identical
   supported behavior and acceptable maintenance cost. These thresholds are
   proposed decision criteria. A deployment/ownership requirement can also justify
   native ownership, but should be evaluated explicitly.

A broader rewrite starts to make sense only after the native event/state layer
owns a substantial fraction of the real workload and measured remaining costs
are product kernels or repeated boundary copies that a broader port can remove.
Keep porting one product family at a time behind parity tests. Rewriting the API,
metadata database layer and research workflows need not accompany a native compute
engine. Native compute can remain inside the existing durable Python worker.

## Validation and remaining work

Final source-frozen validation:

| Check | Result |
|---|---|
| Complete engine suite, including built native ABI integrations | **166 passed**, zero skipped, 96.56 s |
| Complete API/storage suite with isolated SQLite and PostgreSQL 17.6, including process restart | **94 passed**, 83.58 s |
| Updated guide structure, links and numerical examples | Passed: 25 chapters, 13 feature panels |
| Guide browser/rendering checks | Passed: desktop/mobile/print, 263 MathML equations, three architecture diagrams |
| Version locks and mirrored operator skill packaging | Updated to 0.24.0; archive integrity verified |

The API suite reports one existing Starlette/httpx deprecation warning. No
application UI was changed in this continuation; application browser tests and
the web build were not rerun. Guide browser validation is separate from those
application tests. Rust source was unchanged; the engine suite exercised existing
built DLLs, rather than rebuilding them or rerunning Rust lint/unit suites.

The AFS/trading basis gap is closed for the supported simulation semantics. This
does **not** complete production GSIB modeling. Exact daily/scenario product
cashflows, saved hedge-to-netting adapters, received/initial margin, closeout/XVA,
FX translation and consolidated capital, complete jurisdictional rules, deferred
tax/hedge accounting, bank-calibrated behavior and nonlinear policy optimization
remain open. The ledger still caps explicit requests at 2,000 positions; no
60,000-instrument full-ledger run or Rust ledger speedup is claimed. S3/Iceberg
production deployment and actual bank-data validation are outside these local
tests. See the [current ledger contract](../balance-sheet-ledger.md).

Reproduce:

```powershell
uv run --project apps/api --with psutil python scripts/profile_balance_ledger.py
$env:NUMBA_NUM_THREADS='4'
uv run --project apps/api python -m pytest packages/portfolio-risk/tests -q
uv run --project apps/api python .data/storage-verification/run_postgres_tests.py
```

The PostgreSQL helper is a local verification artifact under ignored `.data`;
it launches and stops an isolated PostgreSQL instance. Committed API test sources
are under `apps/api/tests`, including persistence and process-restart tests.
