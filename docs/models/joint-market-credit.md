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

The checked-in example is synthetic and deliberately reports
`adoption_eligible=false`. Observed market/borrower calibration, uncertainty,
product-specific policies, cohort/saved-book adoption, daily journal/FTP
integration and full mixed-book scale acceptance remain separate gates. This
boundary follows model-risk guidance that implementation tests and synthetic
fixtures do not establish predictive or regulatory validation.
