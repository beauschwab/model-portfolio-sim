# Rust daily state engine and partitioned journal validation

29 September 2026 · Engine 0.26.0 · Local working-tree implementation

Follow-up: [durable worker integration](2026-09-29-streamed-worker-integration.md)
adds an opt-in API and local/S3 partition publication. The measurements below
remain engine-only and exclude that additional boundary.

## Delivered behavior

The complete existing daily research state/event loop now has a Rust
implementation. It runs in a separate process and emits bounded batches into
immutable Parquet partitions. An independent Python replay reads the saved
journal, verifies ordered balanced transactions across file boundaries and
reconciles every closing GL key before a local manifest is published.

This extends the [0.25 reduction-only pilot](2026-09-29-native-ledger-pilot.md).
The earlier pilot received financial postings from Python. The new executable
generates those financial events itself and owns the daily position, account,
collateral, credit, margin and policy state.

The API/worker default remains Python. The new engine-only runner can explicitly
admit up to 60,000 positions and 90 million work units. This report does not claim
that the separate product-pricing kernels, API, database layer or full GSIB model
coverage have been rewritten in Rust.

## Ownership and event order

| Operation | New runner owner |
|---|---|
| Typed input validation and opening account reconciliation | Python, before starting the child |
| Opening positions, basis, fair value and AOCI | Rust |
| Recovery receipts and secured funding maturities | Rust |
| Forward originations, marks, draws, migration/default/provision | Rust |
| Interest accrual/settlement, deposit runoff, asset/funding maturity | Rust |
| Dated contractual flows and linked collateral repayment | Rust |
| Netting-set marks, counterparty losses and historical margin targets | Rust |
| Funding carry, fees/costs and dividends | Rust |
| Policy scheduling, outage delays, sale/funding/transfer/dividend actions | Rust |
| Tax after completed actions, daily GL checks and research limits | Rust |
| Independent persisted replay, closing statements and attribution | Python |
| Parquet partitions, checksums and atomic local manifest publication | Python |
| Product Monte Carlo, OAS and model calibration | Existing product engines, outside this runner |

Event order, policy insertion order, collateral linkage, accounting classes,
proportional scheduled-flow survival, shared liquidity caps and day-zero limits
follow the Python reference. Baseline, scenarios and reverse severities run
sequentially with detached state. The implementation does not assume that extra
native threads automatically improve throughput.

Baseline and named scenario journals are retained and independently replayed.
Reverse-severity runs retain summary diagnostics and perform daily GL checks;
their journals are not exported or independently replayed from persisted files.

## Output and publication contract

The native process emits at most 16,384 rows per transport batch. Journal batches
use numeric columns and incremental string dictionaries. This reduces repeated
per-line JSON labels, but the protocol remains columnar JSON over a local pipe;
it is not Arrow IPC, zero-copy exchange or an S3 upload protocol. Pipe backpressure
keeps the producer from queuing the complete journal in memory.

The Python sink writes at most 65,536 rows per Parquet partition by default.
Partitions record schema, row count, byte count and SHA-256. The attempt remains
in a fresh hidden directory while the engine runs and verification completes.
The manifest contains the detached input snapshot, model/source/native identity,
capacity tier, ordered partition references, validation scope and stage timings.
Only after verification and file flushes does a directory rename expose the
finished `run-*` artifact.

Independent replay retains GL balances and one transaction's amounts; it does
not retain every historical journal row or transaction. Transactions can cross
partitions. Sequence checks reject missing/reordered/duplicated transaction
batches; line validation rejects malformed debit/credit values; transaction sums
and closing balances use the existing tolerances. Non-journal partition schemas,
counts and checksums are also checked before publication.

Normal exceptions remove the unfinished attempt. A hard process termination
can leave an unreferenced hidden directory. There is no checkpoint-resume support:
recovery starts from the saved input snapshot. Daily GL checkpoints are verified
in memory; they are not advertised as restartable durable engine snapshots.

Native failure, a missing binary, write error, corruption, failed replay,
cancellation or timeout prevents publication. A watchdog can kill a native child
that emits no output. The native binary and engine source must remain unchanged
through the run. Callback cancellation checks also guard Python daily checkpoints
and persisted replay. Production worker lease/revision/attempt fencing remains a
separate admission requirement; a local manifest is not a substitute for it.

## Correctness evidence

