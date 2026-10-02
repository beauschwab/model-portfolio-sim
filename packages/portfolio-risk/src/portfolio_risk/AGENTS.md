# AGENTS.md — src/portfolio_risk

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

Version 0.28.0 provides a native built-in calculation runtime: raw calibration,
CRN, scenario/edit dependency graph, all existing product drivers, accounting,
forecast controllers, KPIs, units, strategy orchestration, and daily ledger/reports.
HiGHS stays C++; Python remains an adapter and independent reference/verifier.
Historical version sections below record intermediate states. The latest ownership
contract and acceptance evidence are in docs/reviews/2026-10-01-native-owned-workflow.md.

## Native strategy ownership (0.27.1)

The Rust branch of `optimize_balance_sheet` serializes raw library arrays and
base KPIs into `strategy-library-1`; no Python coefficient construction, SciPy
solve or Python financial replay runs in that branch. Native `library.rs` owns
purchase shifts, balance-scaled DV01 and regulatory vectors; `solver.rs` owns
rows, HiGHS and independent financial replay. Python remains the test oracle.
Null asset cap means no cap. Validation remains coefficient-only, not dynamic
ledger/HTM acceptance. Native commercial labels use indexed actor identifiers.
Library pricing, graph orchestration and calibration are still Python; do not
describe this boundary as full Rust lifecycle completion. Track remaining scope
in `docs/reviews/2026-09-30-rust-lifecycle-migration.md`.

The model-assumptions ledger and the extension recipes. Every modeling
choice that affects a number is listed here with its location, so model
validation and future edits start from one place. Read the root AGENTS.md
first for global invariants.

## Package layout (v0.17 reorg)
```
core/      config curve vol lmm conventions pricing kernels scenarios interfaces
models/    models (CC/PS/HPI) prepay
products/  corp deposits cds mm hedges
analytics/ risk stress accounting kpis
strategy/  strategies unitlib optimizer
demo.py __main__.py
```
Old flat import paths (portfolio_risk.kernels, portfolio_risk.corp, ...) remain valid
via sys.modules aliases in __init__ -- tests, apps/api, and the skill use
them unchanged. New code should import from the layered paths. The alias
table is the back-compat contract: removing it is a breaking change.

## Module map

| module | owns | key entry points |
|---|---|---|
| config | constants, dtype/sigmoid switches, stress grid | — |
| curve | par-swap bootstrap | bootstrap_curve, forwards_from_dfs |
| vol | loadings, abcd, calibration, vol features | calibrate_abcd |
| lmm | shifted-lognormal simulation | simulate_rates |
| models | CC / PS / HPI fitters + paths, registered classes | TrendingCC, OUSpread, RateLinkedHPI |
| prepay | spline anchors, LUTs, multipliers (DATA) | static_multipliers |
| interfaces | protocols, registry, ModelSuite | ModelSuite.default |
| kernels | MBS fast engines + generic factory + scalar helpers | engine, stress_engine, make_generic_engine |
| pricing | A-matrix PV / OAS Newton (monthly grid) | pv_from_A, solve_oas_from_A |
| scenarios | CRN, path builders, shock templates, setup | build_paths, build_rate_paths, shocked_paths |
| risk / stress | MBS KRDs+vegas / 9Q stress pack | run_risk, run_stress |
| conventions | day counts, US calendar, BDC, schedules, MBS delay | year_fraction, Calendar, gen_schedule |
| corp | corporates: deck, exact-time engine, risk | CorpDeck, run_corp_risk, corp_pv/_solve_oas |
| deposits | NMD: rate model, attrition, engines, risk, stress | run_deposit_risk, run_deposit_stress |
| cds | CDs: deck (duck-types corp pricing), engine, risk | CDDeck, run_cd_risk |
| demo | synthetic markets/histories/books | demo_* |

## Incremental spot pricing (v0.19.0)

`analytics.incremental.price_books` owns the pricing dependency graph; the API
only queues it against a snapshot. `core.dependency` resolves content-addressed
nodes and batches missing instruments by product. `core.batch` defines immutable
CSR path-sum/time/offset buffers and a versioned discount-backend protocol.
There is no Rust runtime dependency. Scope: MBS, corp loans/debt, CDs and NMDs;
no hedge/MM, risk, NII or strategy-library incremental execution yet.

