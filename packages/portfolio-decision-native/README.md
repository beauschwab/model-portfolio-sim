# Native decision and workflow execution

Rust owns instrument-delta lookup, staged aggregate updates, strategy coefficient
state, a persistent HiGHS model, allocation replay, and versioned publication.
Each session is an actor; its C++ HiGHS object is created/used/dropped on one thread.
There is no `unsafe impl Send` for solver pointers. HiGHS runs one solver thread.

The Rust backend links `portfolio-risk-native` and `portfolio-ledger-native` directly.
Its actor prices changed contracts and templates without Python callbacks. The
Python reference route can still supply contributions to the older `create` API.
`strategy/decision.py` independently checks native results and publishes only
after its API snapshot guard succeeds.
Decision pricing requests parallel DV01 and NII only: key-rate bumps are pruned
because this LP does not consume KRDs. Ordinary full-risk calls retain them.

From the repository root:

```powershell
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py --test
uv run --project apps/api python scripts/benchmark_decision.py
```

A C++ toolchain is required (Visual Studio Build Tools on Windows). The helper
uses an isolated current CMake and libclang; it does not upgrade system tools.
Stop processes using the DLL before rebuilding it on Windows. The library is
discovered in this crate's release folder, or through `PORTFOLIO_DECISION_LIBRARY`.
An installed Python package needs an explicit native-library location.

The ABI has two symbols: `decision_request` (owned UTF-8 JSON response) and
`decision_free`. Callers free every response exactly once. Requests are copied,
not zero-copy. Initialization sends the whole snapshot; subsequent requests send
only changed positions/columns or constraints. Session handles are process-local
and are not authentication tokens. The Python adapter adds the DLL SHA-256 to
the context identity. This interface is internal to the prototype, not a stable
public service contract.

Operations: `create_owned`, `update_owned`, `publish`, `abort`, `eval`, `status`,
`close`, plus the reference `create`/`plan`/`stage` contract. `run_owned` executes
raw-book edit steps standalone. Owned actors reject externally supplied priced
contributions and columns.
Every mutation checks a version and transaction token. Stage never changes
published aggregates. Aborting discards the candidate and resets solver state;
ordinary successful updates retain the solver model. Row-layout changes rebuild
the model. Reuse is measured separately from simplex iterations: it does not
promise fewer iterations for every edit.

The LP matches the existing robust NII optimizer: monthly LCR including both
Level-2A cap branches, NSFR, two-sided EVE, funding, horizon CET1, asset caps and
commercial bounds, all across scenario intersections. Variables/RHS are scaled
to millions internally; published amounts and duals use dollar conventions.
Only optimal finite replayed allocations publish. Infeasibility publishes an
explicit outcome without an allocation. Time limits and other solver failures
abort; they are not mislabeled as infeasible.

Limits: 8 native sessions (API: 4), 8 queued native messages/session, 200k position
records/session, 10k edited records/request, 1,024 units, 13 scenarios, 120 months,
128 MiB request payload, 15 seconds per solver call. The API evicts stale/idle
sessions when opening another session; explicit close releases a session sooner.
Pricing cache is bounded separately. These bounds are not an aggregate RSS cap.

Durable storage and tenant/workspace routing remain outside the financial runtime.
No GPU or MILP/nonlinear optimizer is included. The standalone workflow has a
cooperative deadline; the outer adapter can kill its process for hard cancellation. See the comparison report in
`docs/reviews/2026-09-28-decision-prototype.md` for measured scope and limitations.

## Native workflow ownership (0.28.0)

Rust now owns the transactional decision repricing coordinator: dirty-row
selection, scenario dispatch, fixed-base-OAS repricing, KPI contribution deltas,
selective unit-template rebuilding/merging, HiGHS invocation and staged state.
Python retains raw table transport, independent coefficient replay and the API
publication guard. Constraint-only updates invoke no pricers. The complete
what-if baseline/revised graph and difference calculation also execute natively.

`portfolio-strategy` accepts `run_owned` with raw books/markets/histories and edit
steps. `portfolio-workflow` combines those stages with bounded accounting capture,
explicit saved-book/candidate mapping and daily journal/report execution in one
native process. `analytics.owned_workflow.run_owned_workflow` transports inputs,
writes immutable Parquet partitions, independently replays their postings, and
atomically publishes a manifest only after verification. It stores the immutable
raw request and its hash. The existing native saved-book route also uses this
coordinator; its materialized output remains limited to 250,000 cashflow rows.

The native streaming workflow captures at most 256 contracts per batch and retains
numeric schedules with dictionary position indices in a 128 MiB capacity budget.
It admits at most 62,000 total ledger positions, including saved-book instruments
and generated candidates, and 120 million daily work units. This is a separate
native generated-schedule tier; ordinary raw-spec API admission and the existing
60,000-position/90-million-work tier remain unchanged. No multi-million-row
cashflow JSON document crosses the runtime boundary.

Native cooperative deadlines run between stages/batches/days. The transport also
has a process watchdog for cancellation, deadlines, broken pipes and failed
writes. HiGHS retains its C++ implementation and bounded solve time. Component
cache/schedule limits do not constitute an aggregate RSS cap. Custom Python model
extensions remain explicitly separate from the built-in Rust path.

Model scope is unchanged: monthly capture uses 30-day reporting months, regulatory
weights are research assumptions, forward unit pricing remains approximate,
saved hedge-to-netting mapping is explicitly rejected, and HTM restrictions are
checked by daily replay rather than a category-aware unit-template LP. Offline
empirical-driver calibration and held-out diagnostics remain independent analysis
tools. See `docs/reviews/2026-10-01-native-owned-workflow.md` for acceptance evidence.

Session creation reserves capacity without holding the global registry during
pricing; failed initialization releases the reservation. Unrelated session
evaluation and close requests can proceed during another session's initialization.
The build helper places product runtime third-party notices beside the linked
decision and workflow binaries; retain those notices when distributing binaries.
