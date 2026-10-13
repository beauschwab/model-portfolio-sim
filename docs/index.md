# Rates Workbench Documentation

- [Product model implementation and evidence](models/implementation-plan.md)
  tracks shared native credit/date contracts, product work, primary-source
  research and the remaining validation gates for issues 10 through 25.
- [Native model contracts](models/native-contracts.md) documents shared credit,
  observed calibration, dated term cashflows and supplied-fixing coupon entrypoints.
- Commercial model contracts: [credit and daily journals](models/commercial-credit-ledger.md),
  [native conditional exercise](models/commercial-exercise.md) and
  [graphs, coefficients and allocation](models/commercial-graph-strategy.md).

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
- [Actual-date daily replay](models/dated-ledger.md), [bounded source store](models/dated-store.md),
  [coupled credit operator](models/credit-operator.md) and [explicit credit accounting](models/credit-accounting.md).
- [Dated replay and credit foundation review](reviews/2026-10-01-dated-ledger-and-credit-foundations.md): local 0.29.6 implementation, validation evidence and remaining integration scope.

- Explicit native product contracts: [installment lending](models/consumer-installment.md),
  [revolving cards](models/consumer-card.md), [deposit state](models/deposit-state.md),
  [mortgage/agency state](models/mortgage-state.md), [corporate/CD enhancements](models/term-enhancements.md),
  [funding and collateral](models/funding-contracts.md), [dated income](models/dated-income.md)
  and [market models](models/market-models.md).
- [Source-linked market quote bootstrap](models/market-quote-bootstrap.md) retains explicit par schedules,
  curve pillars and bootstrap/validation residuals; [market-driver calibration](models/market-driver-calibration.md)
  retains chronological splits and uncertainty diagnostics.
- [Observed deposit behavior calibration](models/deposit-calibration.md) fits explicit runoff,
  closure and migration spells while preserving the supplied daily deposit policy boundary.
- [Aged strategy coefficients](models/aged-units.md), [stochastic credit paths](models/credit-path.md),
  [credit panel calibration](models/credit-snapshot-calibration.md) and [derivative/CSA contracts](models/derivative-contracts.md).
- [Joint daily credit replay](models/credit-replay.md): physical state, reviewed accounting,
  complete claims, independent immutable verification and durable source routes.
- [Joint stochastic market/credit generation](models/joint-market-credit.md) composes
  named PSD market shocks with explicit borrower hazard elasticities while retaining
  the shared CRN and adoption boundary.
- [Credit covariates](models/credit-covariate.md), [realized credit ledger](models/credit-path-ledger.md),
  [consumer graph capture](models/graph-dated-ledger.md), [frozen driver replay](models/driven-credit-ledger.md),
  [card pricing graph](models/card-product-graph.md) and [reviewed derivative accounting](models/derivative-accounting.md).
- [Mortgage pricing graph](models/mortgage-product-graph.md), [deposit liability graph](models/deposit-product-graph.md),
  [funding/collateral graph](models/funding-product-graph.md), [term/CD graph](models/term-product-graph.md)
  and [representative/account diagnostics](models/graph-cohort-audit.md).
- [Performing product coefficients](models/product-units.md),
  [native product allocation](models/product-strategy.md),
  [deposit source capture](models/deposit-graph-capture.md) and
  [verified deposit daily replay](models/deposit-graph-dated-ledger.md).
- [Original-yield deposit accounting](models/deposit-original-income.md) retains
  explicit purchase/carrying anchors and complete unchanged expected cash.
- [Performing mixed replay](models/product-mixed-dated.md) combines a supplied
  dated backbook with exact-date candidate purchases in one chronological journal.
- [Acquired installment accounting](models/consumer-credit-accounting.md) couples
  one physical auto/personal source to original-yield and reviewed credit intents.
- [Mixed installment credit](models/product-mixed-consumer-credit.md) defines its
  exclusive monetary owner in the same independently verified dated daily journal.
- [Acquired card accounting](models/card-credit-accounting.md),
  [tagged event owners](models/product-event-owner.md) and
  [tagged mixed credit replay](models/product-mixed-product-credit.md) preserve
  complete statement/commitment and claim tails in the same daily GL; wider
  release/adoption scope is recorded in the validation report.
- [Candidate acquisition journals](models/product-candidate-journal.md) retain
  original-yield recognition and full closing rights for explicit performing allocations.