Keys must include model/backend version, market/history content, valuation date,
seed, path count, applicable product assumptions and instrument terms. Price
targets belong to calibration keys; notionals only scale output. Adding any
cashflow input requires auditing the key exclusions in `cashflows`.
Dependency nodes are immutable and bounded; never key by mutable global revision
alone, row order, object identity or instrument ID alone. Eviction is safe
recomputation. Do not materialize path x instrument x time tensors.

Saved books and base markets establish calibration inputs. Temporary scenario
markets and additive instrument spread overrides preserve unshocked base OAS.
Global scenario spread excludes NMDs; explicit NMD instrument spread overrides
are allowed. MBS base/scenario pricing uses n_paths_base, other books n_paths.
One CRN per path count per evaluation is shared across base/scenario path builds.
Per-row solver convergence is frozen so calibration does not depend on neighbors
in the batch. Model constants/prepay LUT edits still require process restart.

## Model assumptions ledger

**Published forecast income (v0.20.1, analytics.forecast).** Explicit relative
replay of source months onto the saved book's monthly projection. Source units
and averaging/end-period conventions are preserved by ingestion. Quarterly
average rates repeat monthly; endpoint rates interpolate; HPI log-interpolates
from a mandatory preceding anchor. Flat tails extend through remaining-life
cashflows and are disclosed. Treasury/policy-to-model basis is assumed zero.
Multiple source tenors interpolate with flat ends; policy-only paths retain
initial model tenor slope. Shifted-rate deviations are rescaled to source means
with one shared CRN object. Missing mortgage/HPI inputs retain base model paths.
These conditional paths are NOT arbitrage-free pricing paths: only NII/runoff
is supported, never OAS/EVE or derivative valuation. Base effective yields and
deposit opening rates are held; floaters add contractual income changes to the
base effective-yield roll (approximation). No macro-to-default/capital mapping.
`accounting.run_balance_sheet_nii` exposes keyword-only anchors/plan/CRN for this
route; existing callers retain their behavior. See `docs/forecast-scenarios.md`.

**Curve (curve.py).** Annual fixed leg par swaps at 10 pillars; sequential
brentq on log-DF; log-linear DF interp; flat-zero extrapolation past 30y
to the 40.25y forward grid.

**Research DF adapter (v0.19.1).** `market_discount_factors_to_par` accepts
ACT/365F times from source valuation date and positive DFs normalized at zero.
It derives annual-unit-accrual model par rates and reports sparse reconstruction
error over 0.25–30y. It does not import an exact full curve: missing sub-year
pillars can cause material short-end error. Preserve diagnostic warnings and
keep observed/derived/assumed provenance distinct. This is not model calibration.

**Rates (lmm.py, vol.py).** Shifted lognormal, shift = 2% (rates floored
at −2%; no skew beyond the shift). Spot measure, monthly log-Euler,
quarterly forwards. 3-factor PCA of exp-decay correlation (beta = 0.10).
Rebonato abcd time-homogeneous instantaneous vol, calibrated by least
squares to 9 ATM points using frozen-t=0-weight approximation; shifted →
lognormal ATM conversion is vega-equivalent (≈, not exact). Point vegas
are PROJECTED onto the 4-param abcd family → smeared toward neighbors;
per-point φ multipliers are the fix (risk.py vol loop is the only caller).

**Current coupon (models.TrendingCC).** Partial adjustment to an OLS fair
value on [s2, s5, s10, s30, six swaption vols, 1]. Vol features are
DETERMINISTIC time paths (a deterministic-vol LMM has no stochastic
implied vol — SV-LMM required for vol-feature dynamics). 2-month
incentive lag. SOFR-vs-mortgage-index basis absorbed in the intercept.

**PS spread (models.OUSpread).** OU via AR(1), floored at 0, shocks
independent of rate factors.

**HPI (models.RateLinkedHPI).** One-factor lognormal; drift = mu +
beta·(s10 − s10_0) with the rate deviation clipped ±5% and log-HPI
clipped ±3 (tail guards). No geographic dispersion beyond state mult.

