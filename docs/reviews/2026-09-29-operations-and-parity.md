# Operational completion and Rust/Python parity audit

29 September 2026 · Local implementation and measured validation

Historical snapshot: native product/strategy coverage was extended on 30 September.
See [the current coverage report](2026-09-30-native-product-coverage.md); measurements
below remain specific to the earlier daily-state workload.

## Answer: full application parity is not achieved

The full **existing daily balance-sheet state model** has a Rust implementation
whose 13 financial output tables pass comparison with the independent Python
reference. This is numerical equivalence over tested cases, not proof over all
possible inputs and not bitwise equality. The wider quant application is still
hybrid. Describing it as a full Rust rewrite would be incorrect.

| Layer | Current ownership and parity evidence |
|---|---|
| Daily positions, credit/default/recovery, deposits, collateral, margin, funding, policy actions, tax and daily research limits | Rust and original Python implementations; all 13 tables compared with `rtol=1e-12, atol=1e-8` |
| Journal generation and closing GL | Native events independently replayed in Python from persisted partitions; transactions, order and closing keys checked |
| Reverse stress | Output parity covered in regression fixtures; reverse journals are not exported and independently replayed |
| Prepared-cashflow PV reduction | Separate Rust discount kernel, with NumPy/Numba comparison gates |
| Rate-path simulation, behavioral product cashflows, calibration and OAS root solving | Python/Numba; no equivalent complete native product engine |
| Portfolio risk orchestration and effective-interest NII | Python orchestration and product models, with native reduction available on selected paths |
| Strategy/decision session and LP | Optional Rust state/aggregation plus C++ HiGHS; Python builds product contributions/template coefficients and independently checks results |
| Saved-book mapping and dynamic candidate replay adapter | Python; the saved-book route has not been moved wholesale to the partitioned Rust runner |
| Validation, persisted replay, closing statements, attribution and storage | Shared Python components; intentionally not an independent full Rust implementation |
| Full GSIB regulatory/economic coverage | Not established by either implementation; existing calibration/product-flow/accounting approximations remain |

The daily parity suite now contains **51 cases**, including 20 additional
deterministic randomized cases varying rates, payment intervals, PD, recovery
timing, runoff, scenario starts and reverse severities. Premium/discount AFS and
trading basis, partial sales, forward purchases, collateral maturity, liquidity
caps, limit breaches, partition boundaries and failure cases are also gated.
No numerical or reconciliation tolerance was loosened.

## Remaining operational work implemented

### Iceberg delivery

`app.iceberg` delivers completed partitioned stress results to a configured
PyIceberg catalog. Schema fingerprints select stable workspace/result tables;
each row carries run/revision/workspace/source-result provenance. Data and a
per-run marker commit atomically within each table. Retry reloads the marker and
recreates partition iterators after optimistic conflicts. A version-2 SQL
catalog-publication receipt marks completion across all tables and retains exact
snapshot IDs. Version-1 application databases receive an additive migration.

Tests use actual Iceberg metadata/data files and a SQL catalog, not a catalog mock.
They interrupt execution after a successful catalog commit but before SQL receipt,
retry, verify all financial values, republish without duplicate rows, append another
run to the same tables, and exercise conflicting concurrent catalog transactions.
REST catalog configuration uses the same adapter but has not been tested against
a deployed remote catalog. Cross-table catalog publication is not atomic; consumers
requiring a complete run must use the SQL receipt. Original artifacts remain the
replay source. Snapshot expiration/compaction and marker retention need catalog
operations policies.

