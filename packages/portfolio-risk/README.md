# portfolio-risk

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

Engine 0.28.0 runs the built-in calculation lifecycle in Rust: raw calibration,
shared CRN, product/scenario pricing, the bounded dependency graph, accounting,
KPIs, unit pricing, transactional decisions, and daily ledger/reporting. Rust
constructs and replays strategy problems; HiGHS remains C++. Python is the
application/storage adapter and independent verification/reference path.
`portfolio-workflow` combines raw books, edits, optimization and daily replay in
one native process. Product ABI 8 and ledger protocol 2 require current builds
and worker restart. See the [ownership, performance and validation report](../../docs/reviews/2026-10-01-native-owned-workflow.md)
for measured scope and inherited model limitations.

Published forecast income (v0.20.1): `analytics.forecast` replays external monthly
drivers through existing cashflow engines, with shared CRN and frozen accounting
anchors. Outputs conditional NII/runoff; no OAS/EVE or credit/capital forecast.
See [forecast contracts and limitations](../../docs/forecast-scenarios.md).

Shifted-lognormal LMM OAS / risk / 9Q stress-capital engine for bank
balance sheets: MBS and whole loans, corporates, deposits, CDs, money
markets, and hedge overlays on shared rate paths.

## Quick start

Version 0.19.1 adds `core.curve.market_discount_factors_to_par`: a validated
projection of dated source discount factors onto the annual-payment model grid,
with explicit reconstruction errors. It does not change the simulation curve
interface or resolve missing sub-year pillars. See [research data adapters](../../docs/market-data.md).

```
pip install -e .
python -m portfolio_risk 10000 bench     # throughput probe + projection
python -m portfolio_risk 10000 risk      # KRD01s + vegas
python -m portfolio_risk 10000 stress    # 9Q forward valuation + shocks
pytest tests/ -v
```

## Incremental spot pricing (0.19.0)

`analytics.incremental.price_books` prices MBS, loans, debt, deposits and CDs
through a bounded content-addressed dependency graph. Unchanged instrument
cashflows and calibration survive edits elsewhere in the book; cache misses
are dispatched together by product. A spread override refreshes its price only,
and notional changes reuse the per-unit price. Existing risk, stress, NII and
strategy drivers retain their separate execution paths.

Use an explicit `RunConfig` and a reusable `core.dependency.DependencyCache`.
Saved contract/target/base-market changes establish new calibration inputs;
`scenario_market` and `spread_overrides_bp` hold that base OAS fixed. MBS uses
`n_paths_base` for both base and scenario spot prices; other books use `n_paths`.
Model constants still require restart, including baked prepay LUTs. OAS solvers
now freeze each converged instrument independently of the rest of its batch.

The `core.batch.CashflowBatch` / `DiscountBackend` boundary uses immutable CSR
buffers of path-summed discounted cashflows, year-fraction times, offsets, and
decimal OAS. NumPy is the current backend; no Rust or third-party pricer is used.
Cache identity includes backend/model versions, markets, histories, dates,
seed, path counts, product assumptions and contract inputs. The cache defaults
to 128 MiB of accounted retained nodes and 50,000 entries; in-flight allocations
and output frames are additional memory. Outputs disclose actual graph work.

See the root [implementation report](../../docs/reviews/2026-09-28-incremental-pricing.md)
for API usage, validation, measured performance and scope limitations.

## Module map
| module       | responsibility |
|--------------|----------------|
| config       | all constants, dtype/sigmoid switches, stress grid |
| curve        | par-swap bootstrap -> quarterly DFs/forwards |
| vol          | factor loadings, abcd, swaption approx, calibration, vol features |
| lmm          | shifted-lognormal spot-measure simulation |
| models       | trending CC (partial adjustment), PS OU, HPI — fitters + paths |
| prepay       | spline anchors, LUTs, static multipliers (DATA; replace with fits) |
| kernels      | numba: shared `_month_step`, `engine` (A/FV/checkpoints), `stress_engine` |
| pricing      | PV-from-A, vectorized Newton OAS solve |
| scenarios    | CRN, path building, forward-starting shock templates, setup |
| risk         | 10 KRD01s + 9 vegas, fixed-OAS central differences |
| stress       | 9Q monthly forward valuation, shocks, fwd DV01 profile |
| demo         | synthetic market/histories/portfolio |

