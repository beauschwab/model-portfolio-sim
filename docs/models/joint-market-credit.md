# Joint stochastic market and credit generation

`joint-market-credit-1` composes two explicit native stages for analysis:

1. `model-market-1` generates a bounded PSD Gaussian tape for named rate,
   mortgage-spread and regional-HPI drivers. The tape retains the declared
   covariance, factor truncation, driver order, source revision and common
   random-number identity.
2. The annualized innovation for each path and period enters supplied hazard
   elasticities. For a positive baseline hazard `h` and coefficient vector
   `beta`, the resulting hazard is

   `h(path, period) = exp(log(h) + beta · z_annualized(path, period))`.

   A zero baseline requires a zero coefficient vector. Hazards above the
   admitted domain are rejected; they are never clipped. Behavioral exit,
   credit default and prepayment then use the shared competing-exponential
   state machine from `joint-state-1` with the caller-owned uniform pairs.

The period policy supplies actual dates, discount factors, recovery fraction
and recovery lag. The generated rate shock is retained as a market driver; it
is not silently converted into a discount curve, pricing measure, OAS or P/Q
mapping. The state output therefore remains an analysis receipt, not a saved
product valuation, accounting journal, CECL/IFRS 9 result or regulatory model.

The request binds market source revision and shared-draw identity to the joint
identity, requires market durations to equal the declared actual-date periods,
and requires one random pair per position-period. Rust validates dimensions,
driver domains, dates, hazard coefficients, positions and work before output.
The market generator and state loop accept cooperative cancellation; no partial
result is returned after a failed checkpoint.

## Independent market-source verification

Publication independently binds the returned tape to the original covariance,
factor count and supplied normals or NumPy-compatible PCG64 seed. The verifier
reconstructs the antithetic draw ordering, checks every increment, and then
recomputes sample diagnostics, hazards and competing events. A coherent result
generated with a different covariance or seed cannot certify the queued source.
The same checks run on freshly reread persisted tables before publication.

The independent NumPy eigensolve certifies the raw correlation spectrum and
retained covariance. At a repeated eigenvalue across the truncation boundary,
the historical numerical implementation has no platform-independent basis
choice. The verifier checks the retained invariant subspace, top-factor spectrum
and PSD root against the raw operator rather than choosing a different basis.
This is a spectral certificate, not a claim of a unique basis at a degeneracy.

Numerically null modes need special care: taking a square root can amplify
eigensolver roundoff to the square root of machine precision. A bounded set of
raw draw vectors supplies a candidate transformation in that null subspace.
An independent direct SVD, symmetry/PSD checks and squared-operator residual
certify it within the existing native `8 * epsilon * driver_count` bound.
Every tape cell must still match its original draw at the unchanged `3e-12`
relative/absolute verification tolerance. Producer diagnostics cannot enlarge
either bound. Zero-variance rows remain exactly zero; the correlation trace
counts positive-variance coordinates only.

Verification has bounded dimensions and cooperative cancellation checkpoints.
The receipt records `raw_covariance_verified`, `raw_draws_replayed` and
`truncation_subspace_certified`; none grants product adoption or empirical status.

The checked-in example is synthetic and deliberately reports
`adoption_eligible=false`. Observed market/borrower calibration, uncertainty,
product-specific policies, cohort/saved-book adoption, daily journal/FTP
integration and full mixed-book scale acceptance remain separate gates. This
boundary follows model-risk guidance that implementation tests and synthetic
fixtures do not establish predictive or regulatory validation.
