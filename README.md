# Rates Workbench

Engine 0.28.0 provides a Rust-owned built-in calculation runtime: raw calibration,
shared seeded paths, product pricing, dependency graph and edits, accounting/KPIs,
forecasts, unit libraries, strategy coordination, and daily ledger/closing reports.
HiGHS remains C++, invoked and validated by Rust. Python remains the API/data
adapter, independent verifier and reference implementation. As of 0.29.4,
`compute_backend="rust"` is the default and the only production calculation backend.
Python calculation is deprecated; historical Python configurations require an
explicit switch and library/session rebuild. See [the execution contract](docs/rust-production-contract.md).
Build the product, decision/workflow and ledger binaries and restart workers.
Product ABI 8 and ledger protocol 2 are unchanged.
See the [migration and optimization report](docs/reviews/2026-10-01-native-owned-workflow.md)
for the complete execution boundary, 60,000-instrument measurements, validation
evidence and inherited model limitations. The
[migration ledger](docs/reviews/2026-09-30-rust-lifecycle-migration.md) retains the
implementation history.

Public forecast research: Market & Scenarios now fetches Fed stress paths, SEP,
Philadelphia Fed consensus and New York Fed expectations. Preview the source
coverage and assumptions, then run conditional monthly income and runoff on the
saved book. See [forecast sources and workflow](docs/forecast-scenarios.md).

Monorepo: a fixed-income/balance-sheet risk engine
(`packages/portfolio-risk`), a FastAPI service (`apps/api`), and a Vite/React
trader dashboard (`apps/web`).

```
apps/
  api/        FastAPI wrapper: books, market, assumptions, scenarios, runs
  web/        Vite + React + TS + Tailwind + recharts (Aperture Risk design system)
packages/
  portfolio-risk/   LMM Monte Carlo engine: MBS OAS, corporates, NMD deposits,
              CDs, money markets, ASC 815 hedges (swaps/swaptions);
              KRD/vega risk, 9Q stress capital, NII accounting with
              amortized-cost basis, EVE/LCR/NSFR/CET1 KPIs, forward
              strategies, and the unit library powering interactive
              strategy analysis. Self-documenting via a layered
              AGENTS.md hierarchy (root / src / tests / skills).
```

Docs: PRODUCT.md (what/who/journeys), ARCHITECTURE.md (layers and
invariants), DESIGN.md (UI system), AGENTS.md hierarchy (modification
contracts at every layer).

## Quick start

Research feeds are available in **Market & Scenarios â†’ Research market data**.
Fetch and inspect immutable source snapshots, then optionally apply an Eris SOFR
curve. Other feeds remain reference datasets until explicitly calibrated.
See [source coverage, configuration and curve limitations](docs/market-data.md).

```bash
bun run setup     # uv Python envs + bun workspaces
bun run db:init   # SQLite + synthetic example inputs; preserves existing edits
bun run dev:worker # :8002 â€” calculations and interactive sessions (own terminal)
bun run dev:api   # :8000 â€” API and durable jobs (own terminal)
bun run dev:web   # :5173 â€” proxies /api to :8000
bun run test:py   # engine + API regression gates
```

Toolchains: Python environments are managed with `uv`; frontend packages
and scripts are managed with `bun`. The Makefile exposes the same targets for
systems that have `make` available.

## What the UI does
- **Dashboard** â€” book balances, stacked KRD-by-pillar across all five
  books, 27-month NII forecast, 9Q stress P&L lines.
- **Balance Sheet** â€” browse/edit each book (MBS pools, commercial loans,
  LT debt, NMD deposits, CDs); PUT replaces wholesale for auditability.
- **Market & Scenarios** â€” par-curve editor with live preview; 9Q scenario
  builder in trader space (10y level, 2s10s twist around the 5y pivot,
  spread, vol). The comparison shows nine independent full-horizon NII
  forecasts on the unchanged book, not a stateful rolling scenario. Spread
  affects named risk/KPI valuations; it does not alter contractual NII.
- **KPIs** â€” EVE & duration gap (IRRBB 15% outlier test), LCR, NSFR,
  CET1 9Q projection; stylized weight tables are the calibration seam.
- **Strategy Lab** â€” build the unit library once (~20s; new origination
  of every product through the live behavioral engines), then drag
  allocation sliders: every edit recalcs ALL top-level KPIs in real time
  (~sub-ms sync endpoint).
- **Assumptions & Settings** â€” paths/seed/horizon/shock grid; deposit
  attrition segment editor (the panel-fit seam); prepay vector is
  displayed read-only because numba freezes constants at first compile
  (restart required â€” engine AGENTS.md invariant 5).

