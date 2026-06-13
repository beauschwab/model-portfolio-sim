# mbs-risk

Shifted-lognormal LMM OAS / risk / 9Q stress-capital engine for agency MBS.

## Quick start
```
pip install -e .
python -m mbs_risk 10000 bench     # throughput probe + projection
python -m mbs_risk 10000 risk      # KRD01s + vegas
python -m mbs_risk 10000 stress    # 9Q forward valuation + shocks
pytest tests/ -v
```

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
stylized prepay anchors (fit to loan-level before use); parallel-only
forward shocks; frozen-weight Rebonato; no payment delay; demo fitters
run on synthetic histories.

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
src/mbs_risk reorganized into core/ models/ products/ analytics/
strategy/ (matching ARCHITECTURE.md layers); old flat import paths kept
working via module aliases -- zero changes needed in tests, API, or the
skill. 34 tests green post-move.
