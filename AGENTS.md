# AGENTS.md â€” rates-workbench (monorepo root)

## Production execution contract (0.29.4; supersedes historical defaults)

The authoritative policy is `docs/rust-production-contract.md`.
Follow its deployment, compatibility and validation rules. Rust owns built-in
pricing, graph/coordinator, simulation, capital/FTP and solver financial logic.
HiGHS remains C++; Rust constructs/invokes/validates its financial problem.
Python API/storage adapters and independent verification remain application services.

Rust is the new/omitted backend default. Historical Python inputs/results remain
readable unchanged but need an explicit Rust selection and library/session rebuild
before execution. Missing/mismatched native artifacts fail without fallback.
Production pricing/what-if enters the native graph without a custom discount override.
Never add Python/NumPy/Numba financial options to production requests or UI.

Preserve fixed base OAS, shared CRN, unchanged final-output tolerances, immutable
verified publication and coefficient-only interactive evaluation. Python test
oracles are explicit and independent; fresh-process default/ownership tests run
outside their reference context and forbid Python financial callbacks.

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
  theme tokens in `src/styles/aperture.css`, mapped in `tailwind.config.js`
  (Aperture Risk design system; see DESIGN.md before changing UI styling).

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

## Architecture and interface maintenance

Current architecture: `docs/architecture.md`. Interface/transport and dependency rules are in
`docs/contracts/README.md`. Run `make check-architecture` from the root
for boundary/contract changes. Dated version sections document historical migration
stages; current production and shared-model contracts supersede their defaults.
Preserve independent reference tests and all applicable numerical/publication gates.
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
baseline. Funding uses quoted purchase costs and complete principal/coupon cash tails;
face-notional and purchase-cost caps remain separate.
Behavioral state conditioning, full mixed candidate journal and scale gates remain.

`credit_ledger` capital bridges independently replay the verified signed credit
journal and retain separate live/default exposure lineage. Reviewed provision,
chargeoff, recovery and allowance amounts are applied once; economic modeled loss
is not automatically a second capital deduction. Regulatory and FTP policy inputs
remain explicit. See `docs/models/product-analysis-workbench.md`,
`docs/models/native-contracts.md` and the complete issue implementation plan.
### Native product graphs and modeled replay (0.29.7 follow-through)

Explicit consumer, card, mortgage, deposit, term/CD and whole-book funding graphs retain separate supplied
physical/pricing histories, paired pricing bumps, full cash/rights/recovery tails
and immutable original base OAS. Their metrics disclose contractual recognition
and cash; effective-yield income, empirical fitting and saved-book adoption need
their own policies and gates. Graph admission exposes validated aggregate work
before cohort generation; component limits never imply an aggregate RSS guarantee.

`credit-path-ledger-1` selects stream protocol 5 and independently recomputes
raw event-time paths, reviewed accounting, ordered postings and closing balances.
Expected protocol 4 and historical monthly/dated protocols remain separately
readable. Reviewed realized credit journals can feed native closing capital;
realized paths are not expected-flow FTP inputs. Signed accrued liabilities retain
gross presentation and economic loss is never deducted twice.

`graph-dated-ledger-1` and `driven-credit-ledger-1` run source preparation and
daily replay in one native `portfolio-model-ledger` process. Durable modeled jobs
retain raw requests and prepared sources through independent persisted verification,
source/artifact hashes, deadlines, cancellation and lease/token publication.
Consumer capture admits reviewed performing par-note simple interest; unsupported
credit/fees reject. Frozen credit covariates retain chronological training/holdout
separation and explicit analytical/adoption eligibility. Supplied projected drivers,
LGD, CCF and recovery policies are not inferred empirical evidence.

Model analyses use lossless canonical JSON cells for nested variants. Reviewed
derivative and realized-credit accounting sources are independently checked before
and after persistence. These analytical receipts do not imply mixed-book journal
adoption. Every analysis and dated-library publication checks final executable,
source and deadline fences. Interactive allocation remains coefficient-only.

`product-units-1` retains supplied actual-purchase performing consumer/card/
mortgage and deposit-runoff histories, reviewed original effective yield, verified
locked purchase OAS/quote and replayed physical age prefixes. Full-tail funding is
scenario-specific; income is effective interest rather than gross coupon cash.
Unsupported draws, credit/nonaccrual, migration, fees and modifications reject.
`strategy-product-1` constructs native HiGHS problems from these coefficients,
replays source cash/amounts and retains face, purchase-cost, category and explicit
capital dependencies. Constraint-only actor updates and evaluation use retained
coefficients. Dirty unit edits selectively retain exactly equal native sources;
shared identity/grid/count changes require complete preparation. Refreshed capital
dependencies remain required. Native and independent raw-source replay gates both
product reports and persisted libraries, including their exact adapter cells.
This stage does not establish mixed candidate journal acceptance.
The explicit `product-candidate-journal-1` route posts standalone performing
candidate acquisition and original-yield recognition through complete physical
cash and rights tails. Independent raw-source, ordered journal and persisted
replay gate its durable analysis. Supplied opening provenance is not verified
opening evidence. Existing mixed owners, credit/default, fees, taxes and
collateral overlays reject. Native flags retain the separate mixed acceptance
boundary. Unit face must reconcile to source opening principal before reuse or
pricing; final source/artifact/ownership/deadline fences follow progress reporting.

