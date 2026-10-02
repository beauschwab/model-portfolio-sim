# Tape-to-position workflow

Open **Tape & Cohorts** in the workbench. Start the API and durable worker using
the existing SQLite development configuration. Rebuild the product library with
`uv run --project apps/api python scripts/build_native.py` and restart both
processes after deploying this feature. Grouping uses the native `cohort-build-1`
protocol regardless of the default simulation backend.

## Workflow

1. Upload CSV/Parquet, or provide a configured local path or S3 URI. The worker
   retains the original bytes, their SHA-256, source metadata and a Parquet table.
   Adopt the imported tape into a revision of the workspace. Loan IDs must be
   strings; CSV imports preserve leading zeroes. Parquet identifier columns must
   already be strings.
2. Edit the JSON configuration. `column_map` maps canonical names to source
   columns; `defaults` records explicit missing-value assumptions. There are no
   implicit currency, entity, accounting-category or assumption-set defaults.
3. Build cohorts. Rust performs grouping and balance-weighted aggregation.
   Rules, source hash, normalized inputs, representatives, dispersion and loan
   lineage are immutable artifacts attached to the durable job.
4. Pin a build, edit the rules, and rebuild the same tape. Compare cohort counts,
   balance conservation, changed identities, membership migrations and within-
   cohort dispersion. Identity changes include changed rule/assumption versions;
   they do not necessarily mean a loan moved to a different set of peers.
5. For supported products, price representatives or an original loan. Publish
   selected cohorts into the corresponding simulation books when satisfied.
   Publication replaces those complete books, checks the input revision, and
   preserves a receipt linking every generated position to its build and tape.
   Manual book replacement clears that book's publication receipt.

## Product-specific rules

| Product | Preset behavioral dimensions | Pricing adapter |
|---|---|---|
| Mortgage | FICO, age, remaining term, original LTV, state, channel, delinquency, rate type | Current fixed-rate USD whole loans, existing mortgage model |
| Auto | FICO, age, term, LTV, new/used, delinquency, channel | Dedicated consumer pricer still required |
| Personal | FICO, age, term, DTI, delinquency, channel | Dedicated consumer pricer still required |
| Credit card | FICO, age, utilization, transactor/revolver, delinquency, promotional status | Revolving-credit pricer still required |
| Deposit | Segment, age, insurance status, relationship, paid rate, balance tier | Existing USD non-maturity-deposit model |

Product, currency, entity, accounting category and assumption set are mandatory
separation keys. They cannot be removed or bucketed. Additional dimensions can
be categorical or numeric. Numeric intervals are left-closed: a value equal to
an edge enters the next bucket. Missing dimensions fail unless their rule
explicitly sets `separate_missing: true`. Averages always require finite values.
Zero/negative balances and duplicate loan IDs are rejected; closed accounts
must be excluded explicitly upstream. IDs are unique across the entire tape.

Rates, LTV, DTI, utilization and payment rates use decimals. Prices are per 100.
Balances use the stated currency's units. Numeric fields have balance-weighted
means, minimum, maximum and standard deviation. Mortgage remaining term and age
are rounded to model months at position construction. Loan/account size is
total balance divided by member count. Each tape row represents one loan/account.

For deposits, `assumptions` maps an `assumption_set` name to optional
`attrition_base`, `attrition_amp`, `attrition_slope`, and `attrition_gap` values.
These are validated engine-domain inputs, not fitted recommendations. Set the
simulation backend to Rust when publishing such overrides. Mortgage features
feed the existing prepayment model; arbitrary mortgage calibration overrides are
rejected. The global model/prepayment restart contract is unchanged.

## Lineage and analytical meaning

`lineage` retains `loan_id`, `cohort_id`, product, original balance, allocation
weight, build ID and source SHA-256. Cohort IDs hash the group, its full rule and
behavioral assumptions; build IDs also include the source hash and complete
configuration. Row order does not change cohort identities or reductions.

There are three explicitly labeled analytical bases:

* **Representative cohort repricing:** run the existing Rust risk and accounting
  engines on weighted representative terms.
* **Allocated cohort result:** distribute additive dollar PV, DV01, NII and
  cashflows using original balance weights. Totals reconcile to the cohort.
  OAS, prices, duration and ratios cannot be allocated as dollar quantities.
* **Individual model repricing:** use the retained original loan's terms and
  balance in the product model. This is exact with respect to that existing
  model, not evidence that its research behavior matches observed loan outcomes.

Within-cohort dispersion is a data approximation diagnostic, **not a bound on
pricing error**. Nonlinear prepayment, attrition and rate response make weighted
representatives differ from a sum of individual results. Compare loan repricing
with allocated results before choosing production buckets. The accuracy audit now compares all selected mortgage/deposit records with their
representatives. Dedicated calibrated credit/default/loss models for the new
consumer product labels remain open. Other-currency tapes can be grouped, but
cannot be published through the current USD pricing adapter.

## API and storage

