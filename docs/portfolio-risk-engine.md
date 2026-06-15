# Portfolio Risk Engine

`portfolio_risk` is the Python quant engine run by the Rates Workbench
backend. The FastAPI app in `apps/api` is intentionally a thin adapter:
routes manage books, scenarios, settings, jobs, and JSON conversion while
all quant logic remains inside `packages/portfolio-risk`.

The package prices a synthetic bank balance sheet on shared Monte Carlo
rate paths: agency MBS and whole-loan proxies, commercial/corporate
loans, long-term debt, non-maturity deposits, CDs, money-market balance
sheet lines, and ASC 815 hedge overlays. Outputs include OAS, KRD01s,
vegas, 27-month forward valuation, 9-quarter stress P&L, monthly NII,
EVE/duration gap, LCR, NSFR, CET1 projection, strategy KPIs, and robust
optimizer allocations.

## Architecture

The engine is layered so each stage has a narrow responsibility.

| Layer | Responsibility | Key modules |
| --- | --- | --- |
| Market setup | Bootstrap par swap curves, derive forwards, calibrate volatility | `core.curve`, `core.vol` |
| Path generation | Simulate shifted-lognormal rates and model-specific state variables under common random numbers | `core.lmm`, `core.scenarios`, `models` |
| Product engines | Generate discounted path cashflow factors and undiscounted accrual outputs | `core.kernels`, `products` |
| Pricing | Solve base OAS and reprice scenarios without regenerating cashflows | `core.pricing`, `products.corp` |
| Risk and stress | Full revaluation KRDs, vegas, and forward stress packs | `analytics.risk`, `analytics.stress`, product risk drivers |
| Accounting and KPIs | NII, runoff vectors, EVE, liquidity, funding, and capital ratios | `analytics.accounting`, `analytics.kpis` |
| Strategy stack | Forward-starting programs, unit tensor, interactive evaluation, robust LP | `strategy.strategies`, `strategy.unitlib`, `strategy.optimizer` |

The central design is an A-matrix factorization. Kernels generate
cashflow path sums once. OAS enters only through discounting, so base OAS
can be solved once and held fixed across risk and stress revaluations.
That keeps scenario P&L interpretable as market movement rather than a
different spread calibration.

## Quant Models

**Curve and rates.** The curve bootstrap solves annual fixed-leg par
swaps sequentially on log discount factors, interpolates log-linearly,
and extrapolates flat-zero beyond the last pillar. Rates follow a
3-factor shifted-lognormal LMM under the spot measure with monthly
log-Euler stepping. The shift floors rates at negative 2 percent; the
model is deterministic-vol and does not introduce stochastic skew.

**Volatility.** Swaption volatility uses a Rebonato-style abcd
time-homogeneous instantaneous vol parameterization. Calibration is least
squares to ATM points with a frozen time-zero weighting approximation.
Point vegas are projected through abcd unless the risk loop applies
point multipliers, which is why reported vegas should be read as model
family sensitivities rather than independent surface-node greeks.

**Current coupon, PS spread, and HPI.** The mortgage state model combines
a trending current coupon fit, an OU primary-secondary spread, and a
rate-linked lognormal HPI process. Current coupon reacts partially to an
OLS fair-value estimate using swap rates and deterministic vol features.
The PS spread shocks independently of rate factors. HPI drift responds to
10-year swap deviations with clipping guards for extreme tails.

**MBS and whole-loan prepayment.** MBS cashflows use a refi S-curve in
borrower incentive, burnout, CLTV spline effects, static pool
multipliers, seasoning, seasonality, HPA, and rate lock-in. Burnout is
based on cumulative in-the-money incentive rather than pool factor alone.
Payment delay is handled at the OAS discounting layer. The fast MBS
kernels duplicate model blocks for base, stress, and batched risk paths
because the shared helper version measured materially slower; tests gate
zero-shock consistency across copies.

**Corporates and commercial loans.** Schedule-driven assets and debt use
exact-time cashflow discounting. Coupon amounts are day-count exact on
adjusted schedules, and within-month discounting extends the monthly
deflator to true payment dates. Floating-rate fixings interpolate the
simulated front 3-month rate. Calls and puts are rule-based, not
option-exact; the exercise block is the documented replacement point for
an LSMC continuation model.

**Non-maturity deposits.** Deposit valuation treats runoff like the
liability-side analog of prepayment. The deposit rate model is a
logistic long-run beta to the simulated short-rate proxy plus asymmetric
error correction, fit by joint nonlinear least squares over the actual
recursion. Attrition combines segment base runoff, account age, balance
size, opportunity gap, and positive rate-velocity effects. Stress runs
rerun the rate recursion on shocked paths, so flight behavior is
nonlinear rather than a static template.

**Certificates of deposit.** CDs reuse the schedule and exact-time
discounting machinery through a deck that duck-types the corporate
pricing surface. Retail CDs include an early-withdrawal put modeled as a
hazard in reinvestment incentive net of penalty. Brokered/callable CDs
use the same rule-based issuer call convention as corporates. Existing
books run to contractual maturity; rollover is out of scope.

