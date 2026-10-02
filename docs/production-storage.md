# Durable backend and calculation workers

The default backend is SQLite plus local Parquet artifacts. PostgreSQL uses the
same SQLAlchemy repository and schema. Production artifacts can use S3 through
the standard AWS credential chain. No change to pricing mathematics is included.

## Development

From the repository root, run `bun run setup`, then `bun run db:init`. Start
`bun run dev:worker`, `bun run dev:api`, and `bun run dev:web` in separate terminals.
Equivalent Make targets are `init-db`, `dev-worker`, `dev-api`, and `dev-web`.

Defaults are `.data/workbench/workbench.db` and `.data/workbench/artifacts` under
the repository root, independent of the shell's working directory. The API and
worker both use tenant `dev`, workspace `example`. Initializing or restarting
preserves existing inputs. The example portfolio and market are synthetic.
SQLite uses WAL, explicit transactions, foreign keys, and a 30-second busy timeout.
Keep SQLite on local disk on one machine; use PostgreSQL for multiple hosts.

Existing lightweight API/browser tests explicitly use `WORKBENCH_EXECUTION=memory`.
Durable tests exercise SQLite separately, including actual separate processes.

## Production configuration

Set the same storage, scope, and engine configuration on API and worker processes:

| Variable | Meaning |
|---|---|
| `DATABASE_URL` | `postgresql+psycopg://USER:PASSWORD@HOST:5432/workbench` or `sqlite:///absolute/path.db` |
| `ARTIFACT_URL` | Local directory or `s3://bucket/prefix` |
| `WORKBENCH_ENV` | `production` enables startup configuration checks |
| `WORKBENCH_TENANT_ID` | Explicit tenant identifier |
| `WORKBENCH_WORKSPACE_ID` | Explicit workspace identifier |
| `WORKBENCH_SEED_DEMO` | `0` in production; imports are explicit |
| `WORKBENCH_WORKER_URL` | Internal worker URL used by the API |
| `WORKBENCH_API_TOKEN` | Secret service credential required by the API in production |
| `WORKBENCH_WORKER_TOKEN` | Separate secret internal worker credential |
| `S3_ENDPOINT_URL` | Optional endpoint for an S3-compatible object store |

Use workload IAM credentials with access limited to the selected bucket prefix;
configure bucket encryption, versioning and lifecycle policy externally. Do not
put cloud credentials in browser code. The browser should access the API through
an authenticated gateway that injects the backend service credential.

From `apps/api`:

```text
uv run python -m app.storage_admin migrate
uv run python -m app.storage_admin import /path/to/exported-inputs
uv run uvicorn app.worker:app --host 127.0.0.1 --port 8002 --workers 1
uv run uvicorn app.main:app --host 127.0.0.1 --port 8000
```

Run the migration command once before starting replicas. It installs schema
version 2 or upgrades version 1 by adding the catalog-publication receipt table;
existing revisions are preserved. Unknown versions are rejected. Future changes need
explicit migrations; `create_all` is not a general upgrade mechanism.

API and worker should use the same immutable application image, dependency lock,
Python/platform, and optional native binaries. Run manifests record source and
binary hashes, versions, paths/seeds/settings and exact typed inputs. A queued
run or recovered interactive cache from an incompatible build requires a new run.
The identity includes the product/decision libraries, raw ledger executable and
`portfolio-workflow` executable selected by `PORTFOLIO_WORKFLOW_RUST_BIN`.
Durable native saved-book jobs check the current workflow binary before execution
and its executed hash before publication, including replacement after worker start.
Deploy immutable binaries and restart API and worker together after rebuilding.

### Deployment boundary

One API deployment and one active compute actor serve one configured workspace.
All database operations and object paths are scoped by tenant/workspace. Multiple
API replicas may share that workspace; workers use an expiring lease to prevent
two actors publishing into it. Scale independent workspaces with separate actors.
This preserves the existing module-dictionary facade and native solver ownership.

This release does **not** dynamically route arbitrary tenants through one process,
implement end-user OIDC/RBAC, or implement user-specific approvals. The production
service-token gate is designed to sit behind an authenticated gateway. Audit
revisions currently identify the API service, not the individual human. Restrict
network access to the internal worker. A future workspace router and identity
adapter can replace deployment-level isolation without changing numerical APIs.

## Data and publication

Database tables hold workspaces, append-only input revisions, jobs, accepted
session journals, library references, and research snapshot references. Business
inputs live in immutable typed snapshot manifests whose table leaves are Parquet;
they are not duplicated into a second independently editable instrument database.

