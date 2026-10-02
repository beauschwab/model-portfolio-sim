# Source-linked capital and FTP build review — 2026-10-01

Engine version: **0.29.3**. This builds on the 0.29.2 capital ratios, FTP allocation,
explicit solver limits and stale-coefficient rejection.

## Delivered

| Path | Ownership and behavior | Evidence |
|---|---|---|
| Reconciled ledger to closing capital | Rust maps opening equity, P&L, distributions and OCI; explicit scenario/account policy supplies regulatory adjustments | Hand-calculated components and native AFS/trading/credit/collateral/mixed closing comparisons |
| Source verification | Python independently replays persisted postings and checks every trial-balance key; API compares native bridge against source closing statements | Materialized and 37-row partitioned source tests, including transaction boundaries |
| Cohort or individual cashflows to FTP | Rust derives monthly average principal, remaining funding WAL and signed effective-interest income; retains source identity | Hand-calculated WAL/tail cases and actual native cohort NII reconciliation |
| Durable operation | Compatible completed source jobs, frozen policy, source revision/hash receipt, existing immutable Parquet storage | SQLite worker/restart tests; corruption and cancellation publish no result |
| Interactive interface | Source selector, source-job template, editable policy, paged audit reports | Real API/worker/browser tape-to-FTP workflow |
| Closing-date allowance fix | Scheduled loan principal releases the repaid allowance share immediately in both daily engines | Partial and complete repayment regressions; independent journal and backend parity |

The allowance fix was discovered during bridge validation: a loan repaid on the
last day previously retained the pre-payment allowance in its closing trial
balance. This produced negative net mapped credit exposure in the test portfolio.
The corrected entry releases the reserve through earnings on the payment day;
no ratios were clipped, tolerances relaxed or accounting plugs introduced.

## Contracts and limits

- Native financial preparation uses `treasury-bridge-1`; Python transports data and
  independently verifies publication. HiGHS remains C++ and is unchanged.
- Policies are specific to `(scenario, account)`. Source currency and amount-unit
  conversion are checked. Missing nonzero asset mappings fail closed.
- Ledger admission: 250,000 closing GL keys and 1,000 scenario/account policies.
  Source journals are read one partition at a time; replay retains closing keys.
- FTP admission: 120 months and 60,000 position-month rows. Every position must
  have all months; missing/duplicate/negative/overpaid schedules fail closed.
- An incomplete projection requires a residual-tail assumption. The report exposes
  the amount and share that depend on this assumption. Deposit reset assumptions
  remain separate from the funding life and are mandatory for non-maturity deposits.
- Original loan/deposit IDs retain their complete source values, including IDs
  beginning with `HL-`; only the generated mortgage wrapper prefix is removed.
- Conditional published income forecasts are not accepted as capital or FTP source
  jobs. Existing source analytics without capture metadata must be rerun.
- No changes to fixed OAS, CRN, scenario calibration, pricing cache or synchronous
  strategy evaluation.

## Validation

- Full API suite: **119 passed** in 112.60 seconds. Includes seven source bridge
  cases spanning cohort/individual income reconciliation, materialized/partitioned
  ledgers, restart, corrupt journals, cancellation and incompatible sources.
- Browser acceptance: **3 passed** in 30.1 seconds against isolated real SQLite,
  API, worker and web processes. Includes the complete tape-to-native-FTP path.
  The source workflow passed again after adding a template-reload regression
  that checks stale report tables are cleared (22.7 seconds including startup).
- Native unit suites: **20 product tests and 4 ledger tests passed**. Product and
  ledger Clippy pass with warnings denied; both formatting checks pass.
- Web production build and skill packaging pass. Existing large-chunk and API
  TestClient deprecation advisories remain.
- Full engine suite: **733 passed, 2 skipped** in 284.69 seconds on the final
  financial sources and native artifacts. The two skips are Python parameter
  cases for native-only decision ownership tests; their Rust cases execute.

## Remaining model and product scope

This is a closing accounting-to-capital bridge, not a regulatory capital engine.
AT1, Tier 2, deductions, eligible total TLAC/LTD, average assets, market/operational/
advanced RWA and effective requirements remain explicit closing inputs. TLAC does
not automatically change with CET1. Regulatory eligibility, GSIB scoring, SA-CCR,
CVA and capital instrument maturity treatment are not inferred. Weighted net
carrying values are research mappings. No automatic consolidation/eliminations or
full daily capital rollforward is added. Accounts without journal entries need the
manual snapshot route.

FTP is a management allocation using captured behavior and an explicit residual
tail; it does not calibrate deposit replication, contingent draw risk or optionality.
Representative cohort results remain cohort results, not independently repriced
loans. Consumer products without dedicated validated pricers retain their prior
restrictions. Source templates are editable JSON, not yet a guided policy editor.

Existing capital solver limits still require refreshed action coefficients. These
bridges do not generate them automatically or change the consolidated external-NII
objective. HTM/category-aware optimization and saved hedge/netting-set mapping retain
the previously documented limitations.

Storage interfaces remain SQLite/PostgreSQL and local/S3 Parquet with optional
Iceberg delivery. This turn's durable acceptance is local SQLite; it is not live
PostgreSQL, S3 or Iceberg deployment certification. It also is not a new 60,000-
instrument full pricing/solver/ledger benchmark.

## Operation

Rebuild product, ledger and decision artifacts and restart deployed workers before
using the new protocol. Existing workers cannot hot-load the replacement libraries.
The operator skill was repackaged. See [capital-and-ftp.md](../capital-and-ftp.md)
for inputs, formulas, endpoints and source bounds.
