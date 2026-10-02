# Ledger implementation and validation report

> Historical 0.23 snapshot: AFS/trading premium support and further ledger profiling were added in [0.24](2026-09-29-rust-ledger-decision.md).

Date: 29 September 2026. Engine: **0.23.0**, specification: **balance-stress-2**.

The two reproduced accounting defects are corrected. The simulator now has a
double-entry ledger and explicit saved-book/candidate workflows. **The full
production GSIB model remains incomplete.** The distinction below is intentional:
working accounting and infrastructure do not make synthetic risk assumptions
calibrated, nor do they supply missing financial product or regulatory coverage.

## Review findings and delivered changes

| Review finding | Implemented change | Evidence |
|---|---|---|
| Sale earnings escaped tax | Taxes follow completed policies; dividends are excluded from deductions | Original AFS sale example now closes at cash 78/equity 58, rather than 80/60 |
| Pledged maturities became unrestricted cash | Explicit claim links, restricted cash, debt maturity release and collateral-principal repayment | Borrow 90 against 100 collateral: maturity closes cash 120/liabilities 80, rather than cash 210/liabilities 170 |
| Aggregate event log was not a GL | Balanced instrument-level journal, typed entries/trial balance, daily state comparison, independent replay | Tampered debit rejected; failed posting does not mutate journal |
| No independent financial statements | GL-derived closing statements and within-currency intercompany eliminations | Independent journal and materialized-state reconciliation |
| Saved books disconnected | Six-book product cashflow capture and explicit mapping with source IDs and units | Real MBS/loans/debt/deposits/CD/MM run reconciles equity change to existing NII |
| Cash and accrued income conflated | Separate coupon cash/accrual/principal/effective-interest amortization | Hand-calculated premium/accrual case and real product parity |
| LCR/NSFR displayed without failure floors | Daily LCR/NSFR/HTM headroom and breach checks, including reverse severity zero | All three independently invalidate dynamic acceptance |
| Separate liquidity formulas | Shared Level 2A and inflow cap arithmetic between KPI and stress engine | Existing KPI gates retained; explicit daily ruleset |
| Solver replay was only linear | Separate durable candidate origination/cashflow/journal/limit replay; optimizer validation scope labeled | Real unit candidate with no cash fails dynamic validation despite structurally valid allocation |
| No observation workflow | Provenance-bearing realized outcome comparison and chronological driver-fit diagnostics | Duplicate/nonfinite checks; altered holdout cannot change training fit/scenario selection |
| No ledger scaling evidence | Isolated-process full-journal benchmark with RSS sampling | Results below and machine-readable artifact |

An additional regression covers opening linked funding interest when collateral
matures: reversing input position order must not change cash or earnings.

## What was built

- Engine modules: `analytics/journal.py`, `balance_rules.py`,
  `balance_workflow.py`, `balance_calibration.py`; extended `balance_stress.py`.
- Product accounting optionally exports instrument cashflows/opening bases.
  Unit libraries retain coupon cash separately from income for candidate replay.
- `/balance-stress/inventory` and `/balance-stress/saved-book` use the existing
  snapshot, durable worker and immutable Parquet architecture. The synchronous
  strategy evaluator has no new engine work.
- UI exposes journal, trial balance, funding claims, limit headroom and saved-book
  mapping. Optimizer output explicitly distinguishes linear and dynamic checks.
- Version, mirrored operator skills, packaged skill, specifications and HTML
  model guide are updated. No deployment, commit or migration of user data ran.

See [current field/workflow documentation](../balance-sheet-ledger.md).

## Validation

Stable-source validation completed:

| Gate | Result |
|---|---|
| Full Python engine suite, including available native integration tests | **147 passed**, 94.04 s, no skips |
| Full API/storage suite with isolated SQLite and PostgreSQL 17.6 | **94 passed**, 79.34 s; includes real process restart/Parquet export |
| Balance-sheet Stress browser suite | **4 passed**, 20.4 s; real engine run, journal/trial balance, mapping skeleton, stale input and narrow viewport |
| TypeScript and Vite production build | Passed; existing large-chunk warning remains |
| HTML guide structure/numerical examples | Passed: 25 chapters, 13 panels, no broken links or unbalanced markup |
| Guide browser/MathML/print and architecture checks | Passed; 263 rendered equations, 3 diagrams, no script errors or external runtime requests |
| Scoped whitespace check and skill packaging | Passed |

Commands: `uv run --project apps/api python -m pytest packages/portfolio-risk/tests -q`;
`.data/storage-verification/run_postgres_tests.py` (isolated PostgreSQL lifecycle);
`bunx playwright test tests/balance-stress.spec.ts --reporter=line`; `bun run build`;
the three `.data/guide-verification` checks; `scripts/package_engine_skill.py`.
The API suite retains a Starlette/httpx deprecation warning. No live S3 service,
Iceberg catalog, production deployment or real-bank calibration was validated.

An intermediate SQLite restart test correctly returned `MODEL_VERSION_CHANGED`
because source was edited between submission and worker restart. This is the
existing immutable-model publication safeguard. Final reruns use frozen source.
No reconciliation tolerance was loosened to make the new tests pass.

## Measured journal scalability

Synthetic daily accrual; 30 days; baseline plus one scenario; full journal
retention, independent replay and GL-derived statements. Each size runs in a
fresh process. RSS sampled every 10 ms includes interpreter, Polars and retained
outputs. Product path generation is excluded.

| Positions | Time | Sampled peak RSS | Journal lines | Max identity residual |
|---:|---:|---:|---:|---:|
| 100 | 0.086 s | 138.4 MiB | 12,604 | 1.92e-11 |
| 500 | 0.455 s | 183.0 MiB | 63,004 | 5.22e-10 |
| 2,000 | 1.869 s | 388.6 MiB | 252,004 | 2.09e-9 |

[Raw benchmark](2026-09-29-ledger-benchmark.json).
This demonstrates the measured workload only. It is not a 60,000-instrument
full-ledger run or evidence that all GSIB risk functions scale to production.
The engine still admits at most 2,000 explicit positions per stress request.

## Remaining gaps and simplified assumptions

| Area | Current boundary | What remains |
|---|---|---|
| Ledger | Balanced floating-point simulation GL | Currency precision, accounting periods, reversals, close controls, production audit approvals |
| Cashflow timing | Existing engine monthly outputs mapped to 30-day reporting months | Exact product calendars, intraday liquidity, scenario-specific product path regeneration |
| Credit | Expected defaults, proportional survival, one-year allowance, watch uplift | Bank-calibrated PD/LGD/EAD, rating transition structure, CECL/IFRS9, correlated realized defaults |
| Accounting | AC/HTM premium roll and simple taxes | AFS/trading premium fair-value adapter, deferred tax/loss carryforward, full hedge accounting |
| Dealer | Explicit aggregate netting sets and posted VM | Saved hedge trade mapping, dynamic received/initial margin, collateral substitution, closeout/XVA |
| Rules | Versioned research weights, shared liquidity caps, six daily limits | Complete jurisdictional mappings and consistency with all legacy static capital approximations |
| Group | Within-currency intercompany elimination | FX translation, consolidated legal-entity capital and cross-currency transfer constraints |
| Strategy | Explicit candidate replay; inherited unit forward/time-shift assumptions | Nonlinear re-optimization/cuts, contingent reinvestment/repricing/hedging policies |
| Calibration | Training/holdout driver diagnostics and observation errors | Actual bank data, behavioral model fits, independent validation and uncertainty/tail estimates |
| Scaling | Measured bounded cohort ledger | Streaming/partitioned journals and actual 60,000-instrument full-ledger benchmarks |

Thus the ledger and core integration work are delivered, while the broader
“all production GSIB functionality” interpretation is **not complete**. Missing
bank data, regulatory policy and product adapters are not replaced by guessed
defaults or claimed as validated by the tests.