Artifacts are addressed by SHA-256 under a tenant/workspace-specific prefix.
Manifests preserve dates, tuple keys, numerical array shapes/dtypes, models, and
Object schedule columns without pickle. Schedules use typed JSON inside Parquet
strings and are reconstructed on load. Arrow remains the browser response format.
Checksums and workspace identity are verified on artifact reads.

Each edit writes its artifacts first, then advances the database revision with
compare-and-swap. A failed commit restores the process's current durable state.
Clients can send `If-Match: <revision>` on edits to reject stale forms; legacy
clients without this header retain whole-book replacement semantics.
Every revision keeps a timestamp, service actor, and reason. Effective valuation
dates and market provenance remain inside the snapshot. This is not a full
bitemporal instrument master or regulatory policy management system.

Job submissions persist an allowlisted operation and an immutable input snapshot.
`Idempotency-Key` deduplicates retries; reusing a key for a different request gives
409. The database itself is the durable queue, so there is no database/broker
dual-write gap. Queue admission is bounded per workspace.

A worker acquires a lease, claims a job, and runs one heavy calculation at a time.
Its kernels control numerical parallelism. Heartbeats extend the lease. A new
owner retries an interrupted job with a new attempt token (maximum three attempts).
Lease and attempt checks fence obsolete workers and cancelled jobs at publication.
Do not configure multiple Uvicorn worker processes for the calculation service.

Outputs upload before the result pointer commits. The same database transaction
publishes the accepted session journal or unit-library reference. An input edit
prevents stale interactive publication; ordinary historical results remain tied
to their original revision. Failed uploads never mark jobs done. Unreferenced
objects from failed attempts are harmless but require eventual garbage collection.

`DELETE /jobs/{id}` cancels publication. It does not forcibly interrupt an executing
Numba/native kernel. The worker finishes/discards that work before taking another
job. Cancellation is represented as `status=error` with an explicit detail to
preserve existing frontend job contracts.

## Interactive recovery

Unit libraries are saved as typed arrays/Parquet and loaded after restart.
Decision sessions persist the build inputs and ordered accepted update commands.
The worker rebuilds and replays them before accepting interactive requests.
Live native pointers are never serialized. Closed sessions are not recovered.
Code/model changes require explicit rebuilding instead of silently replaying under
a different model. Recovery can be expensive and has no instant-resume guarantee.

Allocation evaluation continues to use precomputed coefficients. The API forwards
it to the owning worker; it performs no pricing on that request. End-to-end network
latency is additional to the engine's sub-millisecond computation target.
During calculation/publication, interactive evaluation returns a retryable 409.

## Results, DuckDB and Iceberg

- `GET /jobs/{id}/result`: existing Arrow envelope, regenerated from saved output.
- `GET /jobs/{id}/manifest`: model identity, input settings and artifact/table references.
- `GET /jobs/{id}/table?path=/...&offset=0&limit=100`: bounded DuckDB table paging.
- `GET /jobs/{id}/parquet?path=/...`: download one authorized result table.
- `GET /revisions`: recent input revision history.

DuckDB runs in-process for reporting, with one thread and a 256 MB query budget;
it is not the transactional state store. The first implementation fetches and
verifies the selected Parquet object before querying its local copy. It does not
yet use remote S3 predicate pushdown or accept arbitrary user SQL. The object
download itself is outside DuckDB's memory budget. Arrow result reconstruction
also materializes the result; use table paging/downloads for large runs.

**Parquet on S3 and an optional Iceberg publisher are implemented.** Install the
API `iceberg` extra and configure a named PyIceberg catalog, then run:

```text
uv run --extra iceberg python -m app.iceberg JOB_ID --catalog analytics
```

The publisher accepts completed partitioned stress jobs only. It writes one stable
table per workspace/result-kind/schema fingerprint, adding run ID, revision,
tenant/workspace and source-result identity columns. It commits each table's rows
and idempotency marker in one catalog transaction; conflict retries reconstruct
the input partition iterator. After every table commits, a scoped SQL receipt
records exact table UUIDs and snapshot IDs. A crash between catalog and SQL commits
can be retried without duplicating data. Data from different runs shares tables.

This is **not a multi-table atomic catalog transaction**. Consumers requiring a
complete run must wait for the SQL publication receipt exposed in the job manifest.
Original immutable artifacts remain the replay source. Catalog snapshot/metadata
expiration, compaction, table-layout tuning and catalog-owned orphan cleanup remain
catalog operations; the application's collector does not touch the Iceberg warehouse.
Run markers must be preserved while retries are permitted. Metadata grows with
retained runs, and bounded per-partition appends may create many small data files.

