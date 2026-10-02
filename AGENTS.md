# AGENTS.md â€” rates-workbench (monorepo root)

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

Three layers, three contracts:
- `packages/portfolio-risk` â€” the quant engine. Its OWN AGENTS.md hierarchy
  (root/src/tests) is authoritative for model assumptions, invariants,
  and extension recipes. Never modify kernels without reading it.
- `apps/api` â€” FastAPI over the engine. Holds NO quant logic: adapters in
  `app/store.py` call engine drivers and JSON-ify Polars frames. Long
  runs are durable jobs executed by `app.worker` in a separate process,
  one at a time per workspace (kernels saturate cores).
- `apps/web` â€” Vite/React. Talks only to the API via `src/lib/api.ts`.
  UI primitives are hand-rolled shadcn-style in `components/ui.tsx`;
  theme tokens in `tailwind.config.js` (Supabase dark: zinc + emerald).

Commands: `make install | dev-api | dev-web | test-py | build`.

Rules that cross layers:
1. Engine invariants (fixed-OAS, CRN, numba constant freezing) surface in
   the API/UI as behavior: scenario runs never re-solve OAS; prepay
   assumption edits report RESTART_REQUIRED rather than silently no-op.
2. Adding a product = engine recipe first (packages/portfolio-risk/src AGENTS),
   then a `run_*` adapter in store.py, then a book tab + KRD key in web,
   and a unit template in unitlib.TEMPLATES if it should be available to
   the interactive Strategy Lab.
2b. The interactive loop: POST /run kind="unitlib" builds the unit
   tensor + caches base KPIs; POST /strategy/eval is SYNCHRONOUS and
   must stay sub-ms -- never put engine calls in that path; anything
   slow belongs in the unit-library build.
3. Keep the repository shape (functions over module dicts). These dicts
   materialize SQLite/PostgreSQL revisions through `app.persistence`;
   do not introduce a second mutable source of truth. Large tables live
   in immutable Parquet artifacts, locally or on S3. Read `apps/api/AGENTS.md`
   before changing persistence, job publication, or worker ownership.

## Raw mixed-book lifecycles (0.27.8)

Rust owns deposit risk/stress, hedge risk, cross-book effective-yield accounting,
conditional forecast paths, parallel risk/KPIs, raw unit-library pricing/coefficient
preparation, forward programs, saved-book/candidate mapping and daily closing,
consolidated and attribution reports derived from replayed journal postings.
Product ABI 8 and ledger protocol 2 require rebuilding and restarting workers.
`portfolio-lifecycle` exposes bounded raw JSON requests without Python. HiGHS
remains C++; Rust owns its financial problem construction and allocation validation.

Adapters transport raw inputs and format tables. API interactive evaluation keeps
the library's selected backend and uses prepared coefficients; it never reprices.
Caller-supplied unprepared libraries also support native coefficient construction.

Both deposit fitters use analytic derivatives; zero-shock deposit stress is an exact
identity; conditional-forecast initial means accumulate in f64. Accounting yield
powers use a native recurrence. Final-output tolerances are unchanged. Existing
research regulatory weights, forward-unit approximations and monthly mapping remain.
Saved hedge-to-netting-set mapping is still explicitly unsupported.

At the 0.27.8 checkpoint, decision repricing coordination and raw daily-spec
normalization were still Python-owned; later sections supersede that status. The 0.27.9 graph update below supersedes
earlier graph-ownership notes.
The native entrypoints remain separate. Unified deadline/cancellation and complete
graph/solver/ledger 60,000-position acceptance are still required. Independent
persisted-journal verification remains a publication gate. The migration ledger is
`docs/reviews/2026-09-30-rust-lifecycle-migration.md` at the monorepo root.

## Native incremental dependency graph (0.27.9)

With `compute_backend="rust"`, the public cross-book pricing graph is native: Rust
resolves bounded cashflow batches, target-specific OAS, temporary scenario marks,
parallel/key-rate risk, frozen-yield income, runoff and KPI aggregation. Python
transports raw tables and holds an opaque cache handle. Temporary assumption
validation/defaults also have a native entrypoint. An explicitly supplied custom
discount backend remains a reference extension path.

The instrument cache retains complete input/parent identity, collision-safe
equality, shared identity interning and O(log N) recency eviction. It charges
retained nodes/identities against its byte and entry budgets; active batches,
JSON transport and the separate bounded native market cache are additional.
Requests retain live base/current cashflow chunks through risk and income so
eviction cannot change economics. Price and notional are validated even on hits.

Raw daily specification defaults, input domains, references, work admission and
opening reconciliation also execute in Rust, with independent Python test oracles.
The streamed Rust runner uses native validation before simulation.