The implementation uses the pinned PyIceberg 0.10.x transaction interface; the
[official transaction documentation](https://py.iceberg.apache.org/api/) describes
append and property updates. Local Windows catalog tests use FsspecFileIO because
the default PyArrow file-URI path handling rejected `/C:/...` paths.

### Reference-aware cleanup

`app.maintenance` defaults to a dry run, walks scoped durable roots and checks
manifest/object integrity before proposing deletion. It preserves every referenced
revision, job, session, library, research snapshot and catalog receipt. Only old,
recognized content-addressed orphan objects within the workspace prefix qualify.
Local and S3 deletion paths are tested, including fail-closed corruption handling.

Apply requires an explicit **offline** assertion and rejects active jobs or worker
leases. Operators must stop API replicas and catalog publishers too; this is not
an online collector. Default grace is seven days. Published history is retained;
there is no automatic job-expiration policy. S3 object-version lifecycle and the
catalog warehouse remain outside this collector.

Worker scratch now has a configurable workspace-specific location. Normal runs
clean themselves; offline maintenance handles stale attempt directories after
checking descendant timestamps. No user development/production objects were deleted
during validation; deletion tests used isolated fixtures.

### Full-path validation harness

`scripts/validate_balance_deployment.py` can start an isolated SQLite API and worker,
optionally with a local S3 HTTP emulator, or target an explicitly supplied deployed
API. It submits the same synthetic mixed book to Python and Rust, waits for durable
completion, pages the journal and downloads every table partition. Ordered raw
financial rows are compared across different partition boundaries without rounding.
The harness records source/runtime identities, timing and sampled combined service
RSS. It does not change saved books or settings.

The output manifest now references the immutable queued input artifact instead of
duplicating the complete specification. Table paging and partition downloads only
parse partition metadata; a regression check rejects reads of the input snapshot
during paging. The input remains retained by the durable job and cleanup roots.

The emulator exercises real SDK HTTP uploads/downloads and object checksums. It
does **not** establish live AWS, IAM, regional network, remote catalog or production
concurrency behavior. A real deployment run still requires the user's endpoint,
bucket/prefix, existing credential profile and catalog selection.

## Validation results

- **250 engine tests passed**, no skips.
- **121 API/storage tests passed**, no skips, with SQLite and temporary PostgreSQL.
- Additional focused operations checks passed after adding the two-run catalog
  replay test. One existing Starlette/httpx deprecation warning remains.
- Local API/worker/S3-HTTP smoke validation passed for both backends with all saved
  financial rows compared.
- Final 60,000-instrument full-path run passed ordered raw parity for all **13
  tables**, including **11,050,715 journal rows**.

### Final 60k full-path measurement

60,000 mixed positions, 30 days, two named scenarios; isolated local SQLite,
API, durable worker and S3 HTTP emulator. One sample per backend, Python then
Rust on the same persistent worker. Timings include submission, request
persistence, simulation, replay, object uploads and durable completion. Startup
and post-run table downloads/comparison are excluded.

| Backend | End-to-end seconds | Combined service peak RSS |
|---|---:|---:|
| Python | 119.487 | 1,788 MiB |
| Rust daily loop | 64.354 | 1,894 MiB |

Rust was **1.86x faster** in this observation, with
**5.9% higher** sampled service RSS.
RSS includes API, worker, native child and emulator, sampled every 25 ms; it
excludes the harness/download-comparison process. The persistent worker order,
shared components and single samples preclude general memory or scaling claims.
This is not a live AWS benchmark or a full product Monte Carlo/strategy benchmark.

The compact Python result manifest is **85,783 bytes**
versus **16,952,127 bytes** for its retained input snapshot.
Result metadata scales with partition descriptors rather than duplicating the
instrument specification. Both backends passed every downloaded financial row
at the unchanged `rtol=1e-12, atol=1e-8` tolerance.

[Final machine-readable results](2026-09-29-deployment-capacity.json) retain
runtime/source/binary identities and per-table row counts. The
[pre-compaction observation](2026-09-29-deployment-capacity-before-manifest.json)
is retained separately; the two observations are not a controlled performance
ablation. Earlier engine-only benchmarks used different measurement boundaries.


Runtime numerical source and the release Rust executable were unchanged from the
prior daily-engine measurements. New changes are operational adapters and tests.
The new storage CI workflow is configured; no remote CI run is claimed here.

## External completion requirements

Live S3 validation and deployed capacity testing remain **unverified**, pending
the target resources and credentials profile. The same applies to the selected
remote Iceberg catalog. Local success does not mark these deployment tasks complete.
No live bucket, paid service or production database was created or modified.

## What would still be required for complete native engine parity

1. Implement independent native rate-path, behavioral cashflow and product pricing
   models for every supported instrument, including calibration and OAS solving.
   Preserve fixed base OAS, shared random draws and restart-required prepayment
   semantics. Compare final risk and accounting outputs, not only PV reductions.
2. Move the saved-book adapter and dynamic candidate repricing pipeline behind a
   common typed batch contract. Exercise instrument edits, dependency invalidation,
   strategy coefficients and realized candidate replay through that same contract.
3. Validate every solver constraint and accepted allocation against independent
   Python recomputation, including binding/infeasible liquidity and HTM cases.
   Rust session aggregation and a C++ LP solver do not replace product economics.
4. Repeat complete-workflow timing and peak-memory measurements with representative
   portfolios, scenario/path grids, concurrency and deployment storage. The 60k
   daily-state study does not measure full product Monte Carlo or optimization.

These are remaining engineering boundaries, not completed work implied by the
daily-state parity result. A native rewrite also preserves model simplifications:
duration-based scenario marks, research regulatory categories, calibrated behavior
and complete contractual/accounting coverage require separate model development
and validation. See the [ledger scope](../balance-sheet-ledger.md) and
[stress assumptions](../balance-sheet-stress.md).

See [production commands and contracts](../production-storage.md),
[daily engine capacity study](2026-09-29-native-state-streaming.md), and
[UI workflow](2026-09-29-partitioned-ui.md).
