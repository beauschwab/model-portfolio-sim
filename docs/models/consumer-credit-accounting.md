# Acquired installment events and reviewed credit accounting

`consumer-credit-accounting-1` is a bounded native component for one purchased,
initially performing auto or personal loan with one supplied physical history.
It calls `consumer_installment::project` once as the sole economic owner and
feeds its default events into `credit_accounting::evaluate`. It does not compose
two daily ledgers or make the performing mixed-book route credit capable.

The request retains the complete raw contract, path and economic-source identity,
purchase principal/cost/date, original annual effective yield and anchor, income
review, accounting/recovery/cost policies and explicit allowance/chargeoff reviews.
The economic-source revision must equal the contract's source revision. Purchase
principal equals the contract's original and opening principal. The admitted
`supplied_non_pcd_gross_cost-1` convention seeds gross carrying value at purchase
cost. Supplied allowance is a separate contra-asset opening balance. No purchased
credit deterioration gross-up or acquisition equity/provision posting is inferred.

## Economics and original income

The contract must start performing with no opening accrued/unrecognized interest
or fees. Only explicit payment, payoff and default observations are admitted.
Original payment, actual payment dates, ACT/360 or ACT/365 fixed contractual day count and product recovery
assumptions remain the consumer projector's inputs. Its terminal scheduled payment,
payoff and default have distinct source state and intent identities. Contractual
interest in an interval is `principal * note_rate * actual_year_fraction`; daily
recognition apportions that exact interval amount by each calendar day's fraction
of the interval. A whole coupon is not counted as daily income a second time.

Before default or payoff, `dated_income::evaluate` advances gross carrying value
using the supplied frozen original annual effective yield:

```
effective_interest = opening_gross_carrying * expm1(log1p(original_yield) / 365)
accrued_change     = contractual_earned - cash_interest
basis_change       = effective_interest - contractual_earned
principal_change   = -cash_principal
```

This uses actual calendar days, including leap days; the annual EIR convention is
ACT/365 fixed. Contractual coupon accrual retains its separately declared day count.
The native prefix suppresses terminal payoff cash in its recurrence so it exposes
the carrying adjustment immediately before the explicit retirement policy. The
adapter then posts actual cash/principal/accrued settlement and recognizes the
remaining signed basis in earnings under `recognize_in_earnings_at_payoff-1`.
Default consumes the same predefault principal/accrued/basis; original EIR stops
at default, and the reviewed shared accounting policy transfers or reverses the
accrued right and transfers or recognizes the basis. Negative original EIR and
discount basis remain distinct from contractual negative coupon; the installment
projector admits nonnegative contractual rates only.

The original yield is supplied and retained, with no inference from a pricing
history, target price or risk-neutral recovery discounting. Incompatible negative
gross carrying value is rejected by the existing income operator. There is no
income, cash, principal or terminal basis plug.

## Gross recoveries and costs

`gross_recovery_with_credit_loss_cost-1` keeps costs outside the principal claim:

```
principal_only_loss = default_principal - gross_recovery
recovery_cash      = +gross_recovery
cost_cash          = -reviewed_recovery_cost
total_economic_loss = principal_only_loss + recovery_cost
```

The shared credit operator receives the gross principal-only recovery tail and
principal-only expected loss. Actual cost has a separate same-date debit to credit
loss expense and credit to cash, retaining its review and claim identity. The
consumer projector's net recovery and total economic loss are retained and
reconciled; economic expected loss does not create another journal provision.
The initial lane requires `0 <= cost <= gross <= default_principal`. It rejects
surplus collateral proceeds and costs exceeding gross recovery until ownership
of those cash obligations is explicitly supplied.

The synthetic example has principal 100, note 36%, ACT/360 and default after 10 days,
so accrued coupon is 1. Gross recovery 60 and cost 10 settle after 20 more days. The
principal-only loss is 40, net cash 50 and total economic loss 50. An independently
supplied opening allowance 40 and chargeoff 40 consume allowance once; cost 10 is
the remaining credit expense. Under original EIR 0, accrued reversal and signed
basis recognition offset; neither creates additional net coupon income.

## Order, rights and integration seam

`Daily.owned` is the state after that actual day's complete processing. Order is:

1. Original EIR and contractual recognition, followed by actual product cash.
2. Shared before-credit allowance review, default transfer and accrued/basis policy.
3. Reviewed chargeoffs, gross recovery cash and after-credit allowance review.
4. Separately reviewed recovery cost cash/expense.

`acquisition_opening` seeds live principal, cost-minus-principal book adjustment
and allowance only. It emits no acquisition cash/equity journal. An eventual
exclusive mixed owner must acquire and seed those balances once against its actual
opening account, apply each allocation and physical path weight once, retire future
generic contractual/default ownership atomically and process these intents in the
same daily GL. Standalone journal concatenation is not integration.

Every intent has source/policy/claim metadata; lineage retains path, original yield
anchor and source observation/origin coordinates. Reporting state is an actual-date
snapshot. Full replay retains all supplied recovery and review dates. Live contractual
tails must finish within the full source horizon; default claims with unpaid
principal, accrued interest or basis remain explicit unless reviewed actions retire
them. The primitive does not delete those claims or claim full-account publication.

## Admission and limitations

Limits are 1080 days, 64 review events, 2 million conservative work units, 1 MiB serialized
raw input, 64 MiB serialized result and the shared 1e15 monetary magnitude bound.
An optional native control closure runs before and between stages and daily steps;
it supplies no financial values. The existing consumer projector and income calls
are individually bounded and checked immediately before/after. Component budgets
are not an aggregate process-RSS limit. No fast math or final tolerance change is
introduced; ownership reconciliation uses existing `1e-9 + 1e-12 * abs(reference)`.

Modifications, nonaccrual, fees, prepayment penalties, seasoned/PCD acquisition,
interest recoveries, draws, migrations and negative contractual coupon are excluded.
Histories, recovery assumptions and review dates are supplied, with no empirical
hazard/recovery fit, stochastic CRN or measure conversion claim. Card and mortgage
economics require their own event adapters. Native mixed ledger ownership, independent
persisted verification, capital/solver adoption, empirical and combined scale gates
remain open.

## Primary references and evidence

[FASB ASU 2016-13](https://storage.fasb.org/ASU_2016-13.pdf), summary pages 2–4,
distinguishes an allowance from amortized cost and requires historical, current and
reasonable/supportable forecast information for expected losses. It also describes
different acquisition accounting for purchased credit deterioration. This component
keeps those judgments supplied and excludes that acquisition lane; its policy names
do not establish accounting compliance. The ASU communicates amendments and is not
itself the authoritative Codification.

The example and dedicated native fixtures independently check accrued coupon,
principal/gross/net recovery and cost conservation, original-EIR premium payoff,
retained default rights, same-day order, invalid ownership and cooperative cancellation.
The public component's native suite passes 10 tests, and strict all-target shared
core Clippy passes. The installed independent verifier and real debug and rebuilt
official `portfolio-lifecycle` processes pass 63 tests: 39 pure financial/source cases and
24 process cases, including 15 complete accepted results and unsupported-source
rejections. These retain exact ordered source/state/identity checks and the existing
financial tolerance. The explicit analysis adapter is `account_consumer_credit`;
the durable model worker checks sources before and after persistence. Ten focused
durable primitive publication and failure-fence checks pass against the rebuilt
official artifact. These component and application gates do not establish the
mixed-ledger, broader product or empirical acceptance gates.