This is not full migration completion. At this historical checkpoint the decision coordinator, unified runtime and
combined scale acceptance were open; the 0.28.0 section below supersedes that status. HiGHS
remains C++. See `docs/reviews/2026-10-01-rust-owned-lifecycles-review.md`.

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
It admits at most 60,000 backbook instruments plus generated candidates within
62,000 total ledger positions and 120 million daily work units. This is a separate
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

## Native public-controller and allocation follow-through (0.28.0)

The `financial-controller-1` protocol also owns saved-book base OAS calibration,
spread application, published forecast date/unit/interpolation preparation,
base-anchor/conditional replay orchestration, and empirical joint-driver moments,
holdout diagnostics and observed joint-scenario selection. Forecast paths remain
income-only. Scenario fitting/selection uses training observations only; holdout
rows affect diagnostics. Equal-distance scenario ties use stable observation
order in both backends; forecast driver columns use stable sorted names.

The full native workflow captures at most 256 contracts per batch, retains
bounded numeric schedules, reconciles daily GL against independently constructed
subledgers, and emits closing reports. Immutable contract labels and integer GL
keys reduce daily allocation. Flat OAS/PV result buffers remove per-instrument
nested allocations without changing reduction order. No fast-math or tolerance
relaxation is permitted. Strategy actor initialization does not hold the global
registry across pricing. Failure/cancellation must publish no manifest.

`test_native_controllers.py` gates the public controllers against independent
references with Python financial callbacks forbidden. `test_owned_workflow.py`
gates complete saved-book/candidate and coupon-edit daily output parity with
original accounting anchors, original posting order and failed-publication cleanup.
See docs/reviews/2026-10-01-native-owned-workflow.md for scale and validation scope.


## Cohort accuracy and reliability (0.29.1)

The cohort audit reprices every selected mortgage/deposit record through Rust in
batches of at most 256. It compares representative and summed individual PV,
DV01 and NII under base and +/-200 bp scenarios with fixed base OAS and shared
random numbers, plus monthly base cashflows. Product/metric absolute-dollar and
relative tolerances are explicit. Suggested splits are diagnostics requiring a
new build and audit. The independent comparison driver is Python; built-in
product calculations remain Rust. Loan results and errors stream to immutable
Parquet partitions. This does not add consumer-credit models or empirical model
calibration. Intake remains bounded to 100,000 records and 32 MiB.

Tape refresh comparison reconciles additions, removals, term changes and balance
movements without inferring payments or posting a journal. Selective publication
replaces one tape's positions while retaining other sources; identifiers include
a tape namespace and canonical cohort IDs remain attached. Historical-yield books
require explicit compatible book yields. Cohort analytical tables can be delivered
to Iceberg with the existing retry/idempotency contract.

Cache admission can refuse a result after shared identities are evicted, without
panicking or poisoning the handle. Workflow and streamed-ledger deadlines include
persisted replay and the final publication check. Independent strategy validation
supports an omitted asset cap. Regression tests cover these failure paths.
See docs/reviews/2026-10-01-cohort-followthrough.md for validation and open scope.


## Capital coverage and funds transfer pricing (0.29.2)

`analytics.treasury.evaluate` transports explicit eligible capital snapshots and
FTP exposures to Rust (`treasury-1`). Thirteen ratios cover standardized and
advanced CET1/Tier 1/total capital, Tier 1 leverage, SLR, TLAC and LTD on risk and
leverage bases, and tangible common equity/assets. Minimum, buffer and management
headroom remain distinct. Missing requirements/denominators are unconfigured or
unavailable, never passes. Eligibility, RWA, buffer calibration, GSIB scores and
stress capital movements are supplied inputs, not inferred from the research book.

FTP separates repricing/reference and behavioral funding/liquidity tenors; option
and contingent charges are explicit. Signed business transfers offset in treasury
within entity/currency/scenario/period. External NII and regulatory capital are not
changed by internal FTP. Pretax profitability, economic profit and annualized RAROC
retain loan/cohort lineage. Linear rate interpolation and flat tails are disclosed.

Prepared capital deltas can produce native solver limits tied to explicit unit
identities. Rust constructs HiGHS rows and independently replays capital amounts;
Python provides an independent reference. Dirty decision edits with retained
capital limits require refreshed limits (CAPITAL_REFRESH_REQUIRED). This is
coefficient feasibility, not automatic exposure generation or daily ledger
acceptance. The existing NII objective is unchanged. The API `/treasury/runs`
uses durable jobs, immutable policy inputs and Parquet tables; the Capital & FTP
panel supplies a synthetic example. SQLite/PostgreSQL and optional Iceberg use
the existing storage contracts. See `docs/capital-and-ftp.md` at the repository
root for coverage, assumptions and remaining work. Rebuild both native engines
and restart API/workers before use.


## Capital coverage and funds transfer pricing (0.29.2)

