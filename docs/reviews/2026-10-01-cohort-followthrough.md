# Cohort follow-through and reliability — 0.29.1

Validated locally on Windows. This closes the three confirmed Rust-process review
defects and extends the tape workflow. It does **not** complete all the remaining
consumer-product modeling and production-GSIB work.

## Closed defects

| Finding | Change | Evidence |
|---|---|---|
| Cache eviction can panic after shared identities become unshared | Refuse admission when no evictable node remains; callers retain computed values | Native cache regression plus repeated public one-loan requests at 3,000/3,250/3,500/4,000-byte budgets, compared with uncached pricing |
| Workflow deadline excludes final replay/publication | Carry an absolute deadline through persisted replay and check immediately before atomic rename; same protection added to standalone streamed replay | Injected elapsed-time overruns after replay and manifest sync publish no output; standalone Python/Rust runners also tested |
| Null asset cap crashes independent replay | Omit the asset-cap replay row when the cap is absent, retaining commercial constraints | Native and reference decision sessions solve and independently validate a commercial-bounded problem with a null global asset cap |
| All-target Rust lint fails on items after the test module | Move the public compute-pool helper before the test module | All-target Clippy with warnings denied passes |

## Added functionality

- Guided per-product bucket-edge and missing-value editing, alongside advanced JSON.
- Full selected-population accuracy audit through native product pricing, with
  individual base OAS retained under shocks and the existing CRN convention.
- PV, DV01 and NII comparisons at base and +/-200 bp; monthly base cashflow
  comparisons; configurable absolute-dollar and relative tolerances per metric
  and product; proposed numeric bucket splits. Suggestions require a new audit.
- Original-loan risk/cashflows and comparison errors emitted as bounded Parquet
  partitions. The API coalesces small pricing batches into partitions of at most
  65,536 rows to avoid one S3 object per small pricing batch. API paging does not
  hydrate the complete partitioned output.
- Cross-vintage ID reconciliation for additions, removals, term/classification
  changes and balance movements. Removed IDs are not assumed to be paid off.
- Selective tape-position replacement with tape-namespaced instrument IDs and
  retained canonical cohort IDs. Equal tapes from different sources remain
  separate; repeated refreshes do not delete another tape's positions.
- Compatible historical-yield handling: explicit book yields are required when
  merging mortgages into a historical-yield book. Existing mortgage HPI/payment
  delay defaults are materialized before schema concatenation.
- Cohort analytical tables supported by the existing Iceberg publisher, with
  retry-safe run markers. Raw source tables are excluded from this export.
- Storage CI now builds all required native components before exercising the API.

The independent audit/comparison driver remains Python; built-in financial
calculations are Rust. HiGHS remains C++. No production inputs/settings were edited.

## Validation

| Check | Result |
|---|---|
| Full engine suite | 694 passed, 2 intentional Python-parameter skips for native-only ownership contracts; 213.82 seconds |
| Full API suite, isolated SQLite | 108 passed; 84.69 seconds |
| Product Rust unit tests | 20 passed |
| Product and HiGHS-linked decision/workflow release builds | Passed |
| Native formatting and all-target Clippy | Passed |
| TypeScript/Vite production build | Passed; existing bundle-size advisory remains |
| Browser → API → durable worker → native grouping/pricing → saved positions | 1 passed; guided rule edit and full-population accuracy audit included |
| Local SQL-catalog Iceberg retry test | Passed; publishing twice leaves 20 lineage rows, not 40 |

An initial full API pass overlapped an API source edit and its restart test did
not restore the old-version cached library. The complete suite was rerun after
freezing code and passed. No tolerances or restart assertions were relaxed.
Live PostgreSQL/AWS S3 were not exercised: the local Docker engine did not become
available. A temporary Docker Desktop startup attempt was stopped. Existing S3
protocol tests and local Iceberg integration are separate evidence, not live AWS
validation. The updated CI workflow has not been run on a remote CI runner here.

## 60,000-record accuracy experiment

