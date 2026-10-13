# Rates Workbench MCP

The MCP server exposes the supported Rates Workbench application operations to an
agent through **stdio**, backed by the existing authenticated HTTP API. It owns
no financial calculations, database, worker or saved workspace. Rust execution,
fixed base OAS, shared CRN, coefficient-only evaluation and independently verified
publication remain unchanged.

## Implementation plan and acceptance

1. Inventory every public FastAPI operation and analytical contract. Generate
   tool schemas from OpenAPI, with an explicitly reviewed permission map. Fail
   the contract check when a route is added, removed or changes shape.
2. Implement the official MCP SDK transport, schema/resource discovery, workflow
   prompt, structured failures and bounded HTTP/Arrow transport. Grant capabilities
   at process launch and enforce them again on each call.
3. Close workflow recovery gaps through scoped job/session discovery and bounded
   JSON book pages. Add validated edit previews and atomic targeted updates using
   the existing revision store and cache invalidation.
4. Require observed revisions on compute/edit/publication calls and caller-held
   idempotency keys for durable submissions. Preserve uncertain outcomes; do not
   automatically retry writes or silently change revisions.
5. Verify the real stdio handshake, tool dispatch, transport bounds, permission
   denials, durable discovery/reconnect, revision conflicts and failed-write cleanup.
   Run repository boundary and generated-contract checks.
6. Provide separate operations and strategy agent launchers, instructions and
   enforced action sets. Test real intake/publication and native strategy session
   journeys, role denials, coefficient reuse, stale versions and recovery.

The generated `src/workbench_mcp/catalog.json` is the exact route-to-tool coverage
matrix; `policy.json` is its reviewed permission source. The initial implementation
covers **92 operations** (including six new discovery/edit routes) and **51
analytical contract names**. Every operation has a named tool. This is application
interface coverage, not empirical model acceptance or proof that every financial
workflow has been run through an LLM.

## Operations and strategy agents

Two ready-to-connect profiles are available. Launch each as a separate MCP server
in your assistant and use its corresponding workflow prompt. The assistant/model
is supplied by the host client; this package does not run a background LLM loop.

| Profile | Default tools | Tools with all grants | Scope |
|---|---:|---:|---|
| `ops` | 68 | 82 | Portfolio data intake, job recovery, pricing/risk processing, cohort audit, ledgers, capital/FTP and authorized saved edits/publication |
| `strategy` | 58 | 61 | What-if, forecasts, native optimization, decision-session experiments and coefficient evaluation |
| `workbench` | 47 | 92 | Complete reviewed interface; read-only by default |

Counts describe this catalog revision. Both specialized profiles default to
`read,compute`. The operations role needs explicit `edit`, `publish` or `cancel`
launch grants for those actions. Strategy cannot edit saved inputs or publish
tapes even if all grants are present. Its optional `publish` grant permits dated
library preparation, and `cancel` permits job cancellation/session closure.
Library builds can replace prepared interactive state. These roles share the
configured workspace; they are not separate data-security principals.

Role checks apply during tool discovery and again during execution. Multipurpose
`post_run` calls have role-specific run-kind restrictions; strategy cannot request
base-OAS recalibration. New non-read actions are denied until explicitly added
to a reviewed role. Agent text cannot expand these permissions.

```powershell
# In your MCP client's launch configuration, choose one entrypoint per agent:
uv run --project apps/mcp rates-workbench-ops
uv run --project apps/mcp rates-workbench-strategy
# Equivalent: rates-workbench-mcp --profile ops (or strategy)
```

Read `workbench://agent` or invoke `workbench-ops` / `workbench-strategy` with an
`objective`. Server initialization also supplies the role instructions. Configure
the host's agent instructions to follow those instructions and inspect cited
results. Use separate host chats/agents so only the intended profile is attached.
Attaching a broadly privileged server to the same host agent defeats role isolation.
[`agents.example.json`](agents.example.json) contains both launch definitions for
this checkout. Adjust its project path and API URL for another machine, supply the
API token through your client's secret environment, and enable only the intended
server in each agent. Client configuration syntax may differ.