## Known limitations
Deterministic CC vol features (no SV-LMM); abcd-projected point vegas;
stylized prepay and attrition anchors (fit to loan/account-level data
before use); parallel-only forward shocks; frozen-weight Rebonato;
rule-based exercise for callable paper; demo fitters run on synthetic
histories.

## v0.5: modular components, conventions, corporates
- **Swappable component models** (`interfaces.py`): CC / PS / HPI / prepay
  behind protocols + registry; `ModelSuite` threads through build_paths,
  run_risk, run_stress. Custom prepay = jitted step fn via
  `kernels.make_generic_engine` (~25-30% slower than the open-coded fast
  path; promote winning models into the MODEL-BLOCKs for production).
  run_stress rejects custom prepay until promoted (kernel-consistency guard).
- **Conventions** (`conventions.py`): 30/360, ACT/360, ACT/365F, ACT/ACT
  day counts; rule-generated US bond-market holiday calendar (incl. Good
  Friday via computus); F/MF/P adjustment; backward-rolled schedules; MBS
  stated payment delay (`pay_delay_days` column -> OAS-layer discounting,
  measured ~3bp at 24 days).
- **Corporates** (`corp.py`): fixed/floating (index = simulated 3m rate),
  caps/floors, bullet/annuity/sinking-fund amortization, call/put schedules
  with RULE-BASED exercise (not option-exact; LSMC slot documented in the
  EXERCISE-BLOCK). `CorpDeck` packs convention-exact accruals into CSR
  arrays; `run_corp_risk` gives OAS + KRDs + vegas on the same LMM paths.
- **Exact-time discounting (corp)**: each cashflow deflates to its true
  pay date by extending the monthly MMA deflator within the pay month at
  that month's simulated short rate; OAS discounts per period at exact
  pay times (CSR A-vector). No month-snapping timing error remains beyond
  the monthly rate discretization itself. Floater fixings likewise read
  the index at the exact fixing date (interpolated between monthly
  observations); index tenor is the front 3m forward regardless of
  accrual frequency (no 1m/6m index curves yet).

## v0.6: non-maturity deposits
`deposits.py`: NMD valuation as MBS-OAS with the signs flipped (runoff =
prepayment, deposit rate = coupon, liability priced as PV of all outflows;
price 96.5 => 3.5% franchise premium). Deposit rate model = logistic
long-run beta to fed funds + asymmetric error correction, fit by JOINT NLS
over the simulated recursion (static fits attenuate b_max severely; the
fitter warns when the history never visits the logistic plateau --
high-rate beta extrapolation is flagged, not hidden). Attrition = hazard
analog of prepay: segment base decay x account-age curve x balance-size
multiplier x opportunity-cost S-curve x rate-velocity accelerator
(anchors stylized; fit to account-level panels). `run_deposit_risk` gives
liability OAS, KRDs/vegas (positive dv01 = sticky long-duration liability),
premium %, and runoff WAL on the same LMM paths.

## v0.7: deposit stress capital
`run_deposit_stress`: 9Q monthly forward liability valuation + forward-
starting shocks for NMD books, mirroring the MBS stress pack. Deposit
shocks are EXACT, not templated: the rate recursion re-runs on the
shifted short path (full nonlinear logistic-beta + asymmetric-ECM
response; velocity recomputes, so shock-induced deposit flight is
captured). deposit_stress_engine restarts from balance checkpoints;
zero-shock invariant gates the duplicated DEPOSIT-MODEL-BLOCK. Output is
EVE-convention (eve_pnl = -d liability value): sticky books gain EVE when
rates rise, hot money doesn't.

## v0.8: certificates of deposit
`cds.py`: term-deposit liabilities under the SECURITIES construct --
schedule-driven via conventions (day counts, calendars, MF adjustment,
at-maturity or periodic interest), exact-time discounting and OAS via the
corp CSR machinery (CDDeck duck-types corp_pv/corp_solve_oas). Two
embedded options: depositor EARLY-WITHDRAWAL PUT (hazard S-curve in
reinvestment incentive net of the interest-forfeiture penalty amortized
over remaining term -- the CD analog of prepayment) and ISSUER CALL on
brokered/callable CDs (corp rule-based exercise, same LSMC caveat).
Channel drives defaults: retail = withdrawal on / no call; brokered =
withdrawal off / call honored. Cross-engine consistency is TESTED: a CD
with both options off reprices the corp bullet on identical paths.
Rollover/retention at maturity is out of scope (existing-book pricing).
Note: callable CDs can show locally negative dv01 near the exercise
boundary -- correct economics, asserted in tests.

