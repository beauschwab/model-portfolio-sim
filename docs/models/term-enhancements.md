# Explicit contractual term paths, conditional exercise and CD elections

`portfolio-risk-native::term_enhancements` adds three separate Rust capabilities:

| Native function | Raw protocol | Interpretation |
|---|---|---|
| `indexed::build_path(ContractPathRequest)` | `indexed-contract-1` | Supplied complete dated contractual/index scenario path |
| `exercise::value_exercise(ExerciseRequest)` | `conditional-exercise-1` | Native conditional continuation-value exercise on a supplied pricing tree |
| `cd::step_cd(CdElectionRequest)` | `cd-election-1` | Actual-date supplied physical withdrawal/maturity elections |

Every inner request requires schema `term-enhancements-1`, model version
`term-contracts-1`, uppercase three-letter currency, and monetary unit
`currency_units`. The native product JSON protocol wraps it with its route schema,
`threads` and `request`. Calculations execute in Rust; Python transports raw input.
These APIs are independent of the historical saved `annuity`, front-quarter
floating proxy, monthly option dates and threshold kernels. They do not change
those saved economics. Adoption requires explicit new input/model identity and a
rebuilt graph/unit library.

## Contractual path and floating index selection

The path specifies contract, assumption and path identities; actual accrual and
settlement dates; opening principal; a complete principal plan; and one coupon
specification per period. Payment may precede its unadjusted accrual end, including
modified-following or preceding settlement. Exact ordinals are retained. A
principal balance is constant within each supplied period; changing balances
require separate periods. Contractual cash recognition is separate from daily
earned-interest accounting.

Fixed coupons use `principal * annual_rate * actual_year_fraction`. Supported
actual day counts are ACT/360, ACT/365F and ACT/ACT ISDA. ACT/ACT splits leap-year
accrual at January 1. A caller requiring historical 30/360 uses the existing
explicit dated-term producer rather than silently changing its convention.

Indexed coupons require an index ID, kind and tenor/window; reference-calendar
identity; declared complete calendar mapping; exact reset and observation dates;
selected observed or conditional projected rates; separate index and all-in
bounds; margin treatment; and explicit lookback/observation-shift settings.

* `term_simple` requires a term or published-average index and one in-advance
  observation with an exact UTC reset cutoff at/before the accrual-start date.
* `daily_simple` and `daily_compounded` require an overnight index. Interest
  intervals exactly partition the coupon period. Under observation shift,
  observation intervals are also contiguous.
* Index bounds apply first. All-in bounds apply to selected index plus margin.
  They do not cap the final compounded effective period rate.
* With simple margin, index compounding uses the bounded all-in rate minus margin;
  margin accrues on the actual interest dates. A shifted observation changes the
  index weights, not the simple-margin weights.
* Compounded margin includes margin in each compound factor. Balance compounding,
  rate fallback, inferred publication, and missing observations are rejected.

For observation `i`, let `w_i` be its actual interest or shifted observation
year fraction and `u_i` its actual interest year fraction. Let `a_i` be the bounded
all-in rate and `m` the margin. On constant principal `B`:

```
daily simple, simple margin: B * sum((a_i - m) * w_i + m * u_i)
daily compounded, simple margin:
    B * (expm1(sum(log1p((a_i - m) * w_i))) + m * sum(u_i))
daily compounded, compounded margin:
    B * expm1(sum(log1p(a_i * w_i)))
```

Each compound factor must be positive. `log1p`/`expm1` preserve near-zero rates.
Signed negative interest is permitted within the positive claim-factor domain.

An observed source carries immutable fixing ID/revision and UTC publication time.
It must be available at the observation-selection cutoff and the declared observed
snapshot cutoff. The snapshot cutoff cannot follow the valuation date. A term
observation must also be available by its exact reset cutoff: an 08:00 publication
is rejected for a 07:00 reset and accepted for a 09:00 reset.

A projected source carries conditional model ID/revision, path ID/revision,
measure and UTC information time. Path and measure must equal the outer request;
projected coupons reject `Contractual` measure. Information must be available at
the exact observation-selection cutoff and, for term coupons, at reset. Projected
overnight selection must be on/before its observation date. The path is an explicit
conditional-model input, never inferred from a realized future discount ratio or
relabeled as a published benchmark.

**The arithmetic verifies dates, coverage and information availability; it does
not independently verify business-day calendar contents or the supplied business-
day lookback mapping.** Indexed audit records explicitly return
`calendar_mapping_verified=false`. The reviewed calendar/mapping producer must
establish that mapping before calling this API. Different index tenors are
different input identities even when a synthetic fixture happens to supply the
same rates.

