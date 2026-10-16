# AGENTS.md — tests

## Production execution contract (0.29.4; supersedes historical defaults)

The authoritative policy is `../../../docs/rust-production-contract.md`.
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

Version 0.28.0 provides a native built-in calculation runtime: raw calibration,
CRN, scenario/edit dependency graph, all existing product drivers, accounting,
forecast controllers, KPIs, units, strategy orchestration, and daily ledger/reports.
HiGHS stays C++; Python remains an adapter and independent reference/verifier.
Historical version sections below record intermediate states. The latest ownership
contract and acceptance evidence are in docs/reviews/2026-10-01-native-owned-workflow.md.

`test_native_strategy_owner.py` (0.27.1) gates the public native optimizer's
execution ownership: Python coefficient/solver/replay functions are forbidden,
raw libraries are replayed against the independent reference across randomized
scenarios, and the standalone Rust executable consumes the same request. It also
gates uncapped public solves, infeasibility, horizon/grid/shape validation and
ignoring stale Python coefficient caches. Build the decision DLL and executable
before running release evidence; skipped native tests are not a validation pass.

What each test GATES. If your change touches the left column, the right
column must pass — and "loosen the tolerance" is never the first answer
(one tolerance was tightened this project, none loosened; one assertion
was corrected because the ENGINE was right and the test was wrong —
callable negative dv01 — and that correction is documented inline).

