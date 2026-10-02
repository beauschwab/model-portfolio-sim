# Partitioned simulation UI

29 September 2026 · Local implementation and verification

## Delivered workflow

Open **Balance-sheet Stress**, then choose **Partitioned simulation** in
**Simulation workflow**. The panel supports explicit Rust/Python selection,
editable stress assumptions, submission, progress, cancellation and reopening
historical runs by ID. The latest job ID is remembered in browser-tab session
storage under a versioned workspace-specific key; financial input data and output
tables are not stored there.

The API capability response reports durable execution, compatible Rust availability,
large-book admission and an opaque workspace identity. The UI disables unavailable
engines and the large-book option accordingly. Server-side admission remains
authoritative. The default capacity is 2,000 positions; enabled deployments can
accept an explicit large-book request up to 60,000.

The manifest exposes execution/validation metadata without copying the large input
specification to the browser. The result browser fetches **100 rows per page** from
one selected table. It never calls the full `/result` endpoint. Each Parquet download
selects exactly one partition; empty tables disable downloads. Saved results are
labelled with their input revision and kept separate from the editable draft.

Changing the selected table aborts the old request and suppresses late responses.
Job polling, submissions and downloads have lifecycle guards. Failed table queries
can be retried without resetting the table selection. Leaving the panel does not
cancel a durable calculation; returning can restore monitoring by the saved job ID.

Standard and saved-book analysis remain available as the original workflow. This
change does not route saved-book mappings through the partitioned engine or add
financial assumptions, arbitrary table filtering, a full-journal CSV export, live
S3 credentials or Iceberg catalog commits.

## Verification

- Production TypeScript/Vite build passed. The pre-existing large-chunk warning
  remains; this change did not attempt an unrelated bundle reorganization.
- **8 browser tests passed** against an isolated SQLite API and separate worker.
  Existing scenario analysis, saved mapping, stale-input hiding and invalid input
  checks passed alongside the new workflow tests.
- Real Rust and Python jobs completed, pages and individual downloads worked,
  results reopened after reload, and the client made no full-result requests.
- Controlled browser fixtures exercised delayed responses, transient query failure,
  cancellation UI and missing-run errors. They supplement real worker cancellation
  and restart tests rather than claiming a browser fixture is native execution.
- A narrow viewport check confirmed that engine controls fit the panel; the live
  page and screenshot were inspected. Real-run browser checks saw no page errors.
- **109 API/storage tests passed**, no skips, including SQLite and temporary
  PostgreSQL. One existing Starlette/httpx deprecation warning remains.

The engine and native executable were unchanged. The previous 60k benchmark remains
an engine measurement; no deployed 60k browser/S3 latency claim is made here.

Reproduce browser checks with `bunx playwright test --config playwright.stream.config.ts`
from `apps/web`, after building the native engine. The configuration starts an
isolated SQLite database, local artifacts, API, worker and Vite server on dedicated
ports. It never uses the default development database. API/worker binaries and
source must match. Temporary test artifacts can be removed when no test process
is using them; retained files are not a production retention policy.

## Remaining work

Follow-up: [operations and parity audit](2026-09-29-operations-and-parity.md)
implements offline scratch/orphan retention and optional Iceberg commits, and
measures the full local API/worker/S3-emulator path at 60k positions.
Real S3 deployment and selected remote catalog validation remain outstanding.
The research model's calibration, product-flow and regulatory limitations remain.

See [worker contracts](2026-09-29-streamed-worker-integration.md) and
[storage configuration](../production-storage.md).