`PrincipalPlan::Supplied` supports supplied sinking/equal-principal schedules and
balloons whose complete cash principal sums to opening principal.
`RetainedPayment` uses the supplied original payment, without recomputing it over
remaining tenor. Each regular principal amount is `original_payment - interest`.
Unsupported negative amortization rejects the request. The terminal rule is an
explicit residual payoff or a supplied extra balloon that must reconcile to the
recurrence. The last regular payment can shorten to the remaining principal.
Signed contractual fees are separate from principal/interest and do not imply
premium, discount or fee amortization.

The result retains all selected source identities, opening/closing principal,
coupons and terminal balloon audits. Its `DatedSchedule` contains the full tail;
`principal_after_reporting` is a separate report-horizon balance.

## Conditional call/put exercise

The exercise engine performs backward Bellman valuation over a supplied
chronological, topologically ordered conditional state DAG. Node zero is the sole
initial state at `asof`. Every node must be reachable; edges lead to later actual
dates. Branch probabilities are finite, positive and sum to one. Recombining
branches are supported. Child principal must equal parent principal after its
scheduled payment. Every unexercised leaf fully settles principal: no terminal
grid clipping or valuation residual is permitted.

Requests carry immutable contract, exercise policy, tree, market discount source,
base OAS anchor and common-random-number identities. Measure must be `Pricing` and
discount basis must be `market_without_oas`. Each edge supplies a conditional
probability and market discount factor. Rust applies

```
edge_discount = market_discount * exp(-fixed_base_oas * edge_year_fraction)
continuation = scheduled_cash + sum(probability * edge_discount * child_value)
```

Fixed base OAS is supplied and applied once. The component never recalibrates OAS,
draws random numbers or generates a rate tree. It preserves the supplied CRN
identity. Tree calibration, state sufficiency, branch probabilities and discount
source compatibility belong to the native market producer and its independent
validation.

Permitted call and put dates must occur exactly in the state DAG. Redemption is
face principal plus separately supplied accrued rights, clean redemption premium
and exercise fee. Clean price factors affect premium/discount cash fees rather
than changing extinguished face principal. A rule declares whether exercise
occurs before scheduled cash (scheduled payments are replaced) or after scheduled
cash (those payments survive and redemption applies to remaining principal).
Accrued rights have their own immutable source identity. An after-coupon right
must exclude the coupon just settled; that contractual rights allocation remains
the source producer's responsibility.

Issuer call selects `min(continuation, redemption)` and holder put selects
`max(continuation, redemption)` from the holder's perspective. Conflicting
same-date rights require explicit issuer or holder priority: the priority holder
acts last in the min/max composition. Strict comparisons retain the prior
decision on exact ties; absent a prior exercise, ties continue. Receipt/payment
direction changes the final reported sign, not the economic holder policy.

This is continuation-value exercise on the supplied pricing model. It does not
assert actual issuer behavior or optimality outside that model. It supports
immediate permitted-date settlement. Notice periods, delayed settlement,
make-whole reference pricing and other omitted contract mechanisms require a new
explicit policy representation, rather than a rate threshold or implicit date
move.

Durable `conditional-exercise-1` publication now independently replays the raw
tree's values, stopping policy and complete expected cash rights before and after
Parquet persistence. Native cooperative controls reach the tree through Rayon.
See [exercise publication and validation](term-exercise-publication.md) for the
receipt, unchanged financial tolerances and remaining adoption boundaries.

Forward alive-probability propagation follows the selected stopping policy and
removes future coupon/principal rights after exercise. The result reports node
continuation/value/decision and alive probability, a complete undiscounted expected
dated schedule, and expected principal after reporting. The expected schedule is
**Pricing measure**, not observed or Physical ledger cash. All call/put principal
cash reconciles to the initial face balance in expectation; premium is separate.
`canceled_future_principal` refers to the unexercised child principal tail.

## CD withdrawal, penalty and maturity state

`step_cd` takes a supplied `Physical` election and immutable contract/election/
policy identities. Opening state includes principal, signed accrued right,
last-accrual date, actual maturity, fixed annual rate and actual day count. Rust
earns interest to the election or maturity date, then applies the explicit grace
rate if election is within the contract's grace interval. It does not infer a
renewal during that delay. The earned delta is returned separately for state-aware
daily accounting; cash settlement is not itself another accrued recognition.

Early withdrawals must precede actual maturity, respect the earliest withdrawal
date, explicit channel restrictions, partial-withdrawal permission and minimum
remaining balance. Brokered withdrawals require an explicit contract override;
channel labels do not imply that liquidity exists.