* `GET /cohorts/presets`, `GET /cohorts/example`
* `POST /cohorts/imports` with `{name, format, content_base64}` or `{name, format, uri}`
* `PUT /cohorts/tapes/{name}` with `{job_id, expected_revision}`
* `POST /cohorts/builds` with `{tape_id, config, expected_revision, baseline_job?}`
* `GET /cohorts/builds/{job}/summary`
* `GET /cohorts/builds/{job}/lineage?loan_id=...` or `?cohort_id=...`
* `POST /cohorts/builds/{job}/analytics` with `{products, loan_ids?}`
* `POST /cohorts/builds/{job}/attribution` with `{analytics_job}`
* `PUT /cohorts/builds/{job}/publish` with `{products, expected_revision}`

Existing job status, manifest, table paging and Parquet downloads work for these
jobs. Table paths include `/cohorts`, `/dispersion`, `/lineage`, `/migration`,
`/risk` and `/cashflows`, depending on the job type. SQLite/PostgreSQL hold
revision and job references. Large tables use the existing immutable local/S3
Parquet object store; no second mutable tape database is introduced. Original
tape bytes and identifiers are sensitive workspace data and inherit workspace
access controls. Cohort analytical tables can now be delivered through the existing Iceberg
publisher, with per-table atomic run markers and a SQL completion receipt. Raw
tape data and nested provenance are excluded from that export. Immutable Parquet
remains the primary result boundary. Input snapshot export/import preserves original tape bytes
and published rule provenance, but does not migrate job histories; rebuild the
cohorts in the destination workspace to query lineage there. Generic Arrow job
results encode retained byte leaves as `__binary_base64__` and dates as ISO text.

Server file reads require `WORKBENCH_TAPE_ROOT`; S3 reads require a bucket/prefix
in `WORKBENCH_TAPE_S3_PREFIX`, for example `s3://bank-data/loan-tapes`.
Credentials use the worker's AWS credential chain. Imports capture the fetched
object's ETag/version ID and freeze its bytes; later builds never reread its URI.

Admission: 32 MiB input, 100,000 loans, 128 source columns, 60,000 cohorts,
24 dimensions and 32 averages per product. Loan/representative analytics are
bounded to 256 positions; larger supported cohorts can be published for normal
batch runs. Attribution is bounded to 250,000 output rows per result partition.
These limits do not claim full million-loan streaming support.

Validation commands:

```text
apps/api/.venv/Scripts/python.exe -m pytest packages/portfolio-risk/tests/test_cohorts.py
cd apps/api && .venv/Scripts/python.exe -m pytest tests/test_cohorts.py
cd apps/web && npx playwright test --config playwright.cohorts.config.ts
```

The browser configuration creates isolated SQLite/object storage and uses ports
8025, 8026 and 5186. It does not attach to the user's running workspace.


## Accuracy audit and refreshed tapes (0.29.1)

Use **Audit all selected loans** after building cohorts. The guided editor changes
product-specific bucket edges and missing-value handling; advanced JSON retains
column mapping, dimensions, averages and assumptions. The audit shows checks
outside tolerance and exports individual loan risk/cashflows in partitions. It
uses full selected populations, never a silent sample. The admission limit is
200 million position-path-month-scenario work units, and pricing batches are <=256.
Every absolute threshold is in dollars; relative thresholds are decimal ratios
(the UI accepts percent). Passing a default 1% tolerance is not a model approval.

Base PV is calibrated to each loan's supplied price; base PV agreement alone does
not establish accuracy. Stressed PV, DV01, income and cashflow differences expose
nonlinear aggregation errors. Scenario OAS remains fixed at each instrument's own
base calibration. Monthly cashflow comparisons currently cover the base scenario
only. Suggestions identify a varying numeric dimension and propose a median split;
they are not guarantees of improvement and must be rebuilt and re-audited.

POST `/cohorts/builds/{job}/audit` accepts `products`, `shocks`, `tolerances`, and
`timeout`. GET `/cohorts/audits/{job}/summary` reads compact metadata without
hydrating loan outputs. The job exposes `/errors`, `/suggestions`, `/loan_risk`, and
`/loan_cashflows` through the existing partitioned paging/download endpoints.

Build requests accept `previous_job` separately from the same-tape rule baseline.
The UI retains the previous build when replacing a tape of the same name. Refresh
rows distinguish added, removed, changed and unchanged IDs, list changed fields,
and reconcile opening balance plus movements to closing balance. A missing loan
is not automatically treated as paid off; source completeness must be established.

Publication `mode=replace_tape` retains other positions and replaces this tape's
positions in selected books. The default `replace_books` retains the original
explicit whole-book replacement behavior. New position IDs are namespaced by tape;
`cohort_id`, `source_tape_id` and `cohort_build_id` preserve provenance. Selective
mortgage publication into a historical-yield book requires `book_yield` explicitly
in mortgage averages with valid input values (or an explicit user-supplied default).
The importer will not invent historical yields. Removing an entire product from
a refreshed tape still requires an explicit book edit; an empty selection is not
silently interpreted as a deletion instruction.