Tests compare all financial output tables against the original Python engine:
journal, trial balance, daily paths, exposures, event ledger, actions, breaches,
funding claims, reverse grid, attribution, closing statements and within-currency
consolidation. Streaming uses stable nullable schemas; the legacy engine sometimes
infers `Null` for entirely absent columns, so parity compares values with those
documented dtype differences. Numeric comparisons use `rtol=1e-12, atol=1e-8`;
daily reconciliation and posting tolerances were not loosened.

Fixtures include mixed bank/dealer scenarios, randomized rates and policy/funding
timing, default/recovery, AFS/trading premium amortization and partial sales,
forward purchases, collateral maturity, zero accounts and reverse stress. Tests
also force 37-row partitions, split transactions into one-line pieces, inject
disk failure and corruption, check path escape rejection, cancel active attempts,
kill a silent child on deadline, and preserve the old default admission limit.

Final local validation on the same engine source and release binary used by the
benchmarks:

- **230 engine tests passed**, with no skips, including 31 streaming/state tests.
- **94 API tests passed** with SQLite and a temporary PostgreSQL server available,
  including durable worker process/restart cases. One existing Starlette/httpx
  deprecation warning remains.
- **3 Rust unit tests passed**; formatting and clippy with warnings denied passed.
  The full state executable is exercised by the Python cross-process parity tests.
- The operator skill mirror and archive were refreshed. Guide structure, numerical
  examples, browser behavior and architecture diagrams were checked separately.

Reproduce the runtime gates with `NUMBA_NUM_THREADS=4` and
`uv run --project apps/api python -m pytest packages/portfolio-risk/tests -q`;
run API tests from `apps/api` with `TEST_POSTGRES_URL` pointing to an isolated test
database. Build the release native executable first with
`uv run --project apps/api python scripts/build_ledger_native.py`.

Engine source SHA-256:
`0ebd015f6d2124391a69f93147cd3b0751f2d9b8f2a8aa13233fc417140d1225`.
Native release executable SHA-256:
`d4e59d251481c91ecf7a4c65c3f44430f4ae623923855fdeb7061b40ad7e0b4c`.
The Windows/Ubuntu CI matrix is configured but was not run remotely here.
No application UI build, live provider, S3/Iceberg or deployment validation is
claimed by these checks.

## Measurement design

`scripts/benchmark_streamed_balance.py` compares the original Python daily model
with a partitioned journal sink against the full Rust daily model using the same
Parquet sink and independent replay. Each workload has three fresh-process
samples with alternating backend order. The process-tree guard sums Python and
native child RSS, samples every 10 ms and terminates the attempt above 2 GiB.
Shorter peaks can be missed; RSS is not committed memory or a production quota.

The matrix covers 2k/30 days, 2k/180 days, 10k/30 days and 60k/30 days. It uses
daily floating-rate loan accrual under baseline and +200 bp conditions, with full
journal retention on disk, not an aggregate-only result. Reported times include
validation, native startup, partition writing, independent persisted replay,
checksum/schema reads and local manifest publication. Python interpreter startup
and post-run comparison fingerprints are excluded. A 2 GiB ceiling is a harness
guard, not a claim about all future 60k portfolios.

The separate `scripts/benchmark_mixed_state.py` exercises an exactly 60,000-position
six-kind daily book: loans, deposits, AFS/HTM/trading securities, funding, reverse
repo and dealer repo, plus netting exposure, delayed margin, sparse expected
defaults, commitments, funding/sale/transfer/dividend policies, tax and explicit
LCR/NSFR/HTM limits. It compares raw ordered output tables at the same numerical
tolerances without loading all partitions together. One full run per backend is
a capacity observation, not a latency distribution.

## Repeated capacity measurements

Median of three fresh-process runs per backend/workload, including saved output
and independent replay. Peak RSS is the median sampled combined process-tree peak.

| Positions / days; two scenarios | Python reference | Rust state engine | Speedup | Python / Rust peak RSS |
|---|---:|---:|---:|---:|
| 2,000 / 30 | 1.885 s | 1.034 s | 1.82× | 296 / 286 MiB |
| 2,000 / 180 | 10.263 s | 4.650 s | 2.21× | 586 / 558 MiB |
| 10,000 / 30 | 25.560 s | 8.606 s | 2.97× | 579 / 601 MiB |
| 60,000 / 30 | 200.959 s | 83.012 s | 2.42× | 1,383 / 1,316 MiB |