Rust `treasury-1` owns thirteen capital ratios with explicit eligible inputs,
minimum/buffer/management headroom, and matched-tenor FTP. Signed business
transfers offset in treasury by entity/currency/scenario/period. Loan/cohort
lineage, pretax profitability, economic profit and annualized RAROC are retained.
Missing requirements or denominators never pass. Eligibility, RWA generation,
GSIB scores and stress capital movements remain supplied inputs.

Prepared capital deltas produce native HiGHS limits tied to the unit grid and
independently replayed amounts. Decision edits with retained capital coefficients
require refreshed limits (CAPITAL_REFRESH_REQUIRED). This is coefficient
feasibility, not daily ledger acceptance. Internal FTP does not alter external
NII, OAS or capital; the strategy objective remains worst-case external NII.
`analytics.treasury.evaluate`, durable `/treasury/runs` jobs and the Capital & FTP
panel preserve immutable policy/input identity and Parquet reports, with optional
Iceberg delivery. The example is separate from saved-book KPIs. See
`docs/capital-and-ftp.md` at the monorepo root for schemas, bounds, coverage,
regulatory sources and remaining integrations. Rebuild native product/decision
engines and restart API/workers before use.


## Source-linked capital and FTP (0.29.3)

Rust `treasury-bridge-1` prepares closing capital from journal-verified trial
balances and monthly FTP from captured principal/effective-interest cashflows.
Ledger policies are explicit per account/scenario, with an explicit monetary
unit multiplier. Common equity/distributions, retained P&L including tax and
provisions, OCI inclusion, net carrying-value credit RWA and leverage adjustments
are traceable. Additional capital, regulatory eligibility, average assets,
advanced/market/operational RWA and requirements remain supplied closing inputs.

FTP uses average monthly principal and remaining principal-weighted funding life;
residual principal requires an explicit tail beyond the captured horizon. Reset
tenors, costs and capital allocation remain assumptions. This is monthly expected
flow preparation, not a new behavioral fit or arbitrary future reinvestment model.
Source jobs, immutable result hashes and source revision are retained. The API
queues bridges on the durable worker; source templates intentionally leave unknown
regulatory inputs null. Independent journal replay gates ledger publication.
Conditional forecast jobs are not capital/FTP bridge sources. The Capital & FTP
panel accepts compatible completed ledger/cohort analytics jobs. See
`docs/capital-and-ftp.md` for current integration scope. Rebuild native product and
decision/workflow binaries and restart workers.

## Shared model foundations (0.29.5)

`portfolio-model-core` supplies reusable, bounded Rust credit transitions,
exact-observation hazard fitting, dated cashflows, calendar/overnight arithmetic
and supplied-fixing floating coupons. The native `portfolio-lifecycle` protocol
and `analytics.model_contracts` expose explicit analysis entrypoints; adapters
perform no financial calculation or fallback. These entrypoints do not yet
replace the daily ledger's monthly capture or integrate joint behavioral/credit
product scenarios.

Fixed corporate contracts may explicitly select `amort_type="level_payment"`.
It preserves equal total payments on the actual accrual schedule and rejects
unsupported floating/sinking/negative-amortization/truncated-grid inputs.
Historical `annuity` remains exactly equal principal. A seasoned loan's original
fixed payment is not inferred from remaining balance/term.

`dated-term-1` emits fixed, nonoptional corporate/CD contractual events with
separate scheduled, accrual and settlement dates; the accrual policy and cash
direction are required. Credit, withdrawal, options, fees and accounting
amortization require their own policies/models. `floating-rate-1` uses complete,
available, supplied historical fixings and explicit reset/coupon conventions.
It neither invents future fixings nor treats the existing quarterly forward as
SOFR. Reference-calendar mapping is supplied, not certified by coupon arithmetic.

`observed-credit-calibration-1` uses known transition times and elapsed state
exposure. Chronological holdout rows cannot alter the fit; unobserved origins
require explicit assumptions or rejection. It does not fit interval-censored
monthly snapshots, LGD/EAD or borrower/macro covariates. Synthetic fixtures do
not prove empirical predictive accuracy. Credit recoveries remain recognized
from period-end defaults; correlated credit paths and journal integration remain
open. No generic expected-loss output implies CECL/IFRS9 compliance.

All issue acceptance gates remain in `docs/models/implementation-plan.md` and
[tracker #25](https://github.com/beauschwab/model-portfolio-sim/issues/25).
Detailed mathematics, schemas, sources and limits are in `docs/models/`;
`docs/models/native-contracts.md` documents transport and validation. Rebuild
the product and decision/workflow artifacts and restart API/workers before use.
Product ABI 8 and existing ledger protocol remain unchanged; additive schemas
require new binaries. Saved economics require explicit model selection/rebuild.
