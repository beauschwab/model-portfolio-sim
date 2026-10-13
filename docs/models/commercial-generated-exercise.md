# Generated commercial exercise

`commercial-generated-exercise-1` explicitly selects supplied-law joint Gaussian
market and event-time credit generation with training-only least-squares
continuation and a fresh evaluation sample. This is an analytical contract.
It does not silently replace the finite-distribution graph or saved economics.
The example is synthetic; its separate evaluation sample is a Monte Carlo
holdout, not an observed borrower history or empirical predictive validation.

## Source and measure

The raw `commercial-source-credit-accounting-proof-1` prototype retains the
original contractual source, current source, original acquisition cost/yield,
actual dates, factor conventions and complete credit recovery tails. Both term
sources explicitly select Pricing. There is one prototype credit path; native
generation replaces its path identity and credit stream separately for each
sample. Native source owners calculate every earned right and cash event.
Neither caller continuation values nor caller-generated monetary checkpoints
enter the fit. Future performing draw tranches currently reject in this lane.
The separate source/ledger draw contract retains its own supported scope.

The discount curve is supplied and deterministic, with explicit interpolation,
tail and currency policy. Discounting is `D(t) exp(-(base_OAS + bump)t)`.
The original base OAS is supplied and retained; this operation does not fit it.
Repeating base/down/up with identical sample identities preserves common
random numbers, although exercise decisions can change. A supplied Gaussian
Pricing law is an assumption; this contract does not certify a calibrated,
arbitrage-free relationship between its coupon drivers and discount curve.

## Exact Gaussian transitions and credit coupling

For each observable coordinate, `dX_i = a_i(mu_i-X_i)dt + dW_i`, with supplied
instantaneous covariance `cov(dW_i,dW_j)=C_ij dt`. All coordinates are observable
at the supplied midnight UTC ordinal. Hidden future coordinates reject.
Conditional mean over actual elapsed ACT/365 time is
`mu_i + (X_i-mu_i)exp(-a_i dt)`; innovation covariance is
`C_ij (1-exp(-(a_i+a_j)dt))/(a_i+a_j)`, with exact zero-reversion limit `C_ij dt`.
A symmetric covariance square root maps independent shared native Gaussian
draws into the joint innovations. Covariance must be symmetric positive
semidefinite. The two halves of each market sample are antithetic.

Every future indexed observation requires an explicit coupon loading and a
generated state on its actual fixing date. Only conditional projected fixings
can be replaced; historical observed fixings remain supplied. The coupon law
uses `baseline + loading*(X_fix-X_initial)`, followed by the existing native
margin, compounding and cap/floor conventions. All future observations must
be mapped, rather than silently freezing an unmapped future rate.

Credit hazards are piecewise constant over the supplied credit intervals.
An explicitly mapped hazard uses `h_base exp(beta dot (X_start-X_initial))`,
conditioned on the observed state at the interval start. A zero baseline cannot
acquire an implicit hazard. Invalid or out-of-domain hazards reject rather than
clip. Native continuous-time credit transitions and complete source recovery
cash operate on those generated hazards. This interval-start approximation is
disclosed; the process does not integrate a continuously varying hazard within
an interval. LGD, CCF, interest recovery and recovery timing remain supplied.

## Observable contractual state and continuation

At each decision, regression state includes current observable market levels,
live principal, signed legal interest, Watch/Delinquent indicators, the current
stochastic hazard interval's observed anchor, already selected coupon fixings,
and separate coupon entitlement principal and remaining legal balances. Rates
fixed in the future are masked. Current market levels and net interest alone
would omit earlier fixings and gross coupon rights; they are not used as the
entire contractual state. The state is bounded to 64 features before generation.

Only the training sample establishes means, scales, active varying features and
the quadratic basis (intercept, linear terms and all quadratic cross terms).
Numerically constant features are removed from the basis using an explicit
`8*epsilon*max(abs(mean),1)` scale threshold. At evaluation, a change in a
training-constant feature rejects at the unchanged financial tolerance.
An evaluation live credit state absent from training also rejects.