## v0.9: model balance sheet + NII accounting
`demo.model_balance_sheet()`: WFC-proportional synthetic balance sheet
(source: 1Q26 Quarterly Supplement, Mar 31 2026 -- AFS 222.9B@4.44%/HTM
204.1B@2.27%, loans 1,016.8B@5.62%, deposits 1,454.9B (NIB 365.7B),
LTD 183.9B@5.25%) mapped onto the four engines; trading/repo/cash/cards
excluded by construction. `accounting.py`: engines now emit UNDISCOUNTED
expected interest/principal (appended LAST in return tuples); book yields
via per-position IRR on expected cashflows (effective interest, static
level yield -- no retrospective recalc, disclosed); CSR period interest
smeared across accrual months; deposits at rate paid (servicing =
noninterest expense). `run_balance_sheet_nii` -> monthly NII by product +
model NIM. Demo run annualizes to ~$51.8B at full scale vs WFC's ~$50B
2026 guide; model NIM 4.2% vs reported 2.47% reflects the excluded
low-margin balances, compare composition not headline.

## v0.10: NIM reconciliation
mm.py (markets balance sheet as spread-to-short floaters) + book_yield
amortized-cost basis overrides. Ladder vs WFC 1Q26: market-basis core
4.24% -> holder's basis 3.42% -> + markets book 2.94% vs reported 2.47%;
each step matches the decomposition computed from the filing's
average-balance table.

## v0.11: top-level KPIs
kpis.py: EVE + Delta-EVE/duration gap (parallel dv01 by full revaluation;
IRRBB 15% outlier test), stylized LCR (real CD/LTD maturities, segment
runoffs, L2A cap), NSFR (deck-maturity ASF/RSF), standardized RWA with
density calibration, 9Q CET1 projection (filing-calibrated NI/NII
retention + optional AFS-mark AOCI leg). model_balance_sheet now BALANCES
via a deposit-sized equity plug so EVE is meaningful.

## v0.12: hedge products (ASC 815 / FAS 133)
hedges.py: payer/receiver swaps (corp-engine duck-typed legs), European
swaptions (cash-settled annuity MC on emitted par-rate paths),
designation-aware accounting (FVH/CFH/economic; CFH AOCI excluded from
CET1), carry into NII, hedge dv01 into EVE. Demo book clears the IRRBB
outlier: -27.2% -> -12.5% dEVE @ +200bp.

## v0.13: forward-starting strategies
strategies.py: hypothetical purchases/originations scaled in at market at
monthly forward dates (per-path coupon fixing, par price, zero purchase
MtM), fixed monthly notionals or reinvestment fractions of MODELED runoff
(the NII framework now exposes runoff_vectors per book). Outputs
incremental NII, balance trajectory, forward dv01 profile. API: PUT
/programs/{name}, POST /run kind="strategy".

## v0.14: unit library + interactive strategy KPIs
unitlib.py: unit cohorts of every product through the real engines
(batched, shared paths, live behavioral models), then strategy evaluation
as a time-shifted linear scaling -- ~300us per eval with full top-level
KPI recalc. API: POST /run kind="unitlib" (one-time ~20s), then POST
/strategy/eval (synchronous, interactive).

## v0.15: scenario-batched risk + thread control
batched_pv_engine: all 38 KRD/vega revaluations in one kernel launch
(stacked paths, scenario ids). Single-core parity measured; gains are
multi-core architectural (one parallel region vs 38). Gated by a 1e-10
cross-kernel invariant that caught two init bugs on first run. API
settings gain n_threads (0 = all cores).

## v0.16: robust balance-sheet optimizer
optimizer.py: maximin worst-case NII LP over the unit library; absolute
ratio floors + commercial plan constraints holding across multiple
market scenarios simultaneously; 11ms solves with shadow prices on
binding constraints. API: POST /optimize.