REST and SQL catalogs use the standard PyIceberg configuration interface. A real
SQL-backed local catalog is tested; remote REST/Glue deployment behavior is not
asserted by that test. The optional dependency is bounded to PyIceberg 0.10.x and
locked. On Windows, use `py-io-impl: pyiceberg.io.fsspec.FsspecFileIO` for a local
`file:///C:/...` warehouse. Configure cloud catalog authentication outside command
arguments, and keep its warehouse separate from application artifact prefixes.
See [PyIceberg transaction documentation](https://py.iceberg.apache.org/api/).

## Backups, retention and validation

`storage_admin export DIRECTORY` writes a portable input snapshot and its Parquet
objects. Import requires a new workspace and resets its initial revision to zero.
This is an input migration tool, not a complete backup. Back up the database and
referenced object storage together; SQLite requires an online backup or quiesced
database, not a casual copy of the main file while WAL writers are active.

Published outputs are not automatically expired. `python -m app.maintenance`
produces a dry-run orphan plan. The collector walks every scoped revision, job
request/result, session, library, research snapshot and catalog receipt, verifies
the manifest graph and retains all referenced objects. It selects only recognized
content-addressed objects older than the grace period (seven days by default,
minimum one hour), within the configured workspace prefix.

To apply, stop **all API replicas, workers and catalog publishers**, drain active
jobs, then run `python -m app.maintenance --apply --offline`. The offline flag is
an operator assertion: the tool cannot discover every API replica. It rejects an
active worker lease or queued/running jobs and fails closed on corrupt/missing
references. This is an offline maintenance operation, not concurrent online GC.
S3 deletion uses ordinary delete-object semantics; versioned-bucket historical
versions need a separate lifecycle policy. No history is expired by this command.

Worker scratch lives under `WORKBENCH_SCRATCH_DIR` in a workspace-specific folder.
Normal attempts clean themselves. Offline maintenance also removes stale
`balance-*` attempt directories after checking the newest descendant timestamp.
Catalog warehouse cleanup, scheduled maintenance, operational dashboards, per-user
audit identity and production sizing remain deployment concerns.

Run `uv run --extra dev python -m pytest tests -q` from `apps/api`. Set
`TEST_POSTGRES_URL` to a disposable PostgreSQL database to repeat repository
contracts against PostgreSQL. Tests use unique workspaces and never overwrite
existing workspace data. S3 tests use the AWS SDK stub, not a live bucket.

The persistence layer does not expand LCR/NSFR/HTM model coverage. Existing fixed-OAS,
common-random-number and frozen-prepayment rules continue to apply.


### Balance-sheet stress jobs

`POST /balance-stress/run` queues `store.run_balance_stress` with a versioned
explicit specification and expected workspace revision. The specification is
immutable request data, retained again in the result; it does not introduce
a second saved-book source of truth. All result frames use the existing Parquet
codec, manifest, DuckDB table paging and export routes on SQLite or PostgreSQL.
See [model scope](balance-sheet-stress.md).

### Partitioned Rust/Python stress jobs

`POST /balance-stress/stream` accepts `expected_revision`, `specification`,
`backend` (`rust` only in production; Python execution is deprecated) and `large_book` (default false).
This route requires durable execution. The API validates and enqueues; the
separate worker owns all calculations. Existing `/balance-stress/run` and
saved-book routes retain their behavior and limits.

Build with `uv run --project apps/api python scripts/build_ledger_native.py`
from the repository root, then restart both API and worker. Both processes must
have matching engine/API source and native binaries. The queued identity records
the Rust state executable SHA-256. Missing/replaced binaries fail explicitly.
Set `WORKBENCH_BALANCE_LARGE_BOOK=1` on the API to admit explicit requests up to
60,000 positions and 90 million work units; the default remains 2,000/3 million.
Admission does not establish a latency or memory guarantee for arbitrary books.

The worker uses temporary local disk for simulation and independent journal replay,
then imports each verified Parquet partition into the configured local or S3
artifact store. Each object is checksummed and scoped to the workspace. It commits
result references only after every upload succeeds, using the existing SQL lease,
attempt-token and cancellation fence. This same path runs on SQLite and PostgreSQL.
Historical results retain their queued input revision even if inputs subsequently
change. No interactive cache is published by this operation.

Cancellation and lease loss propagate to the native child through the watchdog.
A killed worker is retried from the immutable request by its replacement; engine
checkpoint resume is not implemented. A hard kill can leave temporary directories
or unreferenced uploaded objects; the offline collector described above handles
these after its grace period. Native execution uses the engine's 30-minute timeout; this
does not bound upload or verification time as a whole-request service deadline.

`GET /jobs/{id}/manifest` lists `partitioned-parquet` descriptors with schemas,
total rows and ordered partition references. `/result` returns descriptors and
execution metadata in the existing envelope, without hydrating all table rows.
`/table?path=/journal&offset=...&limit=...` fetches only intersecting partitions,
checks their identity/schema and returns up to 1,000 rows. Ordered partition
pagination uses Polars; existing single-object table reporting still uses DuckDB.
`/parquet?path=/journal&partition=0` downloads one verified partition. A partition
index is required for partitioned tables; the route never silently concatenates a
large journal. A client must explicitly consume this new result contract.

The web workbench provides this contract under **Balance-sheet Stress → Simulation
workflow → Partitioned simulation**. It discovers availability through
`GET /balance-stress/capabilities`, monitors/cancels durable jobs, restores the
latest workspace run in the same browser tab, and supports reopening by job ID.
It fetches 100 rows per page and downloads one partition at a time. Manifest
execution metadata references the immutable queued request instead of duplicating
the input specification. Result paging therefore does not parse the full portfolio
or journal; metadata remains proportional to partition count. Historical results display their saved
revision independently of the current draft.

S3 publication uses the existing AWS object adapter. Local and temporary PostgreSQL
integration tests and AWS SDK protocol/failure tests are separate from live S3
verification. Iceberg delivery is the explicit post-publication operation above.

Reproduce full-path capacity and raw output parity from the repository root:

```text
uv run --project apps/api --extra iceberg --with psutil --with "moto[server]>=5,<6" python scripts/validate_balance_deployment.py --positions 60000 --local-s3 --memory-mib 6144 --output .data/deployment-capacity.json
```

This launches isolated local services and an S3 HTTP emulator, then removes their
temporary state. For an authorized deployment, pass `--base-url` instead of
`--local-s3` and set `WORKBENCH_VALIDATION_TOKEN` in the environment if required.
Remote runs persist two historical jobs and their artifacts in that workspace;
they do not edit its saved books. Remote service RSS is not measurable by this
client. See the [measured scope and parity report](reviews/2026-09-29-operations-and-parity.md).

### Product backend deployment (engine 0.27.0)

Since 0.29.4, `compute_backend` defaults to `rust`; Python calculation is deprecated. Rust requires the
product release library on each worker; optimizer and saved-book replay also
require the decision and ledger binaries. Build them for the target host and
restart workers after replacement. Persist the setting in queued snapshots and
rebuild unit libraries after a switch. The `backend` field on incremental pricing
requests separately selects the final discount reducer. Python still owns durable
publication and independent validation. No database migration is needed for the
new setting: older payloads missing this field receive the Rust default. Explicit
Python snapshots remain readable but cannot execute; select Rust and rebuild
libraries/sessions. See [the production contract](rust-production-contract.md).
# Tape and cohort artifacts

Tape imports and cohort/loan analytics run as durable jobs. Adopted tapes and
position-publication receipts are part of workspace revisions. Original bytes,
source hashes, source tables, rules, representatives and lineage use the existing
immutable artifact store; relational storage remains portable between SQLite and
PostgreSQL. Server reads are restricted by `WORKBENCH_TAPE_ROOT` and
`WORKBENCH_TAPE_S3_PREFIX`. See [the tape workflow](tape-cohort-workflow.md) for
limits, analytical distinctions, source mapping and API endpoints.


Cohort accuracy jobs stream errors and original-loan analytics in immutable
Parquet partitions. The Iceberg publisher also accepts completed cohort build,
audit, attribution and analytics jobs, selecting explicit analytical tables.
Per-table run markers and SQL completion receipts preserve retry idempotency.
Retained raw tape and nested provenance are not exported by this publisher.

## Capital and FTP reports

`POST /treasury/runs` queues versioned explicit capital/FTP input snapshots on the
existing durable worker. SQLite and PostgreSQL use the same operation registry and
revision checks. Capital ratios, loan/cohort FTP results and treasury eliminations
are immutable Parquet tables, with paged reads and optional Iceberg delivery. The
raw policy and input hash are retained with results. This adds no second mutable
capital database. See [capital-and-ftp.md](capital-and-ftp.md) for model scope and
admission limits. Live PostgreSQL/S3 deployment acceptance remains a separate gate.


Source-linked reports use `POST /treasury/bridges` and read completed ledger or
cohort-analytics job artifacts. Ledger journals are streamed partition by partition
through independent replay; only admitted closing tables are materialized. Source
job ID, revision and result SHA-256 bind the report to its inputs. The prepared
specification, policy, bridge audit tables and source receipt persist with the
result. Failed verification/cancellation follows the existing no-publication gate.
`capital_bridge`, `rwa_contributions` and `ftp_preparation` are also eligible for
optional Iceberg delivery. This adds no new database schema or mutable source store.
