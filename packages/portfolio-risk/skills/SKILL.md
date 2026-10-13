---
name: portfolio-risk-engine
description: Operate the portfolio_risk Python package — a shifted-lognormal LMM Monte Carlo engine for bank balance-sheet portfolio risk across MBS, whole loans, corporates, deposits, CDs, money markets, and hedge overlays. Use this skill whenever the task involves portfolio OAS, KRD/duration/vega risk, NII, EVE/LCR/NSFR/CET1 KPIs, stress testing, strategy units, optimizer runs, MBS/prepay modeling, or any mention of the portfolio_risk package. Also use it before MODIFYING the engine — it documents invariants that must not be broken.
---

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

# portfolio-risk-engine

Published forecasts (0.20.1): the API fetches Fed stress/SEP and Philly/NY Fed
surveys. Preview source coverage before a relative replay on the saved book.
`analytics.forecast.run_forecast_nii` produces conditional NII/runoff only;
never use its conditioned paths for OAS/EVE pricing. See the forecast sections
in the references and repository `docs/forecast-scenarios.md`.

Research data (0.19.1): `core.curve.market_discount_factors_to_par` projects
source DFs to annual model par rates and reports reconstruction errors. This
does not provide an exact sub-year curve or replace volatility/behavioral fits.
See `references/internals.md` and the repository `docs/market-data.md`.

Drive and extend the `portfolio_risk` package: OAS, KRD/vega risk,
forward valuation, 9Q stress capital, NII, regulatory KPIs, strategies,
and optimization for a bank balance sheet.

## Setup and verification (always do this first in a fresh environment)

```bash
pip install -e .            # from the package root (pyproject.toml present)
pytest tests/ -q            # full engine suite must pass before trusting output
python -m portfolio_risk 1000 bench   # throughput probe + projected wall time
```

First kernel call in any process pays numba JIT (~20-40s). The bench and
test paths include warmup; account for it when timing anything yourself.

## The three workflows

All inputs are Polars DataFrames / numpy arrays. Exact column schemas:
read `references/schemas.md` BEFORE constructing any input frame.

### 1. OAS only (price the book)
```python
from portfolio_risk.scenarios import setup, solve_base_oas
models, B, abcd0, sec, tgt, face = setup(port, swap_rates, vol_pts, cc_hist, ps_hist)
oas, px = solve_base_oas(swap_rates, vol_pts, abcd0, B, models, sec, tgt)
# oas, px: per-position arrays (decimals; px per unit balance)
```

### 2. Spot risk (KRDs + vegas)
```python
from portfolio_risk import run_risk
df = run_risk(port, swap_rates, vol_pts, cc_hist, ps_hist)
# adds: oas_bps, model_price, dv01, krd01_{1..30}y ($/bp), vega_{e}x{t} ($/vol-pt)
```

### 3. 9Q stress capital (forward valuation + forward-starting shocks)
```python
from portfolio_risk import run_stress
pos, agg, prof = run_stress(port, swap_rates, vol_pts, cc_hist, ps_hist)
# pos:  long frame, position x 27 monthly horizons x shocks
#       (fwd_value_base, fwd_price_base, fwd_value_shock, stress_pnl in $)
# agg:  portfolio P&L by (horizon_m, shock_bp)
# prof: forward DV01 profile ($/bp per horizon) from the +/-100bp pair
```

CLI equivalents: `python -m portfolio_risk <N> [bench|risk|stress|all]` (writes
`risk_results.parquet` / `stress_results.parquet`; uses demo data).

## Critical rules — violating these produces silently wrong numbers

1. **Demo data is synthetic.** `portfolio_risk.demo` exists so the package
   self-tests. Production runs need real monthly histories (`cc_hist`,
   `ps_hist`), real par swaps, and a real vol surface. If the user hasn't
   supplied them, say so explicitly — do not present demo-fitted output as
   production risk.
