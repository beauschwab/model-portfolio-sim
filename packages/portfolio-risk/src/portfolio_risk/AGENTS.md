# AGENTS.md — src/portfolio_risk

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

## Model assumptions ledger

**Curve (curve.py).** Annual fixed leg par swaps at 10 pillars; sequential
brentq on log-DF; log-linear DF interp; flat-zero extrapolation past 30y
to the 40.25y forward grid.

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
