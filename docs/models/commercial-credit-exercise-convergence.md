# Credit-enabled single-exercise numerical benchmark

This is a separate, predeclared test family. It does not change production
financial code or the accepted option-free constant-default benchmark. Every
declared case and any execution, replay, domain or numerical failure is retained.
Its evidence concerns this supplied mathematical family, not empirical credit
accuracy, a general exercise-convergence claim, accounting approval or issues 15/25
completion.

## Contract and exact values

Face F=100 has zero fixed coupons and fees, no draws or purchased rights, principal
settlement on actual day 35, and one optional exercise after scheduled day 15 cash.
The day 15 coupon is zero. OAS and parallel bump remain fixed at zero; every
discount factor is 1. Market covariance and coupon/hazard driver loadings are zero.
The only transition is performing to default; default is absorbing. A supplied
LGD 0.4 and complete one-record zero-lag recovery tail pay R=60 immediately at the
actual default time. There are no surviving coupon rights after default, so a
defaulted path cannot exercise. Full maturity/recovery tails are independently
replayed; contractual accrual end does not replace the actual settlement horizon.

Supplied default intensities are constant separately before and after exercise.
For exercise time e=15/365 and maturity T=35/365, set conditional probabilities
p0=1-exp(-lambda0*e) and p1=1-exp(-lambda1*(T-e)). Survival to exercise is S0=1-p0.
The surviving conditional continuation value is C=R*p1+F*(1-p1). Thus:

- Option-free value: R*p0+S0*C.
- Issuer call: R*p0+S0*min(K,C), exercising when K<C.
- Holder put: R*p0+S0*max(K,C), exercising when K>C.

Equality retains continuation. Only one right is supplied in any optional case;
there is no game-priority ambiguity. The pre-exercise default term is R*p0 under
every policy. A strike never applies to a defaulted or otherwise ineligible right.

## Declared matrix

The full 60-cell matrix crosses these three laws with option-free, call below C,
call above C, put below C and put above C; 128/256 training and evaluation paths;
and training/evaluation seed pairs (7,11), (19,23). The two optional strikes are
predeclared as C-8 and C+8, separated from the exact optimal boundary. No cell is
removed after observing its result.

| Law | p0 | p1 | C | Low/high strike | Option-free | Call at low K | Put at high K |
|---|---:|---:|---:|---:|---:|---:|---:|
| Credit off |0|0|100|92/108|100|92|108|
| Moderate |0.10|0.25|90|82/98|87|79.8|94.2|
| Front loaded |0.35|0.50|80|72/88|73|67.8|78.2|

The other call/put cases continue at the option-free value. Raw seeds, randomness
domains, ordered path keys and original fixed OAS remain the same across rights
within each law/sample cell. Credit randomness is independent of the antithetic
market pairing; default indicators need not be opposite within a cluster.

## Learned policy versus optimal policy

All surviving observed states are identical in this family. The explicitly
selected linear continuation fit therefore has one active intercept, rank 1 and
no varying features. Training uses eligible surviving states only, with fresh
evaluation streams. This domain is checked rather than inferred from output
parity. The native intercept determines the *actual learned policy*. Its analytic
value is computed from the supplied survival law using K if that policy stops,
or C if it continues. This is separate from the optimal min/max value above.

Records retain learned-policy value, optimal value, policy loss, sampling error
against the learned policy, total error, exercise/default counts, original raw
requests and complete native outputs. The existing finite-family two-bp-of-par
policy-loss gate 0.02 and financial tolerances 1e-9 absolute/1e-12 relative remain
unchanged. Sampling uncertainty never replaces raw financial parity.

## Sampling and gates

The independent oracle recomputes the native mean and standard error by pairing
evaluation paths i and i+N/2, then taking the sample standard deviation of those
N/2 cluster means divided by sqrt(N/2). Adjacent-path pairing is not substituted.

