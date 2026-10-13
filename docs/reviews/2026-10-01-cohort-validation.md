# Tape/cohort implementation validation

Validated locally on Windows, October 1, 2026. Engine version 0.29.0.

| Check | Result |
|---|---|
| Complete engine suite | 678 passed, 2 skipped; 169.59 seconds |
| Complete API suite, isolated SQLite | 103 passed; 70.98 seconds |
| Product Rust unit suite | 19 passed |
| Native release build | Passed |
| Native formatting | Passed |
| Production Rust Clippy (`--lib --bins -D warnings`) | Passed |
| TypeScript and Vite production build | Passed; existing large-bundle advisory remains |
| Real browser/API/worker workflow | 1 passed; 16.2 seconds including test startup |

The browser test imports the synthetic tape into an isolated workspace, builds
five native cohorts, changes only mortgage FICO buckets, compares the resulting
seven cohorts, retrieves original-loan lineage, prices representatives, reprices
an original loan, allocates cohort cashflows to loans, publishes the selected
books, and verifies that the Positions panel refreshes to match saved books.
It checks page errors and Vite error overlays. It uses ports 8025/8026/5186 and
fresh temporary SQLite/object storage, without touching the user's workspace.

New engine tests cover stable reductions under row permutation, balance and
weight conservation, hard product separation, rule isolation, bucket boundaries,
explicit missing-value handling, duplicates, invalid balances/rules, original
loan terms, unsupported-pricer rejection and additive-only attribution. Durable
API tests cover original tape bytes, restart recovery, revision fencing,
source allowlists, native financial calculations, allocation reconciliation and
cross-workspace snapshot export/import with retained rule provenance.

A 60,000-record synthetic tape (12,768,343 CSV bytes) parsed in 0.027 seconds and
built five cohorts plus all 60,000 lineage rows in 1.472 seconds. Total balance
reconciled to 6.09 billion. This is a low-cardinality grouping/transport exercise,
not a diverse-book financial simulation or SQL/S3 throughput benchmark. Machine-
readable evidence: [cohort-60000.json](2026-10-01-cohort-60000.json).

`cargo clippy --all-targets -D warnings` additionally exposes an existing
`items_after_test_module` warning in `portfolio-risk-native/src/lib.rs`: the
existing `with_compute_threads` function follows the test module. No numerical
implementation changes were made to address that unrelated ordering warning.

PostgreSQL and live S3 were not exercised in this run. Their existing portable
storage interfaces are reused; configured-source rejection was tested locally.
The full 60,000-position financial/solver/daily-ledger acceptance workload was
not rerun. Dedicated auto/personal/revolving-card pricers, automatic all-loan
pricing-error studies, million-loan streaming, and cohort-specific Iceberg
delivery remain outside the implemented feature scope.

The preceding Rust-process review findings remain recorded separately in
[rust-e2e-review-findings.md](2026-10-01-rust-e2e-review-findings.md); this feature
does not claim to fix those three issues.