**Prepay (kernels MODEL-BLOCK + prepay.py).** Refi = S-curve in
(WAC − primary) × burnout × CLTV spline × static mult. Burnout is
INCENTIVE-ACCUMULATION (multiplicative exp decay in cumulative ITM
incentive), not pool-factor based. Turnover = base × 30m seasoning ramp ×
monthly seasonality × YoY-HPA kicker (floored 0.3) × rate lock-in
sigmoid. CLTV migrates by pool factor amortization ÷ HPI (origination
appreciation proxied from age if `hpi_orig_ratio` absent). FICO / size /
state / channel are STATIC per-security multipliers. All anchors
stylized — fit to loan level. Payment delay: `pay_delay_days` column →
discounting shift at the OAS layer (delay discounted at OAS, not
pathwise short — bp-level residual).

**Corporates (corp.py).** Exact-time discounting: within pay month m,
df(t) = df[p,m−1]/(1+short[p,m]·(t−m/12)); OAS per period at exact pay
times (CSR Acsr). Floater fixings interpolate the 3m simulated rate to
the exact fixing date; index TENOR is always the front 3m forward
regardless of accrual frequency. "annuity" amortization = linear
principal. Exercise is RULE-BASED (call when swap5 < coupon − thr; put
mirrored) — NOT option-exact; deep-ITM callable OAS biased rich; the
EXERCISE-BLOCK is the LSMC seam. Coupon amounts day-count exact on
adjusted dates.

**Deposits (deposits.py).** Rate model: logistic long-run beta to fed
funds (proxied by simulated 3m rate; basis in intercept) + asymmetric ECM.
Fit = JOINT NLS over the simulated recursion — never static levels
(measured b_max 0.22 vs true 0.60); fitter WARNS when the history never
visits the logistic plateau (high-rate beta = extrapolation). Attrition =
segment base × age curve (young churns, seasoned floor) × size mult ×
opportunity-gap S-curve × positive-12m-rate-velocity accelerator, capped
0.5/mo. Liability = PV(interest + servicing + runoff + terminal at T−1)
on the monthly grid; widened OAS bracket to −15% (franchise premia).
9Q stress shocks are EXACT re-runs of the rate recursion on the shifted
short path (no linearized template); velocity recomputes → shock-induced
flight captured. EVE sign: eve_pnl = −Δliability.

**CDs (cds.py).** Securities construct: schedules via conventions,
exact-time discounting via corp duck-typing. Withdrawal put: hazard
S-curve in (short − rate − penalty/remaining_term); bank pays principal
minus forfeited interest. Issuer call = corp rule (brokered). Channel
defaults: retail = ew on/no call, brokered = ew off/call on. No rollover
modeling (existing book to contractual maturity). Callable CDs can show
locally NEGATIVE dv01 near the exercise boundary — correct economics.

**Numerics (config, kernels).** Padé(7,6) rational logistics (max
0.007bp OAS vs exact exp — the (3,2) version measured 10.9bp at the
wings; do not downgrade). SMM/burnout LUTs (interp-exact). float32 is
STORAGE only (scalar math f64; <0.5% KRD jitter). Paths: 512 base /
128 sensitivity (CRN makes differences stable). Checkpoints:
S×P×H f32 (×2 for MBS bal+burnout, ×1 for deposits).

## How to swap a quant model component

1. **CC / PS / HPI:** implement the protocol in `interfaces.py`
   (`fit`/`paths`, plus `shock_response`/`shock_multiplier` for CC/HPI —
   REQUIRED or the 9Q stress templates silently misprice), decorate with
   `@register(kind, name)`, assemble `ModelSuite(cc=..., ps=..., hpi=...)`
   and pass `suite=` into build_paths / run_risk / run_stress.
2. **Prepay:** write an `@njit(inline="always")` step function with the
   exact signature documented in `interfaces.py`; set
   `suite.prepay_step`. It compiles through `kernels.make_generic_engine`
   at a MEASURED ~25-30% penalty (tuple boundary defeats LLVM register
   allocation). `run_stress` REJECTS custom prepay until the model is
   promoted into both MODEL-BLOCKs and the invariant test extended —
   that guard prevents base/stress kernel inconsistency.
