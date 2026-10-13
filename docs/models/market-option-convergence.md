# Native option price and risk comparison

This synthetic study executes the unchanged Rust shifted-lognormal LMM and
cash-settled European swaption kernels. It publishes timestep, path and factor
comparisons required by issue #10; it does not admit a new volatility model,
establish observed calibration or complete that issue.

The accepted local predecessor passes 2,227 engine tests (two reference-only
skips), 590 API tests and 45 scoped browser checks. Its immutable receipt is
`artifacts/product-adoption-0298-capital-browser-repair-acceptance.json`.
The study uses an exact separate copy of its Risk DLL, SHA-256
`0bc08f27a264c07887478cc82f60b8371fda2a4224edbe6fd171f919507eaa42`.
It changes no production settings, source, saved economics or artifacts.

## Method and financial meaning

Thirty-two quarterly initial forwards are flat at either 3% or -0.5%, with a
4% displacement and supplied 25% volatility. The supplied correlation is
`exp(-0.10 * quarter_years * abs(i-j))`. One/two/three-factor loadings are fitted
by the actual native fitter and independently checked. Eight/full-rank factors
are independently supplied reference loadings; the production fitter's three-
factor limit remains unchanged. Truncated loading rows are normalized to retain
individual forward variances, so factor truncation changes cross-forward
covariance. Full-rank covariance Frobenius error is approximately `1.83e-14`.
Volatility is held fixed across factor counts; each representation is not
recalibrated to observed swaption quotes. These differences therefore measure
this supplied covariance approximation, not empirical pricing error between
separately calibrated production models.

The 24 option contracts cover payer/receiver, one/two-year expiries, two/five-
year swap tenors and ATM/plus/minus 150 bp strikes. Grids use 12/24/48 annual
steps. Coarse Brownian increments sum matching fine increments. All price and
rate-bump comparisons retain the same draws. Nested 64/1,024/2,048 paths consist
of complete antithetic pairs, with 1,024 independent pairs at the largest size.
Means and estimated standard errors use pair averages as independent units.
Paired differences against the finest full-rank result report their own standard
errors; overlapping standalone error bars do not establish equal prices.

Reported DV01 is `(PV(-1bp)-PV(+1bp))/2` per unit face. It shifts every initial
simple forward by one basis point with volatility, covariance, strike, shift
and draws fixed. It is not a bootstrapped market-quote or key-rate sensitivity.
Native option marks and this paired risk are independently recomputed from
pathwise discounted payoffs. Four small scalar LMM controls compare complete
discount, swap and short-rate output at the unchanged `1e-10` checks.

## Recorded findings

For the ATM two-year-expiry/five-year payer at 48 annual steps and 2,048 paths,
the three-factor representation remains measurably different from the full-rank
reference. Amounts below are per 100 face; standard errors are paired differences.

| Initial forwards | Three-factor PV | Full-rank PV | PV difference | Difference standard error |
|---|---:|---:|---:|---:|
| 3% | 3.870259 | 3.747406 | 0.122853 | 0.006630 |
| -0.5% | 2.274601 | 2.204173 | 0.070428 | 0.004360 |

The full-rank monthly-to-48-step PV difference is -0.013017 per 100 face for
the positive curve, and -0.001331 for the negative curve. Finer stepping does
not remove the factor approximation. Risk differences have separate uncertainty:
the three-minus-full-rank DV01 difference is approximately +0.000050 and
-0.000192 per 100 face respectively, with paired standard errors approximately
0.000093 and 0.000114. These two risk cases do not support an equally strong
claim about factor sensitivity as their corresponding PV comparisons.

The complete 2,160 comparison rows, pair counts, seed, covariance errors,
scalar controls and source/artifact identities are in
`artifacts/option-product-convergence-study/results.json`, SHA-256
`8922f9f55df4f0bd10a2e63d7f493d04268fa447c855f8cb24aed3a819330e23`.
The scientific figure and its identity are retained alongside it. All finite-
sample results belong to this supplied configuration and seed; no automatic
convergence threshold, empirical accuracy or new-model acceptance is inferred.

## Preserved assumptions and remaining work

The legacy discount output is recorded after its same-index discount update,
while swap rates are recorded before the rate step. The existing cash-annuity
negative-rate floor remains. Grid comparisons therefore include those existing
clock conventions, and cannot certify actual-date or multicurve settlement.
No tolerance, reduction ordering or financial default is changed by the study.

Quote-calibrated smile/skew, joint rate/spread/HPI generation, actual-date
settlement, independent settlement references, observed calibration/vintages,
holdouts, model admission and full product/cohort/saved-book/scale adoption
remain separate requirements. A three-factor legacy price must not be described
as full-rank convergence on the strength of native/reference implementation parity.

Coarse/fine Brownian coupling follows page 22 of
[Giles's multilevel Monte Carlo lecture](https://www2.maths.ox.ac.uk/~gilesm/talks/imperial_15.pdf).
The separation of process grids and product conventions is also explicit in
[finmath's Brownian motion interface](https://www.finmath.net/finmath-lib/apidocs/net.finmath.lib/net/finmath/montecarlo/BrownianMotion.html).
Here these ideas support paired numerical diagnostics; the study does not
implement a multilevel estimator or claim a theorem's convergence order for
the existing shifted log-Euler implementation.