`deposit-graph-dated-ledger-1` admits reviewed par deposit runoff and daily simple
interest expense with complete rights, raw source and independent persisted replay.
Migrations, new inflows, negative rates and non-par yield policy are unsupported
in that capture tier. It does not reinterpret the broader analytical deposit graph.
The separate `deposit-graph-income-ledger-1` route retains explicit purchase quote,
original effective yield and carrying-value anchors, complete unchanged expected
physical cash and daily liability basis amortization. Changed expectations,
seasoned purchases, migrations/inflows and negative-rate accounting reject.
Independent raw-source and persisted journal replay remain publication gates.
Final publication retains the original scalar deadline through persistence and
checks source/artifacts even after rename; failed checks remove only fresh owned
unpublished output.

Full issue 25 acceptance remains required: observed behavioral/credit validation,
product accounting/FTP, saved-book/candidate/cohort/strategy adoption and complete
60,000-position graph/credit/solver/journal acceptance. Rebuild product, ledger and
dependent decision/workflow artifacts before additive-schema execution; restart
deployed API/workers and explicitly rebuild saved libraries/sessions.

## Performing mixed dated acquisition stage (local 0.29.7)

The explicit `product-mixed-dated-1` route combines a reviewed performing dated
backbook and performing product candidates in one chronological native journal.
Actual opening cash/equity are posted once; acquisitions precede recognition and
settlement on their actual dates. Original candidate EIR/OAS/quote and separately
supplied physical expectations remain retained. Backbook recognition and opening
provenance are supplied reviewed inputs. Complete contractual tails are replayed
separately from the requested reporting date; no terminal payoff is invented.
Independent raw-source and ordered persisted-journal verification gate publication.
The verified complete-tail closing can supply native closing capital with an exact
closing-date identity. Report-date capital and modeled product FTP remain separate.

Candidate gross interest rights retain separate physical-path ownership. Uniform
negative coupons classify receivables/deferred income independently of principal
direction and original EIR. Sign-changing contractual coupons require separate
gross obligation ownership and reject at this journal boundary. Zero participation
retains the complete unweighted source policy. The legacy dated route is unchanged.

This performing slice rejects credit/default, fees, taxes, marks, collateral,
netting, legacy mixed owners and reverse-severity journals. Component bounds are
512 positions, eight million aggregate work units and 128 MiB retained capacity;
they are not aggregate RSS or complete 60,000-position acceptance. Joint credit,
behavioral generation, empirical holdouts and all issue 25 acceptance gates remain
required. See `docs/models/product-mixed-dated.md` and the implementation ledger.
Rebuild native artifacts and explicitly rebuild selected saved libraries/sessions;
local validation does not deploy changes or restart production workers.

## Acquired consumer-credit event ownership (local 0.29.7)

`consumer-credit-accounting-1` derives one supplied physical auto/personal path,
original effective-yield prefix and reviewed default/allowance/chargeoff intents
in Rust. Gross principal recovery and collection cost have separate cash and
expense postings. Acquisition cost, original yield, source and policy identities
are explicit. The component neither infers allowance from economic loss nor
claims closure of retained unpaid rights.

`product-mixed-consumer-credit-1` executes those exclusive product-event owners
with performing dated backbook/candidates in the same daily Sim, cash/equity state
and protocol-3 GL. Purchase cash and explicitly reviewed initial allowance are
posted once. Allocation and physical path weights are applied once; complete
unweighted source/claim tails are required even for zero participation. Default
principal, basis and gross signed accrued rights have distinct GL ownership.
Independent raw-source and ordered persisted-journal verification are publication
gates; the producer cannot self-certify persisted verification.

The original installment coupon lane is nonnegative and acquired/non-PCD, with
supplied allowance and recovery policies. Fees, nonaccrual, modifications, PCD,
draws and empirical/joint stochastic adoption require their own reviewed models.
Consumer interim regulatory/liquidity weights must explicitly be zero; related
account constraints reject. Complete-tail closing capital uses verified GL and
separately supplied eligible capital/weight policies, not inferred default RWA.
Report-date capital, modeled product FTP, saved-book integration and the complete
60,000-position joint graph/credit/solver/journal gate remain open.

