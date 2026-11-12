# Card behaviour: payment, purchase, utilisation and exposure at default

`consumer-card-behaviour-1` is a fitted behaviour layer for credit cards. It sits
above the unchanged [`consumer-card-1` statement mechanics](consumer-card.md):
it fits behaviour from an observed statement panel, then turns the coefficients
into ordinary supplied draws, payments and issuer actions. The mechanics remain
the only owner of balances, interest, grace, default and recovery cash.

Rust owns `portfolio_model_core::consumer_card_behavior::{fit, project}`. This
is a native library component for issue #93 under tracker #25. It has no API
route, durable job, saved-book adoption or pricing publication.

## Behaviour states

Each statement is classified from observed cash, never from a label:

| State | Rule |
|---|---|
| `transactor` | payment within `full_payment_tolerance` of the statement balance (a zero statement is a transactor) |
| `minimum_payer` | payment at or below minimum due plus `minimum_payment_tolerance` |
| `revolver` | anything between |

The tolerances are supplied currency amounts. A payment above the statement
balance, a minimum above the balance, or a negative or non-finite amount
rejects. Behaviour states are separate from credit states: delinquency, default
and cure stay with the credit model and are supplied to the projection.

## Panel

One `PanelRecord` is one statement cycle of one account: the statement and next
statement clocks, statement balance, minimum due, payment received against that
statement, the limit, purchases made after the payment and before the next
statement, raw covariates with their availability time, a term level, and
`default_balance` when the account defaulted in that cycle. Records of an
account must be contiguous, with nothing after a default. Covariates use the
declared columns of the transition template (name, role, unit, frozen offset
and scale, raw domain). Typical columns are utilisation band, score band, APR
spread, promotion status and unemployment change; all are supplied numeric
codings. Nothing is imputed, and a value outside its domain rejects.

A record is training when its cycle ends at or before `train_end` and holdout
when it starts at or after it. A cycle that crosses the cut rejects. Holdout
records enter no objective, scaling or selection; they only produce
diagnostics.

## What is fitted

**State transitions.** The request carries a `credit-covariate-1` fit template
with no segments. The module builds one exact segment per cycle and calls the
existing [covariate estimator](credit-covariate.md) unchanged, so bounds,
ridge, identification, eligibility and the coefficient identity are the same.
The estimator is typed on three live origins; behaviour reuses those slots only
(`performing` = transactor, `watch` = revolver, `delinquent` = minimum payer).
Edges to `default` or `prepaid` reject: credit exits are not behaviour.

A behaviour class is determined once per statement, so each record exposes one
full cycle and the fitted rate is transitions per cycle-year. The cycle
probability is `rate × cycle_years`, and the remainder stays. A rate too large
to be a probability rejects; it is never clipped.

**Payment rate.** For revolvers, the payment above minimum due as a fraction of
the statement balance above minimum due:
`payment = minimum + logistic(a + b·z) × (statement − minimum)`.
Transactors pay the statement and minimum payers pay the minimum, so a payment
is always within `[minimum due, full balance]`.

**Purchases.** One model per state: purchases as a fraction of open-to-buy,
`purchase = logistic(a_s + b_s·z) × (limit − balance after payment)`.
Records with no open-to-buy carry no purchase information; they are counted
and not fitted. Purchases above open-to-buy reject.

Both fraction models are quasi-binomial fractional logits fitted by
deterministic Newton steps with step halving inside supplied parameter boxes.
Convergence is tested on the gradient of the mean objective. A model that does
not converge, is not identified by its training rows, or stops on a bound makes
the fit numerically ineligible.

**Exposure at default.** For each training default, the reference statement is
`ccf_lookback_cycles` (3 to 6) cycles earlier. The state-conditional credit
conversion factor is

`CCF_s = Σ max(balance at default − reference balance, 0) / Σ (reference limit − reference balance)`