2. **MODEL-BLOCK duplication.** The per-month prepay/cashflow model is
   open-coded in BOTH `kernels.engine` and `kernels.stress_engine`
   (deliberately — a shared inlined helper measured 28% slower). Any model
   change must be applied to both marked blocks, then `pytest` rerun:
   `test_zero_shock_invariant` is the drift guard and will fail if the two
   copies diverge.
3. **Numba freezes module constants at first compile per process.** Editing
   `config.py` (or `prepay.py` anchors/LUTs) requires a fresh Python
   process. Special trap: `PREPAY_PARAMS` travels as a runtime vector, BUT
   `BURN_LUT` is built from `PREPAY_PARAMS[3]` (burn_k) at import — changing
   burn_k silently does nothing without re-import.
4. **OAS is held fixed** across all risk and stress revaluations (standard
   spread-constant convention). Never re-solve OAS inside a scenario.
5. **Common random numbers are load-bearing.** Risk/stress differences are
   only low-variance because every scenario reuses one `CRN` object. Never
   reseed or rebuild CRN between the two sides of a central difference.
6. **Units:** rates/spreads as decimals internally; `oas_bps` in bp; KRDs in
   $ per 1bp; vegas in $ per 1 lognormal vol POINT (0.01); prices in % of
   par in output frames, per-unit decimals internally; stress P&L in $.

## Performance model (measured, single 2.1GHz core)

| operation | 10k pos x 128 paths |
|---|---|
| engine pass (A + 27-horizon FV + checkpoints) | ~8.7s |
| stress pass (1 horizon, checkpoint restart) | ~6.8s |
| path build (LMM sim + CC + PS + HPI) | ~0.1s |
| full risk (38 reval) + stress (4 shocks x 27 hz) | ~18 min |

Scales near-linearly with cores (prange over securities). Modern 12-16 core
laptop: ~1-1.5 min for the full pack. Memory: checkpoints are
`S x P x 27 x 2 x 4B` (~276MB at 10k x 128) — scale paths down before
securities up if memory-bound. Speed levers in order: reduce
`N_PATHS_SENS`; cohort-bucket the portfolio before calling (real books
collapse ~5x); raise paths only if KRD noise is observed.

## Accuracy switches (config.py)

- `RATIONAL_SIGMOID=True`: Pade(7,6) logistics; measured max 0.007bp OAS
  vs exact. Set False only to prove equivalence (e.g., model validation).
- `USE_FLOAT32=True`: path-array storage only (scalar math stays f64);
  <~0.5% jitter on 1bp KRDs. Accumulation is always f64.
- `N_PATHS_BASE=512` (OAS solve) / `N_PATHS_SENS=128` (scenarios, CRN).

## Known model limitations (disclose when reporting results)

Deterministic CC vol features (no stochastic vol — SV-LMM needed for vol
dynamics in the CC distribution); point vegas projected onto the 4-param
abcd family (smeared toward neighbors); stylized prepay spline anchors and
S-curve params (fit to loan-level before production); forward shocks are
parallel-only; frozen-weight Rebonato calibration; no payment-delay
adjustment; lognormal-equivalent ATM vol conversion is vega-approximate.

## Modifying or extending the engine

Read `references/internals.md` first — it maps modules, data flow, where
loan-level fits plug in, how to add per-pillar forward shocks or per-point
vol multipliers, and which tests gate each change. Never edit kernels
without rerunning the full test suite.

## Runtime contract update (0.18.0)

Read the v0.18.0 section of `references/internals.md` before running named
scenarios or optimizer plans. Use explicit `RunConfig` for path counts and
horizon, retain base OAS across named scenarios, and treat external funding
budgets and forward-DV01 scaling as disclosed assumptions. Library evaluation
and optimizer replay share purchase-date vectors and the exact L2A cap.
The API publishes revisioned library/KPI pairs atomically; edits require rebuilds.

## Incremental pricing (0.19.0)