Both runs price all 60,000 original synthetic mortgage/deposit records using eight
paths, three income months, four compute threads and three rate scenarios, then
write/read local Parquet partitions. The fixture repeats four product-specific
loan types across 120 synthetic groups; it is not a representative production
population. The tighter age buckets deliberately separate those four types.

| Measure | Broad age buckets | Refined age buckets |
|---|---:|---:|
| Original records | 60,000 | 60,000 |
| Cohorts | 240 | 960 |
| Compression | 250x | 62.5x |
| Cohort build | 1.50 s | 1.55 s |
| Full audit including local Parquet | 65.90 s | 64.51 s |
| Peak process RSS | 891.3 MiB | 903.7 MiB |
| Loan-level risk/cashflow rows | 360,000 | 360,000 |
| Checks outside tolerance | 360 / 5,040 | 0 / 20,160 |
| Cohorts outside tolerance | 120 | 0 |
| Largest absolute PV error per cohort | $1,621.40 | less than $0.000001 |
| Largest absolute DV01 error per cohort | $15.32 | less than $0.000001 |

Tolerance is $0.01 + 1% of the corresponding summed individual value, per check.
Base PV matches supplied price by construction, so scenario and flow checks are
necessary. The timing difference is a single-run observation, not evidence that
finer cohorts inherently run faster. The full audit is a background validation
job; the small prepared-coefficient strategy evaluation remains separate.

Evidence: [broad](2026-10-01-cohort-audit-60000.json),
[refined](2026-10-01-cohort-audit-refined-60000.json).

## Separate 60,000-position native workflow regression

The rebuilt graph/decision/HiGHS/accounting/daily-ledger runtime completed the
existing diverse five-book synthetic benchmark with eight paths, 27 months and
four threads in **183.41 seconds**. Compute/partitioning took 166.86 seconds and
independent persisted replay/finalization took 16.43 seconds. It emitted
10,497,266 journal rows, 3,360,392 exposure rows and 364,834 closing GL keys in
129.28 MiB of Parquet. Peak native RSS was 622.6 MiB; process-tree peak was
1,266.1 MiB. Maximum reported reconciliation error was 2.11e-9 in ledger units.

This is execution/reconciliation evidence, **not a feasible strategy acceptance**:
the fixture reports two cash-limit breaches (baseline and stress, first on day 60)
and `dynamic_validated=false`. Its static unit-allocation funding constraint does
not establish full backbook daily liquidity sufficiency. Daily replay correctly
retains that failure. No regulatory/calibration acceptance is asserted. The fixture
uses synthetic amortized-cost loan/funding mapping, without active credit-default,
collateral/netting or management-action coverage, and excludes HTTP/live storage.
This run is separate from the tape audit, not one combined tape-to-ledger acceptance.

Evidence: [workflow regression](2026-10-01-workflow-reliability-60000.json).
Do not infer a speedup from comparison with older runs without controlling host
load, caches and runtime versions.

## Still open — not represented as completed

1. Dedicated auto/personal/card behavioral pricing, default/recovery and revolving
   balance models, their strategy templates, and complete daily-ledger integration.
2. Empirical behavioral calibration, holdout validation, approved model versions
   and production regulatory mappings; observed performance data is not supplied.
3. Million-record streaming intake. The tape still has 100,000-row/32 MiB limits;
   ad hoc drilldown remains 256 positions. The new audit batches a larger selected
   population but retains the admitted source/lineage in memory and caps work.
4. Stress-scenario monthly loan cashflows, adaptive splitting with an economically
   measured stopping rule, and automatic baseline/revised audit comparison.
5. Efficient filtered lineage lookup at very large scale; existing lookup reads
   the selected lineage Parquet table. Removal of an entire product from a tape
   requires an explicit book edit rather than an inferred deletion.
6. Live PostgreSQL/AWS S3/Iceberg deployment acceptance, full tenant authorization
   if the authenticated gateway contract changes, and operational load/failure tests.
7. Category-aware HTM allocation constraints, saved hedge/netting-set mapping,
   richer forward pricing, and the remaining documented balance-sheet assumptions.

The completed work is a tested reliability/accuracy/storage increment, not full
production-model or all-product completion.
