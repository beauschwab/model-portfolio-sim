# Controlled native market-model convergence study

On October 2, 2026, `test_market_model_convergence.py` passed all five tests with
no skips against the public `model-market-1` native analysis boundary. This is a
synthetic mechanical study of supplied curves, smile formulas and constant joint
Gaussian innovations. It is not empirical fitting, a forecast or acceptance of
these new market inputs in the production product graph/cache. Production source
and financial tolerances were unchanged.

The study was refreshed after the official native release rebuild, following the
successful 15-test aged-funding suite. Fresh `portfolio_risk_native.dll` SHA-256:
`94841147112e7814e8db6c203eb058c9a4c3e2ffcc12957b87bad04cffac3da0`.
All five study tests pass with no skips; curve, factor and sampling results below
are unchanged from the initial measurement artifact
`21eb3ec7bd83fb05bc00f4e393776b39edd7309d27d59cb2418db30fd1a828c5`.

Reproduce with:

```powershell
uv run --project apps/api python -m pytest packages/portfolio-risk/tests/test_market_model_convergence.py -q -s
```

## Independent formulas and identity

All Python financial formulas in this file are explicitly independent test
oracles. Production calls enter Rust through `analyze_model_market`. A call payoff
for a normal forward is `(F-K)Phi(d)+s phi(d)`, where `d=(F-K)/s` and
`s=normal_volatility*sqrt(expiry)`; the deterministic zero-vol limit is intrinsic.
For shifted Black, apply the Black formula to `F+shift` and `K+shift` with relative
volatility. Put values use exact call-put parity, and payoff units are applied
once. These conventions are independently described in the primary
[OpenGamma normal formula documentation](https://strata.opengamma.io/apidocs/com/opengamma/strata/pricer/impl/option/NormalFormulaRepository.html)
and [Black formula documentation](https://strata.opengamma.io/apidocs/com/opengamma/strata/pricer/impl/option/BlackFormulaRepository.html).
The study tests 48 queries per convention over three expiries, two tenors, four
absolute strikes and call/put, with negative forward `-.005` and a distinct
strike-dependent smile. This is supplied quote representation, not a fitted
arbitrage-free stochastic-volatility process. Native formula values match the
independent formulas at `rtol=1e-11, atol=1e-13`.

Joint coordinates are rate, mortgage spread and regional **log** HPI. Annual
standard deviations are `.01, .02, .1`; correlation is
`[[1,.4,-.2],[.4,1,.1],[-.2,.1,1]]`. Every covariance entry is exactly symmetric
in the serialized binary-value contract. Correlation eigenfactor covariance is
independently reconstructed with NumPy in the test, and full retention reproduces
the supplied covariance at `rtol=1e-12, atol=1e-15`. There is no drift, mortality,
default, prepayment, measure change or invented HPI level conversion.

The finest normal bank has seed `20261002`, shape `[512,12,3]`. Each native call
contains at most 128 paths, within the test fixture limit. Stable batch IDs name
the finest bank plus batch offset; step and factor comparisons reuse that bank.
Coarse normals are block sums divided by `sqrt(block_length)`, and durations are
`1/steps`. Thus 1-, 4- and 12-step models share the same one-year terminal shocks.
Path studies use nested prefixes of 128, 256 and 512 independent supplied paths.
The separate native-seed fixture proves exact replay of identical requests and
zero covariance produces exact zero increments. Changing a grid is not falsely
described as sharing equivalent seed-generated draws without explicit coupling.

## Numerical convergence and sampling uncertainty

For the supplied smooth collateral truth `logD(t)=.01t-.003t²` on `[0,2]`, the
piecewise logDF maximum error across eight fixed query dates is:

| Intervals | Maximum absolute DF error |
| --- | --- |
| 1 | 0.00297082846 |
| 2 | 0.000708755963 |
| 4 | 0.000141791063 |
| 8 | 0.0000472659031 |

This measures interpolation error against the supplied smooth function. The
piecewise logDF error bound decreases with squared interval width; the sampled
dates need not attain each grid's midpoint bound. The index truth differs by
`-.002t` in logDF. Its forward basis is independently checked to `1e-14` on every
grid; increasing discount factors and negative simple forwards are admitted.

For constant Gaussian innovations the terminal distribution has no Euler drift
approximation. Maximum paired terminal-coordinate difference across the three
step grids and factor choices is `1.1102230246251565e-16`; the test gate is
`rtol=1e-12, atol=1e-14`. This does **not** establish convergence of mortgage
decisions, exercise timing, integrated discounting or any path-dependent product.

Option-heavy joint diagnostics use the explicitly linear basket loading
`[1,.35,-.04]` and strikes `-.015,0,.015`. That basket remains Gaussian; it is not
an option on HPI level. Its retained-model closed-form call value uses variance
`loading' * retained_covariance * loading`. The table shows ATM results:

| Factors | Paths | Estimated payoff | Sampling standard error | Retained-model exact | Bias versus full-model exact |
| --- | --- | --- | --- | --- | --- |
| 1 | 128 | .00485094397 | .000689618356 | .00600487360 | -.0000637715430 |
| 1 | 256 | .00603095152 | .000568255451 | .00600487360 | -.0000637715430 |
| 1 | 512 | .00624760794 | .000417334631 | .00600487360 | -.0000637715430 |
| 2 | 128 | .00506533083 | .000703586409 | .00606512673 | -.00000351841075 |
| 2 | 256 | .00604921257 | .000569912799 | .00606512673 | -.00000351841075 |
| 2 | 512 | .00624111319 | .000418151689 | .00606512673 | -.00000351841075 |
| 3 | 128 | .00505465067 | .000704433103 | .00606864514 | zero to roundoff |
| 3 | 256 | .00605988083 | .000571265439 | .00606864514 | zero to roundoff |
| 3 | 512 | .00625453136 | .000418799306 | .00606864514 | zero to roundoff |

All 27 strike/factor/path estimates lie within six sample standard errors of
their **retained** analytical model, with a `1e-13` numeric floor. Sample
covariances likewise compare with their Gaussian covariance standard errors.
These stochastic diagnostic bands do not replace or relax deterministic
pricing/risk tolerances. Individual estimates need not converge monotonically.
The small factor bias here is less than sampling uncertainty at this path count;
it is known separately from the analytical covariance, not inferred from noisy
Monte Carlo differences. Other basket loadings or nonlinear products can lose
far more variance under the same factor truncation.

## Rejection gates and remaining acceptance

The same public tests reject malformed covariance, unapproved curve right tails,
smile strike tails and shifted-lognormal negative shifted strikes. Native seed
replay and zero-shock identity are exact. No native-missing fallback or empirical
accuracy claim is used.

Issue #10 remains open for source-linked curve/quote bootstrapping and residuals,
arbitrage/calibration diagnostics, observed joint-driver estimation with
chronological held-out observations, regional stress uncertainty, full native
product/graph/cache adoption, option-heavy **product-output** convergence with
fixed OAS and CRN, and combined runtime admission. This study supplies a bounded
analysis baseline for those later gates; it does not satisfy them by itself.