## v0.17: layered package reorg
src/portfolio_risk reorganized into core/ models/ products/ analytics/
strategy/ (matching ARCHITECTURE.md layers); old flat import paths kept
working via module aliases -- zero changes needed in tests, API, or the
skill. 34 tests green post-move.


## v0.18.0: review correctness and run-scoped reuse

- `core.runtime.run_context(RunConfig(...))` supplies sensitivity paths, MBS
  base-calibration paths, stress horizon, and per-run deposit/CD assumptions.
  Defaults outside a context remain compatible. Odd antithetic path counts
  produce exactly the requested dimensions. Never mutate frozen kernel constants.
- Run-local content-keyed calibration/volatility/rate-path reuse is capped at
  128 MiB. Cached arrays are read-only and never escape into a later run's cache.
- Risk vegas use up-minus-down. ModelSuite CC/PS/HPI selections propagate through
  setup and all scenarios. Product risk/stress drivers accept pre-calibrated
  `oas`; `analytics.calibration.calibrate_books` solves under the original market.
  Explicit spread shocks add to this vector rather than solving again.
- Monthly and exact-time OAS solvers reject nonconverged residuals. NSFR uses
  supplied equity or reconciles all books including money-market balances.
- The default purchase grid extends in six-month steps through the configured
  horizon. Unit allocation vectors share purchase-date shifting, truncation and active
  opening-balance conventions across evaluation and optimization. Current KPI
  fields describe month zero; `kpi_path` supplies monthly liquidity/EVE; horizon
  CET1 uses final-month opening RWA and all horizon income (including partial quarters).
- Optimizer maximin NII includes each scenario's base-book income. Monthly LCR
  applies both branches of the exact Level 2A cap, alongside NSFR/EVE/funding
  constraints. Returned solutions are replayed before `validated=True` is returned.
  The default `cash_budget=0` requires funding. A positive budget is additional
  committed funding outside the base book, available throughout the horizon;
  borrowing costs are not automatically priced. Existing reserves are not this budget.
- Forward DV01 is base unit sensitivity scaled by outstanding balance, not re-aged
  risk. Base KPI components and regulatory template weights remain static proxies;
  unit coupons and interpolation retain the existing approximations.
- Emitted 30-year par rates use the full 120-quarter tenor even after simulation
  year 10.25. Beyond the 40.25-year forward grid, the final simulated forward is
  held flat. Grid size and stochastic factors have not been increased.
- NumPy >=2.0 is required by `np.trapezoid`.

These changes are tested on synthetic data. They do not validate production
market histories, firm-specific regulatory mappings, or model suitability.

## v0.20.0: temporary assumptions and incremental analytics

`analytics.whatif.compare_books` compares immutable baseline and revised books.
Temporary coupon, mortgage, deposit attrition and CD withdrawal assumptions hold
baseline OAS; `calibration_mode="recalibrate"` explicitly refits the modified
contracts to saved target prices without saving them. Global frozen prepayment
parameters still require a process restart.

`include_analytics=True` adds parallel DV01, ten KRD01 pillars, monthly NII and
EVE/LCR/NSFR/CET1. Parallel risk uses 25bp central differences; pillars retain
the existing 1bp convention. OAS and effective accounting yields stay anchored
to the baseline. Auxiliary money-market/hedge contributions use existing drivers.

The optional NumPy, fused Numba and Rust backends share an immutable CSR contract.
Build Rust with `uv run --project apps/api python scripts/build_native.py` from
the monorepo root; select it explicitly through `backend="rust"` in API jobs.
No silent fallback. Product models, shared simulation, calibration and graph
ownership remain in Python. See the full measured comparison in
`docs/reviews/2026-09-28-five-step-comparison.md` at the monorepo root.

## Version 0.21.1: bounded pricing cache reuse

Income calculations reuse the current request's immutable base and scenario
cashflows even after global cache eviction. MBS base/sensitivity path counts
remain distinct. Completed sensitivity legs are released before income work.
Shared market/model fits and paths use a protected LRU tier capped at the lesser
of 64 MiB or one quarter of the existing total cache budget (and at most 1024
nodes or one quarter of the entry limit). Instrument results borrow unused space;
the total configured byte/entry limits are unchanged. `cache.info()` includes
shared-tier occupancy and limits. These are retained-result accounting limits,
not process-memory limits. Financial calculations and cache identities are unchanged.

## Version 0.21.0: optional Rust decision sessions