Example operations objective: "Inspect this supplied tape, import it, report data
quality and reconciliation, and identify the exact adoption/publication steps."
Example strategy objective: "At the current revision, compare feasible allocations
under my supplied liquidity, capital and cash limits; return source jobs/session
versions, validated metrics, binding constraints and assumptions."

The strategy brief is an evidence handoff to operations. It is not authorization
to change saved books. Operations returns a receipt of jobs, revisions, source
identity, publication/verification and unresolved exceptions. Neither profile
offers shell access, deployment, service management or database maintenance.

## Run and connect

Start the API and its existing dedicated worker normally. From the repository root:

```powershell
uv sync --project apps/mcp --extra dev
$env:WORKBENCH_MCP_API_URL = 'http://127.0.0.1:8000'
$env:WORKBENCH_MCP_PERMISSIONS = 'read,compute,edit,publish,cancel'
$env:WORKBENCH_MCP_ACTOR = 'portfolio-agent'
# WORKBENCH_API_TOKEN must match the API deployment; supply it through your secret environment.
uv run --project apps/mcp rates-workbench-mcp
```

Configure an MCP-capable client to launch `uv` with arguments:

```json
{
  "command": "uv",
  "args": ["run", "--project", "C:/Users/schwa/Repos/model-portfolio-sim/apps/mcp", "rates-workbench-mcp"],
  "env": {
    "WORKBENCH_MCP_API_URL": "http://127.0.0.1:8000",
    "WORKBENCH_MCP_PERMISSIONS": "read,compute,edit,publish,cancel",
    "WORKBENCH_MCP_ACTOR": "portfolio-agent"
  }
}
```

The default grant is `read`. Remote API origins require HTTPS. Credentials come
only from launch configuration; tools cannot choose another origin, workspace,
token or permission. Redirects and environment HTTP proxies are disabled.
This local process uses one configured API principal. It is not a multi-user
remote MCP/OAuth service. Launch separate processes with appropriate credentials
and grants for separate principals. The API remains responsible for authentication
and workspace scoping. `WORKBENCH_MCP_ACTOR` labels local audit records; it is not
an independently authenticated end-user identity or a durable database audit log.

## Permissions and tool shape

| Grant | Operations |
|---|---|
| read | Workspace/results/discovery, examples, previews and authenticated export references |
| compute | Durable analyses, market intake, library/session computation and coefficient evaluation |
| edit | Saved books/settings/assumptions/scenarios/programs |
| publish | Cohort adoption/publication, active-curve selection and dated-library publication |
| cancel | Cancel durable jobs and close decision sessions |

Prepared library/session construction can change prepared interactive state under
`compute`; `publish` specifically governs the adoption routes listed in the catalog.
No grant implies automatic approval of an action by the client or user.

Each tool has named `path`, `query` and `body` groups where applicable, preserving
the API's JSON schema. Compute/edit/publish calls require `expected_revision` at
the top level. If the API body also requires it, both values must agree. Durable
job tools additionally require `idempotency_key`.

```json
{
  "expected_revision": 12,
  "idempotency_key": "pricing-request-unique-id",
  "body": {"kind": "pricing", "expected_revision": 12}
}
```

Call `post_run` with that payload, poll `get_jobs_jid`, then inspect
`get_jobs_jid_manifest` and `get_jobs_jid_table`. Reconnect through `get_jobs` and
`get_decision_sessions`. Filter `get_jobs` by `idempotency_key` to resolve an
uncertain submission exactly. Explicit cancellation uses `delete_jobs_jid`; cancelling
the MCP request alone does not cancel accepted durable work.

Resources provide the coverage matrix, each tool schema, analytical contract names,
workflow guidance and bundled model/production documentation. Analytical inner
payloads remain explicit native contracts: the API's generic dictionaries are not
misrepresented as complete generated JSON schemas. Read the corresponding reference
and fetch a live example using `get_models_examples_schema`. Synthetic examples
do not establish empirical accuracy or supply reviewed regulatory/accounting policy.

