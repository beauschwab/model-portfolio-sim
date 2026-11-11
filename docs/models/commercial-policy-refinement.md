# Commercial continuation basis and numerical controls

The supplied-law generated exercise contract accepts three explicit regression
policies. A selected policy is retained in the raw model identity; it is never
chosen by the evaluation sample or silently substituted during calibration.

| Policy | Terms over training-standardized observable features |
|---|---|
| `native_observable_linear_svd_train_evaluate-1` | Intercept and linear terms |
| `native_observable_quadratic_svd_train_evaluate-1` | Intercept, linear and all quadratic cross terms |
| `native_observable_cubic_svd_train_evaluate-1` | Intercept, linear, all quadratic and all cubic cross terms |

The existing quadratic policy retains its exact basis term order, reduction
order, output fields and disclosure. Its historical regression rows omit
`basis_degree`. Explicit linear/cubic rows add that integer field. Each richer
basis starts with all terms of the preceding basis in the same order. Constant
feature removal, feature means/scales, credit-state coverage, SVD cutoff and
minimum-norm rank-deficient solution are unchanged. Every selected basis requires
at least twice as many live training observations as columns. Evaluation cannot
change those features, coefficients, degree or training coverage.

For `d` active coordinates and degree `p`, the number of columns is
`choose(d+p,p)`. Admission charges the selected basis within the existing
four-billion work/128-MiB retained-component tier. The admitted SVD column upper
bound is also limited by half the training sample; a realized basis that exceeds
that limit rejects before matrix allocation. This is a refusal, not an automatic
degree reduction. The controlled two-date cubic 512-training/512-evaluation
request exceeds the existing work budget and rejects. Cubic 128/256 controls
and linear/quadratic 128/256/512 controls are admitted. Component bounds are not
an aggregate RSS guarantee.

## Independent numerical benchmarks

`test_commercial_policy_refinement.py` uses a supplied synthetic Brownian index
with volatility 0.2, a par principal of100 and a 15-day coupon. At the day15
exercise date a par issuer call caps principal plus coupon at100. The exact
normal integral is
`100 - (100*15/365)*0.2*sqrt(15/365)/sqrt(2*pi)`.
The matching put has the opposite optionality; shared antithetic paths retain
the call-plus-put par identity. These checks do not use fitted continuation
values as their expected answer.

The two-date case adds a day5 call at99.9. Given observed index `x`, the exact
continuation after the later call is
`100 + (100*15/365)*(x*Phi(-x/s)-s*phi(x/s))`,
where `s=0.2*sqrt(10/365)`. Independent deterministic integration finds the
optimal first-date boundary and value, approximately99.8904049813. A second
integration splits at every real root of the actual training polynomial and
values that learned policy under the same normal law. This separates:

- **Policy approximation loss:** the learned issuer policy's exact expectation
  less the optimal issuer minimum. The controlled gate is at most2bp of par;
  a feasible policy cannot improve on that minimum beyond quadrature error.
- **Evaluation sampling uncertainty:** the Monte Carlo estimate relative to
  that learned policy's exact expectation. Its diagnostic band uses five
  reported independent-antithetic-cluster standard errors.
- **Financial replay correctness:** independent raw source/regression/stopped
  cash verification retains the original `1e-9 + 1e-12*abs(value)` gate. Sampling
  bands do not replace or relax this gate.

No test requires finite-sample errors or policy loss to decrease monotonically
with degree or path count. Another control changes only the evaluation stream
and requires identical training regressions/value. Native unit controls recover
a known cubic function from its training observations and reject insufficient
live coverage. Independent Python/NumPy/SciPy code remains validation only.

## Interpretation and remaining acceptance

The controlled path/basis comparisons do not prove convergence for arbitrary
contracts, joint credit laws, rare default states or a discontinuous original
OAS objective. The receipt's `convergence_established` and `empirical_validation`
flags remain false. Broader contract/seed/driver/basis refinement, policy
uncertainty, full mixed adoption and original issues15/25 remain required.
Rebuilt native artifacts and explicit saved model/library/session rebuilding are
required before selecting a new policy. Local checks do not deploy services.

The approach follows the training-conditioned continuation of
[Longstaff and Schwartz (2001)](https://escholarship.org/uc/item/43n1k4jb).
The joint consideration of basis size and sample count is informed by
[Glasserman and Yu (2004)](https://business.columbia.edu/faculty/research/number-paths-versus-number-basis-functions-american-option-pricing).
Those papers motivate these controls; they do not establish accuracy of the
supplied commercial law or certify the chosen polynomial family for every book.