| change | gating tests |
|---|---|
| kernels.py either MODEL-BLOCK | test_zero_shock_invariant (engine vs stress_engine must agree exactly at zero shock) + test_stress_pnl_signs |
| deposits DEPOSIT-MODEL-BLOCK | test_deposit_zero_shock_invariant + test_deposit_terminal (hand fixtures per terminal mode, zero-shock per mode, Rust parity) |
| Padé / LUT / sigmoid numerics | test_rational_sigmoid_oas_accuracy (<0.05bp asserted; measured 0.007bp) |
| pricing / solver | test_oas_roundtrip (+ every product's end-to-end reprice check) |
| conventions | test_day_counts (hand-computed values), test_us_holidays_and_adjust (rule-generated 2026 dates incl. Good Friday/observed July 4), test_schedule_taus_sum |
| corp engine / deck | test_cd_matches_corp_bullet (cross-engine), test_callable_below_bullet / puttable ordering, test_sink_schedule_shortens_duration, test_exact_time_discounting (12-day maturity shift must move price ~13bp), test_exact_fixing_interpolation (1e-12 vs hand-computed on controlled paths) |
| deposit rate model / fitter | test_rate_model_fit_recovery — asserts the IDENTIFIABLE objects (lambdas + equilibrium function pointwise ≤35bp), not the raw (b_max,k,pivot) triple |
| attrition / deposit engine | test_balance_conservation_and_wal (runoff sums to 1 at 1e-9), test_segment_flight_ordering |
| CD options | test_cd_withdrawal_option_raises_liability_value, test_cd_issuer_call_lowers_liability_value |
| ModelSuite / registry | test_custom_prepay_model_swaps_in, test_stress_rejects_custom_prepay |

## Patterns to copy when adding tests

`test_forecast.py` gates forecast month/tenor construction, source unit conversion,
mandatory HPI jump-off, flat tails, copy-on-write conditioning, zero-change and
discount identities, frozen accounting and contractual floating-income response.
API `test_forecasts.py` gates release eligibility, native SME decimals, SPF lagged
actual exclusion, revision checks and snapshot isolation.

`test_market_curve.py` gates research DF conversion: independent annual par
identities, flat positive/negative-rate round trips, and invalid input rejection.
API `test_market_data.py` gates normalization, snapshot integrity, no future
curve fallback, credential redaction and atomic revision-checked application.


- **Zero-magnitude invariant**: any new stress/checkpoint path gets a
  test where the zero shock must reproduce base values (rtol ≤1e-6).
- **Controlled-path hand replication**: build a 1-path market with a
  deterministic ramp and match the engine to a hand-computed price at
  ~1e-12 (see test_exact_fixing_interpolation) — the strongest pattern
  here for cashflow-logic changes.
- **Cross-engine degenerate contract**: new engine with all options off
  must reprice an existing engine on identical terms and paths.
- **Fit recovery on a known generator**: assert identifiable quantities;
  if parameters are weakly identified, test the FUNCTION they define and
  say so in the docstring.
- Keep fixtures module-scoped and small (≤200 positions, ≤128 paths);
  JIT dominates wall time, not the math.

## Gates added v0.6-v0.14
| change | gating tests |
|---|---|
| deposit engines / fitter | test_rate_model_fit_recovery (joint NLS, identifiable objects), test_balance_conservation_and_wal, test_deposit_zero_shock_invariant |
| CD engine / options | test_cd_matches_corp_bullet (cross-engine), withdrawal/call ordering tests |
| accruals / accounting | test_book_yield_par_bond, test_balance_sheet_nii_end_to_end, test_nim_reconciles_to_reported_with_basis_and_mm (the 2.94%-vs-2.47% ladder) |
| kpis | test_kpis_end_to_end (EVE == equity plug, density 59.6%, CET1 path) |
| hedges | test_swap_pricing_and_parity (par MtM~0, payer/receiver mirror, swaption parity at 1e-12), test_hedge_book_cuts_irrbb_outlier |
| strategies / unitlib | test_strategy_at_market_carry_and_reinvestment, test_unitlib_interactive_kpis (linearity exact, KPI directions) |

## Gates added v0.18.0

`test_review_regressions.py` gates full-driver vega/KRD parity with custom HPI,
exact odd CRN dimensions, terminal-forward tenor extension, isolated bounded
path caching, optimizer-to-evaluator objective replay and funding, inactive and
matured cohorts, fixed-OAS scenario marks, complete NSFR equity and partial-quarter
capital retention. API contract tests cover all product JSON round trips,
validation, revision invalidation/publication, queued snapshots, thread reset,
retention, and real engine jobs. Browser tests gate focus/popover preservation,
late-response rejection, shared telemetry/KPIs, table paging and book saves.

## Gates added v0.19.0

`test_incremental.py` gates independent legacy calibration, full-rebuild parity,
stable OAS when grouping/reordering rows, instrument-only spread/contract/target
invalidation, notional-only scaling, product-assumption isolation, seed/path-count
dependencies, fixed-OAS scenario marks, bounded immutable cache nodes, and invalid
backend rejection. API tests gate pricing scope/ID validation, real background
pricing jobs, cache reuse and queued snapshot isolation across book edits.
Changes to the OAS solver must preserve per-instrument convergence independent
of batch composition and pass all existing roundtrip/product gates.

## Gates added v0.20.0

`test_whatif.py` checks held OAS, explicit recalibration, saved-input immutability,
invalid assumptions, independent full-driver NII/DV01/KRD/KPI parity, selective
23-leg invalidation, auxiliary books, empty scope, default-HPI identity and
batch-independent effective yields. `test_native.py` exercises the real built
Rust ABI against NumPy/Numba on ragged signed cashflows and 1/4 threads, immutable
buffers, invalid/overflow errors and unavailable-library behavior. Native parity
is skipped only when the optional library is absent; build it for release evidence.

API what-if contracts gate revisions, validation, serialization and calibration.
Browser what-if tests exercise real jobs, stale-response rejection and narrow
panels. Native `cargo test --locked`, formatting and clippy are separate gates.
Benchmarks are reproducible measurements, not machine-specific timing assertions.

## Gates added v0.21.0

v0.21.1 cache changes also require the incremental regression cases for shared
dependency retention under instrument churn, total/tier budgets, failed-batch
atomicity, disabled-cache income parity, unequal MBS path counts, edited contract
accounting anchors, and the complete 23-leg curve sweep without base recomputation.

`test_decision.py` exercises the actual native DLL: SciPy objective parity,
independent allocation replay, five-book aggregate deltas versus full repricing,
four product-family selective template builds versus full libraries, constraints
without any product calls, null reset, infeasibility, stale versions, transactional
rollback and session isolation. Native tests gate dirty sets, solver reuse,
malformed ABI requests, overflow and simultaneous actor lifecycle. API tests gate
snapshot changes during publication; browser tests cover real build/edit/solve/
replay and suppression of obsolete results. Native engine tests skip only when
the optional DLL is absent; release validation must build it and have zero skips.


## Balance-sheet stress (0.22.0)

`test_balance_stress.py` gates daily balance-sheet and cash/equity reconciliation,
accrual/payment timing, AFS/HTM sales, allowance/default/recovery identities,
collateral conservation, local transfer limits, historical margin settlement,
zero severity, non-anticipation, bounded execution and strict input validation.
See `docs/balance-sheet-stress.md` for the scope and remaining model limitations.


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
`run_balance_stress` and API/worker defaults were unchanged at that checkpoint;
the 0.29.4 production contract now requires Rust.

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
program, income/accrual and volatility value/Jacobian batches. Python was the default at this historical checkpoint; 0.29.4 supersedes it. `RiskSettings.compute_backend` is durable and frozen into queued
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


`test_native_market_owner.py` (0.27.2) forbids Python financial stages while
constructing native rate/mortgage paths and forward parallel shock templates.
It gates raw par-quote repricing, independent SciPy curve parity, complete shared-
draw path parity, run-local cache isolation/content invalidation, explicit
recalibration and malformed inputs. Rust unit tests exercise MarketContext
directly without Python, including hand-computed deterministic rate/CC/PS/HPI
outputs and flat positive/negative curve identities. Product ABI 2 must be built
for these gates; native skips are not release evidence.


`test_native_calibration_owner.py` (0.27.3) gates current-coupon/PS fits and PCA
against independent NumPy outputs, including rank deficiency, clipped AR
persistence, residual variance, backend cache isolation and malformed histories.
Ownership tests forbid NumPy lstsq/eigh inside native fits. Rust tests check
minimum-norm singular regression and factor orientation/norms without Python.
Run final native product/market and full engine gates: fitted-parameter agreement
alone does not establish unchanged downstream risk or shared-CRN behavior.


`test_native_nonlinear_owner.py` (0.27.4) forbids Python optimizer/model callbacks
inside native abcd and joint deposit fits. Independent SciPy gates cover warm
starts and perturbed quotes, equilibrium/dynamic rates across random histories,
constant histories, invalid inputs, diagnostics and thread determinism. Rust
unit tests run both fits directly and exercise nonlinear boundary convergence.
Do not relax existing final product/ledger parity gates to accommodate a fit.


`test_native_random_owner.py` gates seeded native CRN generation against NumPy,
including multiword integer entropy/carry, odd/even antithetic paths, independent
spread/HPI streams, a million independent normal draws with tail/rejection
branches, Python RNG prohibition, thread invariance and preallocation admission.
Run final native product/risk/ledger and full engine gates with ABI 5 installed.


`test_native_mortgage_risk_owner.py` forbids Python calibration, random generation,
base OAS, pricing and path callbacks in the native spot-risk driver. It gates
raw history/book parity, odd paths and multiword seeds, explicit HPI/payment
delays, supplied fixed OAS, 256-position chunk boundaries, invalid books and the
standalone executable. Keep final-output tolerances unchanged; the reproduced
5y KRD roundoff case requires the tighter f64 Python bootstrap root tolerance.

`test_native_mortgage_stress_owner.py` gates the complete raw-book forward stress
route, one-call/no-Python ownership, position/aggregate/profile parity, fixed OAS,
chunk boundaries, matured and zero-face rows, standalone schema/error handling,
thread invariance and output admission before allocation. Native spot-risk tests
also cover user-added state/channel multiplier categories.

`test_native_mortgage_cache.py` checks unchanged/notional reuse, independent
Python output parity, seed/path/curve/vol/history invalidation against a cleared
cache, and invalid-book isolation. Rust cache tests cover complete-byte identity,
LRU eviction, oversized bypass, active-borrower safety and failed-stage publication.

## Native corporate and CD spot-risk lifecycles (0.27.7)

Rust now owns raw corporate/CD normalization, Gregorian date arithmetic,
calendar adjustment, day counts, coupon/amortization/exercise schedules, market
calibration, shared seeded draws, base OAS and fixed-OAS KRD/vega/DV01. Public
`run_corp_risk` and `run_cd_risk` make one raw-batch native call in Rust mode.
`CorpDeck` and `CDDeck` also use native normalization for their other consumers.
Python retains transport/column formatting and the independent reference path.

`portfolio-term-risk` runs without Python and accepts `term-risk-1` or
`term-deck-1` JSON: an envelope with `schema`, `threads` (1..256) and `request`.
Typed request fields are defined in `term_deck.rs` and `term_risk.rs`; dates are
Gregorian ordinals (0001-01-01 = 1). Missing/unknown fields and invalid contracts
fail explicitly. Native product ABI 7 requires rebuilding and restarting workers.
There is no silent Python fallback or callback into Python financial code.

Corporate/CD cashflow kernels write into preallocated CSR buffers through
disjoint row slices, avoiding per-instrument allocation and assembly copies
while preserving path reduction order. Risk uses 256-contract chunks.
Schedules admit at most
4096 periods per contract and 1,048,576 per constructed deck. The shared immutable
128 MiB market-stage cache now also retains rate-only paths keyed by complete
market/grid/seed inputs. Book edits recompute cashflows/OAS/risk; market edits
invalidate the affected stages. Failed requests return an error envelope; only
successful stages can enter the cache. Raw JSON transport is bounded to 64 MiB
input and 128 MiB output for the standalone/deck protocols. Python spot-risk
uses the same raw JSON input but receives contiguous numeric buffers, removing
bulk result JSON encoding/decoding. All outputs publish only after validation;
result buffers are admitted up to 1 GiB. Random/path budgets are separate. These
are component limits, not a total process RSS cap or zero-copy Arrow transport.

This preserves existing model approximations: adjusted accrual endpoints,
simplified US holiday rules, linear-principal "annuity", monthly exercise
mapping, rule-based exercise, CD withdrawal heuristics and terminal-grid clamping.
The reference calendar's year-scoped New Year observation behavior is retained;
explicit extra holidays can represent missing dates. Custom Python calendar
subclasses are rejected. Parity does not certify these as production conventions.

`test_native_term_owner.py` gates full schedule parity across all four day counts
and business-day conventions, holidays/stubs/leaps, raw-batch ownership, fixed
and solved OAS, odd paths/large seeds, withdrawal overrides, option-rich 257-row
chunk boundaries, empty decks, standalone/thread equivalence, malformed inputs,
warm reuse and curve/volatility/seed/path invalidation. Final risk tolerances
remain rtol=1e-7/atol=1e-5; warm/cold rebuild results are checked exactly.

Remaining lifecycle work includes deposit/hedge and other drivers, cross-book
accounting/KPIs, native unit-library pricing, the global dependency graph,
ledger mapping/reporting and unified cancellation/deadlines. Full mixed-book
60,000-instrument performance acceptance is still open. HiGHS stays in C++.

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

The workflow transport releases private request dictionaries after writing the
immutable input file; caller books and ledger mappings are not consumed. Small
term-risk market grids may be retained per request under a 32 MiB numeric bound;
larger grids continue through the bounded market cache.


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

The 0.29.4 `conftest.py` explicitly selects Python for existing reference tests.
Native contexts override it. `test_production_backend.py` runs fresh subprocesses
outside this harness to gate unconfigured Rust dispatch and missing-library failure.
Do not delete the independent oracle or let default changes make parity tests self-comparisons.

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

Current architecture: `../../../docs/architecture.md`. Interface/transport and dependency rules are in
`../../../docs/contracts/README.md`. Run `make check-architecture` from the root
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


## Earned-fee refund gates (0.29.18)

`test_nonintegral_fee.py` and `test_commercial_fee_journal.py` gate signed,
reviewed refunds bounded by both earned and collected service, separate prepaid
and waived recovery ownership, unchanged loan anchors, once-only source weights,
historical omission and independently rejected monetary/policy tampering.
Core native `tests/nonintegral_fee.rs` supplies hand controls and strict capacity
refusals. API `test_commercial_fee_jobs.py` gates durable raw/persisted source,
gross refund snapshots and rehashed refund/approval/posting publication failures.
These numerical/source gates do not establish observed model or policy approval.
