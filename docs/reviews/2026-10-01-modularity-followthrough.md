# Modularity follow-through — 2026-10-01

Implemented issues **#3, #4, #6, #7, #8 and #9** in the local working tree.
Issue **#5**, decomposition of the large native pricing/daily workflows, remains
open and outside this change. This report records implementation and validation
performed locally before the commit follow-through described below.
The shared checkout already contains substantial unpublished work and concurrent
model development. That work is preserved and is not attributed to this refactor.

## Delivered boundaries

| Issue | Implementation | Compatibility/ownership gate |
|---|---|---|
| [#3](https://github.com/beauschwab/model-portfolio-sim/issues/3) | `app/operations.py` owns the explicit 23-operation allowlist and resolution; worker composition supplies dispatch and an injected `WorkspacePort` supplies snapshot materialization. | Operation names and persisted request fields are unchanged. Persistence and repository no longer import concrete application handlers. Restore updates the existing owner rather than constructing another mutable state store. |
| [#4](https://github.com/beauschwab/model-portfolio-sim/issues/4) | The store facade shrank from 1,092 to 291 lines. `result_transport`, `telemetry`, `job_queue`, `state_views`, `demo_inputs`, and pricing/income/strategy/ledger adapter modules own cohesive responsibilities. | Public facade functions/signatures and module dictionary identity remain available. SQL revisions, revision-bound cache publication, Arrow framing, queue behavior and coefficient-only synchronous evaluation are retained. Memory execution remains explicit and test-only. |
| [#6](https://github.com/beauschwab/model-portfolio-sim/issues/6) | Typed workspace/durable-request/model/library contracts describe current dictionaries without adding runtime reconstruction. A bounded FastAPI OpenAPI generator emits reproducible JSON schemas and TypeScript client types. | Historical snapshots can omit newer optional metadata and retain explicit Python settings; production rejection/rebuild rules still apply. Arrow/Parquet paging remains handwritten and documented. Selected response routes now declare schemas. |
| [#7](https://github.com/beauschwab/model-portfolio-sim/issues/7) | `native-abi-8.json` supplies named IDs, fields, shapes and units for all 31 operations, including behavioral variants. Python enums/layouts, Rust constants/counts and readable layouts are generated. An explicit named-buffer wrapper orders fields at the foreign-call boundary. `compat.py` isolates 23 supported import aliases. | ABI remains 8 with unchanged IDs, exported signatures and batching. Native validation and validation-before-output-write behavior remain in Rust. Aliases resolve to identical module objects. Internal API imports use canonical layered paths. No per-instrument calls or financial callbacks were introduced. |
| [#8](https://github.com/beauschwab/model-portfolio-sim/issues/8) | The architecture guide now describes durable API/worker/storage ownership, Rust/C++ financial execution, independent verification and fenced publication. README, build scripts, crate descriptions and AGENTS guidance link current contracts. | Historical architecture evidence is preserved in a dated review document. Python reference execution, model limitations, fixed OAS, CRN and unchanged tolerances remain explicit. Documentation does not claim Rust HTTP/storage ownership or completion of unrelated model work. |
| [#9](https://github.com/beauschwab/model-portfolio-sim/issues/9) | AST-based Python/TypeScript checks enforce engine/application, storage/handler, canonical import and centralized browser-transport boundaries. Rust source/dependency checks and generated-contract drift checks run through development commands and a new CI workflow. | Deferred/relative imports and longer cycles are inspected without running financial code. Seven deliberate negative-fixture tests exercise enforcement. A reviewed 104-edge existing cycle baseline is ratcheted; storage-to-handler violations cannot be exempted by that baseline. Existing numerical and publication gates remain in place. |

See [current architecture](../architecture.md), [contract reference](../contracts/README.md)
and [production execution contract](../rust-production-contract.md) for ownership,
units, migration and rebuild rules. The static baseline acknowledges remaining
Python reference/compatibility and worker/session dependencies; this work does
not claim the whole project is cycle-free.

## Executed validation

Validation ran locally on Windows. All three native artifact build scripts
completed before the behavioral suites. The existing decision lockfile required
an offline refresh to include the already-present model-core path dependency.

| Check | Executed result |
|---|---|
| API: `uv run --extra dev python -m pytest tests -q` in `apps/api` | **125 passed**; includes contract, historical snapshot, registration, durable restart/recovery, lease/attempt fencing, cancellation, session and publication coverage. |
| Engine: the same pytest command in `packages/portfolio-risk` | **776 passed, 2 skipped**; includes independent reference/native ownership and public-output parity suites. |
| Native product crate: `cargo test --locked` | **42 passed**: 20 unit, 7 amortization and 15 dated-term tests. |
| Native product crate: `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings` | Passed. |
| Web: `bun run build` in `apps/web` | Strict TypeScript checking and Vite build passed. |
| Browser: `bun run test:e2e -- tests/review.spec.ts tests/native-settings.spec.ts tests/whatif.spec.ts tests/decision.spec.ts tests/forecasts.spec.ts` | **13 passed** using Edge and isolated API/web ports. Covers settings, coefficient interaction, what-if/decision jobs and forecast UI cases. |
| Repository: `bun run check:architecture` | Python/Rust/TypeScript boundaries, **7 fixture tests**, native-contract drift and HTTP-contract drift passed. Repeated after the final ABI documentation corrections. |
| Focused compatibility: `pytest tests/test_interface_compatibility.py -q` | **3 passed** after final generated-layout changes. |
| Whitespace: `git diff --check` | Passed. |

The two engine skips are the explicit **Python** variants of
`test_owned_actor_has_no_python_pricing_or_delta_callbacks` and
`test_owned_standalone_matches_actor_without_python_callbacks` in
`test_decision.py`; their contract applies only to Rust. The corresponding Rust
variants passed. Native libraries were present during validation.

The API durable tests use temporary **SQLite** workspaces. PostgreSQL was not
configured for this run. Browser fixtures use isolated memory execution and
include mocked forecast responses; they do not validate live data providers or
production storage. The newly declared GitHub workflow has not yet executed
against these unpublished changes. Existing bundle-size and framework
deprecation warnings do not constitute failed checks.

The adapter extraction also received a structural comparison of moved function
bodies against the saved pre-refactor working-tree files, normalizing explicit
owner access and canonical imports. No financial body differences were found.
Runtime, mathematical assumptions, numerical reduction order, journal posting
order and final-output tolerances were not intentionally changed.

## Publication and remaining scope

Application/native source identity changes still require matching artifact builds,
API/worker restarts, and explicit library/session rebuilds. No saved manifest or
saved economics is rewritten by this task. No deployment or production restart
was performed. Changes remain reviewable in the shared local checkout.

Issue #5 remains the next separately scoped native workflow refactor. Other
product/model and historical-validation issues are unaffected by this completion.

## Commit follow-through — 2026-10-02

The required unpublished Rust/durable-storage baseline (0.29.5) and the modularity
refactor are recorded as separate commits. The baseline is necessary because the
refactor imports native-runtime, worker and storage code absent from the previous
Git HEAD. Newer concurrent model extensions remain in the shared working tree;
build outputs, runtime data, research outputs and local editor settings are excluded.

An isolated copy of the proposed committed snapshot passed all 125 API tests,
42 native product tests compiled from its Rust source, 7 architecture fixture
tests, 3 import/ABI compatibility tests, the web build, native formatting, Python
lockfile checks, Cargo dependency resolution and both generated-contract checks.
The API tests reused local compatible native artifacts copied only into that
isolated directory. The earlier full engine/browser validation remains recorded
above; those suites were not repeated during commit preparation. No push or
deployment was performed.