The 60k workload generated **7,560,004 journal lines** and about **30.6 MiB** of
compressed native-run Parquet output. Compression is fixture-specific, not a
production storage forecast. All 24 full runs completed below the 2 GiB guard.
Every table's ordered-row fingerprint matched after rounding floating columns
to eight decimal places. That screening check supplements the unrounded mixed
model parity tests; it is not a claim of bitwise float equality.

At 60k, compute/partition medians were 169.529 s (Python) and 49.702 s (Rust).
Replay/finalization medians were 29.166 s and 31.871 s, respectively. Native compute
and partitioning improved by about 3.41×, but shared verification reduces the
whole-request improvement to 2.42×. Component medians do not necessarily sum to
the median total; totals also include wrapper/manifest overhead.

The 60k sample ranges were 196.108–201.689 s for Python and 80.504–88.014 s for
Rust. One 2k/180-day Rust sample took 7.828 s, versus 4.575 and 4.650 s for its
other samples. This workstation was not an isolated benchmark host; report the
sample distribution rather than treating the median as a service guarantee.

Memory is broadly similar between the two streamed paths and was slightly higher
for Rust at 10k. The comparison does not establish a Rust memory advantage. Both
avoid retaining the full journal as Python dictionaries. Do not directly compare
these numbers with older retained-journal measurements as if language were the
only changed variable.

[Raw samples, stage timings, memory, identities and fingerprints](2026-09-29-streamed-native-benchmark.json).

## Mixed-book capacity measurement

The separate 60,000-position, 30-day mixed book completed with **11,050,715 journal
lines**, 368,337 closing GL keys and two scenarios. Every raw row in all 13 output
tables matched at `rtol=1e-12, atol=1e-8`, without rounding. The reverse grid was
empty in this capacity fixture; smaller regression fixtures gate reverse stress.

| Single fresh-process run | Python reference | Rust state engine |
|---|---:|---:|
| Total including verification and publication | 241.545 s | 111.220 s |
| Compute and partition writing | 194.477 s | 65.001 s |
| Replay and finalization | 44.599 s | 43.445 s |
| Peak combined process RSS | 1,350 MiB | 1,271 MiB |
| Compressed Parquet output | 44.21 MiB | 43.86 MiB |

Rust was **2.17× faster overall** and **2.99× faster for compute and partitioning**
in this sample. Verification accounted for about 39% of Rust's total time, so
moving additional arithmetic alone cannot remove the remaining latency. Both
runs retained the same 22 policy-action rows, three breach rows and two funding
claims. Matching breaches are model outputs, not a claim that this book passes
every configured constraint. Defaults are sparse in this synthetic fixture;
this is not a worst-case simultaneous default/recovery workload.

[Raw mixed-book results and comparison counts](2026-09-29-mixed-state-60000.json).

Neither benchmark includes product Monte Carlo regeneration, exact contractual
calendars, API/network latency, S3/Iceberg, concurrent tenants or distributed
execution. Inputs are synthetic research assumptions. Accounting consistency and
native parity do not establish economic or regulatory validity.

## Remaining scope and use

Use the runner directly:

```python
from portfolio_risk.analytics.balance_stream import run_streamed_balance_stress

manifest = run_streamed_balance_stress(
    specification, ".data/stress-runs", backend="rust", large_book=True)
```

`backend="python"` selects the streamed reference. `load_streamed_result` is
intended for small-run inspection and parity checks; large consumers should scan
the manifest-listed Parquet partitions. Neither mode changes saved books.

Streaming removes complete-journal retention, not all memory growth. GL keys,
credit recoveries, claims and label dictionaries grow with portfolio/event counts;
the native GL has a two-million-key ceiling. The Python reference retains other
report rows for a scenario before writing them, while Rust streams those too.
This representation difference must be considered in memory comparisons.

The next production integration must bind native identity to queued job snapshots
and admission, retain cancellation/restart/lease fencing, register partitioned
results through the existing artifact repository, and validate local/S3 publication
failures. No PostgreSQL-specific metadata path or second mutable book store was
introduced. An Iceberg catalog is still not implemented.

Exact daily/scenario product cashflows, saved hedge/netting adapters, received and
initial margin, closeout/XVA, FX/consolidated capital, full jurisdictional mappings,
bank-calibrated behavior, deferred tax/hedge accounting and nonlinear policy search
remain model-development work. Native execution preserves the existing research
semantics; it does not fill those gaps.