**Money markets and hedges.** Money-market lines are spread-to-short
floaters with constant balances and intentionally near-zero duration.
Swaps are represented as fixed and floating legs through the same
cashflow machinery. Swaptions are Monte Carlo valued on emitted par-rate
paths. Accounting designation controls earnings, AOCI, and CET1
treatment, but all hedge designations affect EVE identically.

**Accounting and regulatory KPIs.** NII uses expected interest/principal
outputs from product engines, static book yields, amortized-cost basis
overrides when present, and smeared CSR accruals. EVE is market value of
assets less liabilities at solved model prices; duration gap is a
first-order parallel-DV01 view. LCR, NSFR, RWA, and CET1 use explicit
stylized weight tables that are calibration seams for production rules.

**Strategy and optimizer.** Forward-starting strategy programs can add
new purchases or originations at future months. The unit library prices
hypothetical cohorts for every product type through the live engines in
batch, then stores per-unit tensors. Interactive strategy evaluation is a
time-shifted dot product plus closed-form KPI recalculation, keeping
`POST /strategy/eval` synchronous and sub-ms. The optimizer solves a
robust maximin NII LP across scenario-specific unit libraries and base
KPI states, returning shadow prices for binding constraints.

## Backend Orchestration

The API store in `apps/api/app/store.py` is in-memory and
repository-shaped: module dictionaries hold books, market data,
scenarios, settings, jobs, programs, hedges, the built unit library, and
base KPIs. This shape keeps persistence swappable without changing
routes.

Long engine calls run through a single `ThreadPoolExecutor` worker
because numba kernels already saturate cores. A submitted run creates a
job record, binds progress telemetry to the worker thread, applies the
configured numba thread count, converts Polars/numpy outputs to JSONable
objects, and exposes status through `GET /jobs/{id}`.

Primary run kinds are:

| Run kind | Backend adapter | Engine behavior |
| --- | --- | --- |
| `risk` | `run_risk_all` | Product-level OAS, parallel DV01, 10 KRDs, and vegas with fixed OAS and CRN |
| `stress` / `deposit_stress` | `run_stress_all` | 9Q forward valuation and shocked P&L for MBS and deposits |
| `nii` | `run_nii` | Balance-sheet NII, runoff vectors, and summary metrics |
| `kpis` | `run_kpis` | NII plus EVE, duration gap, LCR, NSFR, and CET1 |
| `unitlib` | `build_unitlib_job` | One-time unit tensor build plus base KPI cache |
| `strategy` | `run_strategy_job` | Forward-starting strategy programs using modeled runoff |
| `/strategy/eval` | `eval_strategy_sync` | Synchronous unit tensor evaluation; no engine calls allowed |
| `/optimize` | `run_optimize_job` | Scenario libraries, base KPIs, and robust LP solve |

Named market scenarios are expressed in trader terms: 10-year level,
2s10s twist around a 5-year pivot, spread shift, and vol shift. The
backend maps those legs onto engine inputs for the relevant quarter. For
scenario NII, the API revalues each quarter along the path; for spot
risk, it applies the first-quarter market.

Assumption patches deliberately expose only safe mutation surfaces.
Deposit segment anchors and CD early-withdrawal parameters can update in
process. Prepayment vector changes return `RESTART_REQUIRED` because
numba freezes module constants at first compile, including LUTs built
from prepay parameters.

## Rationale

**Hold OAS fixed across scenarios.** Solving OAS on the base market and
holding it fixed makes scenario P&L a measure of rates, vol, spread, and
behavioral exposure. Re-solving OAS inside every scenario would absorb
market movement into spread and understate risk.

**Use common random numbers.** Every bump side and scenario uses the same
draw set for a run. Central differences are then differences of means
under shared paths, which materially reduces Monte Carlo noise in KRDs,
vegas, and stress P&L.

**Keep quant logic out of the API.** The API owns state, job lifecycle,
and serialization. Product models, accounting, and KPI math stay in the
package so behavior is testable without the web service and persistence
can change later without moving model code.

**Duplicate hot model blocks.** The MBS and deposit kernels repeat
per-period model logic in multiple numba kernels because a shared helper
was slower in the hot loop. Tests enforce identical zero-shock behavior
so this performance choice does not silently create model drift.

**Prebuild unit libraries for interactivity.** Strategy sliders need to
feel live. The expensive work runs once in `POST /run kind="unitlib"`;
the interactive endpoint only scales stored unit tensors and recalculates
KPIs.

## Limitations And Calibration Seams

The seeded balance sheet and histories are synthetic. They are useful for
demoing the mechanics and regression testing the engine, not for
production risk without real market data, histories, positions, and
validated assumptions.

Several model choices are intentionally disclosed seams: stylized prepay,
deposit attrition, CD early-withdrawal anchors, regulatory weight tables,
static RWA add-ons, rule-based exercise, deterministic vol features, and
frozen-weight Rebonato calibration. Performance claims in internal docs
are measured on specific hardware and should be remeasured before they
are repeated as production commitments.
