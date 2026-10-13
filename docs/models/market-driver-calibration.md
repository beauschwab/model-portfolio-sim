# Observed market-driver calibration

`market-driver-calibration-1` is the explicit analysis boundary for fitting
four supplied market-driver columns. It estimates training moments in Rust and
reports diagnostics on a later chronological holdout without allowing holdout
rows to change the fit. The Python function
`analytics.model_contracts.fit_market_drivers` is transport only.

The request retains model, input, source, training-split and holdout-split
identities. `training_end` partitions the strictly increasing actual-date
history. At least 30 training and 10 subsequent holdout observations are
required, and the split identities must differ. Optional scenario selection
uses only training rows and is deterministic under equal-distance ties.

For training rows \(x_i\), the native fit returns the arithmetic mean

\[
  \bar{x}=n^{-1}\sum_i x_i
\]

and unbiased covariance

\[
  C=(n-1)^{-1}\sum_i(x_i-\bar{x})(x_i-\bar{x})^T.
\]

Holdout diagnostics report component bias, RMSE and the fraction outside three
training standard deviations. The fit is never re-estimated on the holdout;
changing only holdout observations must leave the training moments byte-stable.
The optional selected stress rows are ranked by the training covariance
Mahalanobis distance and are bounded to the declared driver domains.

The optional `uncertainty_lag` requests a Bartlett HAC standard error for each
component of the training mean. With centered observations (y_t), the native
diagnostic uses

\[
\widehat{\Omega}_j=\gamma_{0,j}+2\sum_{\ell=1}^{L}
\left(1-\frac{\ell}{L+1}\right)\gamma_{\ell,j},\qquad
SE(\bar{x}_j)=\sqrt{\max(\widehat{\Omega}_j,0)/n},
\]

where each autocovariance uses the training-row count as its denominator. A lag
of zero is an explicit iid estimate; lags above 64 or the training-row count
are rejected. The method and lag are retained in the result so uncertainty is
not mistaken for empirical validation or a calibrated pricing parameter.

This contract is deliberately narrower than a pricing calibration. It does not
bootstrap curves, estimate a risk-neutral drift, fit a smile, infer mortgage or
deposit behavior, or authorize a product graph/cache. A result is always marked
`adoption_eligible: false`; a production model requires reviewed source vintage,
parameter uncertainty, chronological outcomes analysis and explicit adoption.
Synthetic fixtures validate leakage controls and arithmetic only.

The distinction follows current interagency model-risk guidance: conceptual
soundness, limitations, ongoing monitoring and outcomes analysis must be
separately evidenced before extending a model beyond its intended use. See
[SR 26-2](https://www.federalreserve.gov/supervisionreg/srletters/SR2602a1.pdf).