over defaults whose reference state is `s`. A state with no usable training
default needs a reviewed `supplied_ccf`; otherwise the fit rejects. A ratio
above one rejects. `exposure_at_default` is
`outstanding + CCF_s × max(limit − outstanding, 0)`: the undrawn line is
converted once.

## Projection onto the mechanics

`project` takes the fitted model, a complete `consumer-card-1` request with no
steps, an initial behaviour state, one driver per statement cycle and a
supplied issuer policy. For each cycle it:

1. applies supplied credit-state changes, delinquency repricing and limit
   actions on the day after the previous due date (the as-of date first);
2. draws this cycle's behaviour state from the transition probabilities with
   the cycle's supplied shared uniform (stay first, then the other states in
   slot order);
3. makes one purchase of the fitted fraction of open-to-buy, if it is at least
   `minimum_purchase` and the account is performing;
4. reads the statement the mechanics produce and pays it on the due date
   according to the state.

The generated steps are returned as an ordinary `consumer-card-1` request;
replaying it reproduces the result exactly. With `utilisation_column` set, that
covariate is replaced each cycle by simulated billed debt over limit, so
purchases fall as utilisation rises and the account drifts to the utilisation
at which the fitted purchase and payment equations balance. That level is a
property of the state's coefficients, not a separate parameter.

**Default.** `default_cycle` is supplied by the credit model. On that cycle's
due date the payment is replaced by one draw of `CCF_s × undrawn` followed by
the mechanics' default. Draws need a performing line, so a delinquent account
defaults on its outstanding balance only. The mechanics' conservation identity
holds with the conversion counted once in drawn principal.

**Issuer actions** are supplied policy, never fitted: limit multipliers by
cycle and covariate band (a decrease leaves existing debt in place and forces
no payment), closure after a declared number of idle cycles with no rights
(through the mechanics' zero-rights closure owner), and one APR reprice at the
first delinquent cycle.

## Eligibility

- `numerical_eligible` requires the transition fit and all four fraction models
  to be eligible.
- `adoption_eligible` additionally requires an `observed` panel. Synthetic fits
  are never adoptable, and `adoption` projection rejects them;
  `analytical_only` always returns `adoption_output=false`.
- `publication_eligible` is always false. Card pricing publication stays
  rejected until an observed account panel with reviewed holdouts is supplied
  and independently verified.

The model identity is a SHA-256 over the typed model. Changing holdout records
or record order leaves it unchanged; changing training data changes it.
Projection rejects a model whose identity does not match.

## Bounds

At most 20,000 panel records, 60 projected cycles, 200 fraction iterations, and
the covariate estimator's own limits (24 parameters, 20 million work units).
Each projected cycle replays the mechanics, which keep their daily work bound.
These are component limits, not an aggregate memory guarantee.

## Evidence

`tests/consumer_card_behavior.rs` has ten controlled synthetic fixtures:

- A pure transactor book earns no interest and carries no revolving balance.
- A pure minimum-payer book matches a hand recurrence for minimum due,
  statement balance and principal, and replays identically.
- A 300-account, 24-cycle generated panel recovers payment and purchase logit
  coefficients within 0.05, cycle transition probabilities within 25% and CCFs
  exactly; holdout mean fractions are within 0.03.
- Default converts the undrawn line once and reconciles to the mechanics.
- Holdout and ordering changes leave the identity unchanged.
- Issuer actions, utilisation feedback, shared-draw determinism and contract
  refusals.

These fixtures establish mechanics and estimator behaviour. They are not
evidence of predictive accuracy.

## Not done

- No transport, API route, durable job or independent publication verifier.
- Outputs are not yet wired into `credit-model-1` EAD or the card pricer's
  full-tail Physical/Pricing graphs; interactive evaluation is unchanged.
- The interval-censored estimator is not offered for behaviour transitions.
- No observed account panel has been fitted. Promotional balances, cash
  advances and fees are not behaviourally modelled; purchases use the purchase
  class only.
