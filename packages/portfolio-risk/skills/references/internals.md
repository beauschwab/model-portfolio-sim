# portfolio_risk internals and extension guide

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

## Conditional forecast income (v0.20.1)

`analytics.forecast` compiles source-period conventions to monthly drivers and
conditions base paths for NII/runoff. Quarterly averages repeat, rate endpoints
interpolate, and HPI log-interpolates from a required preceding anchor. Replay
maps source month one to book projection month one; it does not redate contracts.
Flat tails and unused macro variables must be disclosed. Basis to model rates
is assumed zero. One CRN feeds base/conditional accounting; shifted-rate
deviations are rescaled to means while preserving the model floor. Fixed-rate
book yields and deposit initial rates are held. Floating contractual income
changes adjust the base effective-income roll (approximation). Missing mortgage
and HPI drivers retain base outputs. No default/provision/capital mapping exists.
These paths are NOT arbitrage-free and MUST NOT feed OAS/EVE or derivative
valuation. Existing risk, instantaneous scenarios and fast strategy eval remain
separate. See `docs/forecast-scenarios.md` for source eligibility and limitations.

## Research discount-curve projection (v0.19.1)

`core.curve.market_discount_factors_to_par(times, discounts)` requires ascending
ACT/365F times from zero through at least 30y, positive finite DFs and D(0)=1.
It derives annual-payment par rates on the standard ten pillars, rejects
forwards outside the shifted model domain and reports reconstruction error.
Source daily DFs remain in API research snapshots; the engine still uses its
existing sparse bootstrap and >30y extrapolation. Applying a source curve does
not change the book date, prices, volatility or behavioral histories.

## Incremental spot pricing (v0.19.0)

`analytics.incremental.price_books` resolves market/history -> shared paths ->
instrument cashflows -> base OAS -> scenario/spread PV -> notional-scaled values.
Missing rows run as one batch per product/stage. Immutable content-addressed
nodes survive unrelated edits; bounded LRU eviction requires recomputation only.
The default cache has 128 MiB of accounted retained data and 50,000 nodes;
in-flight buffers and result frames are outside that budget.

Cashflow keys exclude labels, targets, notionals and book yield; calibration keys
add targets; mark keys add scenario cashflows, fixed base OAS, spread shifts and
backend identity. Keys include model version, as-of, histories, markets, seed,
path count and applicable product assumptions. Deposit segment changes invalidate
that segment; CD withdrawal parameters invalidate CDs. Global numba/prepay
constants require a fresh process. Never mutate cached buffers or bypass an
input dependency. Solver convergence freezes per row to preserve batch invariance.

`core.batch.CashflowBatch` is immutable CSR (offsets, exact year-fraction times,
discounted cashflow path sums, path count); `DiscountBackend` returns one finite
per-unit PV per decimal OAS. NumPy is the sole shipped implementation. Future
native/OSS adapters must declare a versioned identity and pass the same gates.
No replacement of product models or Rust runtime is implied by this boundary.

## Data flow

```
swap_rates --bootstrap--> DFs/forwards --+
vol_pts ---calibrate abcd (Rebonato)-----+--> LMM sim (CRN.Z) --> df, 4 swap paths
                                                  |
cc_hist --fit--> {beta, lambda} --> CC paths <-- vol features (deterministic)
ps_hist --fit--> OU params ------> PS paths (CRN.eps_ps)
                                   mtg = CC + PS, 2m incentive lag
s10 path --> HPI paths (CRN.eps_h) --> hpi, yoy
                                                  |
            kernels.engine(mtg,hpi,yoy,df, sec attrs)
              -> A[s,t]  (pricing: PV at any OAS = cheap einsum)
              -> FV/BAL[s,h] (27 monthly horizons)
              -> ck_bal/ck_burn[s,p,h] (stress restarts)
                                                  |
   pricing.solve_oas_from_A (vectorized Newton, bisection bracket)
   risk.run_risk: fixed-OAS central diffs, CRN, abcd fixed for curve bumps,
                  recalibrated (warm) for vol bumps
   stress.run_stress: shocked_paths templates -> stress_engine per (shock,h)
```

## Why it is fast (do not undo these)

1. **A-matrix factorization**: OAS enters only discounting, so cashflows are
   generated once; pricing/Newton never re-runs the kernel. Never
   materialize a (paths x secs x months) tensor.
2. **Transcendental-free inner loop**: Pade(7,6) logistics, LUTs for SMM
   12th root and burnout exp, annuity factor by recursion (one pow per
   security). Adding an exp/log/pow inside the month loop costs ~15-25%.