The mixed slice shares the existing 512-position, eight-million-work and 128 MiB
conservative retained limits, with complete raw/final JSON bounded to 32 MiB.
It reserves consumer generation/replay before performing capture; component limits
are not aggregate process RSS evidence. Card accounting has a separate explicit
source route; its shared mixed-owner release gates remain required. See
`docs/models/consumer-credit-accounting.md`, `product-mixed-consumer-credit.md`
and the complete issue 25 implementation ledger. Rebuild native artifacts and
explicitly rebuild selected saved libraries/sessions before execution. Local
validation does not deploy changes or restart production workers.

## Explicit acquired card and tagged credit owners (local 0.29.7)

`card-credit-accounting-1` is an explicitly selected acquired non-PCD physical
card source. Native card economics own principal, billed/unbilled interest,
statements, default and gross recovery/cost. Original effective yield and reviewed
allowance/default/basis/chargeoff are retained separately. Principal zero does not
close unpaid interest or commitment rights; reviewed complete closure is required.
The admitted source is fee-free/non-grace/no-draw, and contingent undrawn balances
are memo rights rather than inferred CCF. Shared principal-plus-basis guards and
all final tolerances remain unchanged. The separate richer interest-basis proof
is not selected implicitly by this public schema.

`product-mixed-product-credit-1` uses tagged raw installment/card requests and
the existing daily Sim, pooled cash/equity, account mapper and one ordered GL.
The old consumer schemas, bare native sources, table names and output remain
unchanged. Purchases and explicitly reviewed initial allowance post once;
allocation/physical weights apply once, and zero participation retains complete
unweighted source/claim checks. Statement dates are indexed and not repeated after
source end. Independent raw source/ordered journal replay and a fresh persisted
reread remain publication gates, with original deadlines and final source/artifact
and durable ownership fences. Rebuild matching native artifacts before execution.

The tagged route retains conservative joined 512-position/eight-million-work/
128-MiB capacity admission and 32-MiB raw/final JSON bounds. Those component
controls do not establish aggregate RSS or 60,000-position joint acceptance.
Closing capital uses the verified complete-tail date and separately supplied
eligible inputs; interim default regulatory weights and modeled product FTP are
not inferred. Wider fee/grace/draw/nonaccrual/PCD, empirical/joint stochastic,
saved-book/cohort/solver and complete issue 25 gates remain required. See
`docs/models/card-credit-accounting.md`, `product-event-owner.md` and
`product-mixed-product-credit.md`; the validation report distinguishes debug,
release, durable and browser evidence. Local changes do not deploy or restart
production workers, and saved libraries require explicit compatible rebuilds.

## Explicit card interest-basis ownership (local 0.29.7)

The additive `card-credit-accounting-2` source retains whole-carrying original
effective yield, gross billed/unbilled interest, signed live/default interest
basis, reviewed actual interest collection and complete claim/commitment tails.
Acquisition and full principal settlement keep their original anchors. Original
EIR and reviewed basis realization are distinct P&L, and default claims receive
no continuing EIR. Atomic event groups are the complete posting stream; shared
intents and income receipts are evidence and must never post a second time.

`product-mixed-product-credit-1` accepts the explicit raw `card_v2` tag in the
same dated Sim, cash/equity and ordered GL. Complete unweighted gross rights,
signed basis and commitments must close before allocation or physical path
weight applies, including zero participation; zero net carrying alone is
insufficient. Supplemental daily-right and atomic/acquisition lineage tables
retain native source identities. The durable independent verifier is selected
from the raw tag, and persisted summaries must match the verified source.
Publication retains original deadline, cancellation, ownership and artifact
fences. Older Card1 and consumer sources retain their original economics/bytes.

The bounded Card2 lane admits 64 actual days, 64 steps/cycles, 16 interest
collections, 256 conservative events, 1 MiB raw, 16 MiB component capacity and 2 million
work. Joined mixed 128 MiB/eight-million-work/512-position admission remains.
These are component ceilings, not aggregate RSS or complete 60,000-position
joint graph/credit/solver/journal acceptance. Fees, grace, new draws, default
before principal zero, nonaccrual, modifications, PCD and wider policies reject
until their own ownership is implemented. No empirical or accounting approval
is inferred. Card2 mixed `capital_ftp_eligible` remains false. Separate
`closing_capital_eligible:true` permission requires matching native and independent
persisted receipts, complete unweighted gross/basis/commitment tails, ordered
journal replay and the exact full-tail date. Closing capital still requires
explicit supplied policies/weights; interim capital and modeled FTP remain open.

See `docs/models/card-credit-accounting-v2.md`, `product-event-owner.md`,
`product-rights-reference.md` and `product-mixed-product-credit.md`. Validation
reports distinguish component, DEBUG, official release, durable and browser
evidence. Rebuild libraries/sessions explicitly; local work does not deploy or
restart production workers, or complete wider issue 25 acceptance gates.