Conditioned on the trained constant policy, root cash has two possible amounts:
R/K with default probability p0 when stopping, or R/F with default probability
p0+S0*p1 when continuing. Exact Clopper-Pearson binomial intervals use the actual
low-cash count and a Bonferroni per-cell alpha 0.01/60. These are model-based
diagnostics under ideal independent credit uniforms; deterministic PRNG results
and observed bank populations do not acquire an IID guarantee. All misses remain
failures in the receipt. Normal 95% intervals are descriptive only. A zero-event
sample retains a nonzero exact uncertainty interval, and increasing sample count
is not required to improve every realized error monotonically.

Each native result also passes the existing independent raw stochastic source,
credit, learned stopping and full-cash replay. Credit-off cases have deterministic
cash parity. Controls reject altered domain assumptions, recovery tails, fitted
state coverage, path cash, ineligible exercise, means and cluster SE. Shared-CRN
controls compare continuation cash and surviving exercise masks across rights.

## Reproducible isolated evidence

The standalone harness accepts an explicit matching `--native` lifecycle binary
and an unused `--output` path. It pins the binary before/after the whole matrix,
all native source/Cargo files via the existing test helper, all engine Python
references, the benchmark/test/doc files and the raw example used to construct
requests. It records the checkout commit, runtime versions and every declared
cell. Existing output receipts cannot be overwritten. Test-only transport pinning
does not add a production backend or financial callback.

The matrix tests only deterministic-hazard, immediate-recovery, zero-coupon,
one-date exercise behavior and sampling diagnostics. State-dependent hazards,
stochastic rates, credit/market correlation, nonzero/signed coupons, multiple
exercise dates, delayed/nonlinear recoveries, defaulted transferable rights,
continuous-state/basis convergence, observed holdouts, broad mixed/cohort and
60,000-position graph/solver/journal gates remain separate requirements. Isolated
debug execution is not a release build, deployment or durable API publication.

## Retained initial validation finding

The original 60-cell run against source 79ccfe2 completed 52 passing and 8 failing
cases. All four optional rights failed independent regression replay for moderate
credit with 128 paths/train 19/eval 23 and 256 paths/train 7/eval 11. No cases, strikes,
seeds, sample counts or tolerances were changed to remove those failures.

There were 115 or 238 surviving training rows, respectively. Observed feature
columns 1,5,7 were exactly constant 1. Native ordered addition of 1/N produced means
0.9999999999999973 and 0.9999999999999978, then scales 2.6645352591003773e-15 and
2.2204460492503166e-15. Those exceeded the existing 8-epsilon threshold and falsely
marked constant columns active. Python 3.12's compensated built-in sum produced
exact 1, inactive columns and the fallback scale 1. The constant-state regression
rank was 1 in both implementations, but native continuation was distributed over
four redundant coefficients while the reference retained one intercept.

This is an open native fitting/replay finding in the initial benchmark stage.
It is not an analytic sampling miss or empirical evidence. The original binary,
source hashes, complete raw/native records, 52/8 receipt and failing test report
remain retained. Any later correction requires a separate matching build and a
new immutable full 60-cell receipt, preserving the initial evidence and unchanged
family rather than editing it into a pass.

## Exact-constant centering correction

The follow-through centers a column at its first observed value only when every
retained training observation compares exactly equal to that value. Its variance
is consequently exactly zero and the existing inactive scale-one fallback
applies. Genuinely varying columns retain the original ordered mean and variance
reductions, eight-epsilon threshold and financial tolerances. No approximate
constant test or threshold change is introduced. Native controls cover both
survivor counts, arbitrary signed and tiny constants, near-constant variation on
both sides of the existing threshold, and unchanged evaluation-coverage guards.

This adds one bounded observation-by-feature equality scan without allocations;
the existing regression reservation already bounds the fit by the larger
sample-by-basis-columns-squared allowance. It does not change admission limits.
The correction changes previously spurious active-feature metadata and redundant
fit coefficients for exactly constant columns. The initial receipts remain
immutable; the corrected build must replay the complete unchanged family into a
separate source- and binary-pinned receipt.
