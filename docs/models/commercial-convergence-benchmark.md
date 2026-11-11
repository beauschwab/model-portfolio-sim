# Commercial finite-family convergence and uncertainty benchmark

The independent `commercial_convergence_benchmark.py` harness extends the
existing commercial policy-refinement controls. It runs an explicit **107-cell
matrix**, retains every refusal/failure, and separates financial correctness,
learned-policy approximation and evaluation sampling uncertainty. It does not
declare convergence for every commercial contract or establish observed accuracy.

## Predeclared experiment

All cells use the existing synthetic supplied **Pricing** law and contractual
owners, par principal 100, a fifteen-day indexed coupon (split into two periods
for the economically active two-driver subset), complete cash tail,
original loan source, fixed base OAS 0 and separately seeded training/evaluation.
The selection never uses evaluation results to choose a basis or policy.

| Experiment | Cells | Declared variation |
| --- | ---: | --- |
| One-date analytic or option-free | 36 | No option/call/put × three bases × two driver families × two grids;128 training and128 evaluation paths |
| Two-date learned policy, baseline | 24 | Call/put × three bases × two drivers ×128/256 paths; seeds7/11 |
| Training/evaluation seed replication | 24 | Same rights/bases/drivers at128 paths, seeds19/23 and31/37 |
| Learned-policy grid refinement | 12 | Same rights/bases/drivers at128 paths and seeds7/11 on the refined grid |
| Larger admitted path sample | 4 | Brownian call/put × linear/quadratic,512 paths |
| Required work refusal | 1 | Brownian two-date call, cubic,512 paths; the existing four-billion work cap must refuse |
| Economically active two-driver policy | 6 | Call/put × three bases,128 paths, seeds7/11; correlated OU drivers both load future coupon cash |

The baseline driver is scalar Brownian motion with annual variance 0.04. The
second family contains correlated OU coordinates with reversion 0.7/1.4 and
instantaneous covariance `[[.04,.009],[.009,.01]]`. The first coordinate drives
the coupon. The second is an observable correlated **nuisance feature**: given
the first Markov coordinate it supplies no additional economic information.
This tests regression robustness to extra observable dimensions, not a claim
that a general multi-factor coupon/default law has converged. No stochastic
credit transition, observed calibration or Physical-law benchmark is inferred.

The separate six-cell `active_two_ou` subset splits the future coupon into
nonoverlapping seven-day and eight-day periods. Both reset on day 15 and settle
on day 35; each is explicitly loaded on a different OU coordinate. Thus future
coupon cash is the genuinely two-driver amount `W=w0*X15+w1*Y15`, with weights
`100*7/365` and `100*8/365`. Their unequal mean reversions 0.7/1.4 both affect
conditional future mean and variance. This is an admitted finite linear-Gaussian
multi-factor control, not general nonlinear multi-factor convergence.

The analytic coarse grid is day0/15/35, and the learned coarse grid adds day5.
The refined grid is day0/2/5/10/15/25/35. Every exercise and fixing date remains
present. OU transitions are exact, so this is a distributional step-refinement
control, **not Euler discretization convergence**. Changing grid dimension
changes normal-draw assignment; cross-grid paths are not claimed identical.
The unchanged within-cell native/reference CRN contract remains mandatory.

Native admission runs on every request, before path generation, under the
existing four-billion counted-work/128-MiB retained-component tier. It is not
an aggregate RSS limit. All selected cells remain in the receipt, including
the explicit refusal and any unexpected admission or numerical failures.
There is no automatic smaller basis, path-count fallback, or widened budget.

## Independent expectations

For a zero-mean OU coordinate with reversion `a` and instantaneous variance `q`,
the exact variance at `t` is `q*(1-exp(-2*a*t))/(2*a)`, with its continuous
Brownian limit at `a=0`. The harness independently verifies the covariance
semigroup under grid refinement, including cross-coordinate covariance.

Let `c=100*15/365` and `X15` be the day15 coupon index. The option-free expected
value is100. A par call/put at day15 has value
`100 ∓ c*sd(X15)/sqrt(2*pi)`. The shared antithetic construction permits exact
conditional-cluster checks; it does not turn individual paths into independent
observations.

The learned-policy family adds a day5 call at99.9 or put at100.1. Conditional
on day5 index `x`, the later index has exact normal mean
`m=exp(-a*10/365)*x` and independently known increment standard deviation `s`.
The later optimally capped call continuation is
`100+c*(m*Phi(-m/s)-s*phi(m/s))`; the put uses
`100+c*(m*Phi(m/s)+s*phi(m/s))`. Root finding followed by deterministic normal
integration gives the optimal two-date value. These expected amounts are not
read from native cash or continuation targets.

The separate learned-policy expectation uses the actual training polynomial
only to determine its stop regions. For one coordinate, integration splits at
all real polynomial roots. For two coordinates, each fixed first coordinate
reduces the conditional second coordinate to a normal variable; all real roots
partition its exact Gaussian stop probability. Adaptive outer integration then
values that policy under the original law. Constant policies and cubic policies
with multiple boundaries have independent controls.