## Explicit acquired-mortgage analytical source (local 0.29.7)

`mortgage-credit-accounting-1` explicitly selects paired Physical original/current
whole-loan contracts, reviewed original purchase cost/effective yield and complete
original/current claim tails. The existing Rust mortgage loop provides borrowed
interval/boundary/cash receipts; its ordinary None path preserves historical
output bytes. The coupled owner transports monetary P/A/G and original-EIR reward
without multiplying probability into money again. Changed credit, prepayment,
recovery or supplied ARM observations cannot refit original cost/yield/carry.
Actual cash drives economic claim retirement; book reversal/chargeoff does not
waive an economic right, and missing gross tails reject without a closing plug.

The compiled generator receipt binds nine length-prefixed source/dependency files.
Its preinstrumentation hash is provenance only. Immutable raw and executed input,
current artifact/source, original/current receipt and persisted-source checks
gate analytical publication, with original deadlines/cancellation/worker fences.
The independent Python reference is publication/validation support only; adapters
transport raw inputs and all financial production calculation stays in Rust.

This bounded analytical lane admits 16 intervals per original/current source,
1 MiB raw, 64 MiB retained output and nine million joined work units including
compiled-source hashing. It requires zero acquired accrued rights/remittance lag,
explicit non-PCD policies, physical whole-loan ownership and complete live/claim
closure. Agency/float/allowance/nonaccrual EIR/additional recovery-cost policies
require separate ownership. Existing default-accrued whole-loan valuation guards
remain unchanged. Source defaults/payoffs use the native interval-end convention;
this source does not claim exact daily journal recognition or new P/Q calibration.

Analytical source acceptance is separate from daily mixed journals, cohort/library/
saved-book adoption, capital, FTP, empirical holdouts and full issue 25 acceptance.
The separate exact-day pending-exit component remains an artifact under reviewed
atomic/domain gates and is not an installed journal source. Read
`docs/models/mortgage-credit-accounting.md`, `docs/models/native-contracts.md` and
the implementation/validation ledgers. Select new schemas and rebuild matching
native artifacts/libraries/sessions explicitly; local builds do not deploy or
restart production workers.

## Strict additive financial instructions (local 0.29.7)

New Card1/Card2 termination, CSA/MPOR disposition, credit maturity and product
strategy selector fields reject keys outside their explicit instruction kind.
Source-default/retain/maturity/all-assets/all-liabilities unit variants own only
`kind`; they must not silently discard settlement, repayment or narrowing fields.
Shared enums, valid wire forms, financial formulas and tolerances are unchanged.
Independent strategy checks selector shape before financial preparation or either
feasible/infeasible handling. Independent credit checks every boundary, including
future dates, before financial operators or persisted rows. Value-first outer
decoders can erase duplicate keys; these field seams do not claim complete byte
duplicate rejection there. The new acquired mortgage owner separately preserves
and rejects nested duplicates and enum-object aliases in its original/current
contract and interest-policy inputs. Historical mortgage parsing is unchanged.

Read `docs/models/raw-instruction-validation.md` and the current validation ledger.
Component and synthetic input checks do not close full model/empirical/scale gates.
Rebuild matching native artifacts for new execution; local builds do not deploy
or restart production workers.

## Market indexed sources and acquired mortgage mixed replay (local 0.29.7)

`market-indexed-adapter-1` explicitly selects supplied named-index/collateral curves,
actual reset/accrual/payment dates and retained reviewed original OAS. Rust prepares
indexed cashflows and values swaps through shared native owners. Projected curves
are not observed fixings or physical scenarios. Immutable input/source/OAS clones
use exact typed identity, including signed zero; calculated output tolerances stay
unchanged. This analytical source does not adopt saved books, incremental caches,
joint dynamics, smile calibration, physical journals or capital/FTP. See
`docs/models/market-indexed-adapter.md`.

`mortgage-mixed-accounting-1` explicitly joins complete registered Physical acquired
mortgage sources and reviewed dated backbook owners in one ordered daily GL.
Borrower/economic claim memos remain separate from monetary P/A/basis state.
Original purchase cost and effective yield stay fixed. Every allocation retains
complete unweighted source tails, including zero allocations. The bounded lane
admits 64 days, 1 MiB raw input, 32 MiB stream and eight million joined work units;
exact binary64 gross sums reject state above 10,000 without netting or epsilon.
These limits are not aggregate RSS or full 60,000-position acceptance guarantees.

Independent persisted verification must recompute source, ordered mixed journal,
daily GL and pending memo phases, retain the actual producer completion, and fence
raw/source/artifact identity, original deadlines, cancellation and worker leases.
The new route reports `single_daily_gl_verified` and
`ordered_mixed_journal_recomputed`; legacy receipt markers cannot substitute.
Accounting-standard approval, empirical validation, saved-book/library adoption
and capital/FTP eligibility remain false. See `docs/models/mortgage-mixed-accounting.md`
and the full issue gates in `docs/models/implementation-plan.md`.

