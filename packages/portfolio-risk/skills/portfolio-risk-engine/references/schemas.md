# portfolio_risk input/output schemas

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

## Published forecast replay (v0.20.1)

`POST /forecasts/preview` and `POST /forecasts/run` accept `snapshot_id`,
`scenario` (baseline/adverse/median), `start_period` (ISO first of month),
`horizon_months` (3..120), `alignment: "relative_replay"`, and
`expected_revision`. Source IDs: fed_stress, fed_sep, philly_spf, nyfed_sme.
Preview JSON exposes monthly driver targets, coverage, warnings and unused
variables. Run uses the existing asynchronous job/Arrow envelope and returns
monthly/base/delta NII, summaries, runoff, drivers and snapshot provenance.
Rates in driver targets are DECIMAL; HPI is a ratio rebased to 1. Snapshot
observations retain their native units. No probability, EVE or capital result
is implied. Active books/markets are never replaced by this route.

## Incremental pricing (v0.19.0)

`POST /run`: `{"kind":"pricing","books":["loans"],"spread_overrides_bp":{"loans":{"CML0000":25}}}`.
Use actual IDs from the book. Overrides are additive basis points, bounded to
+/-2000 bp, and apply only to selected existing IDs. They are rejected for other
run kinds. Optional `scenario` names the usual curve/vol/spread scenario.
Omit books to price MBS, loans, debt, deposits and CDs; explicit unsupported
money-market scope is rejected. Empty selected books produce empty frames.

Engine: `price_books(books, asof=..., swap_rates=..., vol_pts=..., cache=...,
config=RunConfig(...), seed=7, mbs_hists=..., dep_hist=...,
scenario_market=(rates, vols), spread_shift=0.0, spread_overrides_bp=...)`.
`spread_shift` is DECIMAL and excludes deposits; explicit instrument overrides
are BP and include deposits. Saved contract/target/base-market changes recalibrate
base OAS; scenario markets and spreads hold that OAS fixed. MBS uses n_paths_base,
other books n_paths. Histories are required for their respective products.

Result `positions[book]` frames contain id, base_oas_bp, applied_oas_bp,
model_price (% par), notional ($), market_value ($). `totals` are per selected book;
`scope_net_value` is MBS+loans minus debt+CDs+deposits, not complete EVE.
`graph` reports reused/computed nodes and batches; `cache` reports resident
accounted bytes and entries; model/backend identities are included. API results
also include the queued input revision. The UI does not yet expose this run kind.

All frames are Polars. Rates, spreads, coupons, vols are DECIMALS
(0.0408 = 4.08%). Prices are % of par (98.5) in frames.

## Portfolio frame (required by run_risk / run_stress / setup)

| column | type | meaning |
|---|---|---|
| cusip | str | identifier |
| current_face | f64 | current balance, $ (drives all $ risk) |
| factor | f64 | pool factor (current/original balance) — CLTV back-out |
| wac | f64 | gross weighted-avg coupon, decimal |
| net_coupon | f64 | passthrough coupon paid to investor, decimal |
| wam | f64 | remaining term, months |
| age | f64 | loan age, months |
| oltv | f64 | original LTV, decimal (0.80) |
| fico | f64 | weighted-avg FICO |
| avg_loan_size | f64 | $ |
| state | str | dominant state code (unknown -> multiplier 1.0) |
| channel | str | "R"/"B"/"C" retail/broker/correspondent |
| price | f64 | market price, % of par — OAS solve target |
| hpi_orig_ratio | f64 | OPTIONAL: H_settle/H_orig; defaults to (1+HPI_MU)^(age/12) |

## Market inputs

- `swap_rates`: np.ndarray (10,) par swap rates at tenors
  [1,2,3,4,5,7,10,15,20,30]y, annual fixed leg, decimals.
- `vol_pts`: np.ndarray (9,3) rows = (expiry_y, tenor_y, ATM lognormal vol).
  Default grid: expiries {1,3,5} x tenors {2,5,10}.

## Model-fit histories (monthly, oldest first)

- `cc_hist` columns: `cc` (secondary current coupon), `s2 s5 s10 s30`
  (par swap rates), `v0..v5` (six ATM swaption vols matching
  config.CC_VOL_POINTS order: 1x10, 2x10, 5x10, 1x5, 3x7, 5x5).
- `ps_hist` columns: `ps` (primary minus secondary spread, decimal).

Fitters print diagnostics: CC lambda + fair-value R2, PS kappa/theta/sigma.
Sanity: lambda in (0.2, 0.6) and R2 > 0.9 typical; PS kappa O(1-5)/yr.

## Outputs

### run_risk -> portfolio frame plus:
| column | units |
|---|---|
| oas_bps | bp |
| model_price | % of par (reprices `price` to 1e-8) |
| dv01 | $ per 1bp parallel (sum of KRDs) |
| krd01_1y ... krd01_30y | $ per 1bp at that pillar (10 cols) |
| vega_1x2 ... vega_5x10 | $ per 1 vol point (9 cols) |