Supported penalty rules are no penalty, a supplied number of interest days with
explicit penalty rate and 360/365 denominator, a principal fraction, or a fixed
amount. Inclusive dated waiver windows and a monetary cap are explicit. A policy
limits the penalty to positive allocated accrued interest or to principal plus
the signed allocated accrued claim. Penalty cannot create a negative total
withdrawal payment. Partial withdrawals allocate accrued rights pro rata to
withdrawn face principal; a contract requiring other allocations needs a distinct
policy, not a silent approximation.

Results separate gross principal removed, actual principal cash returned,
interest cash paid, penalty from accrued interest and penalty from principal.
Settlement components show gross extinguished principal, allocated interest and
the negative penalty fee; their sum equals external cash. Daily accounting must
use that explicit gross/net split. The remaining principal and accrued right
retain their complete dated maturity tail.

Maturity elections explicitly allocate principal and signed interest among
cash-out, renewal and switching. Both allocation sums must reconcile to the
matured state. Renewal and switching require permission and separately identified
new fixed-bullet contracts, with distinct position IDs, maturity, coupon and day
count. New principal includes the selected signed interest capitalization. It
must remain nonnegative. No future renewal is assumed.

`settlement_schedule` reports the same-date gross old claim settlement and each
new contract's reinvestment separately. `net_external_cash` therefore equals the
cash-out principal plus cash-out interest, with the declared direction.
`remaining_schedule` contains the old residual or new fixed bullet maturity
tails, separate from today's settlements. Consumers must not add those tails to
today's funding; they are future cashflows. All new maturities must fit the
explicit complete tail domain even when the report horizon ends earlier.

These policies implement reviewed supplied terms. They do not fit product,
channel or account withdrawal/renewal hazards, infer customer switching, or
certify a disclosure regime. Observed behavior holdout evidence is separate from
the synthetic financial fixtures.

## Bounds, identity and integration

The contractual path admits 1..4096 periods and at most 250,000 total index
observations. Exercise admits at most 100,000 nodes, 400,000 edges and 4096 permitted
dates. CD admits at most 256 ordered, nonoverlapping waiver windows and at most
366 grace days. Lookbacks are at most 366 business days. All actual-date domains
are at most 36,600 days and lie within Gregorian years 1..9999. Monetary values are
finite and bounded by shared `MAX_MONETARY_AMOUNT` (1e15). Metadata IDs/revisions
are 1..128 control-free bytes, with 32 MiB combined metadata admission. The native
JSON transport adds its separate request/output limits. These component bounds
are not an aggregate process RSS guarantee.

Inputs reject unknown fields. Every curve/tree/path/fixing/contract/calendar/
policy/measure/unit/model identity belongs in graph/cache identity, including
selection timestamps, exact dates, clean-price priority and complete tails.
Saved legacy models remain readable under their own identities. Model adoption
requires explicit source preparation, native graph/unit build and worker rebuild;
interactive strategy evaluation consumes prepared coefficients.

`build_path` deliberately generates contractual/indexed flows without borrower
default. Commercial borrower and issuer credit must use the appropriate shared
credit state, EAD/default-draw, nonaccrual and complete recovery tail coordinator;
they cannot be replaced by a second proportional survival haircut. The exercise
module requires the same credit interpretation in tree cashflows and discounts.
The CD module's modeled Physical elections must be joined to state-aware dated
accrual and accounting intents, then independently persisted-journal verified.
No successful publication or deployment is implied by these component tests.

## Independent examples and verification

The checked-in examples are complete native protocol envelopes, not calibrated
market or customer observations:

* `examples/models/term-indexed.json`: February 1 to March 1, 2024 has 29 days.
  One-month selected rate 0.02 plus margin 0.01 on face 100 gives
  `100 * 0.03 * 29 / 360 = 0.2416666666666667`; principal 100 is paid March 1.
  Reporting ends February 1, so principal after reporting remains 100.
* `examples/models/term-exercise.json`: after the January 2 coupon of 5,
  continuation is `5 + 105 = 110`; call costs `5 + 100 = 105`. The issuer calls.
  Value January 1 is `0.98 * 105 = 102.9`; maturity alive probability is zero.
* `examples/models/term-cd-election.json`: five days of interest at 0.365 on 100
  gives 0.5. Withdrawal face 40 allocates 0.2 interest. Penalty is
  `40 * 0.05 * 90 / 365 = 0.4931506849315068`. External cash is
  `40.2 - penalty = 39.70684931506849`; remaining face is 60 and complete
  January 11 maturity interest is 0.6.