Select additive schemas explicitly and rebuild matching native artifacts/libraries/
sessions before execution. Local source/build validation does not deploy or restart
production workers. Historical routes retain their separate ownership contracts.

## Indexed graph control, deposit revisions and closing capital (local 0.29.8)

These additive source contracts require matching official native artifacts and
fresh acceptance. Earlier full suites validate their recorded earlier bundle.
Historical numeric ABI 8 IDs/layouts and financial contracts remain unchanged.
Select new models explicitly; no historical saved economics are silently replaced.

Indexed graph/book owners retain immutable supplied contract/index/calendar/curve
parents and the original reviewed OAS. The raw-book opt-in has a 360-month component
grid cap with unchanged joined work/byte caps; larger path/grid combinations can
still refuse. Unsupported income, saved-book, capital and other product branches
fail explicitly. `graph-call-control-1` is an additive per-call execution capability,
not a financial model input: bounded one-shot native cancellation tokens, fresh
deadlines and captured-library cleanup reach the indexed native dispatcher, Rayon,
owner-lock wait and serialization. No graph retains a creation deadline or callback.
Absent runtime options preserve the old ABI/payload; missing requested capability
fails with rebuild required and no uncontrolled fallback. IO callbacks must be
bounded. A multi-call book operation still requires a caller-owned outer deadline;
worker lease/cancellation/publication fences and the process watchdog remain
separate requirements. Component token/cache/schedule budgets are not aggregate
RSS, hard real-time or 60000-position joint acceptance.

`deposit-revised-income-ledger-1` retains original source, purchase cost, effective
yield and contractual tails. Explicit reviewed revised Physical expectations post
one day-zero catch-up and keep individual gross FIFO earning/cash origins through
retirement. Original raw input and declared native normalization are separate;
actual completion, private independent source/ordered GL/persisted replay and
original deadline/cancel/code/lease/token fences remain mandatory. Old None output
and original-income routes remain separately readable and validated.

`mortgage-closing-capital-1` separately authorizes reviewed complete-tail closing
capital from a completed bounded research source. The new durable job owns a new
original deadline, immutable source/job/code binding and independently encoded
typed binary64 source digest. Rust rebuilds raw source and ordered GL; independent
source/capital replay and checksummed source/output rereads precede final scoped
SQL publication. Signed accounting balances are unchanged; supplied negative
exposure policy, regulatory inputs and monetary multiplier are explicit. Source
journal permission is unchanged. FTP, interim capital, accounting-standard,
empirical and regulatory approval remain false.

See `docs/models/native-contracts.md`, `market-indexed-books.md`,
`deposit-revised-income.md`, `mortgage-closing-capital.md` and complete remaining
issue gates in `docs/models/implementation-plan.md`. Local source/build validation
does not deploy or restart production workers, close empirical gates or complete
tracker 25.



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

## Commercial follow-through (local 0.29.11)

This stage supersedes the unsupported-status notes in the local 0.29.10
commercial section. Native canonical cumulative coupon entitlement reconciles
legal balances without terminal plugs. Explicit performing draw tranches retain
actual funding, separate original cost/yield and complete source rights. Reviewed
provision, chargeoff, legal waiver and recovery actions retain separate modeled,
book and legal money. Before/after cash and reviewed rights-only exercise use
native phase ownership. Acquired units retain a separate original stopped
Physical cash proof; current scenarios cannot refit original effective yield.

The generated exercise lane fits native observable Gaussian market/credit
continuation on Pricing training only and evaluates fresh paths. An explicit
compact generated graph tier retains separately supplied Physical laws, paired
Pricing streams, locked original OAS and sealed original Physical acquisition
proofs. Native retained coefficients remain the sole interactive evaluation
inputs. Ordinary source/graph/journal budgets are unchanged. Component retained
capacity is not aggregate RSS or complete mixed-book scale acceptance.

`commercial-candidate-journal-1` reuses genuine finite native Physical owners in
the existing daily Sim at actual purchase dates, with exclusive commercial-loan
or issuer amortized-cost security classes. AFS/HTM marks, institution funding
liabilities, generic mixed backbooks and generated streaming candidates reject.
Supplied opening identity is provenance, not independently verified opening
accounting evidence. `commercial-treasury-1` independently replays reviewed
complete-tail journals for explicit closing capital and bounded expected FTP;
regulatory eligibility, requirements, units and policies remain supplied.

Durable analyses, acquired libraries and journals retain original deadlines,
raw input and executable identities, independent raw-source/persisted replay,
artifact rereads and final SQL cancellation/lease/token/revision/attempt fences.
Historical schemas/results remain readable; additive execution requires rebuilt
native artifacts and explicitly rebuilt selected saved libraries/sessions.

