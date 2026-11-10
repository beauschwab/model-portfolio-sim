# Fitted consumer states into daily product paths

`consumer-behaviour-product-paths-1` is an explicitly selected analytical route
for auto, personal and card products. It fits the retained
[consumer specification](consumer-behaviour-spec.md), projects shared Physical
CTMC states, then executes the existing daily installment or card cashflow owner.
It does not publish a saved book, journal, capital or FTP receipt.

## One economic owner

Every product source declares the exact shared path ID/key and complete original
product contract. Input order, currency, Physical measure, opening state and
integer start/end dates must match. Shared state paths must have zero principal,
undrawn, accrual and basis, no monetary boundaries, and zero accrual/default-draw
rates. Their fitted hazards and original supplied draws determine states only.
They do not supply a second set of cashflows or economic losses. The product
owner supplies actual principal, coupon, fees, payments, default and recovery.

Auto collateral depreciation/recovery and personal/card collection assumptions
remain the explicitly supplied product laws. This route does not fit LGD or card
spending/payment/EAD behavior (#93/#94). Borrower features and macro paths are
supplied drivers; they are not recomputed endogenously from projected balances.
There is no Physical/Pricing conversion, OAS recalibration or probability-weighted
monetary rescaling. Each path retains its complete unweighted output.

## Explicit daily clock and order

The selected `next_calendar_day_boundary` convention maps a fractional event at
ACT/365F year `t` to `origin_ordinal + ceil(365*t)`. An event already exactly on a
day boundary stays there. Original fractional time and mapped ordinal are both
retained. This is an explicit discretization assumption; it is not exact
continuous-time daily accrual, and it never silently rounds an event backward.

The policy explicitly maps the three shared live states into the product's
performing/delinquent/nonaccrual states. Default and prepaid use economic actions,
not a terminal `SetState`. Two generated transitions on one mapped day reject.
Supplied product credit-state/default/payoff actions reject rather than competing
with fitted events. A supplied action on a generated transition date also rejects,
except for the card closure rule below. No future supplied action is dropped when
an earlier generated exit makes it infeasible; the product owner rejects it.

Installment actions execute under their existing order: accrual through the date,
explicit action, then an automatic payment if still performing. A generated
payoff replaces an automatic payment. Contractual payoff before a later generated
event rejects. Complete supplied contractual dates and recovery delays remain;
insufficient recovery horizon rejects rather than truncating claims.

## Card closed-with-zero-balance

The additive card closure owner is selected only through this analytical route
or its explicit Rust function. Ordinary `consumer-card-1` JSON cannot implicitly
select it. On the mapped closure date, normal transactions, due processing and
statement posting occur first. Closure then requires **exactly zero**, without
epsilon, in every lot's principal, billed/unbilled/unrecognized interest, fees,
contingent grace interest and pending statement rights, plus the remaining minimum.

This permits an explicitly supplied same-day full payment to precede closure.
Closure creates no payoff, waiver, interest, allowance or cash. It sets the
undrawn limit to zero and marks the account paid off. Subsequent supplied actions
reject. Daily zero closing points continue through the complete horizon; future
statement cycles no longer generate statements for the closed account. A zero
balance with an open line stays live under the unchanged historical route.

## Bounds, controls and evidence

Admission precedes fitting: at most 1 MiB input, 16 product paths, 4,096 total
product days and 4,096 prospective product output rows, plus the shared fit/path
bounds. Card admission includes all potential opening/draw/fee lots on every day,
both daily and determination-date recognition, actions and statements. Installment
admission includes every supplied due/action date and maximum generated jump count.
Before fitting, a joined response budget reserves 12 times raw bytes, plus 4 MiB
and 4,096 bytes per prospective shared-event/product-row slot; budgets above
32 MiB reject. Actual output cardinality and serialized bytes must fit the retained
reservation. This is a conservative budget policy, not an exact size estimate.
These are work/output limits, not an aggregate
RSS guarantee. Full raw contracts, states, compiled sources and outputs are kept.

Strict original-byte decoding and the original entry deadline cover the native
protocol. Cancellation and scalar deadlines continue through fitting, path
generation, product execution and serialization. The caller's original control
is checked inside likelihood, shared path and daily product loops, as well as
before and after each stage; the transport separately supplies a watchdog.

Hand cases exercise all three products through panel fit, state projection and
daily product cashflows, including full default recovery tails and unchanged
draws. Card tests exercise same-day collection, zero-line distinction, tiny gross
rights, future-action rejection and unchanged historical statement mechanics.
Synthetic tests do not establish observed accuracy, policy approval or deployment.
Full installed-artifact regression and independent durable publication remain
separate from this standalone analytical implementation.