3. **Open-coded MODEL-BLOCK**: shared inlined helper with tuple return
   measured 28% slower (LLVM register allocation lost across pack/unpack).
   The duplication is guarded by test_zero_shock_invariant.
4. **prange over securities**: each thread owns its output rows -> no
   contention; path arrays are small enough to stay cache-resident.
5. **CRN everywhere**: one draw set across all 39+ revaluations; deltas of
   means, not means of deltas.
6. **Forward-starting shocks need no re-simulation**: deterministic
   templates in (t-h) — CC partial-adjustment ramp, deflator
   (1+d*dt)^-(t-h+1), HPI drift beta — applied to base paths
   (scenarios.shocked_paths).

## Extension recipes

### Plug in loan-level prepay fits
Replace anchors in `prepay.py`: LTV/FICO/SIZE (x, y) arrays feed
`nat_spline` / CubicSpline directly; STATE_MULT / CHANNEL_MULT dicts.
S-curve, turnover, burnout, lock-in live in `config.PREPAY_PARAMS`
(runtime vector — no recompile, EXCEPT burn_k which bakes into BURN_LUT at
import; rebuild process after changing it).

### Real swaption calibration
`vol.calibrate_abcd` is the seam: keep its signature, swap the objective
(e.g., add per-forward phi multipliers: extend the parameter vector and
multiply into `sig_tab` in `lmm.simulate_rates`). For crisp bucketed vegas,
add per-point multipliers on top of abcd and bump those instead of
recalibrating — `risk.run_risk`'s vol loop is the only caller to change.

### Per-pillar (non-parallel) forward shocks
`scenarios.shocked_paths` currently builds parallel templates. For a
pillar/curve-shape shock at horizon h: compute the shocked par curve, take
d_swaps = shocked minus base pathwise swap-rate effect via the CC beta
vector (per-tenor instead of summed), and the deflator template from the
short-rate portion of the shock. Same plumbing; stress_engine is unchanged.

### New shock dimensions (vol, HPI, spread)
- HPI stress: multiply `base["hpi"]` by a scenario path, rebuild yoy.
- Spread (OAS) stress: pass a bumped `oas` vector into stress_engine.
- Forward vol shock: requires re-simulation with bumped sig_tab rows >= h
  (cheap, ~0.1s) but breaks the "no re-sim" shortcut; use build_paths with
  a modified abcd and splice paths at h.

### Adding output (e.g., effective duration/convexity at base)
Derive from existing scenario PVs — never add columns to the kernel for
quantities computable from A or FV downstream.

## Change gates

| change | must rerun |
|---|---|
| anything in kernels.py | full pytest (zero-shock invariant + signs) |
| prepay anchors/params | test_oas_roundtrip + eyeball CPR sanity |
| calibration/vol | calibration RMSE print < ~50bp vol on real surface |
| config dtype/sigmoid switches | test_rational_sigmoid_oas_accuracy |
| new shock template | extend test_zero_shock_invariant pattern: zero-
  magnitude shock must reproduce base FV exactly |

## Module sizes / where things live

config (constants, switches) | curve (bootstrap) | vol (calibration,
features) | lmm (simulation) | models (CC/PS/HPI fits+paths) | prepay
(model DATA) | kernels (engine, stress_engine) | pricing (PV/OAS from A) |
scenarios (CRN, paths, shocks, setup) | risk | stress | demo | __main__.


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

## v0.20.0 incremental comparisons and native boundary

The dependency sequence is shared market/model fits and CRN paths -> per-position
cashflows -> original OAS -> revised marks/curve legs/income -> book aggregates.
Temporary edits preserve baseline OAS; explicit recalibration changes only the
comparison. Cashflow keys exclude notionals and labels, normalize optional
HPI defaults, and include date, resolved assumptions, histories, model version,
market and path configuration. An auxiliary node adds existing money-market/
hedge analytics and is keyed by valuation date too.

Risk adds 22 valuation legs: +/-25bp parallel and +/-1bp at each of ten pillars.
Both sides hold volatility calibration and use common random numbers. Incremental
NII freezes original effective yields and rolls revised expected cashflows;
deposits and CDs use contractual accrued interest. Existing capital/liquidity
approximations apply. Cache eviction changes speed, never calibration policy.