Full #15/#25 completion remains open: seasoned acquisition prefixes, nonintegral
legal fee ownership, general generated OAS calibration, path-specific generated
draw/review accounting and streaming mixed candidates, complete saved-book and
mixed adoption, observed histories/reviewed policies and full 60,000-position
joint graph/credit/solver/journal acceptance. Supplied synthetic laws and passing
local checks do not establish empirical accuracy, policy approval or deployment.
See `docs/models/commercial-generated-exercise.md`,
`docs/models/commercial-treasury-candidate.md` and the complete implementation
ledger. Production API/workers have not been restarted by this local work.

## Retained-Q commercial original OAS (local 0.29.12)

Explicit original generated quote calibration retains one native Pricing training
and distinct evaluation sample. Every trial refits continuation on training only;
final actual stopped native cash must meet the unchanged original quote tolerance.
Unbracketed, ambiguous, sampled nonmonotone and unresolved gap quotes reject.
Accepted policies and cash enter graph/acquired-unit preparation without original
sample regeneration. Current scenarios/bumps retain exactly the selected original
OAS; Physical acquisition cost/EIR remain separate supplied and verified anchors.
Interactive evaluation stays coefficient-only. Existing generated4B/128MiB and
ordinary component limits are unchanged; joined work can refuse expensive controls.

Calibrated process/C ABI requests select original-entry native execution controls
through decode, trial fitting, replay and bounded serialization. Durable jobs retain
original queue deadlines and source/executable/object/SQL authority fences. Historical
omitted/locked selections retain their routes. Additive fields require compatible
product, ledger and decision/workflow artifacts and selected-library/session rebuilds.
Local implementation does not restart deployed workers or close issues15/25.

Seasoned holdings, separate nonintegral fees, daily in-arrears indexed earning,
generated path-specific draws/reviews and mixed candidate streaming, verified opening
and complete saved/product adoption, empirical histories/reviewed policies, general
basis/path convergence and full60000-position mixed acceptance remain required.
See docs/models/commercial-generated-oas.md and the complete implementation ledger.

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


## Complete-origin commercial holding journals (local 0.29.14)

`commercial-holding-journal-1` transfers sealed complete-origin continuation
owners into one existing native daily Sim, retaining original credit clocks,
yields and complete cash/rights/recovery tails. The prospective journal derives
a balanced cut opening from that full journal, rolls prior signed P&L into
opening equity and retains ordered future postings with original transaction
lineage. Closing non-P&L stock bits must remain identical. Python independently
recomputes full raw sources, the complete daily journal and persisted window.
Original execution/publication deadlines and source/executable/object/SQL fences
remain mandatory. Inner admission caps and final tolerances remain unchanged.

This is complete-origin daily replay with a derived journal window. Verified
prior-publication opening, a cut-started mixed Sim, seasoned graph/cohort/strategy
coefficients, secondary conveyed rights and empirical evidence remain separate.
Nonintegral fees, daily in-arrears indexed earning, generated draws/reviews and
streaming, complete saved/product adoption and mixed60000 acceptance remain
required under15/25. See docs/models/commercial-holding-journal.md. Rebuild all
matching native artifacts and explicitly rebuild selected libraries/sessions;
local validation does not deploy or close the full issues.


## Verified modeled commercial prior openings (local0.29.15)

`POST /models/holding-journals` resolves a completed native holding source in
the authorized repository scope, independently replays its raw source/full
persisted journal, and binds its exact published cut opening and every completed
posting to the new complete-origin native replay. No caller balances, source
object descriptors or verification flags grant authority. Separate execution
evidence records verified modeled publication; native producer flags remain
unchanged and observed bank reconciliation is not inferred.

Both holding schemas now use the shared commercial final SQL guard, including
original queue deadlines, source/output bytes, executable/bundle, workspace
revision, request/attempt/token/lease and cancellation. Prior source authority and
all consumed objects are rechecked under that lock. Historical source deadlines
remain provenance; new work obeys the new job's original budget. Integer native
clock reconstruction must preserve retained milliseconds without a float round
trip. Shared bounded source transport retains existing FTP capacity limits.

Cut-started mixed replay, seasoned coefficients and secondary conveyed rights,
fees/in-arrears earning/generated streaming, observed validation and full mixed
60000 acceptance remain required. See docs/models/commercial-prior-opening.md.
Matching deployment/restart and explicit selected-library/session rebuilding
remain required; local numerical/source gates do not close full15/25.


## Separate nonintegral fee ownership (local0.29.16)

`nonintegral-fee-accounting-1` replays reviewed physical fee earning, cash,
prepaid refunds, unpaid-right waivers, future-service cancellation and recovery
in Rust. Signed gross receivables/payables and deferred/prepaid balances retain
complete supplied tails separately from loan principal/G/EIR. Exact source,
service/review/clock and independent ordered/persisted replay gate the durable
analytical route and its original deadline/final SQL authority fences.