The native SVD uses cutoff `epsilon*max(rows,columns)*largest_singular_value`.
Rank and singular values are disclosed; rank deficiency uses the minimum-norm
solution. At least twice as many live training rows as active basis columns are
required. Conditioning the continuation on observable state follows
[Longstaff and Schwartz](https://escholarship.org/uc/item/43n1k4jb).
Basis richness and path count must be considered together; the controlled
checks here do not establish general convergence, as discussed by
[Glasserman and Yu](https://business.columbia.edu/faculty/research/number-paths-versus-number-basis-functions-american-option-pricing).

Backward induction chooses Call/Put/Continue using the fitted conditional
continuation. Continue retains the realized discounted continuation target;
the fitted value is not substituted for actual policy cash. Simultaneous
priority, before/after scheduled cash, clean principal factor and fee remain
explicit native policies. Rights-only termination requires the reviewed
`settle_earned_cancel_future-1` disposition. No principal is invented after
principal settlement. Before-cash exercise precedes scheduled repayment,
maturity and draw funding; credit events at that clock remain observable.

## Fresh evaluation and numerical checks

Training and evaluation require distinct market seeds, credit seeds and stream
identities. Each sample contains an even 128 through 512 paths. Evaluation
outcomes cannot change the fitted policy. Every evaluation path is regenerated
under the same native streams and passed through the sealed native stopping
owner. Its complete stopped source cash must reproduce the retained policy PV
at `1e-9 + 1e-12*abs(PV)`. No final-output tolerance is relaxed.

The reported standard error uses independent antithetic pair clusters:
`Z_i=(V_i+V_(i+N/2))/2`, `SE=sqrt(sample_variance(Z)/(N/2))`.
It does not treat antithetic paths as independent. Controlled tests compare a
one-date Gaussian callable coupon with an exact normal integral at 128, 256
and 512 paths, test the zero-volatility option-free cash identity, separate
evaluation streams, correlated two-driver transitions, multiple exercise
dates and joint credit cash replay. Independent NumPy/SciPy verification
constructs raw paths, fits its own regressions and replays its own stopped
credit source. These are contained mathematical controls, not observed-data
evidence or proof of general policy/basis convergence.

## Admission, runtime and publication

The new explicitly selected `bounded_generated_1024_samples-1` tier admits at
most 1,024 aggregate training/evaluation samples, four drivers, 129 state dates,
64 observable features, four billion conservative aggregate work units and
128 MiB estimated retained compact capacity. The work estimate includes every
source and prospective stop plus bounded SVD work. Existing ordinary source
eight-million and graph 32-million work caps are unchanged. These component
bounds are not an aggregate RSS guarantee. Each source keeps its existing
admission. The request is at most 1 MiB and result at most 32 MiB, with small
outer protocol overhead; the historical process outer read remains 64 MiB.

The original byte selector checks every original schema field before routing
can erase duplicates. The native deadline starts at process entry before stdin
read and covers selected decode, generation, regression, native cash replay and
checked bounded serialization. The Python process transport adds cancellation,
watchdog and executable identity checks without financial calculation.
Durable publication additionally requires independently verified raw source,
persisted nested outputs and the original queue deadline, source/executable,
lease/token and final object-byte fences. Local testing does not restart deployed
workers; additive execution requires rebuilt artifacts and explicit saved model
or library rebuilding before adoption.

`convergence_established`, `physical_journal_eligible` and `empirical_validation`
remain false for this standalone analytical receipt. The separately selected
[generated graph](commercial-graph-strategy.md#generated-policy-graphs) now joins
Pricing policy, separately supplied Physical replay and acquired coefficients.
Physical model calibration, general Monte Carlo OAS calibration, path-specific
draw/review actions, full mixed candidate accounting and complete 60,000-position
joint acceptance remain required gates under issues 15 and 25.

Observed features mask each coupon's canonical entitlement metadata until its
accrual period starts or actual payment has already been observed. Earlier
floating resets can determine a later retained-payment principal balance; this
future balance cannot enter current regression features. The source owner also
requires every amount influencing actual cash and earned interest to be available
at that cash date or earning-segment start. This includes inherited indexed
balance dependencies. Rights-only eligibility uses observed legal rights and
known contract terms, rather than future realized coupon cash.