`CashflowBatch` contains int64 CSR offsets and float64 times/discounted path-sum
values; OAS is a separate decimal vector. Backends return one finite unit PV.
Numba and Rust fuse the reduction; Rust ABI v1 borrows immutable arrays through
ctypes and returns fresh output. No product objects or callbacks cross the ABI;
no pointers survive return; invalid/overflow/unavailable errors never fall back.
The Rust DLL hash versions mark keys. Build through `scripts/build_native.py`.

Engine cache defaults remain 128 MiB/50k nodes; API pricing uses 512 MiB/500k nodes.
Accounted retained bytes exclude transient arrays, frames and allocator overhead.
100k-position benchmarks still scan inputs and assemble full results: one dirty
node does not imply constant-time request handling. Compare prepared-kernel,
packing, whole-engine, HTTP and browser latencies separately. Reproduce with
`scripts/benchmark_comparison.py`; see the monorepo five-step comparison report.

## Decision graph prototype (0.21.0)

Rust session actors own dirty-ID lookup, scenario contribution deltas, strategy
coefficients and a persistent HiGHS C++ model. Coarse JSON batches copy data at
initialization and changed contributions/columns afterwards. No per-position FFI
calls. Solver variables/RHS use millions internally, outputs/duals use dollars.
Python independently validates aggregate deltas, objective, monthly liquidity/
funding/EVE, horizon CET1 and commercial bounds before guarded publication.
Abort resets staged solver state; successful unchanged layouts reuse it.

Instrument edits hold original OAS and accounting anchors. Editable domains come
from whatif.FIELDS; null restores a saved field. Regulatory base totals remain
static except marked agency L2A and retained horizon income. Supported template
override: spread_bp; subset builds must match full-library outputs. Deposit
spread shifts the starting equilibrium paid rate. Existing deterministic-forward,
time-shift, balance-scaled-DV01, static-RWA and NII-retention approximations apply.
No forecast-conditioned EVE, nonlinear stress/vega graph, dynamic reinvestment,
distributed persistence, hard kernel cancellation or execution integration.

API sessions: four, with stale/30-minute-idle eviction when building another.
Native caps: eight sessions, 200k records, 10k edits, 1024 units, 13 scenarios,
120 months, 128 MiB request, 15-second solve. Cache retention: 512 MiB/session;
transient buffers and native solver memory are additional. JSON is not zero-copy.
DLL SHA-256 is part of context identity. Compare graph-only synthetic throughput
separately from product pricing and complete HTTP/browser latency.
Decision sessions request `include_key_rates=False` from incremental pricing:
only spot, parallel DV01 and NII feed this optimizer. The default remains True
for other callers, preserving full KRD analytics. This demand-based pruning is
separate from language choice and is included in the comparison measurements.


## Pricing cache reuse (0.21.1)

Incremental income reuses live baseline/current-market cashflows when path counts
match, independent of global eviction. MBS income at a different sensitivity path
count still builds its own cashflows. Completed risk legs are released promptly.
Market/model fits, rate/mortgage/deposit paths and auxiliary results use a protected
LRU tier inside the existing total budget: at most min(64 MiB, max_bytes/4) and
min(1024, max_entries/4). Instrument nodes borrow unused space; total bounds and
immutable content keys are unchanged. Oversized shared nodes use ordinary LRU.
`cache.info()` exposes shared occupancy/limits; its byte totals still exclude
transient arrays, native memory and allocator overhead. No financial-model change.


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


## Native public strategy ownership (0.27.1)

When compute_backend="rust", optimize_balance_sheet sends raw unit-library
income/balance/DV01 arrays, template weights, base KPIs and constraints. Rust
owns purchase shifting, regulatory coefficients, LP construction, HiGHS invocation
and financial allocation replay. Python coefficient caches are not consumed.
Python independently checks native results in tests, not in the native production
solve. Null max_total_assets means no cap; invalid grids/shapes fail explicitly.
The optional portfolio-strategy executable accepts the same strategy-library-1
JSON contract without Python. HiGHS is C++; library pricing, calibration, graph
orchestration and daily output integration still have Python ownership. Validation
is coefficient-only, not dynamic ledger/HTM acceptance. See the full remaining
scope in docs/reviews/2026-09-30-rust-lifecycle-migration.md.


## Native market-path ownership (0.27.2)

The built-in Rust branch now owns the complete market-path construction stage:
par-curve bootstrap, quarterly forwards, abcd volatility tables, LMM, forward-vol
features, CC/PS/HPI paths, incentive lag and YoY HPI. Forward parallel stress
path templates are native too. The public Rust MarketContext accepts explicit
calibrated parameters and shared CRN buffers, and makes no Python callbacks.
Python adapters only serialize these stage inputs and convert output storage.