3. **Deposit rate / attrition:** rate model via the `deposit_rate`
   registry kind (must expose fit/equilibrium/paths); attrition anchors
   are data in `deposits.py` (SEGMENTS / AGE_ / SIZE_ / CD_EW_PARAMS) —
   replace arrays, no code.
4. **Exercise (corp/CD):** replace the marked EXERCISE-BLOCK with an LSMC
   continuation rule; keep the rule-based version behind a flag for
   regression comparison.

After ANY swap: rerun pytest; if the component feeds stress, add a
zero-magnitude-shock invariant test in the established pattern.

## How to add a new product (recipe proven 4×: MBS→corp→deposits→CDs)

1. **Deck class**: parse a Polars contract/cohort frame into flat numpy
   (CSR offsets for ragged schedules). Schedule-driven products go
   through `conventions.gen_schedule` with exact pay times; behavioral
   monthly products use the grid directly.
2. **njit engine**: prange over positions, inner paths × periods/months.
   Emit `A[s,t]` (monthly grid) or `Acsr[j]` (exact-time) — discounted
   cashflow path-sums ONLY; OAS never enters the kernel except via the
   FV suffix machinery.
3. **Pricing**: reuse. Monthly → `pv_from_A`/`solve_oas_from_A`;
   exact-time → expose `per_off/t_pay/tgt/n` and duck-type
   `corp_pv`/`corp_solve_oas` (CDs prove this works).
4. **Risk driver**: `build_rate_paths` (or `build_paths` if mortgage
   models needed) + the standard fixed-OAS CRN loop (copy run_corp_risk).
5. **Stress (optional)**: forward-value suffix + balance checkpoints in
   the engine; dedicated stress kernel restarting at h; zero-shock
   invariant test is MANDATORY with the duplicated month block.
6. **Tests** (minimum): OAS roundtrip; cross-engine consistency against
   an existing engine on a degenerate contract (options off); option /
   feature sign tests in both directions; fit-recovery on a known
   generator if a fitter ships.
7. **Ship**: demo data, `__init__` exports, README section, skill
   schemas/internals update, version bump, repackage skill + zip.

**Accounting/NII (accounting.py, v0.9).** Engines emit undiscounted
expected interest/principal appended LAST in return tuples (existing
positional unpacks remain valid; new code uses `*_`). Book yield = static
level-yield IRR on time-0 expected cashflows (no retrospective ASC 310-20
recalc as prepays deviate -- production refinement). CSR interest smeared
acc_m->pay_m for monthly accrual; principal at pay month. Deposits booked
at rate paid; servicing excluded (noninterest). model_balance_sheet is
WFC-1Q26-proportional and SYNTHETIC; excluded categories make model NIM
incomparable to the reported 2.47% headline.