This additive component does not bind native credit events or enter the mixed
Sim, graph/units, acquisition fee allocation, capital/FTP or saved economics.
Those integrations, earned refunds/secondary rights, observed/reviewed evidence
and complete mixed60000 acceptance remain required under15/25. Historical
cash-income schemas and ABI/protocols remain readable unchanged. See
`docs/models/nonintegral-fees.md`; matching artifacts/restarts and explicit
selected-library/session rebuilds remain required before adoption.


## Native loan and separate fee journal (local0.29.17)

`commercial-fee-journal-1` binds reviewed original-tranche fee earning/cash and
separate exit dispositions to one already generated physical commercial loan
owner. Native default/prepayment and explicit reviewed maturity events determine
waiver/refund/cancellation/recovery; fee LGD is not borrowed from loan policies.
Separate signed gross rights enter the same existing daily Sim once, with complete
fee tails, unchanged loan cost/EIR/carry and retained unweighted zero allocations.
Protocol7 adds fee lineage/daily/closing snapshots; historical6 is unchanged.
Actual fractional clocks cannot round into the preceding day. Independent raw
source, ordered shared journal, gross/report and persisted replay plus original
deadline/executable/object/final SQL authority fences gate durable publication.

This bounded lane does not yet adopt exercise/candidate/holding fee sources,
draw/secondary fee rights, fee acquisition allocation, Pricing graphs/units or
capital/FTP. In-arrears earning, generated streaming, saved/cohort/mixed strategy,
observed/reviewed histories and full60000 combined acceptance remain required
under15/25. See docs/models/commercial-fee-journal.md and the complete ledger.
Rebuild compatible native artifacts, restart deployed workers and explicitly
rebuild selected saved sessions/libraries before use; local validation does not
deploy or close the full issues.

## Per-pool MBS prepay speed (0.29.6)

An optional MBS book column `prepay_mult` scales one pool's prepayment speed:
monthly CPR = (turnover + refi) x `prepay_mult`, then the existing CPR cap. A
missing column or null cell means 1, which reproduces the model exactly; 0 stops
prepayment, leaving scheduled amortization. Saved books, what-if and
decision edits accept finite values in [0, 10]. The global
prepay vector, LUTs and static FICO/size/state/channel multipliers are
unchanged, and the multiplier is not a fitted pool-level model.

Rust owns the multiplier in every built-in mortgage path (spot risk, 9Q stress,
accounting/NII/KPI capture, the incremental graph, whose cashflow identity now
includes it, and decision repricing). New-origination unit templates use 1. The
Python kernels carry the same term in all three MODEL-BLOCKs as the independent
reference; custom Python prepay steps reject non-unit values. Fixed-OAS and CRN
are unchanged: OAS is still solved to price with the multiplier applied.

The mortgage kernels take a ninth per-pool vector after the static multiplier,
so native product ABI 9 (ops 4/5/10 arity +1; ops 28/29 take the multiplier
last) requires rebuilding scripts/build_native.py and the decision/workflow
artifacts, then restarting API/workers. Saved books without the column are
unaffected. Tests: `tests/test_prepay_multiplier.py` and the decision edit gate
in `tests/test_decision.py`.

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

## Liquidity applicability, transfer approval and HTM accounting (0.29.8)

Each daily-ledger account declares `lcr_applicability`/`nsfr_applicability`:
`internal_limit` (needs a positive floor), `not_applicable` or `unconfigured`
(both forbid a floor). Empty resolves from the floor, so older specifications
keep their limits as internal limits. `regulatory` is rejected:
research-weights-v2 computes research proxies only. Path rows carry
`lcr_status`/`nsfr_status` (within_limit, unavailable when the ratio is
undefined, breach, or the non-limit basis), headroom is null unless the basis
is internal_limit, and the summary reports the basis and the worst status. A
rule that does not apply is never reported as passed.

Transfers may set `approval_day`: a request before it is logged as delayed
("awaiting transfer approval") and re-dispatched on that day. Intercompany
receivable/payable must eliminate per currency every day; a non-zero sum
fails the run. `test_ledger_svb_policies.py` carries the trapped-cash golden
case: a bank entity breaches its cash floor while consolidated cash is ample.