This is still an incomplete full-lifecycle migration: PCA and nonlinear/behavioral
fit controllers, NumPy-compatible CRN generation, multi-scenario risk orchestration,
unit-library pricing, dependency caches and final reporting remain Python-owned.
The bounded market cache remains run-local. Recalibration is explicit; original
OAS and shared random draws remain fixed across revaluation scenarios. The native
curve uses bracketed bisection for the same par residual and bracket as SciPy's
reference bootstrap. Existing model approximations remain unchanged.

Product ABI 2 requires rebuilding scripts/build_native.py and restarting workers.
HiGHS stays in C++ by explicit user instruction; Rust owns its orchestration.
See docs/reviews/2026-09-30-rust-lifecycle-migration.md for remaining requirements.


## Native linear calibration ownership (0.27.3)

Rust now owns the current-coupon OLS design/regression, adjustment-speed estimate
and diagnostics, the PS-spread AR(1) fit and residual volatility, and rate-factor
PCA. Python adapters only extract raw history columns and package the results.
The pinned nalgebra 0.34.2 SVD retains the NumPy minimum-norm cutoff for deficient
rank. First-row factor signs (-,-,+) define the built-in CRN pairing in both
backends; this preserves the existing local reference orientation while removing
arbitrary eigenvector signs. Factor caches distinguish the selected backend.

The product ABI is now 3: rebuild scripts/build_native.py and restart workers.
HiGHS remains C++. Volatility/deposit nonlinear least-squares controllers, seeded
CRN generation, higher-level scenario/dependency/cache orchestration, unit-library
pricing and final reports still need migration. This is not full Rust lifecycle
completion; see docs/reviews/2026-09-30-rust-lifecycle-migration.md.


## Native nonlinear calibration ownership (0.27.4)

Rust owns the bounded TRF controllers for abcd volatility and deposit-rate
calibration, including static deposit initialization, joint asymmetric dynamics,
analytic volatility derivatives, bounded two-point deposit differentiation and
convergence diagnostics. Objectives, starting values, bounds, linear loss,
unit scaling, 1e-8 convergence tolerances and 100*n evaluation budgets match the
Python SciPy reference. The native branch fails on exhausted iterations rather
than returning an unchecked fit. It makes no Python optimizer/model callbacks.
The SciPy-derived controller's BSD attribution is retained in the native crate's
THIRD_PARTY_NOTICES.md. No general-purpose SciPy replacement is exposed.

Product ABI 4 requires scripts/build_native.py and worker restart. Existing
final-output parity gates retain their tolerances. Deposit parameters can be
weakly identified: compare the equilibrium function and dynamic rates as well
as parameter estimates. Native ownership does not resolve identifiability or
constitute real-data model validation. HiGHS remains C++; CRN generation,
global scenario/dependency/cache control, unit-library pricing and final ledger
reporting remain open in docs/reviews/2026-09-30-rust-lifecycle-migration.md.


## Native seeded shared draws (0.27.5)

The Rust backend now constructs common random numbers itself. `SharedDraws`
implements NumPy-compatible integer SeedSequence mixing, PCG64 XSL-RR and the
float64 normal Ziggurat algorithm, including tail/rejection branches. Rate
paths retain ceil(n/2) original paths followed by their antithetic negatives,
truncated to n. Spread/HPI retain the independent seed+101/seed+202 streams.
Never reseed inside scenario revaluation. Python only serializes integer seed
words and output shapes; native mode never calls NumPy RNGs.

Native input admission requires positive dimensions, at most 32768 seed bits
and at most 128M f64 values across retained tapes (1 GiB). The synchronous FFI
also allocates a caller-side output copy, so this is not a 1 GiB process-memory
limit. Oversized requests fail before Python output allocation and are checked
again by Rust. Upstream NumPy/PCG/SeedSequence licenses and source hashes are
retained in the native crate. ABI 5 requires rebuild and worker restart.

Tests compare long tapes and integer carry boundaries against NumPy, retain
odd-path behavior and thread determinism, forbid Python RNG calls and run the
existing final product/risk/ledger parity gates unchanged. On other platforms,
rare log1p tail values may differ by an ulp; the gate permits 2e-15 absolute
roundoff without stream drift. This does not complete global scenario/graph/
cache orchestration, raw-book schedules, unit-library pricing or reporting.
HiGHS stays C++. See docs/reviews/2026-09-30-rust-lifecycle-migration.md.


## Native mortgage risk and forward-stress lifecycles (0.27.6)

