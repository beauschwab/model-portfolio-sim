# Supported scope (v1 ALM core)

This page defines what a default install of the workbench supports. Everything else in
the repository is **experimental**: it is implemented and tested to its own stated
scope, but it is not part of the surface offered to a bank's treasury and model
validation teams, and the API refuses to admit it unless the deployment opts in.

The single source of truth is `apps/api/app/supported_scope.py`. Change the registry and
this page together.

## The setting

| Setting | Default | Effect |
| --- | --- | --- |
| `WORKBENCH_EXPERIMENTAL_MODELS` | `0` | `0`: only the v1 scope below is admitted. `1`: experimental contracts are admitted as well. |

The setting is read by the API process. It gates **admission of new work only**:

- Historical jobs, manifests, tables and Parquet artifacts stay readable either way.
- It changes no economics, tolerance, schema, ABI or protocol.
- It is not an access-control feature. Per-user roles are tracked separately (#65).

A refused request returns HTTP 403 naming the setting. `GET /models/contracts` reports
`supported`, `experimental` and `experimental_enabled`; its `schemas` list contains only
what the deployment admits. Product Models in the UI lists the same set and labels
experimental contracts.

## Supported products

| Product | Books and contracts |
| --- | --- |
| Mortgages and MBS | `mbs` book; `mortgage-state-1`, `mortgage-product-graph-1` |
| Securities and indexed assets | `indexed-contract-1`, `market-indexed-adapter-1`, `market-indexed-books-1` |
| Non-maturity deposits | `deposits` book; `deposit-state-1`, `deposit-product-graph-1`, `observed-deposit-calibration-1`, the deposit dated-ledger contracts |
| Certificates of deposit | `cds` book; `cd-election-1`, `term-product-graph-1`, `term-product-graph-2`, `term-product-reprice-2` |
| Wholesale funding | `debt` book; `funding-contract-1`, `funding-product-graph-1` |
| Swap hedges (economic risk) | `/hedges`; `derivative-contract-1`, `saved-derivative-risk-1` |

The seeded `loans` book (commercial loans at the legacy monthly level) remains part of the
saved balance sheet and its risk/NII/stress runs. The explicit commercial model contracts
are experimental.

## Supported outputs

- Risk: PV, OAS (original base OAS held fixed), key-rate durations, DV01.
- Earnings: NII forecast and scenario comparison; EVE and duration gap.
- Stress: nine-quarter balance-sheet stress, including the saved-book and streamed routes.
- KPIs: LCR, NSFR and CET1 projections with supplied weight tables.
- Optimizer and Strategy Lab: unit library preparation and coefficient-only evaluation.
- Intake: tape import, cohort build, publication and audit for the supported products.
- Markets: curve editing, research market-data snapshots, driver calibration and quote bootstrap.
- Supporting contracts: `model-market-1`, `market-driver-calibration-1`,
  `market-quote-bootstrap-1`, `dated-income-1`, `graph-cohort-audit-1` (non-commercial),
  `aged-units-1`, `product-units-1`, `strategy-product-1`, `product-candidate-journal-1`,
  `product-mixed-dated-1`.

## Limits

| Area | Limit |
| --- | --- |
| Tape intake | 100,000 rows and 32 MiB per tape |
| Cohort audit | Rust batches of at most 256 records |
| Streamed stress, large book | Up to 60,000 positions and 90 million work units, only with `WORKBENCH_BALANCE_LARGE_BOOK=1` and an explicit request |
| Workers | One worker process per workspace; one long run at a time |
| Deployment | One process serves one configured tenant and workspace |
| Strategy evaluation | Coefficient-only; requires a prepared unit library on a matching grid and backend |

Component and cache limits are admission bounds. They are not an aggregate memory
guarantee.

## Experimental (refused by default)

| Area | Contracts and routes |
| --- | --- |
| Commercial lending | every `commercial-*` contract, `nonintegral-fee-accounting-1`, the `/models/*-journals` publication routes, commercial cohort audits, the commercial ledger contracts |
| Consumer credit and cards | `consumer-*`, `card-*`, `lifetime-credit-loss-1`, `product-mixed-consumer-credit-1`, `product-mixed-product-credit-1`, `card-product-graph-1` |
| Credit paths and accounting | `credit-path-1`, `credit-path-accounting-1`, `credit-covariate-1`, `credit-snapshot-calibration-1`, `joint-state-1`, `joint-market-credit-1`, `mortgage-credit-accounting-1`, `/balance-stress/credit`, `/balance-stress/credit-path`, `mortgage-mixed-accounting-1`, `driven-credit-ledger-1`, `/treasury/mortgage-closing-capital` |
| Formal hedge accounting | `hedge-accounting-1`, `hedge-treasury-1`, `net-investment-hedge-1`, `nonfinancial-hedge-1`, `derivative-accounting-1`, `derivative-treasury-1` |
| Other | `conditional-exercise-1`, `advanced-product-graph-1` |

## Not yet classified

These surfaces are neither gated nor documented as supported. They need an owner decision
before v1 is frozen:

- `/treasury/runs`, `/treasury/bridges` and `/treasury/modeled-product-ftp` (capital and FTP reports).
- `/derivatives/mixed/runs` and `/derivatives/deposit-portfolio/runs` (saved derivative and joint deposit journals).
- `/balance-stress/dated` (dated ledger replay).
- The separate Rust liquidity-lifecycle service (`apps/api-rust`), which is deployed and enabled on its own.
- The embedded analyst pilot, which has its own `WORKBENCH_ANALYST_ENABLED` setting.

## Known limitations

- The seeded book is synthetic. Synthetic fixtures and supplied laws are not observed
  predictive evidence.
- Regulatory weight tables, capital requirements, eligibility and policy inputs are
  supplied by the bank. No output implies regulatory, CECL/IFRS 9 or accounting approval.
- Behavioural models (prepayment, deposit attrition and repricing) need calibration and
  back-testing on the bank's own history (#73).
- Prepayment constants are frozen per process; edits require a restart.
- Authentication is a single deployment credential at an authenticated gateway. SSO and
  per-user roles (#65) and an immutable audit log (#66) are not yet implemented.
- Local and repository checks do not deploy anything or establish provider, empirical or
  regulatory acceptance.
