# API and worker contracts

## Production execution contract (0.29.4; supersedes historical defaults)

Rust is the required production financial runtime. Python calculation is deprecated;
it is retained only as an independent test/reference oracle, not an API/UI backend.
New and omitted `compute_backend` settings resolve to `rust`. Historical explicit
Python snapshots remain readable, but must be switched to Rust and their libraries/
sessions rebuilt before production execution. Never silently change saved economics
or fall back when native libraries are missing.

Implement new built-in pricing, graph/coordinator, simulation, capital/FTP and solver
financial logic in Rust. HiGHS stays C++; Rust owns LP construction, invocation and
allocation validation. Production pricing/what-if must enter the native graph with
no custom discount-backend override. Do not reintroduce Python/NumPy/Numba execution
choices in application requests or controls. Keep Python oracles explicit in tests
so parity cannot accidentally compare Rust with itself. Defaults/ownership tests
must run outside the reference test context and forbid Python financial callbacks.

The existing FastAPI/worker, persistence/artifact transport and independent verifier
still use Python; this change does not claim a Rust HTTP/storage service. Keep quant
logic out of adapters. See `docs/rust-production-contract.md` at the repository root
for deployment, compatibility and validation rules. Preserve fixed OAS, CRN, final
output tolerances, immutable publication and the coefficient-only interactive path.

- `store.py` is the existing repository-shaped facade and engine adapter.
  It contains no product-pricing mathematics. `persistence.py` implements
  durable operations; `repository.py` is portable SQLAlchemy Core targeting
  SQLite and PostgreSQL. Do not add PostgreSQL-only SQL to shared operations.
- One process serves one configured tenant/workspace. Never trust a request
  parameter to choose a tenant or object-storage prefix. End-user authentication
  and routing belong at the authenticated gateway until explicitly implemented.
- New mutable inputs must enter the typed snapshot, revision commit, and run
  manifest. Write artifacts before committing references; never publish done
  before storage succeeds. Never pickle user-controlled or persisted objects.
- Keep every query scoped. Concurrent edits compare the expected revision.
  Worker publication checks its lease, attempt token, cancellation status, and
  the current input revision for interactive state, in one transaction.
- API replicas enqueue durable jobs; only `app.worker` runs heavy calculations.
  Keep one worker process per workspace. Its native solver handles and pricing
  caches are process-local; accepted session commands and inputs are durable.
- Strategy evaluation uses prebuilt coefficients only. Recover/reprice outside
  request handlers. Never expose uncommitted worker candidates to evaluators.
- Preserve fixed-OAS, common-random-number, and frozen-prepayment behavior.
  Database durability does not change model coverage or validation status.
- Parquet local/S3 persistence and optional `app.iceberg` delivery are implemented.
  Iceberg rows and run markers commit together per table; the SQL receipt marks
  completion across tables. Preserve retry idempotency and original artifact roots.
  `app.maintenance` is offline-only when applying deletion; never present its
  operator offline assertion as an online concurrency lock.
- `uv run --extra dev python -m pytest tests -q` is the API gate. Set
  `TEST_POSTGRES_URL` to repeat storage and process-restart tests on PostgreSQL.
  Tests must use temporary files, unique workspaces, and loopback ports. Never
  modify a user's existing dev or production database to validate changes.
- Keep `docs/production-storage.md` and `.env.example` aligned with behavior.
- `/balance-stress/stream` is durable-worker-only. Preserve queued native identity,
  cancellation/lease fencing and partition descriptors; never hydrate its full
  journal in the API or concatenate all partitions for paging/downloads. Large-book
  admission requires both the deployment flag and an explicit request. Existing
  stress/saved-book routes remain independent of this opt-in result contract.

- `RiskSettings.compute_backend` selects the built-in product path and must be
  propagated into worker, incremental and decision `RunConfig` snapshots. Keep
  the prepared-discount request `backend` independent. Never silently fall back
  when native libraries or custom-model support are missing. Native saved-book
  replay materializes only the existing bounded route; large journals still use
  `/balance-stress/stream` descriptors.
- Durable model identity includes the selected `portfolio-workflow` executable.
  Preserve both saved-book preflight and executed-hash publication checks; a
  process-cached identity alone does not detect a binary replaced after startup.