`strategy.decision.DecisionSession` connects existing product pricing to a Rust
session graph, incremental KPI contributions, persistent HiGHS optimization and
independent Python allocation replay. Use `update(version=..., edits=...,
templates=..., constraints=...)`; `evaluate(allocation, version)` uses stored
coefficients only. Always call `close()` when done. The native library must be
built separately; missing native support fails explicitly and never falls back.

`build_unit_library(..., template_names=[...], template_overrides={...})` supports
selective template pricing and temporary `spread_bp` assumptions. Existing calls
retain all templates and defaults. See the monorepo decision prototype report
and native crate README for installation, contracts, bounds and limitations.
Decision sessions request `include_key_rates=False` from incremental pricing:
only spot, parallel DV01 and NII feed this optimizer. The default remains True
for other callers, preserving full KRD analytics. This demand-based pruning is
separate from language choice and is included in the comparison measurements.


## Balance-sheet stress (0.22.0)

`analytics.balance_stress.run_balance_stress` accepts a versioned explicit cohort
specification. It reconciles daily entity/currency accounts, interest accrual and
settlement, deposit flight, commitments, credit/provision/recovery, AFS/trading/HTM
marks, netting-set losses and posted margin, and lagged management policies.
Baseline and four synthetic joint scenarios include sampled reverse stress and
cash/equity event attribution. Opening equity is supplied and must reconcile.
Use `example_specification()` or `GET /balance-stress/example`; submit via
`POST /balance-stress/run` with expected revision. Work runs on the durable worker;
Parquet artifacts use the existing SQLite/PostgreSQL/local/S3 architecture.
No pricing book or Strategy Lab allocation is automatically mapped into this
specification. LCR/NSFR and capital weights are explicit research proxies, not
regulatory compliance. Policies are non-anticipative; negative cash is an
unfunded obligation, not assumed financing. Pricing/forecast paths and the
coefficient-only strategy evaluator are unchanged. See
`docs/balance-sheet-stress.md` for equations, assumptions and validation gates.


## Ledger and dynamic replay (0.23.0)

`analytics.balance_stress` schema `balance-stress-2` posts balanced journal
entries, checks every materialized subledger, independently replays persisted
lines, and derives closing statements and within-currency intercompany
eliminations. Tax follows actions; linked collateral principal repays funding.
`analytics.accounting` optionally captures instrument cash/accrual/principal/basis
flows. `analytics.balance_workflow` maps all six saved books explicitly and replays
unit-library candidates through daily limits. Missing mappings and unsupported
saved hedge trades fail closed. Monthly timing and proportional survival remain
approximations. The fast evaluator stays coefficient-only; its validation is
explicitly scoped. `balance_rules` shares LCR composition/inflow cap arithmetic
with KPIs and centralizes daily acceptance. `balance_calibration` separates
chronological training/holdout driver diagnostics; it is not PD/LGD calibration.
See `docs/balance-sheet-ledger.md` for API fields, assumptions and remaining gaps.
No regulatory compliance or full production GSIB coverage is asserted.

## Security basis and ledger scaling (0.24.0)

AFS/trading securities separate cost basis from `opening_market_price` (clean
price per unit principal, default 1). Opening AFS OCI is FV minus cost, included
in supplied equity; it is not new income. Amortization, partial sales, forward
purchase and redemption reconcile cost/FV/OCI without duplicate losses. Saved
book capture supplies the engine quote; metadata cannot override it. Stress marks
remain duration-based and captured product flows remain monthly approximations.
`test_security_basis.py` gates hand-computed premium/discount accounting, quote
validation, saved mapping, zero-account statements and cached-schema validation.
Static type metadata is cached; collateral links and claim due dates are indexed.
No user financial state is cached by this metadata optimization. The daily ledger
is still Python, bounded to 2,000 positions; the earlier 60,000-position pricing
benchmark does not validate a 60,000-position journal. See
`docs/reviews/2026-09-29-rust-ledger-decision.md` for native pilot gates.

## Native journal pilot (0.25.0)

