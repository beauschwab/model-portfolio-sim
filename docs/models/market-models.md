# Supplied multi-curve, smile and joint market analysis

`model-market-1` is a bounded Rust analysis contract implemented in
`portfolio-risk-native::model_market`. The native transport accepts the same raw
JSON shown in `examples/models/model-market-1.json`. Source revisions, currency,
as-of ordinal, model and assumption identity are explicit. All parameters are
supplied inputs; a synthetic example is not a fitted or observed market.

## Curves and basis

Collateral discount factors and index pseudo-discount factors have distinct typed
roles. A caller supplies positive factors with exact `(time=0, factor=1)` anchors,
strictly increasing times and an explicit right-tail policy. Negative interest
rates are admissible: factors can exceed one. The contract does not bootstrap
par instruments, infer collateral agreements or create an index curve from the
collateral curve. Separate discount and forward curves follow the distinctions
in [OpenGamma's multi-curve research](https://quant.opengamma.io/Multi-Curves-Variations2.pdf).

Between nodes, `log D(t)` is linear. Thus for `w=(t-t_i)/(t_j-t_i)`,
`D(t)=exp((1-w) log D_i+w log D_j)`. This is the log-linear definition in
[Strata's interpolation documentation](https://strata.opengamma.io/apidocs/com/opengamma/strata/market/curve/interpolator/CurveInterpolators.html).
Exact nodes return the supplied factor. The right tail is one of: reject,
flat continuously compounded zero rate (`log D(t)=t log D(T)/T`), or flat
instantaneous forward (`log D(t)=log D(T)+(t-T) last_log_slope`). Tail use is
returned independently for collateral and index. A left tail is unnecessary
because the time-zero anchor is compulsory and negative times are rejected.

For a caller-supplied contractual accrual fraction `tau`, the simple forward is
`F=(D(start)/D(end)-1)/tau`, evaluated using `expm1(log D(start)-log D(end))`.
The result reports the index forward, collateral-implied forward, their difference
as index basis, and collateral discounting at the payment endpoint. Times and
accrual fractions are separate: no implicit 30-day/calendar/day-count assumption
converts between them. Queries enforce typed roles and common currency.

## Quote surfaces and option values

A complete rectangular grid has axes expiry, underlying tenor and **absolute
strike**, with row-major `[expiry, tenor, strike]` values. Strike-dependent quotes
represent supplied smile/skew rather than an ATM label applied to all strikes.
Normal volatility uses absolute rate units per square-root year; shifted-lognormal
volatility uses relative units per square-root year and a supplied nonnegative
shift. Normal conventions permit negative forwards and strikes. Shifted-lognormal
requires strictly positive shifted forward and strike, including at zero
volatility. Normal and Black formulas/conventions are described in the primary
[finmath Bachelier](https://www.finmath.net/finmath-lib/apidocs/net.finmath.lib/net/finmath/functions/BachelierModel.html)
and [analytic formula](https://www.finmath.net/finmath-lib/apidocs/net.finmath.lib/net/finmath/functions/AnalyticFormulas.html)
documentation.

Interpolation is multilinear in total variance `v=sigma^2*expiry`, including the
strike and tenor axes. Returned `sigma=sqrt(v/queried_expiry)`; the zero-expiry
price is intrinsic. Each axis has an explicit reject or flat-volatility tail.
For an expiry tail, boundary volatility is held flat and variance grows with
queried expiry. Tail and interpolation flags accompany every result. This is a
representation/interpolator, not an arbitrage-free calibration: arbitrary inputs
may violate calendar monotonicity or call-price convexity, and interpolation does
not enforce those conditions. No SABR, local/stochastic volatility dynamics or
quote fit is inferred.

Let `U` be supplied payoff unit, `s=sigma sqrt(T)`, `phi` the standard normal
density and `Phi` its CDF. A normal call is
`U*((F-K)*Phi((F-K)/s)+s*phi((F-K)/s))`. Shifted Black uses `f=F+shift`,
`k=K+shift`, `d1=log(f/k)/s+s/2`, `d2=d1-s` and
`U*(f*Phi(d1)-k*Phi(d2))`. Put-call parity is `call-put=U*(F-K)` in both
conventions. Implementation evaluates out-of-the-money time value plus intrinsic
to avoid in-the-money subtraction; small Black ATM values use a centered CDF.
The caller owns the economic annuity/discount/notional behind `U`; this analysis
does not silently infer swaption or caplet settlement conventions.

The native Gaussian CDF evaluates a convergent centered power series for small
arguments and the error-function/Mills-ratio continued fraction for larger ones,
consistent with [NIST DLMF 7.9](https://dlmf.nist.gov/7.9). The series has a
128-term bound and the fraction 512 levels. Beyond a standardized option argument
of 38 the numerically negligible tail is zero; this is an explicit option
numerical guard, not truncation of simulated regional HPI innovations. A negative
time value within a machine-roundoff bound is set to zero; a larger discrepancy
is an error. Monetary values above the shared `1e15` bound are rejected.

## Joint rate, mortgage spread and regional log-HPI innovations

Drivers have stable ordered identities and coordinate units: rate factors and
mortgage spread in decimal rates, regional HPI in log changes. At least one of
each type is required. The input is an annual covariance matrix, positive actual
year fractions per step, path count, retained eigenfactor count and either native
seed words or an exact supplied standard-normal tape. The output contains additive
zero-drift Gaussian increments, cumulative terminal changes, and diagnostics.
There is no inferred mean reversion, market price of risk, forward-measure drift,
LMM, mortgage discount path or HPI-level conversion in this contract. The shock
analysis alone is not a physical- or pricing-measure product projection.

For diagonal standard deviations `S`, form correlation `R=S^-1 C S^-1`.
Symmetry is exact at the supplied covariance boundary. A zero-variance row must
be identically zero. Correlation bounds and PSD are checked in normalized units
so an invalid small-variance coordinate cannot be hidden by a large coordinate.
Negative eigenvalues below `-8*machine_epsilon*driver_count` reject the input;
only roundoff-size negatives are zeroed for the square root and reported.
Diagonal correlation is exactly one for nonzero variance. A computed off-diagonal
correlation outside one by at most four machine epsilons is bounded to one and
the adjustment is reported; larger violations fail. This is no nearest-PSD
repair of materially invalid covariance.

For sorted correlation eigenmodes, let
`L=S*V_k*sqrt(Lambda_k)*V_k^T`. Each step is `delta=L*z*sqrt(dt)`, with `z`
still having the full number of named driver coordinates when factors are
truncated. Full rank reconstructs the supplied covariance to floating-point
accuracy. Explicit truncation is of **correlation** eigenmodes, not necessarily
the largest raw rate/log-HPI covariance eigenmodes; returned retained covariance
`L L^T` and Frobenius discarded error make that approximation measurable.

Native draws reuse the existing `random::SharedDraws` PCG/SeedSequence/Ziggurat
Gaussian machinery, using its full rate tape as the joint driver tape. Antithetic
paths negate all joint coordinates together. Complete seed, ordered driver list,
path/step shape and duration grid identity must be retained by consumers. Changing
covariance or retained factors keeps these standard normals unchanged; changing
dimensions does not promise the same CRN sequence. Supplied tapes allow
independent controlled-covariance and timestep fixtures. No simulated Gaussian
tail is clipped. The legacy helper additionally allocates its unused spread and
HPI tapes; these component allocations remain additional to the joint result.

Diagnostics report means and population covariance of `delta/sqrt(dt)` pooled
over paths and steps, and theoretical retained covariance. They assess this
simulation; they are not held-out statistical calibration diagnostics. Historical
regime, source, sampling frequency, covariance estimation, forward drift and
regional dependency calibration must be supplied/validated by a separate stage.

## Admission and validation scope

There are at most 64 curves, 4,096 nodes per curve and 4,096 combined queries.
Curve times are in `[0,100]`, factors in `[1e-100,1e100]`, and query simple
forwards in `[-100,100]`; overflow fails instead of clipping. The smile allows
128 expiries, 128 tenors, 256 strikes and at most 65,536 complete quotes. Expiry
and tenor are bounded by 100 years; strikes/forwards by `[-2,2]`, normal volatility
by `[0,2]`, shifted-lognormal volatility by `[0,5]`, and shift by `[0,2]`.
Joint requests have at most 16 drivers, 4,096 steps, 100,000 paths, one million
scalar increments, and eight million transform-work units per matrix pass.
Variance is at most four in declared coordinate units; total duration is at most
100 years. Supplied standard normals outside `[-100,100]` fail admission. Sources
and identifiers are bounded to 256 bytes and rationale to 1,024 bytes. Unknown
fields, missing source identity, duplicate curves/drivers/regions, mismatched
currency, non-finite domains, and malformed shapes fail. These are component
work/size bounds, not an aggregate RSS guarantee or complete graph deadline gate.

`tests/model_market.rs` has 19 independent fixtures: exact curve nodes and
log-DF midpoint, distinct tails and basis, negative rates, hand total variance,
normal/shifted-Black ATM and out-of-the-money price/parity, small-volatility
stability, controlled exact covariance, rank deficiency, invalid tiny-axis PSD,
nonbinary variance roundoff, native seed golden draw and antithetics, covariance
CRN scaling, Brownian timestep aggregation, explicit factor error, admission and
strict JSON checks, and the persisted synthetic example. The companion deposit
example retains physical measure and two actual-day interest expense independently.

Remaining #10 acceptance includes source-linked quote bootstrapping and residuals,
arbitrage diagnostics/calibration, observed joint-driver estimation and chronological
held-out tests, uncertainty and regional stress validation, product/native graph
and cache adoption with full curve/quote/grid/CRN identities, discretization and
factor-convergence reports on real product outputs, and combined runtime limits.
The analysis route does not automatically change legacy built-in economics or
establish empirical accuracy. Those acceptance gates remain open.

## Retrospective published-rate diagnostic artifact

`artifacts/observed-market-driver-stage/` contains an isolated Rust diagnostic
and independent reference over a hashed Treasury history download. It is not an
installed native model or pricing calibration. Six nominal/real par-yield series
have 2,750 complete dates from 2015–2025, with 1,999 training intervals through
2022 and 749 later holdout intervals. The interval crossing Treasury's declared
2021-12-06 methodology change is excluded; values are not filled or interpolated.
The source is a latest-download retrospective vintage, not a point-in-time archive.

Rust fits constant calendar-time drift/covariance and reports holdout normalized
bias, RMSE and three-sigma exceedances. Eight native hand/admission/cancellation/
leakage tests and strict all-target Clippy pass; five numerical result fields match
independent NumPy calculations, and holdout mutation leaves the training fit exactly
unchanged. Nominal 2y holdout exceedances are about 8%, so this pooled covariance
does not establish predictive adequacy. No production, pricing-measure or empirical
predictive acceptance flag is set. See the artifact README, retained request/raw
source hashes and `observed-validation.json` for equations, bounds and limitations.
Holdout bias/RMSE have units decimal yield per square-root year; they are not
dimensionless sigma-standardized scores.
Product/factor adoption, publication vintages, regime/uncertainty diagnostics and
the full issue #10 criteria remain required.

The isolated successor `artifacts/observed-market-regime-stage` diagnoses eleven
supplied calendar-year periods without changing the pooled training fit. Six
native tests and strict Clippy pass; independent references verify every period,
exact predecessor output identity and exact held-out training-fit invariance.
Nominal two-year three-sigma exceedances are 15.6% in 2023, 4.8% in 2024 and about
4.0% in 2025 under the frozen training moments. This descriptive instability
assessment does not select regimes or establish statistical/predictive acceptance.
The retained source, vintage and pricing/product limitations above still apply.

The isolated `artifacts/market-smile-diagnostics-stage` additionally audits
supplied strike rows with the existing native option arithmetic. It reports
intrinsic/shifted-underlying bounds, vertical-spread bounds and nonnegative
butterflies with explicit rate/slope thresholds. Six native tests, strict Clippy,
36 independent normal/shifted-Black CLI rows and seven raw rejections pass.
Quotes and financial comparison tolerances are unchanged. This does not certify
calendar, interpolation or wing behavior and does not calibrate/repair the smile
or adopt it into production pricing. The complete #10 criteria remain required.