Use `analytics.incremental.price_books` with explicit `RunConfig` and a reusable
`DependencyCache` for spot pricing after local edits. The API exposes this as
`POST /run` kind `pricing`. See `references/schemas.md` for spread override units
and `references/internals.md` for dependencies and scope. This endpoint covers
MBS, loans/debt, deposits and CDs; it does not incrementally compute risk, NII,
hedges, money markets or the strategy library. Saved book/base-market inputs
recalibrate; temporary scenario markets/spreads preserve base OAS. Prepay edits
still require restart. Synthetic benchmark results are not production validation.

## Temporary comparisons and native execution (v0.20.0)

Use `analytics.whatif.compare_books(..., cache=DependencyCache(),
assumption_overrides={book: {id: {field: value}}}, include_analytics=True)` for
unsaved instrument experiments. Baseline OAS and effective accounting yields
are held; request `calibration_mode="recalibrate"` explicitly to refit revised
contracts. Neither mode persists input changes. Domain fields/bounds live in
`analytics.whatif.FIELDS`. Global frozen prepay parameters require restart.

The API exposes `GET /pricing/assumptions` and `POST /run` with `kind="whatif"`,
`expected_revision`, `assumption_overrides`, `spread_overrides_bp`,
`include_analytics`, `calibration_mode` and `backend` (production: rust only;
NumPy/Numba reducers are deprecated application options).
The Instrument What-if panel provides the corresponding workflow. Full-scope
KPIs include money markets and hedges; spot-value cards cover only the five
supported pricing books. KRD01 uses 1bp bumps, parallel DV01 25bp; NII retains
the engine's static effective-yield approximation and regulatory ratios remain
stylized. Check `references/internals.md` for native ownership and cache limits.

## Rust decision sessions (0.21.0)

The optional `strategy.decision.DecisionSession` executes temporary instrument
and template assumptions through a native dependency/aggregate/solver session.
Build `packages/portfolio-decision-native` first. Original product models remain
Python/Numba. Existing pricing/Strategy Lab routes do not require this library.
Use explicit versions; close sessions; rebuild after saved market/book/runtime
edits. Only independently replayed optimal allocations publish. An infeasible
outcome contains no allocation. Never describe the prototype as a full Rust
product-pricer rewrite or as production/distributed validation. See references.


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

Native compute v0.27: read the built-in product coverage section in
`references/internals.md` before selecting `compute_backend="rust"`. This is a
historical hybrid checkpoint, superseded by the 0.29.4 production contract above. Rust is now required for production calculation. Rebuild/restart native libraries after upgrades.


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

Use `consumer-credit-accounting-1` for explicit physical installment source and
reviewed original-yield/default accounting analysis. The separate
`product-mixed-consumer-credit-1` route owns acquisitions and complete credit
events in one dated daily GL. Durable publication requires independent raw-source
and ordered persisted-journal verification, including zero participation tails.
Saved-book adoption, modeled FTP, empirical and complete joint scale gates remain
open. Read [native schemas](references/schemas.md) and
[ownership details](references/internals.md) for limits, gross recovery/cost,
allowance policy, full-tail clocks and supported capital sources.

## Explicit acquired card and tagged credit owners (local 0.29.7)

`card-credit-accounting-1` retains supplied card runoff, original-yield income,
reviewed credit/recovery policies and complete interest/commitment rights.
`product-mixed-product-credit-1` selects tagged installment/card owners in the
same dated daily GL. Public selection and matching rebuilt artifacts are explicit;
independent raw and persisted source/journal verification gate publication.
Read [schemas](references/schemas.md) and [ownership](references/internals.md).
Broader grace/fee/draw/basis policies, saved-book, empirical and scale gates remain.

## Explicit card interest-basis ownership (local 0.29.7)

`card-credit-accounting-2` explicitly retains gross unpaid card interest and
separate live/default signed carrying adjustments. Original effective interest
and reviewed basis realization remain separate income. The `card_v2` raw tag in
`product-mixed-product-credit-1` selects the same native source and its complete
atomic actions in the existing daily Sim and journal. Rebuild matching native
artifacts and select the schema explicitly. Read the schemas and ownership
references before use. Full reviewed policy, empirical and adoption gates remain.

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
