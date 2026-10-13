# Capital and FTP implementation review — October 1, 2026

Version 0.29.2 adds a Rust capital/FTP reporting framework and explicit capital
constraints in the existing Rust-owned C++ HiGHS optimizer. **This is not full
GSIB capital-model completion.** The new framework accepts eligible exposures;
the automatic tape → ledger → regulatory capital mapping remains unfinished.

## Before and after

| Area | Before | Delivered |
|---|---|---|
| Capital coverage | Synthetic CET1/RWA/NII-retention/AOCI skeleton | 13 explicit ratios: standardized and advanced CET1/Tier 1/total; Tier 1 leverage; SLR; risk/leverage TLAC and LTD; tangible common equity/assets |
| Requirement policy | Small set of research floors | Versioned per-snapshot minima, buffers and management margins; separate amount/basis-point headroom |
| Missing inputs | Limited capital model scope | Missing denominator or requirement is unavailable/unconfigured, never passed |
| Solver | Existing CET1 horizon plus LCR/NSFR/funding/EVE/commercial rows | Optional prepared limits for all 13 capital metrics, native LP construction and allocation replay, independent Python reference |
| Incremental decisions | No richer capital coefficients | Unit-grid identity validation and CAPITAL_REFRESH_REQUIRED after dirty edits with retained capital limits |
| FTP | Absent | Separate reference/repricing and liquidity/behavioral funding curves, option and contingent charges, signed deposit credits |
| Profitability | NII-based strategy objective | Additional pretax business profit, economic profit and annualized RAROC reports; NII objective remains unchanged |
| Consolidation | No FTP allocations | Treasury offsets reconcile internal transfers within entity/currency/scenario/period; no artificial external income |
| Lineage | Tape/cohort lineage exists | Supplied loan/cohort IDs retained on FTP output; not automatic loan-tape expansion |
| Product access | KPI/strategy pages | Capital & FTP panel; durable API jobs, immutable inputs/hash, paged Parquet downloads, optional Iceberg delivery |

The [detailed guide](../capital-and-ftp.md) defines every field/convention,
coverage limitation, current primary regulatory sources and operation.

## Validation evidence

- Full engine: **713 passed, 2 skipped**, 204.91 seconds. The two existing skips
  are Python-parameter cases for native ownership contracts; the native binaries
  were present and used. No tolerance was relaxed.
- Full API: **110 passed**, 94.26 seconds, against isolated SQLite storage.
- After final input-error handling and additional Iceberg tests: **19 focused API
  tests passed** across treasury, cohorts and operations. This includes immutable
  queued inputs, restart, paging/download, revision conflicts, nonfinite rejection,
  nullable columns, empty tables, and idempotent Iceberg retries.
- New capital/FTP and capital-constrained optimizer tests: **19 passed**, including
  hand-calculated numerators, distinct leverage denominators, buffers, adverse
  capital, signed transfers, unchanged consolidated profitability, missing inputs,
  malformed curves, zero allocated capital, native prepared limits, binding limits,
  infeasibility, stale unit grids and independent allocation replay.
- Native product: **20 Rust tests passed**, all-target Clippy with denied warnings.
- Native decision: **8 Rust tests passed**, release Clippy with denied warnings.
  The new transaction test rejects stale retained capital coefficients and accepts
  refreshed ones without publishing failed state.
- TypeScript and production web build passed. Existing large-chunk advisory remains.
- Two real browser workflows passed (cohorts and treasury). After final paging
  race protection/panel ordering, the treasury workflow passed again in 2.8 seconds
  (11.6 seconds including server startup). It used an isolated API, separate durable
  worker, SQLite, actual native computation and Parquet paging; no mocked results.
- Native product, decision and workflow release artifacts rebuilt. Engine version
  and both lockfiles updated; embedded engine skill package regenerated.

## 60,000-row measurement

[Reproducible receipt](2026-10-01-treasury-60000.json), produced by
`scripts/benchmark_treasury.py` in a fresh local process:

| Measure | Result |
|---|---:|
| FTP positions | 60,000 |
| Supplied capital snapshots | 2 |
| Capital ratio rows | 26 |
| Elapsed, including JSON transport and Polars formatting | 1.5905 s |
| Peak process RSS, including Python/native/transient data | 638.48 MiB |
| Maximum treasury elimination roundoff | $0.00002149 |

This is a bounded synthetic management-report batch. It **does not** include
product cashflow pricing, strategy optimization, daily ledger simulation, HTTP,
object storage, or a production regulatory model. It is not a repeat of the
earlier full-workflow benchmark and does not establish an end-to-end speedup.
The 32 MiB API request limit and 64 MiB native request limit apply independently
of row admission; long IDs/fields can reduce the admitted record count.
Report refresh belongs in a durable job. Existing interactive strategy evaluation
remains coefficient-only; this work adds no repricing to that path.

## Remaining integration work

1. **Regulatory capital bridge:** reconciled journal → eligible capital, tax,
   OCI elections, distributions, deductions, capital issuance and TLAC/LTD
   amortization by legal entity/consolidation scope. The old saved-book CET1/NII
   skeleton and daily CET1-based leverage proxy are unchanged.
2. **Exposure engines and policies:** bank-specific standardized/advanced credit
   RWA, SA-CCR/CVA, market/operational risk, leverage exposure adjustments,
   GSIB scoring, effective buffers, applicability and capital transfer restrictions.
   Reporting these supplied aggregates is not calculating them from raw contracts.
3. **Automatic FTP preparation:** derive defensible behavioral tenors and average
   balances from loan/cohort cashflows, version curve calibrations, preserve source
   receipts, and connect updates to the existing dependency graph. Current FTP
   rows and loan/cohort IDs are explicit inputs.
4. **Integrated strategy acceptance:** generate capital coefficients from the same
   revised exposures as the strategy, then reconcile dynamic capital and funding
   against daily ledger replay. Existing category-aware HTM, saved hedge/netting,
   and backbook daily funding gaps remain. Optional LP rows alone do not close them.
5. **Economic-profit strategy objective:** include real operating, credit and
   capital costs with treasury offsets. Internal FTP must not be counted as a new
   consolidated expense. Current optimization still maximizes external NII.
6. **Existing consumer/scale roadmap:** dedicated auto/personal/card models,
   empirical behavioral validation, stress monthly loan cashflows and larger tape
   intake remain as recorded in the [cohort follow-through review](2026-10-01-cohort-followthrough.md).
7. **Production acceptance:** live PostgreSQL, S3/catalog, authentication/routing
   and operational recovery/load validation were not run. Local SQLite and local
   SQL-catalog Iceberg evidence is not production acceptance.

Restart API and worker processes after the rebuilt native artifacts are deployed.
No existing user database was changed and no production deployment was performed.