`tests/term_indexed.rs`, `term_exercise.rs`, `cd_elections.rs` and
`term_examples.rs` provide 21 independent tests. They cover retained original
payments/balloons, leap stubs, fees, signed/zero coupons, complete tails,
publication/reset availability, distinct tenors, weekend/shift/bounds/near-zero
compounding, conditional exercise versus wrong perfect foresight, call/put bounds,
cash ordering, simultaneous priority, fixed OAS once, CRN identity, partial/full
withdrawals, penalty limits, brokered restrictions, grace, renewal/switch funding
and failure admission.

A stochastic refinement fixture uses normalized binomial states converging to
`100 exp(0.3 Z - 0.3^2/2)`. Its independent analytical call cap is
`E[min(100, terminal)] = 200 Phi(-0.15) = 88.0764615259515`.
16/64/256-step approximations approach this value. This validates the conditional
exercise arithmetic and discretization of this synthetic fixture; it is not
convergence evidence for a calibrated production rate tree. A separate
deterministic discount subdivision preserves option-free economics exactly within
existing numerical tolerance.

## Primary sources and remaining issue scope

The [ARRC syndicated-loan technical appendices](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2020/ARRC-Syndicated-Loan-Conventions-Technical-Appendices.pdf)
distinguish daily simple/compound methods and lookback/observation-shift weighting.
This implementation makes those choices explicit rather than substituting the
front-quarter simulated rate. The caller still supplies the reviewed date mapping
and conditional index model.

[Lim, Li and Linetsky's callable/putable bond research](https://arxiv.org/abs/1206.5046)
formulates exercise as a stopping game with backward valuation. The
[QuantLib callable-bond source](https://github.com/lballabio/QuantLib/blob/master/ql/experimental/callablebonds/discretizedcallablefixedratebond.cpp)
separates call/put application and coupon ordering. These support the explicit
Bellman and same-date-order design; this implementation does not import QuantLib
or claim its market-model calibration.

[CFPB Regulation DD account-disclosure guidance](https://www.consumerfinance.gov/rules-policy/regulations/1030/4/)
addresses early-withdrawal penalty calculation and renewal/grace terms. Its
[interest guidance](https://www.consumerfinance.gov/rules-policy/regulations/1030/7/)
also distinguishes withdrawn principal, accrued interest and grace-period
interest. These motivate explicit supplied contract policies. They do not imply
that one penalty or renewal rule applies to every CD.

Issues #15/#16 remain broader than these components: native conditional market
projection/tree generation and graph risk adoption, complete commercial/issuer
credit integration, unit/ledger/NII/FTP follow-through, supported delayed or
make-whole exercise, and empirical withdrawal/renewal calibration still require
their own acceptance evidence. A component implementation alone does not close
those issue acceptance criteria.

## Chronological CD sequence stage

The contained Rust `cd-election-sequence-1` owner in
`artifacts/cd-election-sequence-stage/` uses the same native CD contractual step
for repeated withdrawals, renewal and later elected actions. Native-derived
balances and original/new contract identities condition each subsequent action.
Only actual election settlements and the final unsuperseded bullet tails enter
its complete cash schedule. Earlier step-level maturity forecasts remain source
receipts and must not be added to that cash a second time. Same-date action
precedence is explicit; renewal acquisition cash offsets old gross settlement
only through signed external cash, without erasing either transaction.

For face100 at36.5% actual/365 from Jan1 to Jan11, withdrawing40 on Jan6 and20
on Jan9 allocates interest0.20 and0.16; remaining face40 pays interest0.40 at
maturity. Total0.76 is the outstanding-face time integral. A Jan8 reporting
date neither truncates the tail nor implies an invented repayment. Penalty,
signed coupon rights and renewal math remain owned by the existing shared native
contract. The sequence CLI retains original JSON separately from its typed DTO.

Eight native cases and four independent actual-process tests pass, including
twelve independently calculated signed rate/penalty fixtures; all-target strict
Clippy passes. The retained validation receipt SHA-256 is
`ba7df7d1e97de6d9d70a1960fc0db1242551d1782052434d8f9e87bf2157fa28`.
No installed source/default native artifact changed. This stage requires at least
one election; a shared no-election terminal projection is required before
complete graph-history adoption. Expanded switch/grace policies, compounding/
periodic payout and explicit remaining-rate changes require additional source
policies and independent raw replay. The original issues still require public
graph/base-OAS/cache, accounting/FTP/library/saved-book adoption, joint exercise
generation and observed holdouts. Detailed schemas, control bounds and primary
modeling references are in the stage README; none of these checks establish
empirical behavior or regulatory/accounting eligibility.
