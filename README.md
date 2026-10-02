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
  web/        Vite + React + TS + Tailwind + recharts (Supabase-dark theme)
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
Rust is optional; NumPy remains the default. The pricing cache is bounded at
512 MiB of accounted retained nodes and 500,000 entries, excluding in-flight
arrays, request results and Python allocator overhead.

Browser tests use isolated ports 8001/5174 by default. Override
`WORKBENCH_TEST_API_PORT` / `WORKBENCH_TEST_WEB_PORT` if needed; only set
`WORKBENCH_REUSE_TEST_SERVERS=1` for known disposable test servers.

## Decision Lab prototype

Open **Decision Lab** for the complete temporary-assumption â†’ dependency update
â†’ strategy coefficients â†’ robust optimization â†’ independent replay workflow.
Build the optional Rust decision library with:

```powershell
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py
```

Then start the API normally and build a decision session in the panel. Constraint
changes skip pricing; instrument edits price only affected positions; template
spread edits rebuild only affected unit columns. Saved input edits require a new
session. Existing Strategy Lab and other pricing routes remain available.

[Full prototype comparison and validation](docs/reviews/2026-09-28-decision-prototype.md)
documents what Rust owns, measured end-to-end latency, native solver comparisons,
and remaining scaling work. This is a hybrid prototype, not a full model rewrite.

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