## Results and bounds

JSON book pages support column selection, position ID and a maximum of 1,000 rows.
Arrow results are decoded into explicitly paged JSON frames via optional
`view: {"offset": 0, "limit": 100}`. This bounds tool output, not the underlying
legacy API calculation or Arrow generation. Prefer manifests and existing paged
Parquet tables for large jobs. The adapter rejects HTTP responses above 16 MiB
and tool JSON above 2 MiB; it never silently drops data. Requests are capped at
46 MiB (including base64 tape expansion) before the API applies its own operation-specific limits.

Export tools return authenticated HTTP references and MCP resource links without
embedding credentials or claiming the artifact was fetched/verified. Reading a
resource revalidates permissions and fetches it with the configured credential,
up to the 16 MiB transport bound. Larger downloads belong in an authorized file
client, not the model context. The adapter does not provide arbitrary SQL,
filesystem writes, URL fetches, engine callbacks or a fallback financial backend.

## Validation and maintenance

```powershell
uv run --project apps/mcp --extra dev python -m pytest apps/mcp/tests -q
cd apps/api
uv run --extra dev --with-editable ../mcp python -m pytest tests/test_agent_routes.py tests/test_agent_mcp_integration.py -q
cd ../..
uv run --project apps/api python scripts/generate_mcp_contracts.py --check
make check-architecture
```

API tests should run from `apps/api` (or with `PYTHONPATH=apps/api`), consistent with
the rest of the repository. They use temporary SQLite workspaces. Optional
`TEST_POSTGRES_URL` repeats the shared durable fixture on PostgreSQL.

After an intentional API/model-documentation change, review permissions and run
`uv run --project apps/api python scripts/generate_mcp_contracts.py`. Regeneration
imports FastAPI without entering lifespan, opening storage or running pricing.
The MCP package pins the official SDK's compatible v1 line and checks in its lockfile.

Remote MCP transport/OAuth, per-user durable action attribution, complete native
JSON schemas for every inner model variant, and empirical/scale acceptance remain
separate work. This implementation does not deploy services or restart workers.

The concurrent `/models/holding-journals` addition is included in the reviewed
catalog. Contract checks intentionally reject new routes until their permissions
have been reviewed, even when those routes are added by another workstream.

### Local verification (2026-10-12)

- 40 MCP tests passed, including real stdio handshakes for both roles, SDK
  calls/resources, authenticated artifact reads, role/grant enforcement,
  role-specific schemas, revision/key transport and size bounds.
- 64 focused API/persistence/contract tests passed. The operations journey runs
  intake, adoption, cohort build, native audit, publication, targeted edits and
  reconnect. The strategy journey runs real Rust/HiGHS build, constraint-only
  update with zero repricing, coefficient evaluation, stale-version rejection,
  saved-edit denial and durable session recovery. HTTP transports are in-process;
  financial execution and persistence are real, using temporary SQLite workspaces.
- 8 architecture-checker tests passed. Python/Rust/TypeScript boundary checks and
  generated HTTP, native ABI and MCP contracts passed.
- The isolated full API suite finished with **1,159 passed and one failed**. The
  failure was independently reproduced: the copied native executable predates the
  concurrent `nonintegral-fee-accounting-1` contract. After rebuilding the native
  lifecycle executable inside the snapshot, **all 235 model-analysis tests passed**
  using the suite's explicit test-binary hook. The entire API suite was not repeated
  against that rebuilt executable. See the [validation receipt](../../artifacts/mcp-validation/summary.json)
  and adjacent logs, JUnit results and source/binary hashes.
- `make` is unavailable on this Windows host; its architecture commands were run
  individually. No deployment, live account, PostgreSQL run or worker restart was
  performed for this implementation.

The deployed/default native artifacts were not replaced. Executing newly added
contracts there still requires a compatible native build and the normal API/worker
restart procedure. The role profiles require an MCP host/model; no live LLM decision
quality or unattended agent service was evaluated by these deterministic tests.