### run_stress -> (pos, agg, prof)
- `pos`: cusip, horizon_m (1..27), shock_bp, fwd_value_base ($),
  fwd_price_base (% of par on forward balance), fwd_value_shock ($),
  stress_pnl ($).
- `agg`: horizon_m, shock_bp, pnl_$, base_mv_$.
- `prof`: horizon_m, fwd_dv01_$ (present only if shocks include +/-100).

Sign conventions: positive KRD = long duration (gains when rates fall);
up-shocks produce negative stress_pnl for a long book.

## v0.18.0 run and allocation contract

`RunConfig(n_paths=128, n_paths_base=512, horizon=27)` is scoped with
`run_context(config)`. `n_paths_base` controls MBS calibration and MBS scenario
spot marks; other products calibrate at `n_paths`. Engine horizons are 1..359;
the API accepts 3..120. Existing no-context callers keep the module defaults.

Allocations: `{template, purchase_m, notional}`; template must exist, purchase_m
is a nonnegative integer, notional is finite and nonnegative. Purchases at/after
the horizon contribute zero. `evaluate_strategy` adds `kpi_path`, `funding_gap`,
`horizon_months`, `dv01_method`, and `cet1_horizon_pct` (the legacy `cet1_q9_pct`
alias remains). Current KPI scalars describe month zero, not the purchase date.

The optimizer returns `validated`, `horizon_months`, `cash_budget_$`, and total
`worst_case_nii_$` including base-book earnings. `cash_budget` is additional
committed funding outside the base book, not reserves already counted in HQLA.

## v0.20.0 temporary instrument assumptions

`assumption_overrides` is `{book: {stable_id: {field: numeric_value}}}`.
Use MBS `cusip` or other products' `id`; all IDs must exist in selected books.
Rate/coupon fields are decimals, spread overrides are basis points, ages/terms
are months. The authoritative allowed fields and bounds are
`portfolio_risk.analytics.whatif.FIELDS` / API `GET /pricing/assumptions`.
MBS allows coupon/term/age/LTV/factor/FICO/size/HPI ratio; loans/debt allow coupon,
cap/floor and call threshold; CDs allow rate/penalty/withdrawal multiplier/call
threshold; deposits allow rate/age/size/servicing and four attrition parameters.
Unknown fields, nonfinite values, out-of-domain values and fractional WAM fail.
Targets, notionals, IDs, dates and global frozen model constants are not temporary
assumption fields. Null virtual attrition fields retain per-segment defaults.

Comparison result: `baseline`, `revised`, `comparison` (book frames),
`net_value_change`, `calibration_mode`; API adds input `revision`.
Position outputs include original/revised price and value delta, base/applied OAS,
notional and market value. Analytics adds `dv01`, ten `krd01_{tenor}y` fields
and signed `nii_total`; report-level `nii` contains monthly income/expense,
runoff and total. `kpis` includes scope and auxiliary-book inclusion flags.
Zero-denominator ratios serialize to null, never a fabricated zero.

## Decision session API (0.21.0)

`POST /decision/sessions`: `{expected_revision, options}` where options has the
existing optimizer constraints and scenario names; returns a background job.
Job result contains session_id, revision, version, context/native identity,
allocation, scenario replay, binding constraints, changed IDs/templates, timing
and actual pricing work. Infeasible results omit numerical allocation outcomes.

`POST /decision/sessions/{id}/update`: `{expected_revision, version, edits,
templates, constraints?}`. Edits use keys `book:stable_id`, e.g.
`{"loans:CML0000":{"coupon_or_spread":0.06}}`; null removes a field override.
Templates use `{"cml_fixed_5y":{"spread_bp":250}}`. Constraints are a complete
replacement of optimizer options, not a patch; scenario changes require rebuild.
The result advances version only after independent replay and revision checks.

`POST /decision/sessions/{id}/eval`: `{expected_revision, version, allocation}`
with `{template,purchase_m,notional}` rows on the session purchase grid. Stored
coefficients only; no engines or optimizer. Manual replay is exploratory and
does not certify constraint feasibility. `DELETE /decision/sessions/{id}` closes
the session. Handles/IDs are process-local, not tenant authentication.


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


Cohort audit endpoint: POST /cohorts/builds/{job}/audit with products, optional
shocks (distinct, include 0, +/-500 bp max), tolerances[product][metric] containing
absolute dollars and relative decimals, and timeout <=3600 seconds. Results contain
partitioned errors, suggestions, loan_risk and loan_cashflows tables. Monthly flows
are base-only. Build accepts previous_job for cross-vintage reconciliation;
publish accepts mode=replace_books (default) or replace_tape. Explicit book_yield
may be supplied as a mortgage average for historical-yield publication.


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

Source-linked FTP requires an explicit repricing tenor for non-maturity deposit
cashflows (`FTP_RESET_REQUIRED`). Original deposit IDs are preserved literally;
only generated mortgage `HL-` wrappers are removed when recovering source IDs.

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
