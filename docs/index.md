# Rates Workbench Documentation

- [Product model implementation and evidence](models/implementation-plan.md)
  tracks shared native credit/date contracts, product work, primary-source
  research and the remaining validation gates for issues 10 through 25.
- [Native model contracts](models/native-contracts.md) documents shared credit,
  observed calibration, dated term cashflows and supplied-fixing coupon entrypoints.

Rates Workbench is a bank balance-sheet analytics workbench backed by the
`portfolio_risk` package, with Python reference and Rust computation backends,
and exposed through a FastAPI service.

- [Native calculation migration and optimization](reviews/2026-10-01-native-owned-workflow.md)
  describes the 0.28.0 Rust runtime, C++ HiGHS boundary, allocation/cache
  optimizations, 60,000-instrument comparisons and current validation scope.
  [Migration history](reviews/2026-09-30-rust-lifecycle-migration.md) records the
  intermediate checkpoints.

- [Model Validation & User Guide](model-validation-guide.html) is a standalone,
  searchable HTML guide covering every workspace feature, end-to-end mathematics,
  worked examples, model limitations, and validation evidence. Open it in a
  browser; its formulas and interactive examples work offline.

- [Portfolio Risk Engine](portfolio-risk-engine.md) documents the quant
  package used by the backend, including model assumptions, orchestration,
  rationale, and known limitations.
- [Architecture](architecture.md) explains the engine design, why it is
  different from a traditional distributed pricing stack, and how it
  scales.
- [Production storage and workers](production-storage.md) covers SQLite development,
  PostgreSQL compatibility, local/S3 Parquet artifacts, job and session recovery,
  deployment configuration, and optional Iceberg delivery.

- [Native ledger pilot](reviews/2026-09-29-native-ledger-pilot.md) compares the
  original Python journal, a columnar Python control and the optional Rust batch
  kernel, including accounting parity, local Parquet I/O and process memory.
- [Rust daily state and partitioned journals](reviews/2026-09-29-native-state-streaming.md)
  covers the full native event loop, immutable local manifests, independent
  persisted replay and the larger capacity benchmarks.
- [Partitioned durable worker integration](reviews/2026-09-29-streamed-worker-integration.md)
  covers the opt-in API, native identity, SQLite/PostgreSQL publication, local/S3
  partitions, bounded result access and restart/failure validation.
- [Partitioned simulation UI](reviews/2026-09-29-partitioned-ui.md) covers browser
  submission, monitoring, cancellation, saved runs, bounded paging and downloads.
- [Operational completion and parity audit](reviews/2026-09-29-operations-and-parity.md)
  covers Iceberg delivery, offline retention, full-path capacity validation and
  the exact boundary of Rust/Python parity.

- [Balance-sheet Stress](balance-sheet-stress.md): daily liquidity, credit/capital, entity constraints, management policies and reverse stress. Explicit synthetic cohort research model with accounting reconciliation.


## Ledger and dynamic replay (0.23.0)

`analytics.balance_stress` schema `balance-stress-2` posts balanced journal
entries, checks every materialized subledger, independently replays persisted
lines, and derives closing statements and within-currency intercompany
eliminations. Tax follows actions; linked collateral principal repays funding.
`analytics.accounting` optionally captures instrument cash/accrual/principal/basis
flows. `analytics.balance_workflow` maps all six saved books explicitly and replays
unit-library candidates through daily limits. Missing mappings and unsupported
saved hedge trades fail closed. Monthly timing and proportional survival remain
approximations. The fast evaluator stays coefficient-only; its validation is
explicitly scoped. `balance_rules` shares LCR composition/inflow cap arithmetic
with KPIs and centralizes daily acceptance. `balance_calibration` separates
chronological training/holdout driver diagnostics; it is not PD/LGD calibration.
See `docs/balance-sheet-ledger.md` for API fields, assumptions and remaining gaps.
No regulatory compliance or full production GSIB coverage is asserted.

Native product and strategy coverage (0.27.0), parity evidence, measured performance
and shared-runtime limits: [comparison report](reviews/2026-09-30-native-product-coverage.md).
- [Capital and funds transfer pricing](capital-and-ftp.md): coverage, explicit inputs, solver limits and remaining regulatory integrations.
- [Source-linked capital and FTP review](reviews/2026-10-01-treasury-source-bridges.md): ledger/cashflow bridges, validation and remaining model scope.

- [Rust production contract](rust-production-contract.md): required native calculation, Python backend deprecation, migration and agent rules.
