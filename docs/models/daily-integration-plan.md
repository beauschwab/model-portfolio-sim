# Next stage: dated schedules and native credit in daily replay

Version 0.29.7 implements the explicitly selected loan-only
[joint credit state](credit-state.md) and [credit replay](credit-replay.md) route,
with independent persisted source/accounting checks and durable API/UI publication.
It also connects verified credit closing journals to native capital preparation.
The mixed product/graph/candidate adoption and complete scale gates below remain
required; component execution is not their completion.

Current follow-through adds [realized event-time credit](credit-path-ledger.md),
[native consumer graph capture](graph-dated-ledger.md) and
[frozen covariate-driver replay](driven-credit-ledger.md). The modeled routes
retain raw model inputs, prepared economic sources and independent source/posting
verification through durable API publication. Consumer capture admits explicitly
reviewed performing par-note simple interest; the driver route is physical,
loan-only and preserves analytical versus adoption eligibility. These boundaries
do not replace the remaining mixed product, yield, candidate and scale gates.

This design preserves the full scopes of issues #11, #12 and the dependent
accounting/product issues. Version 0.29.6 delivers the explicit
[dated scheduled-recognition route](dated-ledger.md), [store](dated-store.md),
[credit operator](credit-operator.md) and separate [accounting intents](credit-accounting.md).
Automatic joint credit/product adoption and complete integrated acceptance remain
open; the full design below is not a claim of delivered coverage. The
[foundation checkpoint](../reviews/2026-10-01-model-foundations.md) documents
what is implemented and tested.

## Economic ownership and compatibility

Add a versioned dated-ledger route with an explicit Gregorian valuation date,
physical measure, scenario/run identities and exactly one source owner per
position: `legacy_monthly_overlay`, `contractual_dated_native_state`, or
`fully_scenario_generated`. Reject overlapping monthly/datetime schedules or
duplicate ownership of credit/behavior. Preserve old specifications and their
posting order without changing saved economics. Rebuild to adopt the new model.

The current `daily.rs` both applies credit/withdrawal behavior and rescales
scheduled cashflows by current/reference balance. Feeding it pre-adjusted scenario
events would double-count survival. Native-owned/generated positions must bypass
those branches, including generic interest. Reverse-severity scenarios require
regenerated state-aware inputs or explicit rejection; an unrelated baseline
schedule cannot stand in for reverse behavior.

## Shared transition and credit state

Implement a reusable bounded operator in shared core `credit_operator.rs`, then
ledger `credit_state.rs`/`credit_accounting.rs`. Maintain live state probabilities
separately from monetary principal, undrawn commitment, accrued interest and book
adjustment vectors. `CreditRequest` EAD is conditional per original account;
aggregate ledger balances cannot be passed to it as EAD without another survival
weight. Do not normalize surviving mass to one.

Transport migrations/cures, competing default/payoff flux and integrated interest
with one operator per complete hazard/scenario/interval identity. For principal
row vector B, accrued row vector A, generator Q and accrual-rate diagonal R,
propagate `B'=BQ` and `A'=AQ+BR` or an equivalent uniformized reward system.
Closing-balance interest or end-survival times a whole day's interest introduces
a new timing approximation. Nonaccrual/cure resumption is explicit policy.

Maturity/disposal are separate ownership exits; they are not voluntary
prepayment. Principal can terminate while delayed/deferred/recovery rights remain.
CCF estimates default-contingent borrowing; it is not ordinary utilization.
Default-contingent draws create cash outflow and loan principal before default,
with unused commitments canceled once. Ordinary draws need their own declared
dated plan/model and priced exposure schedule.

Economic default/LGD/recovery outputs do not set journal allowance policy.
Declare allowance horizon/discount/staging, charge-off timing, recovery-claim
recognition, accrued-interest reversal and premium/discount treatment separately.
Specify whether LGD includes accrued interest, preventing duplicate loss. Reviewed
opening allowances reconcile explicitly; no balancing plug is inferred. Physical
ledger inputs cannot relabel pricing hazards or multiply scenario-conditioned
credit terms by legacy stress multipliers again.

## Bounded dated scheduler and accrual rights

Add `dated_store.rs` and an accrual-plan module alongside legacy `FlowStore`.
Retain interned source/position/event IDs, five components, accrual endpoints,
scenario identity and stable same-day ordering. Support distinct same-day events
and day-zero settlement. Compile interval accrual separately from cash settlement;
the producer's whole-coupon `accrued_interest` is not daily NII.

Early coupons create deferred-income/prepaid-expense rights that unwind through
accrual end even after principal settles. Late coupons retain receivables with
declared credit treatment. Opening accrued balances require compatible anchors.
Split actual elapsed intervals at hazard/contract events. Version daily phase
ordering: opening rights, interval accrual/credit propagation, boundary credit
recognition, dated settlements, liquidity actions and closing verification.

Preserve the whole contractual/recovery tail beyond the reporting horizon. The
existing ledger's 1,080-day reporting cap does not permit clipping a longer
contract into a false maturity. Add dated native mapping/capture and workflow
dispatch; exclude these positions from `capture_cashflows=true` monthly capture
and `month*30` mapping. Candidates need actual origination dates in the new route.
The legacy monthly candidate route remains explicitly identified.

## Implementation ownership and gates

| Responsibility | Files |
|---|---|
| Shared transition operator | New core `credit_operator.rs`, independent CTMC/reward tests |
| Daily credit/policy | New ledger `credit_state.rs`, `credit_accounting.rs`, state/draw/claim tests |
| Dated event/accrual scheduler | New `dated_store.rs`, accrual-plan module and date/right tests |
| Native integration | Ledger `src/bin/types.rs`, `spec.rs`, `daily.rs`, mapping and owned-workflow dispatch |
| Independent publication verification | `balance_stream.py`, `owned_workflow.py`, explicit test oracles and immutable lineage/report schemas |

Retain bounded batches, capacity/work admission, cooperative deadlines, process
watchdogs and final publication fences. Add immutable event lineage linking
transaction/run/position/event/source/model/input revisions. Balanced GL alone
does not prove contractual correctness: independently reconcile once-only events,
principal/draw/repayment/default conservation, allowance movements, recovery
claims, deferred interest rights and final credit subledgers.

Hand fixtures include competing one-day outcomes (100 opening, 80 live, 5 default,
15 prepay, LGD40%: 2 loss and 3 eventual recovery); cure/nonaccrual; default draw
100 drawn/40 undrawn/50% CCF; independently integrated pre-exit interest; maturity
with delayed interest; beyond-horizon recoveries; identical economics under
different allowance policies; leap/holiday/month-end dates; pre-as-of accrual;
early/late/day-zero/same-day settlement; coupon-day default/prepay/sale; mixed
legacy/dated books; source/scenario/measure failures; tampered lineage; and
cancellation at capture, execution, persisted replay and final publication.
The complete graph/credit/dated-settlement/solver/journal large-book acceptance
remains required; component tests are insufficient.