For the active two-driver subset, `w` is the contractual cash-loading vector,
`D5` is the two-component observed state and `A` is diagonal mean reversion.
The future coupon's conditional mean is `m=w' exp(-A*10/365) D5` and its
conditional variance is `w' Cov(OU_increment) w`. The exact optimal policy
depends on that projected mean. To value an actual two-coordinate learned
polynomial, the oracle conditions the joint Gaussian state on `m`, integrates
the remaining independent normal coordinate by polynomial roots, and then
integrates `m`. It never replaces the native learned policy with the optimal
projection policy.

Outer integration explicitly splits where the inner stopping polynomial loses
degree, has repeated real roots (quadratic/cubic discriminant roots), or its
roots cross the finite normal integration tails. The initial 101-cell run
correctly retained two cubic seed 19 correlated-OU **oracle quadrature failures**;
financial replay passed, but the independent integral emitted roundoff warnings.
The revised root partition and integration of deviations from par resolve that
oracle defect without changing native coefficients, admission, financial
tolerances or the declared quadrature tolerance. The failed initial receipt
is retained separately rather than relabeled a successful run.

The final-date learned polynomial must reproduce the independently known linear
remaining coupon on a five-point tensor grid extending beyond twelve marginal
standard deviations. Only then is the analytic final-date payoff used in the
learned early-policy integral. Quadrature warnings and estimated errors remain
visible; they cannot be treated as a successful policy-loss gate.

## Three different checks

1. **Financial replay:** every admitted result is checked by the existing
   independent raw path/regression/stopped-cash verifier. The existing
   `1e-9 + 1e-12*abs(value)` gate remains unchanged. A dedicated test alters a
   cash value by only `1e-5`, inside the sampling interval, and requires refusal.
2. **Policy approximation:** a feasible call policy cannot beat the exact
   issuer minimum; a feasible put policy cannot beat the exact holder maximum.
   The finite-family gate remains at most 0.02 currency units, two bp of par,
   with a separately stated quadrature allowance. It is not a financial replay
   tolerance or a guarantee for an arbitrary book.
3. **Evaluation uncertainty:** standard errors are independently recomputed
   from `paths/2` antithetic clusters. The per-cell predeclared sanity band is
   five cluster standard errors around the **learned policy's** exact value,
   not around the optimum. Normal 95% interval coverage is descriptive and need
   not be 100% in a finite matrix. No finite-sample coverage theorem is claimed.

The repeated training seeds expose variation in learned-policy expectations
separately from fresh-evaluation sampling noise. Three seeds are a small
descriptive replication, not a reliable tail uncertainty estimate. No test
requires errors, policy loss or standard errors to decrease monotonically with
sample count, degree or step refinement. Fresh evaluation seeds must leave
actual native training coefficients and training value exactly unchanged.

## Execution and evidence

Use an explicitly selected immutable standalone lifecycle executable. The
harness records its SHA-256, exact executed-input identity/timeout, the
worktree commit, every declared matrix cell and every selected result. The
regular transport retains its executable-before/after checks and native
deadlines. An executable change invalidates the contained evidence. Selection
is local to this test harness; production dispatch and artifact selection do not
change.

```text
python packages/portfolio-risk/tests/commercial_convergence_benchmark.py \
  --native <pinned-portfolio-lifecycle-executable> \
  --output <ignored-evidence-directory>/matrix-receipt.json
```

The test module also accepts `PORTFOLIO_CONVERGENCE_NATIVE` for an explicitly
pinned standalone artifact in an isolated benchmark checkout. Without it, the
normal matching installed native artifact is mandatory; no test is skipped or
replaced with Python execution when it is missing. Python/NumPy/SciPy remain
explicit independent validation tools.

The machine-readable receipt is evidence for this finite synthetic family.
Native `convergence_established`, `empirical_validation` and
`physical_journal_eligible` flags remain false. General contracts, rare credit
states, nonlinear or broader economically active multiple factors, discontinuous OAS
calibration, Physical policy transfer, observed holdouts and mixed/saved-book
adoption require their own evidence under the original #15/#25 gates. This work
does not deploy artifacts or rebuild saved libraries/sessions.

The methodology builds on the existing [policy-refinement document](commercial-policy-refinement.md)
and its primary Longstaff–Schwartz and Glasserman–Yu sources. Numerical evidence
from these finite fixtures does not establish empirical model accuracy.

## Contained validation checkpoint: 2026-10-16

The isolated Windows run used the parent lifecycle executable pinned at SHA-256
`753a583625aef95832511cdf9222ce5dd3e65d05361da764e6a5619201dfc409`.
All **134 new pytest controls** and **39 existing stochastic/refinement controls**
passed. The separately retained complete benchmark receipt has 106 numerical
passes and the one required cubic 512-path work refusal, with no missing cells.
Its maximum learned-policy loss is 0.0033311801 currency units, approximately
0.333118 bp of par; the maximum reported quadrature error is 1.87e-9. All six
economically active two-driver cells pass.

The descriptive normal 95% intervals cover 100 of 106 learned/analytic policy
expectations. Those cells share streams and symmetric contracts, so this ratio
does not estimate coverage from independent trials. Every cell passes the
separate five-SE sanity band. The architecture checks pass with the unchanged
110-edge baseline. No native build, financial implementation, admission cap,
saved model or production artifact was changed by this benchmark work.
