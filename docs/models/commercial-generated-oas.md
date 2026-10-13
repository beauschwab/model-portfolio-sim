# Original generated commercial OAS

An explicitly selected `commercial-product-graph-1` position can now use
`oas_target.kind="calibrate_positive_cash"` with `generated_policy.oas_calibration`:

```json
{
  "policy": "native_retained_q_refit_bracket-1",
  "lower_oas": -0.02,
  "upper_oas": 0.06,
  "max_trials": 48
}
```

The target must exactly identify the original signed cash quote and retain its
source/revision. Controls require a finite increasing bracket within [-5,5] and
3..128 trials. Locked OAS requires no calibration controls. Historical omitted
controls retain their existing route and output. This is an additive explicit
selection, requiring compatible native rebuilding and selected-library rebuilding.

## Native sample and policy ownership

The native owner generates original Pricing training and distinct evaluation
tapes once. It retains native complete unexercised cash and observable states.
Each trial spread refits continuation using only the same training observations;
evaluation observations never enter the regression. Changing spread while retaining
a policy fitted at another spread would change exercise economics.

With the supplied original collateral curve D and spread s, discounted cash uses
`D(t) exp(-s t)`. Backward training targets include actual future native cash and
subsequent policy exercise values; regressions estimate conditional continuation
from observable state. Fresh evaluation uses that trial's fitted policy and
actual realized future cash. The calibrated objective is the mean present value
under that evaluation policy. Discount clocks retain actual settlement and complete
recovery/rights tails, including settlements after the accrual end.

The bounded solver evaluates both endpoints and then bracket midpoints. Matching
both endpoints is ambiguous and rejects. Unbracketed targets, sampled nonmonotone
values and unresolved final quote residuals reject. A finite evaluation objective
can jump when predicted continuation changes exercise while realized future cash
differs; bisection cannot turn a price in that gap into a converged quote.

Acceptance requires fresh native stopped-source replay for every selected
evaluation path, followed by the original quote tolerance
`abs(actual-target) <= 1e-9 + 1e-12*abs(target)`. No larger spread/price tolerance
or statistical confidence band replaces that residual gate. Sampled monotonicity
checks do not certify global uniqueness of every possible fitted-policy root.

The accepted fitted policy and actual stopped Q sources enter graph preparation
directly. Those original samples are not regenerated during preparation. Separately
supplied original Physical laws then construct the original stopped cost/EIR proof.
Physical outcomes do not calibrate the Pricing quote. Every current scenario and
paired Pricing bump retains exactly the selected original OAS bits; it can fit its
own current policy at that fixed spread. Original Physical cost/yield remain supplied
acquisition anchors, independently verified against complete expected cash.

## Acquisition coefficients and receipts

Commercial unit builds may select this original generated calibration. Native
graph preparation runs in the build; interactive strategy evaluation remains
coefficient-only. Units still require independently supplied Physical acquisition
cost matching the Pricing purchase quote, original effective yield, complete
physical cash and separate face/purchase-cost identities. Calibration never infers
a seasoned payment, missing history or original yield from remaining balance.

The graph's `generated_oas_calibration` receipt retains controls, original target,
trial spreads/evaluation values, selected spread, actual stopped cash and residual,
and original sample counts. Independent Python verification generates its own Q
sample, replays each recorded bracket step/refit and independently reconstructs
stopped cash before checking P acquisition, policies, graph metrics and coefficients.
Persisted nested receipts retain canonical JSON types and the existing final guards.

## Work, memory and execution

Admission charges native generation/final replay once and every permitted additional
discount/regression trial. Retained feature-derived column limits are checked against
the first fitted policy; an unexpectedly varying state rejects before further trials.
Existing source, graph and coefficient work limits remain. The generated aggregate
ceiling remains four billion work units and 128 MiB retained component capacity.
Calibration samples precede graph P/Q/proof preparation; only the accepted policy,
source receipts and trial receipt cross that phase boundary. These limits are not
an aggregate process RSS guarantee. Expensive brackets/scenario combinations can
reject even if a smaller permitted trial budget would converge.

The process and C ABI adapters select native execution controls for calibrated
requests. The original process-entry clock covers decode, generation, trial fits,
stopped replay and bounded serialization, including inherited caller deadlines.
The process transport also enforces cancellation/watchdog/executable identity.
Durable publication retains the original accepted queue deadline, source/executable
and object hashes, and SQL lease/token/revision/attempt/cancellation fences. An
expired or unresolved run publishes no accepted result.

## Controls and remaining scope

`commercial-generated-calibrated-graph-1.json` is a synthetic zero-volatility
control: accrual ends on day30, principal settles on day35, and original spread
0.017 is independently recovered from the complete day35 cash quote. The exercise
variant supplies a Gaussian law and quote at spread0.02; original Physical cost
uses its separately supplied original effective yield. Both call and continuation
occur in the Pricing evaluation sample. Their trial ceilings reserve the entire
joined graph within the unchanged work limit.

These controls do not establish observed accuracy, a calibrated arbitrage-free
joint law or general basis/path convergence. Generated path-specific draws/reviews,
seasoned holdings, nonintegral fees, daily in-arrears indexed earning, compact mixed
candidate streaming, verified opening evidence, full saved-book adoption and the
complete 60,000-position joint acceptance remain original #15/#25 gates.

Research context: [Glasserman and Yu's paths/basis convergence analysis](https://business.columbia.edu/faculty/research/number-paths-versus-number-basis-functions-american-option-pricing)
supports retaining a separate convergence boundary. A successful finite-sample
quote fit and a fresh evaluation sample do not prove broad convergence.