**Money market / NIM reconciliation (mm.py, v0.10).** Spread-to-short
floaters for IEDB/resale/trading/repo/ST/trading-liab -- no optionality,
constant balances, near-zero duration BY DESIGN (don't add KRDs).
`book_yield` column on MBS/corp frames switches accounting to
amortized-cost basis (holder's historical yield vs market-implied IRR).
Reconciliation ladder vs WFC 1Q26 reported 2.47% NIM: market basis 4.24%
-> amortized cost 3.42% (-82bp basis) -> + markets book 2.94% (-48bp
dilution); residual ~47bp = synthetic deposit/CD costs below the actual
1.43% all-in. Decomposition replicates the reported 2.47% exactly from
the filing's average-balance table -- keep that script logic if the
quarter rolls.

**KPIs (kpis.py, v0.11).** EVE = MV(A) - MV(L) at solved model prices;
EQUALS the demo equity plug because model_balance_sheet balances the cut
through deposit sizing (without the plug, EVE is an artifact of excluded
categories). Parallel dv01s by +/-25bp full revaluation, shared CRN,
base OAS fixed. Delta-EVE is FIRST-ORDER; convexity belongs to the 9Q
stress pack. IRRBB outlier flag at 15% EVE -- fires on the demo book
because NO swap hedge overlay is modeled (the natural next product).
LCR/NSFR/RWA weight tables are module data, STYLIZED -- the calibration
seam for internal 12 CFR 249 / NSFR / standardized mappings; CD & LTD
legs use real deck maturities, deposits use segment runoffs. Capital
path: retained = NII x NI_TO_NII (0.43, filing-calibrated to carry
provisions/opex/fees) x (1 - payout); PAYOUT=0.45 excludes buybacks
(WFC's actual share count fell 6% YoY -- raise it to model that). RWA
static; density add-on calibrated to the filing's 59.6%.

**Hedges (hedges.py, v0.12).** Swaps = two CorpDecks (fixed bond minus
par floater; principal exchange cancels) -- the duck-typing recipe's 5th
use, zero kernel code. Swaptions = MC on the emitted par-rate paths
(tenors {2,5,10,30} ONLY) with the cash-settled annuity at the realized
rate -- approximation to physical, disclosed; payer-receiver parity is a
path identity and is tested exactly. ASC 815: designation column drives
accounting (fvh -> earnings + basis adjustment; cfh -> AOCI, EXCLUDED
from CET1 per 12 CFR 217.22(b); economic -> earnings); all designations
hit EVE identically. Hedge carry (net settlements, smeared) books into
NII. Validation arc: demo hedge book (~$510B full-scale net pay-fixed)
takes Delta-EVE +200bp from -27.2% (outlier) to -12.5% (clear) --
test_hedge_book_cuts_irrbb_outlier gates the dv01 sign and size.

**Strategies (strategies.py, v0.13).** Forward-starting at-market
purchase/origination programs: coupons fix PER-PATH at the purchase
month's simulated reference (short or emitted 2/5/10/30y par swaps) +
spread, price = par -> zero purchase MtM by construction; the program
contributes carry, balance, and forward dv01 only. Reinvestment programs
size off the NII framework's modeled runoff vectors
(run_balance_sheet_nii(...)["runoff_vectors"]) -- principal from the
ACTUAL behavioral engines, not assumptions. Simplification seams: "cpr"
cohorts amortize at constant CPR (promote by routing cohorts through the
MBS engine); fwd dv01 is closed-form annuity duration (first-order).
NII increments add to the base monthly frame by simple addition -- same
paths, same CRN, so the sum is internally consistent.

**Unit library (unitlib.py, v0.14).** Hypothetical new origination of all
product types runs through the SAME engines as the backbook -- batched
into one portfolio frame per product per curve bump, shared CRN paths,
ALL behavioral/option models live (prepay, attrition, withdrawal,
exercise). Outputs are per-unit and linear in notional, so
evaluate_strategy is a time-shifted dot product: ~300us per full strategy
incl. closed-form recalc of dEVE/duration gap/LCR/NSFR/CET1 against
stored base KPI components. Disclosed approximations: deterministic
forward coupon fixing per purchase-month grid point (per-path fixing
lives in strategies.py), t0-evaluation time-shifted to h (valid under
time-homogeneous abcd vol), linear h-interpolation between grid points.
Measured: 35 units in 17s one-time, 336us/eval, linearity exact.

**Batched risk (kernels.batched_pv_engine, v0.15).** run_risk now builds
all 38 bumped path sets (shared CRN Z), stacks them along the path axis
with scenario ids, and prices in ONE kernel launch -> PV[38, S]; KRDs and
vegas are row differences. MEASURED single-core: parity with the
sequential loop (0.92x, 500 pools) -- the win is multi-core (one parallel
region vs 38 launches each paying ramp-up + serial pv segments);
re-measure on target hardware before quoting speedups. The kernel is the
THIRD MODEL-BLOCK copy, gated by test_batched_pv_matches_engine (1e-10 vs
pv_from_A) -- that gate caught two real init divergences on first run
(ofh used * not /, wam missing int()), which is exactly why the pattern
exists. Custom-prepay suites fall back to _run_risk_sequential. Threads:
API settings n_threads (0 = all cores) -> numba.set_num_threads per job.

**Optimizer (optimizer.py, v0.16).** Robust balance-sheet LP over the
unit-library allocation space: maximin worst-case 27m NII (epigraph)
s.t. absolute ratio floors (LCR/NSFR/CET1/EVE-limit), commercial
business-plan rows, and EVERY constraint replicated per market scenario
(each scenario = its own unit library + base KPIs -- behavioral models
live per scenario). HiGHS via scipy; MEASURED 11ms for 3 scenarios x 35
units. Duals are the deliverable: shadow_price on each binding row =
marginal worst-case NII per unit of constraint (the price of liquidity /
the cost of the loan mandate). Infeasible solves return the row labels
-- "the plan cannot hold LCR in the bear steepener" is the answer, not
an error. Linearizations disclosed in the module docstring (static RWA
add-on, L2A cap at base mix, NII-retention-only CET1 row).


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

## v0.20.0: incremental comparison invariants

- `analytics.whatif.apply_overrides` is the sole domain whitelist for temporary
  numeric instrument assumptions. Never silently accept frozen prepay constants,
  IDs, target prices or unsupported fields. Saved frames are immutable.
- `compare_books` holds baseline OAS and accounting yields for temporary edits;
  explicit recalibration creates new comparison calibration without persistence.
- `price_books(..., include_analytics=True)` reuses per-instrument cashflows for
  spot, 25bp parallel DV01, 1bp KRD01 pillars and NII. Curve bumps hold current
  market volatility calibration and share CRN. Named scenarios still hold base OAS.
- Keys include numerical/model identity, histories, date, market, seed/paths,
  resolved instrument/default assumptions and upstream identity. Auxiliary
  money-market/hedge nodes include valuation date. Added optional HPI columns
  must not invalidate untouched instruments.
- Effective-yield solves freeze converged rows so regrouping cache misses cannot
  change yields. Empty-scope ratios return unavailable values for zero denominators.
- `core.native` implements optional fused Numba and Rust CSR discounting. Inputs
  are immutable path-summed discounted cashflows; OAS is decimal continuous,
  times are years, output is PV per unit original principal. Never double-discount.
  ABI v1 borrows synchronous buffers, retains no pointer and reports errors without
  fallback. Rust binary identity participates in mark keys.
- The API cache uses 512 MiB / 500,000 nodes; engine default remains 128 MiB /
  50,000 nodes. These are retained-node accounting limits, not process RSS limits.
  Strategy evaluation remains independent of this graph and calls no pricers.

## Decision execution prototype (v0.21.0)

`strategy.decision` is the product-batch bridge and independent replay gate.
`portfolio-decision-native` owns versioned instrument indexes, per-scenario
contributions, portfolio aggregate deltas, unit columns and persistent HiGHS LP
state. Each native session is an actor; its solver never crosses threads.
Plan → stage → replay → guarded publish is transactional. Failed validation,
stale saved-input revision or solver failure must leave published state intact.

Instrument patches are restricted to `analytics.whatif.FIELDS`; null removes a
temporary override. Original OAS/accounting anchors remain fixed. Notional,
price target, maturity, segmentation and market changes require a new snapshot.
The incremental contribution vector is signed MV, signed parallel DV01, signed
horizon NII, uncapped agency L2A MV, asset MV. Regulatory base quantities other
than L2A/CET1 stay fixed under these allowed edits; retained income updates final
CET1. Full-repricing parity tests gate this assumption. Native L2A cap is 40%,
matching `kpis.L2_CAP`; changing regulatory conventions requires both sides and
the parity tests. New-business template spreads rebuild only that template;
deposit spreads adjust the initial equilibrium paid rate. Existing unit-library
approximations remain: deterministic forward coupons, time shifts, linear
notional scaling, balance-scaled DV01 and static base regulatory quantities.

The native LP covers monthly LCR (both cap branches), NSFR, two-sided EVE,
funding, horizon CET1, total-asset and commercial limits across market scenarios.
Allocation replay checks all these independently with the Python evaluator.
Native infeasibility is an explicit outcome, not a validated allocation.
Forecast-conditioned NII, nonlinear stress, vega, dynamic reinvestment and trade
execution are NOT incorporated into this decision graph. No forecast paths may
be used for pricing EVE/OAS. Global prepay parameter changes still require restart.
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
