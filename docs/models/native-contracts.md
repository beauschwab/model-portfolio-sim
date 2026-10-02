# Native model contract entrypoints

Version 0.29.5 adds explicit analysis contracts to the existing bounded native
product runtime. `portfolio-model-core` contains shared financial logic;
`portfolio-risk-native` exposes it through the same standalone/FFI protocol.
`portfolio_risk.analytics.model_contracts` is transport only. Missing or invalid
native artifacts fail; there is no Python calculation fallback.

## Contracts and their scope

| Envelope schema | Python adapter | Detailed contract | Current scope |
|---|---|---|---|
| `credit-model-1` | `evaluate_credit(request)` | [Credit transitions](credit-model.md) | Deterministic state probabilities, competing exits, EAD/LGD and complete recovery tails |
| `observed-credit-calibration-1` | `fit_observed_credit(request)` | [Observed calibration](credit-calibration.md) | Exact transition-time homogeneous hazard MLE and chronological holdout diagnostics |
| `dated-cashflow-1` | `resolve_dated_cashflows(schedule, scenario)` | [Dated events](dated-cashflows.md) | Validation and identity-preserving scenario selection of supplied dated events |
| `dated-term-1` | `generate_dated_term_flows(request)` | [Fixed term producer](dated-term-contracts.md) | Fixed, nonoptional corporate/CD contractual cashflow generation |
| `floating-rate-1` | `evaluate_floating_coupons(request)` | [Floating coupons](floating-rate.md) | Arithmetic from available supplied fixings and explicit observation/principal intervals |

The request's inner schema is required where its typed model contains a schema.
The dated-cashflow envelope contains `{schedule, scenario}`; the schedule carries
its own `dated-cashflow-1` schema. Unknown fields fail, including unknown fields
inside tagged enums. Dates use Gregorian ordinals (`0001-01-01 = 1`), rates are
annual decimals and monetary units are explicit. Currency identifiers are
structural labels; no FX conversion or legal currency eligibility is inferred.

## Native boundary and adapters

The standalone `portfolio-lifecycle` reads one JSON object from stdin:

```json
{"schema":"credit-model-1","threads":1,"request":{"schema":"credit-model-1","monetary_unit":"USD","initial_live_probability":[1,0,0],"periods":[{"duration_years":0.5,"transition_hazards_per_year":[[0,0,0,0.2,0.3],[0,0,0,0,0],[0,0,0,0,0]],"exposure_at_default":[{"drawn":100,"undrawn":20,"credit_conversion_factor":0.5},{"drawn":100,"undrawn":20,"credit_conversion_factor":0.5},{"drawn":100,"undrawn":20,"credit_conversion_factor":0.5}],"loss_given_default":[0.4,0.4,0.4],"recovery_distribution":[{"lag_years":1,"weight":1}]}]}}
```

This is a synthetic hand-calculation example, not a fitted risk policy. Its
performing survival is `exp(-0.25)` and its default probability is
`(1-exp(-0.25))*0.2/0.5`. Default EAD is 110; nominal recovery is 60% of default
EAD, paid one year after the interval-end default recognition. The recovery
remains in the output even though it falls after the six-month model horizon.

Success returns `{ok:true,result:...}`; failure returns `{ok:false,error:...}`.
The adapter unwraps the result and raises a `ValueError` for native failures.
All financial calculation stays behind that boundary:

```python
from portfolio_risk.analytics.model_contracts import evaluate_credit
result = evaluate_credit(request)  # request is the inner typed input above
```

The FFI and standalone input/output limits are 64/128 MiB. Threads must be
1..256. Typed models add independent event, period, work, date, monetary and
metadata bounds documented on their respective pages. These are component
admission limits, not a total process-memory cap. The transport serializes
dates/JSON and manages result ownership; it contains no quant formulas.

## Model adoption and compatibility

These functions do not automatically replace saved product economics, assign
accounting policies, publish a journal or populate a Strategy Lab library.
Fixed corporate `level_payment` is also available through existing native term
pricing; its [amortization contract](contractual-amortization.md) preserves
historical `annuity` economics. The new dated producer rejects floating/options,
heuristic sinking schedules and active CD withdrawal behavior. It supplies full
contractual coupon amounts; daily income recognition requires an accounting
policy and journal integration. Conditional income forecasts remain separate
from pricing and physical credit measures.

Keep the immutable complete request with the result when an artifact is stored.
Source IDs/revisions identify supplied assumptions; they do not certify source
quality, calendar generation or empirical accuracy. New calibrated policies
require explicit adoption, a fresh library/session build and new result identity.
Scenarios retain the original base OAS and shared random numbers. No prepared
interactive evaluator reprices instruments.

Rebuild the product and decision/workflow binaries after upgrading, then restart
API/workers. These additions retain product ABI 8 and existing ledger protocol;
an older ABI-8 binary still cannot serve the new additive schemas. Rebuilding
locally is separate from restarting an executing deployment.

## Validation and remaining work

Independent Rust fixtures and `test_native_model_contracts.py` cover analytical
expectations, domain failures and both native entrypoints. Fresh-process tests
run outside the explicit Python reference context. Level-payment pricing checks
use independent calendar dates, annuity payments, a scalar OAS root and fixed
spread marks. Existing convention, term ownership and full engine gates remain
required. Numerical, parity and synthetic tests do not replace real-data model
validation or persisted-journal replay.

[The implementation ledger](implementation-plan.md) preserves every issue's
full acceptance scope. Joint product credit/behavioral cashflows, correlated
credit draws, interval-censored fitting, future index projection, daily ledger
adoption and empirical validation remain open.
