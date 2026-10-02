# Partitioned balance-sheet jobs through the durable worker

29 September 2026 · Engine 0.26.0 · Local implementation and validation

## Delivered behavior

`POST /balance-stress/stream` now queues the Rust daily state runner or its Python
reference through the existing durable worker. The request contains an explicit
stress specification, expected input revision, backend and capacity tier. No
financial calculations run in the request handler. Existing stress, saved-book,
strategy and decision routes retain their contracts.

The API and worker pin the native state executable SHA-256 as part of deployment
identity. The queued request also pins its selected executable; the adapter checks
the binary immediately before execution and verifies the resulting execution
identity before SQL publication. There is no fallback to another backend when the
requested binary is missing or changed. Deploy the same source and native binary
to both API and worker, then restart both processes.

Large-book admission requires `large_book: true` and
`WORKBENCH_BALANCE_LARGE_BOOK=1` on the API. Default admission remains 2,000
positions/3 million work units. The explicit tier allows 60,000/90 million, as in
the engine capacity study. It is not a performance guarantee for arbitrary books.

## Storage and recovery

The engine first writes local temporary Parquet partitions, independently replays
the saved journal and reconciles the closing GL. The adapter verifies each
partition's path, checksum, schema and row count while importing it into the
configured immutable object store. Local storage and S3 use the same workspace
scope and content identity. Results publish only after all objects exist and the
existing SQL transaction accepts the worker lease, attempt token and cancellation
state. SQLite and PostgreSQL use the same repository operations.

Failure after some uploads can leave unreferenced objects but cannot expose a
partial successful result. Normal exits clean local temporary files. A hard kill
can leave temporary files; automatic orphan cleanup is not implemented. A
replacement worker retries the immutable request from the beginning, without
engine checkpoint resume. Historical results retain the submission revision;
they do not replace interactive state after later input edits.

Cancellation, worker shutdown and lease/attempt loss are checked during simulation
and partition import. A native watchdog terminates a child on cancellation or its
engine timeout. Database probes are throttled to approximately five per second;
SQL completion is the final authority even if cancellation arrives after a probe.
An in-progress object upload is not synchronously interrupted by that callback.

## Result contract

The durable codec stores partitioned table descriptors. Loading `/result` returns
metadata and ordered references, without reconstructing multi-million-row tables.
The new route therefore needs an explicit client integration; the current UI does
not silently switch to this response shape.

| Route | Partitioned behavior |
|---|---|
| `/jobs/{id}/manifest` | Table schemas, row totals and ordered object references |
| `/jobs/{id}/result` | Existing envelope with descriptors and execution metadata |
| `/jobs/{id}/table?path=/journal&offset=...&limit=...` | At most 1,000 rows; reads only partitions intersecting the page |
| `/jobs/{id}/parquet?path=/journal&partition=0` | One verified Parquet partition; explicit index required |

Paging preserves persisted row order and verifies each selected partition. It uses
Polars for this bounded slice; existing single-object reporting continues to use
DuckDB. It does not add arbitrary SQL, S3 predicate pushdown, a second database or
an Iceberg catalog.

## Validation

Source-frozen local validation passed:

- **109 API/storage tests**, no skips, with SQLite and temporary PostgreSQL.
  One existing Starlette/httpx deprecation warning remains.
- **230 engine tests**, no skips. The engine and native binary were unchanged
  by this integration; the prior native capacity measurements retain their scope.
- Guide structure, source references, numerical examples, browser interactions
  and architecture diagrams were refreshed and checked separately.

Run `uv run --extra dev python -m pytest tests -q` from `apps/api`, with
`TEST_POSTGRES_URL` set to an isolated PostgreSQL database for both-backend coverage.
The added contract file is `apps/api/tests/test_streamed_jobs.py`.

New tests cover immutable queued specifications, complete native worker execution,
cross-partition pagination that reads exactly the two required objects, individual
downloads, result retrieval after reopening storage, deployment capacity admission,
native identity mismatch, partial upload failure, cancellation, lease loss and
successful replacement-attempt retry. Separate API/worker process tests queue while
the worker is absent, restart the API, execute the real native job and retrieve
identical results after another API restart. AWS SDK stubs verify each partition
upload's key, body and checksum metadata; corruption is rejected before upload.

These checks are local and contained. Live S3, deployed infrastructure, concurrent
tenant load and Iceberg commits were not exercised. The earlier
[60k native capacity results](2026-09-29-native-state-streaming.md) remain engine
measurements; they do not include this new HTTP/SQL/object publication boundary.

## Remaining work

The [partition-aware UI](2026-09-29-partitioned-ui.md) is now implemented.
The [operations follow-up](2026-09-29-operations-and-parity.md) adds offline
reference-aware retention, scratch cleanup, optional Iceberg publication and a
60k local API/worker/S3-emulator measurement. Full deployed workflow and remote
catalog validation still require the target infrastructure. The native
port preserves existing research assumptions: it does not add contractual daily
product pricing, bank calibration, full regulatory mapping, XVA or hedge accounting.

See [deployment configuration and API usage](../production-storage.md#partitioned-rustpython-stress-jobs).