The built-in Rust `run_risk` route now makes one call with raw mortgage book
columns, histories, market inputs and immutable tabulated model data. Rust owns
natural-spline static multipliers, original-HPI defaults, price/delay conventions,
CC/PS/PCA/abcd calibration, seeded CRN, base cashflows/OAS, all curve and volatility
bumps, shared-draw revaluation and dollar KRD/DV01/vega aggregation. Supplied OAS
is retained exactly across all scenarios. Python only encodes enums/tables and
formats the returned columns. Custom Python model suites fail explicitly.

`portfolio-mortgage-risk` accepts `mortgage-risk-1` JSON with named fields matching
its typed request structs and emits one ok/error envelope. Input is limited to
64 MiB. Model tables are explicit immutable input. It needs no Python runtime.
The Rust driver executes one scenario and at most 256 positions of base cashflows
at a time, avoiding the previous all-scenario path stack. This is not yet a
unified native worker or deadline/cancellation boundary. Cross-book dependency
resolution remains outside this driver.

The Python bootstrap now sets brentq xtol=2e-15 (formerly the default 2e-12).
A reproduced 5y KRD discrepancy came from curve precision straddling a float32
path boundary and also occurred in the older hybrid route. Tightening the root
resolved it; risk acceptance tolerances are unchanged. Native product ABI 6
requires rebuilding scripts/build_native.py and restarting workers. HiGHS remains
C++. Other product risk/stress drivers, unit libraries, cross-book accounting/KPIs, graph/cache
orchestration and final ledger reports remain open in the migration ledger.

The same raw-input contract now powers native `run_stress`: Rust owns base OAS,
forward checkpoints, horizon/shock scheduling, fixed-OAS revaluation, position
P&L, portfolio aggregates and the forward DV01 profile. `mortgage-stress-1` adds
explicit horizon and shock arrays to the standalone executable. No Python
financial callback or portfolio aggregation executes in this branch. State and
channel multiplier tables are explicit inputs, including user-added categories.

Stress checkpoints/cashflow buffers are admitted in chunks of at most 256 rows
against a 64 MiB scratch estimate, independently of total book size. Result
buffers have a 1 GiB admission ceiling checked before FFI allocation and in Rust.
These are component budgets, not total process memory limits: paths, inputs,
transport copies and formatted output use additional memory. Horizons must be
positive, increasing and inside the simulation; shocks must be finite and unique.
Existing forward valuation conventions and float32 checkpoint quantization remain.

Native mortgage stages now share a process-local immutable LRU cache, capped at
128 MiB of retained key/value accounting and 512 entries. It stores CC/PS/PCA/
curve/abcd calibrations and mortgage market paths, keyed by complete length-tagged
input bytes. Hash collisions cannot alias financial inputs. Only successful
stages publish; concurrent borrowers use immutable Arc values and eviction is
safe recomputation. Clear also prevents earlier in-flight work from repopulating
the cache. Book terms, prepay cashflows, target/OAS solves and final position
results are not cached. Price/notional/category edits therefore reuse market
stages but recompute instrument results. Seed, paths, market/history and relevant
model/grid changes alter stage identity. Component retention excludes active
borrowers and transport copies; it is not a total RSS cap. ABI operation 30 exposes
entries/bytes/hits/misses/evictions and optional clear for diagnostics/testing.
This is mortgage market-stage reuse, not the full cross-book dependency graph.

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

Scheduled loan cashflows also release the repaid share of the allowance on the
payment day, including the final reporting day. Rust and Python emit the same
`repayment_allowance_release` journal event; partial/full payoff regression tests
reconcile allowance to surviving principal.

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

Local0.29.13 exposes `commercial-holding-continuation-1` complete-origin holding windows through the native model analysis contract. Raw prefix and original yield are retained; verified prior-publication opening, cut-started mixed journal, seasoned strategy coefficients and secondary conveyed rights remain separate. See docs/models/commercial-continuation.md.


Local0.29.14 exposes `commercial-holding-journal-1`: sealed complete-origin sources share one existing native daily Sim, with independently replayed prospective opening/postings and retained original credit clocks/yields. Verified prior-publication opening and cut-started mixed/seasoned/secondary adoption remain separate. See docs/models/commercial-holding-journal.md.


Local0.29.15 adds server-scoped modeled prior-opening adoption through `/models/holding-journals`, full independent source/journal replay and exact published-cut matching, with source/object/clock/SQL publication fences. Native financial execution remains complete-origin; observed bank and cut-started mixed/seasoned/secondary acceptance remain separate. See docs/models/commercial-prior-opening.md.


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