## Honesty notes
The seeded book is **synthetic**, sized to Wells Fargo's published 1Q26
mix (sources cited in `model_balance_sheet`'s docstring); it is not WFC's
positions. The backend now uses SQLite by default and supports PostgreSQL through
the same repository. Revisions, jobs, and session journals are durable; pricing
caches are worker-local. Tabular artifacts are Parquet, stored locally in dev or
in S3. See [storage, workers, recovery, and production setup](docs/production-storage.md).

## Review remediation (engine 0.18.0)

The September review fixes numerical signs, fixed-OAS named scenarios,
optimizer allocation replay, book round trips, immutable queued inputs,
and revisioned strategy caches. All panel runs use shared job telemetry.
Input changes require rebuilding the strategy library; its synchronous
endpoint remains computation-only over precomputed vectors.

- Simulation settings apply to actual paths, thread masks and forecast horizons.
  MBS base calibration has a separate `n_paths_base` setting.
- Optimizer purchases require matching funding at each month by default.
  `cash_budget` means additional committed funding **outside the base book**,
  available throughout the horizon; it is not permission to double-count existing
  reserve cash. Its financing cost must already be included in the user's plan.
  Default zero introduces no unpriced external funding.
- Liquidity and EVE constraints use active opening-month balances; retained
  capital and RWA are evaluated at the configured horizon. Strategy Lab labels
  its forward DV01 as a balance-scaled proxy. The Level 2A cap is applied exactly.
- Durable jobs retain their results until an explicit retention policy removes
  them. At most 32 active/queued jobs are admitted per workspace; saturation
  returns 429. The isolated memory test mode retains its original one-hour,
  50-job, 256 MiB limits.
- Long-dated par swaps retain their stated tenor using a flat final simulated
  forward beyond the 40.25-year grid; this is an explicit model approximation.

Run `bun run test` for engine, API and browser regressions. Browser tests start
local services when absent and use Edge on Windows; elsewhere install Chromium
with `bunx playwright install chromium` from `apps/web` first. An already-running
API must use a disposable synthetic book for the unchanged-book save test.
`uv run --project apps/api python scripts/benchmark_review.py` reproduces the
paired warm benchmark. See [implementation evidence](docs/reviews/2026-09-28-remediation.md).

## Incremental pricing (engine 0.19.0)

`POST /run` with `kind: "pricing"` runs the new dependency-aware spot pricer.
Supported books are `mbs`, `loans`, `debt`, `deposits`, and `cds`; omit `books`
to select those five. Optional `spread_overrides_bp` maps book names to position
IDs and additive basis-point shifts. A named `scenario` uses the existing market
scenario definition and holds base OAS fixed. Fetch results through the normal
job/Arrow endpoints. Results include prices, selected-book totals, input revision
and counts of reused/computed graph nodes. Money markets and hedge overlays are
outside this endpoint's scope; the net value is not a full balance-sheet KPI.

Only changed instruments are sent through their product's cashflow/calibration
batch. Spread and notional edits reuse upstream work. Saved book and base-market
edits establish new calibration inputs; temporary spread shocks do not. The
Strategy Lab's synchronous evaluator still uses its prebuilt library.

The [implementation report](docs/reviews/2026-09-28-incremental-pricing.md) includes
the request contract and measured end-to-end engine timings. Reproduce with
`uv run --project apps/api python scripts/benchmark_incremental.py`.

## Instrument what-if and backend comparison

Open **Instrument What-if** from the workspace rail. Apply temporary assumptions
or instrument spread shifts, compare original/revised values, and optionally
include curve risk and earnings. Full scope adds balance-sheet KPIs; recalibration
is an explicit comparison action. Saved contracts stay unchanged until edited in
the Book Editor. Late results from obsolete inputs are hidden.

The five-step implementation and NumPy/Numba/Rust measurements are documented in
[the full comparison report](docs/reviews/2026-09-28-five-step-comparison.md).
Rust is required for production; Python/NumPy is retained as an explicit independent test reference. The pricing cache is bounded at
512 MiB of accounted retained nodes and 500,000 entries, excluding in-flight
arrays, request results and Python allocator overhead.

Browser tests use isolated ports 8001/5174 by default. Override
`WORKBENCH_TEST_API_PORT` / `WORKBENCH_TEST_WEB_PORT` if needed; only set
`WORKBENCH_REUSE_TEST_SERVERS=1` for known disposable test servers.

## Decision Lab prototype

Open **Decision Lab** for the complete temporary-assumption â†’ dependency update
â†’ strategy coefficients â†’ robust optimization â†’ independent replay workflow.
Build the required Rust decision/workflow runtime with:

```powershell
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py
```

Then start the API normally and build a decision session in the panel. Constraint
changes skip pricing; instrument edits price only affected positions; template
spread edits rebuild only affected unit columns. Saved input edits require a new
session. Existing Strategy Lab and other pricing routes remain available.

[Full prototype comparison and validation](docs/reviews/2026-09-28-decision-prototype.md)
documents what Rust owns, measured end-to-end latency, native solver comparisons,
and remaining scaling work. Those dated prototype measurements describe an earlier checkpoint. Current Rust ownership and remaining application-service scope are in [the production contract](docs/rust-production-contract.md).

- [Balance-sheet Stress](docs/balance-sheet-stress.md): daily liquidity, credit/capital, entity constraints, management policies and reverse stress. Explicit synthetic cohort research model with accounting reconciliation.

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


## Tape cohort construction (0.29.0)

`analytics.cohorts` supplies versioned tape normalization, Rust-owned cohort
construction, product-specific rules, immutable loan lineage, weighted-feature
dispersion, same-tape rule comparisons and additive-dollar allocation. The API's
Tape & Cohorts panel uses durable import/build/analytics jobs and revision-checked
position publication. Mortgage and deposit USD cohorts use the existing product
models; auto, personal and credit-card cohorts explicitly require dedicated
behavioral pricers. Original-loan repricing and balance-weighted allocation are
labeled separately. See `docs/tape-cohort-workflow.md` at the monorepo root for
schemas, admission bounds, source allowlists and limitations. Grouping does not
change fixed-OAS, CRN or global prepayment/restart contracts.


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

## Dated replay and credit accounting foundations (0.29.6)

`dated-ledger-1` explicitly selects native scheduled recognition and actual-date
settlement through `analytics.dated_ledger.run_dated_ledger`. Dated owners bypass
legacy PD/default, generic interest, deposit flight/runoff, proportional scaling
and synthetic maturity. The source supplies complete scenario/reverse-run coverage
and explicit calendar recognition segments. Early/late interest rights and full
contract tails survive principal settlement. Pricing-measure/overlapping sources,
unsupported credit/draw/mark/collateral ownership and incompatible opening anchors
fail before execution. Existing `balance-stress-2` economics and protocol 2 remain;
the dated route uses ledger protocol 3 with separately versioned publication.

Publication retains raw input and source/artifact hashes, event lineage and dated
closing rights. Independent persisted verification checks source amounts/dates,
once-only phases, transaction postings, principal/basis/rights and gross closing
statements. Cancellation/deadline fences remain required through publication.
This is scheduled expected recognition; it does not prove empirical behavior or
accounting-standard compliance. The API/UI/saved-book/native graph and complete
60,000-position joint graph/credit/solver/journal adoption remain open.

Shared `credit_operator` provides reusable transition/earned-interest matrices
for separate probability, principal, undrawn, signed accrual and basis vectors.
No live-mass normalization or second probability weighting is permitted.
`credit_accounting` supplies explicit physical daily policy steps and balanced
journal intents, keeping economic loss separate from reviewed allowance,
chargeoff and complete recovery tails. Those components are not yet automatically
adopted by the dated daily route. Retain the complete issue gates in
`docs/models/implementation-plan.md`; details are in `docs/models/dated-ledger.md`,
`dated-store.md`, `credit-operator.md` and `credit-accounting.md`.
Rebuild ledger and dependent decision/workflow binaries and restart workers before
using new schemas. Component budgets are not aggregate RSS guarantees.

## Architecture maintenance

[Current architecture](docs/architecture.md) identifies financial, application, storage and verification ownership.
[Interface contracts](docs/contracts/README.md) describe generated HTTP/native layouts and retained transport/import compatibility.
Run `make check-architecture` (or `bun run check:architecture`) before changing boundaries.
## Explicit product states and durable source routes (0.29.7)

Additive native analysis contracts cover supplied-path auto/personal installment
and revolving-card state, daily deposit retention/repricing/migration, mortgage
borrower/investor ownership, actual-date funding/collateral/rollover, indexed
corporate cashflows, conditional call/put exercise, CD elections and actual-date
effective-yield income. The inner schemas retain explicit source, assumption,
measure, calendar and policy identities. These contracts do not automatically
replace saved economics or establish empirical calibration.

`credit-ledger-1` couples physical daily loan transitions, independently weighted
money, earned interest and reviewed accounting with explicit repayment/draw/
maturity boundaries and complete recovery claims. Protocol 4 publication is
gated by independent SciPy matrix-exponential, source/policy and persisted posting
replay. This loan-only route admits 62,000 aggregate run-position contracts;
combined 60,000-position mixed-book graph/solver/journal acceptance remains open.
No implicit allowance, chargeoff, regulatory limit or legal methodology is inferred.

Durable `/balance-stress/dated` and `/balance-stress/credit` jobs fence the queued
native executable and validation-library identities through worker publication.
The partitioned panel pages/downloads/reopens source lineage and claims; managed
raw-input references survive restart and garbage collection. Standard/large
admission, cancellation, lease/token checks and immutable publication apply.
Python remains transport/storage and explicit independent verification.

All child issue gates under #25 remain required, including native graph, cohort,
saved-book/candidate, strategy/library, accounting/capital/FTP adoption, empirical
holdouts and complete scale acceptance. Histories and reviewed policies must be
supplied for those evidence gates. See `docs/models/implementation-plan.md`.
Rebuild product, ledger and dependent decision/workflow artifacts and restart
API/workers before executing additive schemas. Product ABI 8 and historical
ledger protocols remain unchanged; new credit execution explicitly selects 4.

### Analytical workbench and dated coefficients (0.29.7 follow-through)

Supplied multicurve/smile/joint-driver, dated derivatives/CSA, event-time credit
paths and interval-censored panel calibration are additive explicit contracts.
Synthetic and supplied assumptions do not prove observed accuracy. `/models/runs`
uses a durable cancellable standalone Rust process with pinned source/executable
identity, immutable raw input and paged Parquet output. Python formats output.

`aged-units-1` prepares actual-purchase fixed corporate/CD coefficients with
retained original yield, complete rights, purchase OAS and actual reporting dates.
`strategy-aged-1` builds/invokes/replays HiGHS from those native coefficients.
`/models/aged-library` publishes them through existing library fencing/recovery;
interactive evaluation must reject missing coefficients/grid without repricing.
The dated lane reports incremental allocations without a monthly backbook KPI
baseline. Funding is currently par-based; allocation rejects nonpar quotes.
Behavioral state conditioning, full mixed candidate journal and scale gates remain.

`credit_ledger` capital bridges independently replay the verified signed credit
journal and retain separate live/default exposure lineage. Reviewed provision,
chargeoff, recovery and allowance amounts are applied once; economic modeled loss
is not automatically a second capital deduction. Regulatory and FTP policy inputs
remain explicit. See `docs/models/product-analysis-workbench.md`,
`docs/models/native-contracts.md` and the complete issue implementation plan.

## Corporate exercise publication (local 0.29.9)

The existing `conditional-exercise-1` analysis requires independent raw-tree and
persisted-result replay of Bellman values, stopped probabilities and complete
pricing cash rights before durable publication. Original OAS matches exactly;
shared-draw/policy identities, exact dates, signed cash and full tails remain.
Native caller cancellation/deadline controls reach the tree through Rayon and
check admission, branches, backward/forward passes and final audit construction.
The standalone process watchdog and immutable source/artifact/lease fences remain.

The receipt is explicitly Pricing measure and `physical_journal_eligible=false`,
with `empirical_validation=false`. It does not infer observed issuer behavior,
calibrated joint market/credit states, physical journals or saved-book/library
adoption. Historical economics and schemas remain readable unchanged. Rebuild
native artifacts and explicitly rebuild selected libraries/sessions; deployed
workers require their existing restart procedure before executing new source.
See `docs/models/term-exercise-publication.md` and all original #15/#25 gates in
`docs/models/implementation-plan.md`. Local validation does not complete those
issues or deploy this change.


## Commercial source, credit and coefficients (local 0.29.10)

The additive `commercial-product-graph-1`, `commercial-units-1`,
`strategy-commercial-1` and `commercial-credit-ledger-1` contracts retain complete
supplied Physical/Pricing sources, original purchase yield/cost, one original OAS
and shared credit draws. Native source factors feed the existing credit loop;
legal and recognized coupon lots remain separate. Reviewed nonaccrual, integral
fee cash, contingent default funding and complete recovery tails are explicit.
Credit-free references reject issuer/borrower hazards and commitments.

The journal uses the existing daily Sim with exclusive commercial owners and
cash accounts. Acquisition is attributed on actual day zero. The original report
cutoff is retained separately from the resolved complete contractual/recovery
calendar. Protocol 6 carries source lineage and daily/closing gross coupon books.
It does not admit arbitrary mixed owners, capital/FTP policies or legal fee claims.
Independent raw-source, CTMC/carry, coupon and persisted ordered-journal/report
replay must pass before publication, with the original deadline, cancellation,
lease/token, source, executable and artifact fences.

Native conditional call/put decisions use only observed prefixes and exact
source/default/recovery cash times at fixed original OAS. The result is exact for
its supplied finite Pricing distribution, not a continuous-model convergence or
empirical claim. Before-cash and rights-only termination reject. Acquired exercise
units require a separately verified original stopped Physical cash/yield anchor;
analytical continuation does not satisfy that adoption gate. Constraint-only
commercial actor updates and evaluation retain native coefficients; creation
controls are not cached as session deadlines.

Performing discretionary draw tranches, seasoned source prefixes, nonintegral
fees, reviewed allowance/chargeoff/legal waivers, full saved/cohort/candidate and
capital/FTP adoption, observed holdouts and combined scale remain required.
See `docs/models/commercial-credit-ledger.md`, `commercial-exercise.md` and the
complete original #15/#25 gates in `docs/models/implementation-plan.md`.
Rebuild compatible product, ledger and decision/workflow artifacts and explicitly
rebuild selected saved libraries/sessions. Local validation does not deploy or
restart workers or close the full issue.

## Complete-origin commercial holding windows (local 0.29.13)

`commercial-holding-continuation-1` replays complete original acquisition histories
and derives a sealed opening plus subsequent native posting indices. Original cost,
EIR, gross coupon lots, draw anchors, undrawn commitment and reviewed default state
remain retained. Completed raw/economic prefixes and CRN identities cannot change.
Future interval/projected assumptions require explicit retained source conventions.
No caller monetary checkpoint, credit restart or second acquisition is accepted.
Independent raw and persisted replay, original deadlines and final source/executable/
object/SQL publication fences remain required. Rebuild compatible native artifacts
and explicitly rebuild selected saved sessions/libraries before additive execution.

This is a holding-window foundation; verified prior-publication opening, cut-started
mixed daily adoption, seasoned graph/cohort/strategy coefficients, stopped acquisition
and secondary clean/dirty rights allocation remain open. Fees, daily in-arrears
coupons, generated streaming, empirical/policy evidence and complete mixed60000
acceptance remain required. See docs/models/commercial-continuation.md and the full
implementation ledger. Local checks do not imply deployment or full issue closure.

## Deposit terminal treatment (0.29.7)

The non-maturity deposit grid ends at month T (360). The balance left after the
final month's attrition is now an explicit, versioned assumption
(`deposit-terminal-1`), selected by `RunConfig(deposit_terminal=...)` and carried
as `assumptions.terminal` in every Rust deposit request:

- `payout` (default; omitted from requests): the residual is repaid at par in the
  final month, exactly the historical behaviour. Saved results are unchanged.
- `truncate`: no terminal principal; the residual is excluded from the liability
  (its value stays with the bank as franchise).
- `runoff_tail`: the residual R runs off beyond T at the final month's attrition
  a, paying the final paid rate r plus servicing, discounted at the final short
  rate s held flat. Closed form at the final deflator:
  sum_k R(1-a)^k (r/12+svc/12+a)/(1+s/12)^(k+1) = R(r/12+svc/12+a)/(a+s/12),
  with a+s/12 floored at 1e-4/month. No extra simulated months are added.

The tail is lumped into the final A column, so OAS discounts it to the horizon
date only (the A-matrix factorization is preserved). Runoff, interest and NII
capture remain in-horizon in every mode. Risk, stress, accounting/NII, KPIs,
balance risk, the incremental graph (identity keys include a non-payout mode),
decision repricing and unit-library deposit templates all use the same kernel
and mode. Deposit risk adds a `residual_balance` column ($ before the terminal
treatment). `deposit_horizon_sensitivity` (Rust `deposit-horizon-1`) reports PV,
residual balance, terminal-treatment PV (PV(mode)-PV(truncate)) and implied OAS
for every mode at the base and extended horizons (up to 482 months) at one fixed
OAS, solved at the base horizon; each horizon is simulated as a production run of
that length, so the base row reconciles with deposit risk. The deposit
kernels (ops 6/7) take the mode code as their last input and op 6 returns a
seventh residual output, so native product ABI 10 requires rebuilding
scripts/build_native.py and the decision/workflow artifacts and restarting
API/workers. Tests: `tests/test_deposit_terminal.py` and Rust deposit_lifecycle
unit tests.
