# Supplied revolving-card product graph

`card-product-graph-1` is an additive Rust analysis component for issue #19. It
adopts the existing `consumer-card-1` native daily state model into explicit
physical and pricing histories, collateral discounting, original base OAS and
paired scenario risk. It does not complete the consumer empirical, saved-book,
ledger, strategy, treasury or combined scale acceptance gates.

The [synthetic raw example](../../examples/models/card-product-graph-1.json)
starts with cash-advance principal 1,000 at APR 0.365 on ACT/365F. One day earns
exactly 1, the statement bills it, the supplied APR becomes zero and next-day
payment is 1,001 followed by a zero line limit. A collateral rate 0.05 and locked
OAS -0.05 cancel in discounting, giving PV 1,001; first-period gross recognition
is 1 and second-period interest cash is 1. This is a hand fixture, not observed
card performance or a trading recommendation.

The public API is `card_graph::analyze(&Request)` or immutable
`CardGraph::new`, `request`, `snapshot`, `base_anchor`, `reprice` and `repriced`.
Every position supplies a known opening `CardRequest` with contractual measure,
empty steps, exact position/currency/as-of identity and its complete native
contract/policy/source version. Each scenario supplies separate P, Q, Q-down
and Q-up action histories, explicit path IDs, weights and assumption IDs. The
graph generates native card requests with the selected measure, scenario run
ID and history identity. It retains all resulting daily states, statements,
recognition intervals, cash and default/recovery records, including the tail.

## Measures, rates and discounting

P histories can have independent IDs and probabilities. Every Q/Q-down/Q-up
collection across scenarios must have identical ordered path IDs and weights;
probabilities sum to one within `1e-12` and are never normalized. Even a
zero-weight path must pass admission and produce complete rights. The shared
randomness domain is explicitly `supplied_paired_histories-1`: this component
consumes supplied histories and generates no stochastic draws. Pairing provides
the same conditional history identity through scenario and curve bumps.

APR and changes in spending, payments, limits, delinquency, nonaccrual and default
are supplied actions. They are independent of the supplied collateral discount
curve. No three-month proxy, index projection, fixing publication assumption,
empirical behavioral response or transition law is inferred. If rates should
respond to a scenario, the caller must supply that scenario's reviewed APR
history. Supplied curves use their explicit collateral role, currency, source
revision, log-linear discount-factor interpolation and declared reject/flat-zero/
flat-forward right tail. No unused index curve is added.

PV is weighted Q lender cash (principal + interest + fees), discounted on the
scenario collateral curve and `exp(-base_oas * actual_days / 365)`. The explicit
`cash_timing=after_opening_actions` convention excludes same-as-of cash from PV
and `(left,right]` reporting; it retains those actions separately in
`opening_action_cash`. Native core actions execute after the preceding accrual
interval, before same-day statement/due processing, in supplied equal-date order.

`oas_target=locked` accepts an explicit source-linked spread in [-5,5].
`calibrate_positive_cash` solves the original positive Q future cash target by
bounded log-sum-exp bisection. Signed future Q cash requires locked OAS, including
signed rights on zero-weight paths. There is no target or backend fallback.
Repricing compares the complete serialized original base dependency: known
contract/opening, base Q histories, base scenario curve, target source and graph
model/paired-history identity. A changed dependency reports
`BASE_ANCHOR_CHANGED`; it requires a new explicit calibration. Reporting dates,
input revision, physical/down/up histories and nonbase scenarios can change
while preserving that anchor. No scenario re-solves OAS.

`supplied_history_dv01` is `(PV(Q-down,-bump)-PV(Q-up,+bump))/(2*bump_bp)` with
native spread-equivalent continuous curve bumps and the same original OAS.
This is risk conditional on supplied paired histories, not an empirically fitted
effective duration claim.

## Income, default and full rights

P reporting distinguishes gross contractual economic interest recognition from
collected interest, fees and signed principal cash. Recognition uses native
actual intervals, proportional overlap and explicit zero-length recognition at
grace loss or nonaccrual cash collection. Contingent grace interest is not earned
income. This is gross economic recognition, not effective-yield NII, accounting
income, reviewed allowance, charge-off timing or a journal.

Default reports principal, recognized and unrecognized contract interest, fees
and principal loss separately. Recovery cash is already net of collection cost
up to gross recovery; uncovered cost is negative fee cash. Gross recovery,
collection cost and net recovery are diagnostic fields and are never added to
cash again or charged a second time. Future recovery is retained through its
explicit date even when the reporting horizon ends at default. The separate
`physical_tail` covers all dates after the last report boundary through the
contract's supplied horizon.

The required terminal policy is `zero_rights_zero_limit_at_tail`. Every native
history must finish with exactly zero principal, billed/unbilled/unrecognized
interest, fees, contingent grace interest, limit and undrawn commitment. This
prevents an implicit balloon, truncated recovery or hidden commitment valuation.
It is a bounded modeling policy, not a legal assertion of account cancellation.
The caller supplies any terminal payoff/default/limit action; the graph does not
invent it. The native card model admits ACT/360 and ACT/365 Fixed; no ACT/ACT
consumer source adoption is implied.

## Admission and validation

Before projection, bounds cover 256 positions, 13 scenarios, 120 actual reporting
periods, 128 paths per role, 4,096 total runs, 4,096 steps plus cycles per path,
36,600 daily horizon and the core's 250,000 daily lot/cycle work per projection.
Worst-case daily points/recognition, statement/grace rows, action allocation and
sorting, calibration and report passes are charged against 250,000 generated
rows and 8 million work units. JSON input identity is streamed against 32 MiB
before canonical allocation; retained input/output/base identity uses a 128 MiB
component capacity estimate. This is not aggregate process RSS or combined
60,000-position workflow acceptance. Monetary values remain bounded by the
shared native limit and all unsupported states, incomplete paths, curve tails
and source/version mismatches fail closed. Cooperative checkpoints remain active.

`card_graph::audit_admitted_work` returns the exact conservative graph work
counter after graph admission and core input-domain validation for every explicit
measure/history request, before generating a schedule. The core public
`consumer_card::validate_request` forwards its existing private validator without
changing arithmetic. Chronological action feasibility (such as overspending or
overpayment after previous actions) still belongs to the native state replay.

`tests/card_graph.rs` includes hand daily interest/payment and curve-bump results,
independent closed-form OAS target, exact fixed-anchor repricing, independent P
weights, signed/as-of draws, default interest/principal separation, recovery
cost conservation, final-date tails and admission failures. Financial reference
tolerances stay `1e-12` relative and `1e-9` absolute.

## Sources and remaining scope

[CFPB Regulation Z §1026.53](https://www.consumerfinance.gov/rules-policy/regulations/1026/2021-02-17/53/)
distinguishes minimum payment from excess allocation and gives deferred-interest
exceptions. The reused native card kernel supports supplied minimum ordering
and descending APR excess allocation; it supports genuine temporary promotional
APR, not deferred-interest retroactive charging or every legal exception. Product
agreements and jurisdiction must still supply applicable reviewed policy.

The [OCC Credit Card Lending handbook](https://www.occ.treas.gov/publications-and-resources/publications/comptrollers-handbook/files/credit-card-lending/pub-ch-credit-card.pdf)
discusses income/nonaccrual, charge-offs and recoveries as distinct practices.
This component retains economic states and supplied recovery assumptions and
does not infer an accounting or regulatory determination. Future spending,
payment/default calibration, empirical validation, pricing-measure construction,
commitment option valuation, effective-yield accounting and durable saved/mixed
book adoption remain explicit separate gates.