`run_balance_stress(..., journal_backend='rust')` selects the optional batched
GL reduction/checkpoint kernel. `python` is the explicit test-reference journal;
`columnar` is the independent Python control with the same compact buffers as
Rust. Product events, policy state, risk limits and regulatory assumptions remain
Python. One native call validates an ordered day of postings; closing replay
starts again at zero. No per-position FFI, global mutable native state or retained
pointer exists. Failed batches publish no GL state; explicit unavailable Rust
requests fail without fallback. Pilot failures may be detected at the daily
checkpoint rather than the individual post. The simulation aborts before return.

`execution` records backend identity and the Rust DLL SHA-256. The pilot is
engine-only and is not selectable through the API/UI; worker/storage behavior
was the default Python path at this historical checkpoint; 0.29.4 supersedes it. Journal rows/schema/order and every financial
output are parity-gated against the original reference, including independent
Python replay of native-produced lines. Tests also gate invalid buffers, atomic
rejection, cancellation-sensitive summation and concurrent isolated calls.
`scripts/build_ledger_native.py` builds the dependency-free optional crate;
`scripts/benchmark_ledger_backends.py` compares fresh processes, local Parquet
round trips and memory. Full journal retention and current 2,000-position limits
remain; no Rust product/state-machine or 60k full-ledger capacity is claimed.
See `docs/reviews/2026-09-29-native-ledger-pilot.md` for measured scope and results.

## Native daily state and partitioned journals (0.26.0)

`analytics.balance_stream.run_streamed_balance_stress` is an opt-in local engine
runner. Rust owns the full daily event/policy/state loop; Python owns validation,
immutable Parquet partition writes, independent persisted replay, closing
statements, attribution and final manifest publication. `backend='python'` is
the original financial reference with a partitioned journal sink. Existing
`run_balance_stress` and API/worker defaults remain unchanged.

Native output uses bounded numeric/dictionary journal batches over a child-process
pipe; it is not zero-copy Arrow. Saved journal partitions are independently
replayed in order, including transactions crossing files, before publication.
Input/source/binary identity, checksums, schemas, row counts and reconciliation
status are retained. Failed writes, native exits, cancellation, deadlines and
corruption abort the attempt. Recovery restarts from inputs, not a saved checkpoint.

An explicit `large_book=True` tier admits <=60k positions and <=90m work units;
default/API validation retains <=2k and <=3m. Financial-model approximations,
fixed-OAS/CRN contracts and calibration limits are unchanged. The native state
loop consumes explicit stress inputs, not a new Rust product pricer. Streaming
does not remove growing credit/claim state or every report-memory cost. Tests in
`test_balance_stream.py` gate full output parity, mixed randomized events,
partition boundary replay, corruption/write failure, cancellation and admission.
See `docs/reviews/2026-09-29-native-state-streaming.md` for measured scope/results.

## Native built-in product coverage (0.27.0)

`RunConfig(compute_backend="rust")` selects native built-in LMM, behavioral
paths, MBS/corporate/CD/deposit cashflows and stress, OAS, swaption, forward
program, income/accrual and volatility value/Jacobian batches. Python remains
the default. `RiskSettings.compute_backend` is durable and frozen into queued
jobs; the UI selector affects new runs. The older request `backend` still means
prepared-cashflow reduction only. Backend identities separate dependency nodes.
Build/restart all three native crates for product + optimizer + daily replay.

The public optimizer uses Rust-owned C++ HiGHS with independent Python allocation
replay. LCR/NSFR, funding, EVE, CET1 and commercial rows retain their existing
model scope. HTM accounting-category limits are daily stress/replay rules, not a
new accounting-category constraint in the unit-template LP. Saved-book native
mode uses native product capture and the full native daily state runner.
Python still prepares schedules/CRN, curve/PCA/fit controllers, coefficients,
reports and persisted validation; interactive evaluation remains coefficient-only.
Custom Python suites/rate models fail explicitly in native mode. No Rust fallback
is implicit. Matching these existing approximations is not GSIB model validation.

Volatility calibration now uses analytic abcd derivatives in both backends to
avoid finite-difference noise amplifying tiny native value differences. Regression
and final-output parity tolerances were not loosened. `test_native_products.py`
covers kernels, final risk/stress/NII/KPIs, optimizer binding/funding constraints,
saved-book/candidate ledger replay, FFI atomic failure and thread determinism.
Native CI builds are declared for Linux/Windows; current measurements are local
Windows evidence only. See `docs/reviews/2026-09-30-native-product-coverage.md`.

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
