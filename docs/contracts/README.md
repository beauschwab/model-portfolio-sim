# Application and native contracts

The current architecture is in [../architecture.md](../architecture.md); financial
ownership and deployment rules are in [../rust-production-contract.md](../rust-production-contract.md).

The [MCP adapter](../../apps/mcp/README.md) exposes the application through a reviewed
route-to-tool catalog. `scripts/generate_mcp_contracts.py --check` verifies API/schema
and bundled model-reference drift without entering application lifespan. New routes
require explicit permission entries. The stdio process talks only to HTTP; it has
no engine/storage imports. Optional HTTP request fingerprints in durable requests
preserve exact retry identity without replacing original deadlines or inputs.

## Contracts and generators

- `apps/api/app/contracts.py`: typed workspace/run snapshots, model identity,
  prepared-library bundles, durable requests, and the storage state port.
  These describe existing dictionaries. They do not add runtime validation or
  extra work to coefficient evaluation. Codec manifests retain schema version 1.
- `portfolio_risk/strategy/contracts.py`: coefficient/identity layouts. Units,
  metric order and missing-field/rebuild rules are explicit; economics are supplied.
- `http-openapi.json` and `apps/web/src/lib/http.generated.ts`: bounded JSON HTTP
  surface generated directly from FastAPI OpenAPI. Defaulted input fields remain
  optional. The client makes default-complete response types `Required` where
  the existing service returns those defaults. Settings/results can describe
  historical Python execution; new production execution still rejects it.
- `native-abi-8.json`: authoritative operation IDs and named positional layouts.
  `abi_generated.py`, `abi_operations.rs` and [native-abi-layouts.md](native-abi-layouts.md)
  are generated. Native domain/shape validation still executes in Rust.
- `dependency-baseline.json`: reviewed existing cycle edges. This includes deferred
  reference/compatibility paths between Python products, analytics and core adapters;
  package reexports; and application worker/session dependencies. These are not
  immediate import-failure claims. Storage-to-handler edges are forbidden even
  if they appeared in a previous baseline.

Generate/check from the repository root:

```text
uv run --project apps/api python scripts/generate_http_contracts.py
python scripts/generate_native_contracts.py
make check-architecture
```

Do not edit generated files or update the dependency baseline merely to make a
failure disappear. A newly accepted cycle needs a documented ownership reason.
No arbitrary module-length limits are imposed. The static checker imports no
application/financial modules. HTTP generation imports the FastAPI assembly to
read its OpenAPI schemas, without entering lifespan, opening storage or pricing.

## Transport exceptions

Explicit product analyses use durable `/models/runs` jobs and a cancellable
standalone native process with immutable input/executable/source identities.
`/models/aged-library` publishes actual-date coefficients through the existing
library transaction; `/strategy/eval` consumes those coefficients only. The
dated lane exposes its own date/template/grid metadata in `/state` and does not
mix a monthly backbook KPI baseline into actual-date reporting. See
[product workbench](../models/product-analysis-workbench.md).

The HTTP generator initially covers settings, market/state views, job metadata,
stress capabilities, forecast/run requests, allocations and named scenarios.
Other JSON routes can be added by declaring their response schemas first.

ARW1 results, Arrow tables, binary downloads and partitioned-Parquet descriptors
remain handwritten in `app/result_transport.py`, `app/artifacts.py`, and the
centralized `src/lib/api.ts` client. ARW1 retains its little-endian skeleton/blob
framing, object-column JSON conversion, uncompressed IPC and oldest Arrow string
layout. Scalar metadata travels in the JSON skeleton. Large journal partitions
are paged/downloaded without full hydration in the API.

## Compatibility and publication

`portfolio_risk/compat.py` registers supported flat imports as identical module
objects. Internal API code uses canonical layered imports; legacy aliases do not
select a Python financial backend. Queued durable operation names are unchanged
and resolve through `app/operations.py`'s allowlist.

ABI 8 IDs and exported signatures are unchanged. Buffers are borrowed synchronous
C-contiguous f64; integer/boolean fields stay within their validated exact domain.
All output validation completes before the first write. Missing/mismatched native
artifacts fail with no fallback. Additive JSON protocols still need new binaries.
Deploy matching artifacts, restart API/workers, and explicitly rebuild libraries
and sessions after source/model identity changes. Historical manifests are never
rewritten to make them appear current.

Snapshot restoration uses an injected state port to update the existing module
materialization. SQL revisions remain authoritative; there is no second state
store. Artifacts are written before their references are committed. Lease, attempt,
cancellation and input-revision checks still gate publication. The independent
persisted-journal verifier remains mandatory.
