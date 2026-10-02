# Production storage implementation and validation

Date: 2026-09-28. API version: 0.2.0. Engine remains 0.21.1.

## Delivered

- One portable SQLAlchemy repository for SQLite and PostgreSQL. SQLite/local
  artifacts are the default development configuration; example initialization
  preserves existing edits.
- Immutable typed input revisions, compare-and-swap edits, run manifests with
  source/native/runtime identity, scoped records and artifact keys.
- Durable database job queue with bounded admission, idempotency keys, leases,
  attempt fencing, crash retry, cancellation of publication and atomic result /
  session-journal / library publication.
- Standalone worker process owning numerical caches and native solver sessions.
  The API forwards interactive evaluation; no pricing enters that request path.
- Typed Parquet artifacts in local directories or S3, checksummed JSON manifests,
  persisted research source snapshots, and unchanged Arrow result responses.
- Recovery of precomputed strategy libraries and replay of accepted decision
  session commands. Native handles remain process-local.
- Bounded DuckDB result-table queries and individual Parquet downloads.
- Input export/import tooling, setup guide, environment examples and a CI job
  that exercises PostgreSQL and SQLite contracts.

## Validation

| Gate | Result |
|---|---|
| Entire API suite with SQLite and real PostgreSQL parameterizations | **88 passed**, 68.34 seconds |
| Engine regression suite | **103 passed**, 87.57 seconds |
| Separate API/worker processes on SQLite and PostgreSQL | Passed; queue survives API restart, queued snapshot remains unchanged after edits, outputs persist, library recovers after worker restart |
| Native decision session recovery on both databases | Passed; rebuilt version-2 session reproduces allocation evaluation after replaying an accepted constraint update |
| Concurrent edits / stale worker / cancellation / retry exhaustion | Passed on both databases |
| Input export/import and preservation of existing workspace | Passed on both databases |
| Artifact upload failure | Failed run remains unpublished on both databases |
| Typed dates, schedule columns, arrays, integrity and cross-tenant access | Passed |
| S3 object writes/reads | AWS SDK Stubber contracts passed; **no live AWS bucket validation** |
| Patch whitespace checks | Passed |

The PostgreSQL checks ran against a private loopback PostgreSQL 17.6 process with
temporary data directories and unique test workspaces. The temporary database
server was stopped after each verification run. The test-only Windows binary came
from the [Zonky embedded PostgreSQL distribution](https://github.com/zonkyio/embedded-postgres-binaries)
on Maven Central, with its published archive checksum checked before extraction.
No existing database or hosted environment was changed. Docker was unavailable,
so local PostgreSQL validation did not depend on it.

The full API output is preserved in
[production-storage-tests.log](2026-09-28-production-storage-tests.log).
The one warning is an upstream FastAPI/Starlette TestClient deprecation.
The new CI workflow is configured but has not been run on GitHub in this session.

## Production boundaries

This is the durable production foundation, not a live deployment. One configured
tenant/workspace is served per API deployment and worker actor. End-user identity,
RBAC, dynamic tenant routing, operational alerting and capacity sizing remain
deployment work; production mode requires service credentials and an authenticated
gateway. Metadata records identify the service actor, not the individual user.

Parquet on S3 is implemented. **Iceberg catalog transactions are not implemented**;
the setup guide describes the publication adapter and idempotency boundary.
Object retention/garbage collection and coordinated database/object backups must
be configured before production operation. The export tool moves inputs only.

Queries currently fetch the selected Parquet object before local DuckDB execution;
remote predicate pushdown and streaming large Arrow envelopes remain opportunities.
Cancellation prevents publication but does not forcibly interrupt native kernels.
Model/source changes reject old queued execution and require interactive rebuilds.

No new HTM policy, regulatory model, product pricer, or quant assumption was added.
Performance was not rebenchmarked at 60,000 instruments for this infrastructure
change; the earlier pricing timings do not include durable storage overhead.

See [production setup and recovery](../production-storage.md) for commands and
configuration.
