# Source-linked par-quote bootstrap

`market-quote-bootstrap-1` is a bounded analytical contract for turning
source-linked fixed-leg par quotes into a discount-factor curve. It is
implemented in `portfolio-risk-native::market_bootstrap` and exposed through
`bootstrap_market_quotes`. The request retains source and curve identities,
every payment time, every accrual fraction, and separate bootstrap versus
validation quotes. The result is adoption-ineligible until a reviewed market
configuration binds its curve, collateral, currency, calendar and quote
vintage to production pricing.

Each bootstrap quote adds exactly one new maturity pillar. For a quote with
rate `c`, payments `(t_k, alpha_k)`, known earlier discounts `D_k`, and final
discount `D_T`, the unit-notional par condition is

```text
1 = c * sum_k(alpha_k D_k) + D_T
```

The final discount is solved in closed form after the caller supplies the
complete schedule. Every non-final payment must match an earlier bootstrapped
pillar; the contract rejects an unseen intermediate date rather than silently
interpolating it. All payment times and accrual fractions are explicit. No
annual, 30/360, business-day, holiday, collateral or compounding convention
is inferred by the adapter.

For each quote, the result recomputes

```text
c_model = (1 - D_T) / sum_k(alpha_k D_k)
residual = c_model - c_observed
unit_par_pv = c_model * sum_k(alpha_k D_k) + D_T
```

Bootstrap residuals must be within the closed-form numerical tolerance. A
validation quote must use already-created pillars and is allowed to show a
nonzero residual; this provides an independent quote-fit diagnostic rather
than a false claim that every source quote was an exact input to the fit.

The result reports a deterministic convergence receipt for the sequential
closed-form method, but this is numerical convergence only. It is not a proof
of quote quality, arbitrage freedom, collateral consistency, or predictive
market calibration. Smile dynamics, basis construction, risk-neutral drift,
observed quote-vintage backtesting and product adoption remain separate issue
#10 gates. Negative par rates are admitted where the supplied schedule produces
a positive discount factor; invalid denominators, nonpositive factors,
duplicate identities, unseen intermediate dates and malformed schedules fail
closedly without clipping.

Controlled native and independent adapter tests cover exact pillar recovery,
validation residuals, identity and schedule rejection, public dispatch and
cancellation. The synthetic example is arithmetic evidence only; it does not
close the empirical or production acceptance requirements.

Independent publication verification also checks the exact admitted payment
work count, bootstrap quote count type and unchanged residual gate. Nonfinite
quote diagnostics and altered tolerances reject. Verification retains the
original deadline/cancellation callback through quote replay.

This separation follows current model-risk practice: [SR 26-2](https://www.federalreserve.gov/supervisionreg/srletters/SR2602.htm)
calls for validation to assess reliability and limitations, while [Strata's
market-quote sensitivity documentation](https://strata.opengamma.io/apidocs/com/opengamma/strata/pricer/sensitivity/MarketQuoteSensitivityCalculator.html)
identifies par-rate quotes as calibration instruments whose quote mapping and
sensitivities should remain inspectable. The contract therefore publishes
source identities and residuals without treating a synthetic fit as approval.
