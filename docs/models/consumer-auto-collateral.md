# Auto collateral: value path, loss given default and equity-driven hazards

`consumer-auto-collateral-1` makes auto loss and prepayment respond to vehicle
value. It builds a collateral value path from a supplied depreciation curve and
a supplied used-vehicle index, turns that value into loss given default and
recovery timing under a reviewed recovery policy, and supplies borrower equity
as a hazard covariate. Hazards come from the unchanged
[`credit-covariate-1`](credit-covariate.md) estimator and are evaluated by the
unchanged `credit-model-1` operator.

Rust owns `portfolio_model_core::consumer_auto_collateral`. This is a native
library component for issue #94 under tracker #25. It has no API route, durable
job, saved-book adoption or pricing publication, and it does not change the
[installment mechanics](consumer-installment.md), whose own fixed-depreciation
repossession policy is unchanged.

## Collateral value

`value(t) = origination_value × retained(age) × index(t) / index_at_origination × (1 − mileage_haircut) × (1 − condition_haircut)`

- **Depreciation** is a supplied, reviewed curve for the loan's segment
  (`new` or `used`) and optional vehicle class: knots of age in months against
  the fraction of origination value retained, starting at age zero with
  increasing ages. Values between knots are linear. An age beyond the last knot
  rejects; no tail is extrapolated. A curve for a different segment or class
  rejects.
- **Age** is ACT/365 years since origination times twelve.
- **The used-vehicle index** is a supplied level per interval from the driver
  tape, relative to the supplied level at origination. No index is forecast,
  interpolated or defaulted here.
- **Mileage and condition haircuts** are supplied assumptions in `[0, 1)` with
  their own review identity.

## Loss given default

For an exposure `EAD` and collateral value `V` at default, with a reviewed
`RecoveryPolicy`:

```
gross sale proceeds  = V × recovery_haircut
costs                = repossession_cost + remarketing_cost
deficiency balance   = max(0, EAD − gross sale proceeds + costs)
deficiency recovery  = deficiency_recovery_fraction × deficiency balance
loss                 = deficiency balance − deficiency recovery
LGD                  = loss / EAD
```

Proceeds above the exposure and costs are surplus owed to the borrower; they
are reported and never become lender income. When costs exceed proceeds the
deficiency exceeds the exposure and LGD is above one: the function reports it
unclipped, and the credit-model feed rejects it, because that excess is an
expense the credit model cannot carry as a recovery.

The recovery lag and the deficiency lag are cash timing only. They do not enter
the loss.

## Hazard covariates

Borrower equity is the one covariate derived here:
`equity = min(value at interval start / balance, equity_cap)`. The cap is a
declared policy, needed because equity is unbounded near payoff. The request
names the fitted column that carries equity and supplies zero in its place.
Every other covariate (loan age or term level, score band, weak rate incentive,
trade-in proxy, unemployment change, origination LTV, term, payment-to-income)
is a supplied column of the fitted model. Prepayment and default are competing
edges of one `credit-covariate-1` model, fitted by that estimator with the same
bounds, identification and eligibility rules. Equity outside the column's
declared domain rejects; it is not clipped beyond the declared cap.

## Projection

`project` takes the fitted model, the collateral, the recovery policy and one
interval per period with its clocks, source, scheduled balance, index level and
supplied covariates. For each interval it:

1. values the collateral at the interval start and derives equity;
2. projects the annual hazards through `credit-covariate-1`;
3. values the collateral at the interval end, where the credit model recognises
   default, and computes the loss on the interval's balance;
4. writes a `credit-model-1` period: hazards, exposure (the balance, no undrawn
   line), LGD for each live origin and a recovery distribution with the
   collateral payment at the recovery lag and the deficiency payment at its lag.

The assembled `credit-model-1` request and its result are both returned, so
term LGD and recovery timing reach the credit model as explicit supplied terms.
Competing default and prepayment conserve probability because that operator
does.

`shared_draw_exit` gives the first exit of a performing loan under one uniform
pair: the exposure threshold `−ln(1 − u)` is carried across intervals, so a
changed hazard moves the exit without a new draw, and the second uniform picks
default by its share of the hazard in the exit interval. It supports direct
default and prepayment from performing only; models with watch or delinquent
edges need the full path engine.

## Eligibility

`adoption` projection rejects an ineligible fitted model; `analytical_only`
returns `adoption_output=false`. `empirical_claim` is always false: observed
repossession and recovery histories with chronological holdouts are required
before any empirical claim. Synthetic fits are never adoptable.

## Bounds

At most 240 intervals and 64 depreciation knots; amounts up to `1e15`; lags up
to 36,600 days; the estimator's and credit operator's own limits apply. These
are component limits, not an aggregate memory guarantee.

## Evidence

`tests/consumer_auto_collateral.rs` has seven fixtures:

- Collateral value by hand: 30,000 × 0.675 × 0.98 × 0.97 = 19,249.65 at month
  24, scaling exactly with the index.
- LGD by hand: a deep-equity loan loses nothing and returns 7,000 of surplus;
  an underwater loan loses exposure minus haircut value plus costs
  (20,000 − 10,800 + 1,000 = 10,200), or 8,160 after a 20% deficiency
  recovery; changed lags leave the loss identical.
- Competing default and prepayment probabilities sum to one in every period to
  `1e-12`.
- With zero equity coefficients the projected hazards, state probabilities and
  expected loss equal the constant-row credit model in every bit, under a 40%
  index fall.
- A two-group synthetic fit recovers default 0.4 → 0.1 and prepayment 0.1 → 0.4
  per year across equity 0.8 → 1.4 to `1e-6`. A 20% index fall then raises
  loss in every interval by exactly the lost proceeds, raises default hazards,
  lowers prepayment hazards and more than doubles expected loss. On 2,000
  shared uniform pairs, defaults rise and prepayments fall, and frequencies
  match the exact probabilities within three standard errors.
- A one-year longer lag moves every recovery payment by one year and leaves
  expected loss and total recovery bit-identical.
- Adoption of a synthetic fit and contract violations reject.

These fixtures establish arithmetic and directional behaviour. They are not
evidence of predictive accuracy.

## Not done

- No transport, API route, durable job or independent publication verifier.
- The index path is supplied; this component is not yet connected to the joint
  driver tape (#92) or to the auto pricer's Physical/Pricing graphs.
- The installment engine's own repossession recovery still uses its supplied
  fixed depreciation; the two are not reconciled into one owner.
- No observed repossession or recovery history has been fitted.