`htm_accounting="asc320-htm-v1"` classifies HTM sales by `sale_reason`.
ASC 320-10-25-6/25-14 exceptions (credit_deterioration, tax_law_change,
business_combination, regulatory_change, and near_maturity within 90 days of
maturity) are permitted. An unsupported near_maturity claim and liquidity or
other sales count against `htm_taint_materiality` x opening HTM amortized
cost. Exceeding it reclassifies the remaining HTM securities to AFS on that
day, with fair value less amortized cost posted to AOCI in balanced journals.
The detail table `accounting_events` records each sale and reclassification.
Without a policy, sales keep the existing internal face limit and are recorded
as not_assessed. This is a research implementation of the stated rule, not an
accounting opinion: management intent, auditor judgment and tainting periods
are not modeled (HTM positions originating after a reclassification stay HTM). Rebuild the ledger and decision/workflow binaries and restart
workers; the new output columns are additive.


## Reviewed earned-fee refunds (0.29.18)

The standalone nonintegral fee owner and native commercial loan/fee journal
support explicitly permitted earned AND collected service refunds. A reviewed
refund reverses fee cash/income, retaining separate cumulative refunded earning
and historical gross service. It cannot grant new earning/collection capacity,
use prepaid cash or waived recoveries, or change loan original cost/EIR/credit
paths. Native exit policies optionally select a bounded refund fraction and
reserve the additional group before generation; absent/zero selections retain
historical outputs and admission. Signed gross rights, full tails, independent
raw-source/persisted replay and original publication fences remain required.

See docs/models/nonintegral-fees.md and commercial-fee-journal.md for the supplied
policies, controls and exclusions. Fee Pricing/acquisition/units/capital/FTP,
exercise/candidate/holding and draw/secondary adoption, in-arrears earning,
generated streaming/convergence, full saved/cohort/mixed adoption and observed/
reviewed/full60000 acceptance remain open under15/25. Rebuild compatible product,
ledger and decision/workflow artifacts and explicitly rebuild saved sessions/
libraries before new execution; local validation does not deploy or close15.

## Formal ASC 815 hedge relationships (local 0.29.19)

`hedge-accounting-1` is an explicitly selected additive lane for issue #35. It
consumes one complete reviewed `derivative-accounting-1` Physical source whose
hedging instruments carry the raw `undesignated_economic_hedge` review, plus
explicit hedged-item and forecast-exposure sources. Rust owns relationship
admission, the append-only lifecycle, hedged-item benchmark-component valuation,
hypothetical valuation, cumulative dollar-offset assessment under reviewed
bounds, mark re-ownership into OCI, hedged-period reclassification, frozen
OCI release, basis amortization/derecognition and the single ordered journal.
Each derivative mark, coupon, collateral and closeout entry is recognized once.

The implemented rule matrix is `asc815-asu2017-12` (post ASU 2017-12 recognition
without separate ineffectiveness). Under an explicit adopted ASU 2025-09 policy
the similar-risk group (815-20-55-23A through 55-23D) and choose-your-rate debt
(815-30-35-37I through 35-37M) elections apply prospectively with per-member and
per-option hypotheticals, dated dedesignation and rate elections, and the
swap/written-option combination (815-20-25-88) designates a swap plus a written
cash-settled swaption of matching notional as one instrument. ASU 2022-01
portfolio layers, the ASU 2025-09 dual-hedge election, net-investment,
nonfinancial, standalone option instruments and counterparty default paths
reject explicitly; relationships designated before the opening date carry
reviewed retained balances posted once against opening equity. Assessment uses
clean cumulative changes at accrual-free after-payment dates before the final
instrument settlement; matched critical terms require a zero-value instrument at
inception. Bounds: 64 relationships, 1,024 events, 32 MiB input/output and one
million hypothetical work units beyond the derivative owner's own admission.

Independent Python replay (`hedge_accounting_reference`) reconstructs the
derivative source, item/hypothetical values, lifecycle, assessments, journal and
roll-forwards before and after persistence. A legacy saved-book `fvh`/`cfh`
label is not an input. Saved-book, shared daily journal, treasury/capital/AOCI
policy, portfolio-layer, empirical effectiveness and complete-scale acceptance
remain open under #35; see `docs/models/hedge-accounting.md`. Rebuild the
product artifact and restart API/workers before executing the new schema.

`hedge-treasury-1` (same stage) maps the verified complete-tail hedge closing
to the treasury bridge: derivative cash/margin/marks and hedge basis/OCI/
reclassification rows once, cash-flow-hedge AOCI under its own GL with a
reviewed `cash_flow_hedge_aoci_eligible` policy flag (default excluded), an
explicit `interim_report` selection of report-date capital at a path node, and
the existing thirteen-ratio bridge. Rows near zero are omitted, unpaid rights
and negative basis with nonzero weight refuse. Interim capital, FTP,
deferred-tax/AOCI opt-out policy (#30/#72) and regulatory approval remain open.
`derivative-treasury-1` feeds the standalone `derivative-accounting-1` journal
to the same bridge with no hedge accounting: per-instrument rows reconciled to
the selected reporting node, an optional AOCI policy block and the same interim
selection and refusals. Mixed saved-derivative treasury feeds remain open.
