# Separate commercial fee pricing, acquired coefficients and candidate journals

`commercial-product-graph-1` and `commercial-units-1` may explicitly select a
`commercial-fee-adoption-1` object in a position's optional `fees` field. This
adds separate service-fee rights to the existing native graph, acquired unit
preparation, retained commercial actor and finite candidate journal. It does not
change the loan's original cost, yield, Pricing quote, OAS, principal, coupon
ownership, LGD or recovery policy. Historical requests without `fees` retain
their original fields, economics and journal protocol.

The supplied classification and acquisition policies are reviewed inputs. This
implementation does not assert accounting approval, empirical accuracy,
regulatory eligibility or full issue #15/#25 completion.

## Classification and research basis

[IFRS 9, B5.4.1–B5.4.3](https://www.ifrs.org/content/dam/ifrs/publications/pdf-standards/english/2022/issued/part-a/ifrs-9-financial-instruments.pdf?bypass=on)
distinguishes fees integral to an instrument's effective interest rate from
separate service fees, including loan servicing fees. The former belong in the
instrument's effective-interest calculation under the applicable policy. The
latter require separate service accounting. [IFRS 15, paragraphs 22–30 and
B48–B51](https://www.ifrs.org/content/dam/ifrs/publications/pdf-standards/english/2024/issued/part-a/ifrs-15-revenue-from-contracts-with-customers.pdf?bypass=on)
requires identifying the promised services and their transfer; payment timing or
an upfront charge alone does not establish service revenue.

These distinctions inform the model boundary. Each fee retains its supplied
source, classification review and service identities. The engine does not infer
that a fee is nonintegral merely because a caller labels it a fee. It also does
not infer that a secondary purchase allocation is a servicing asset, a financial
asset, a contract acquisition cost or any other prescribed accounting category.
The separate signed acquired-cost balance and dated release schedule described
below are an explicit analytical and journal policy. Their suitability and
classification require the relevant accounting review. No general straight-line,
effective-yield or impairment method is silently selected for acquired fees.

## Input ownership and coverage

The optional selection has these fields:

| Field | Required meaning |
|---|---|
| `schema` | Exactly `commercial-fee-adoption-1`. |
| `identity` | Supplied acquisition/model policy identity and revision. |
| `discount_spread` | Explicit fee discount spread in decimal annual units, within `[-5, 5]`; separate from loan OAS. |
| `original_signed_fee_quote` | Signed original purchase quote for the separate fee rights/obligations. |
| `total_purchase_cost` | Original loan purchase quote plus the separate fee allocations. Must be positive. |
| `allocations` | One `{fee_id, allocated_cost, review}` per acquired fee. Asset cost is positive; liability consideration received is negative. |
| `original_pricing` | Complete original fee histories, identified by the original loan Pricing path IDs. |
| `original_physical` | Required alongside original Physical acquisition proof for joint finite exercise; exact original Physical fee paths and reviewed cost releases. Omitted for historical fee-free receipts. |
| `histories` | Exactly one entry per current graph scenario, with complete `physical`, `pricing`, `pricing_down` and `pricing_up` paths. |

Each fee history supplies `{path_id, policy}`. `policy` is the existing
`commercial-native-fees-1` contract: complete service earning, settlement and
refund events; independently reviewed default, prepayment, exercise and maturity
dispositions; and complete fee recovery tails. The native loan owner selects
the actual exit and exact native clock. Loan LGD and coupon recovery fractions
never determine fee losses or fee recoveries.

Physical histories additionally supply `acquisition_releases`. Each release has
`fee_id`, actual `ordinal`, strictly ordered within-day `sequence`, signed
`amount` and `review`. The complete schedule must release exactly that fee's
allocated cost, within the complete fee tail. It cannot over-release, change the
allocation's sign or invent a terminal plug. Pricing histories cannot supply a
Physical cost-release schedule. Fee contract/source/classification/service
identities and entitlements remain identical across measures and paths;
separately supplied service, cash and reviewed exit histories may differ.
Complete acquired/priced sources must settle or explicitly dispose of all
remaining service entitlements and gross receivable/payable/deferred/prepaid
stocks. Canceling future cash alone does not waive an earned right. The native
owner refuses that incomplete cash valuation until a reviewed waiver, refund or
complete remaining settlement is supplied. Generic analytical fee journals keep
their separate ability to report outstanding rights.

There is no second probability field on a fee history. It inherits the exact
already admitted loan path's weight. Each unweighted fee path is retained even
when that weight or the allocation is zero.

## Pricing and purchase reconciliation

For position loan quote `Q_L`, separate signed allocations `C_f`, and fee quote
`Q_F`, native admission checks:

```text
sum_f C_f = Q_F
total_purchase_cost = Q_L + Q_F > 0
```

The original loan Pricing distribution calibrates or verifies the original loan
OAS against `Q_L`, using only loan cash. Under joint exercise, fee economics can
change the selected stop, whose loan cash must still reconcile independently.
Original fee Pricing cash independently reconciles to `Q_F` using its own spread:

```text
Q_F = sum_paths weight * sum_fee_cash_at_or_after_purchase
      cash(t) * DF(t) * exp(-fee_spread * t)
```

The original fee quote is **cum purchase-day cash**. A subsequent mark is **ex
already settled cash**, including at the purchase date. At a mark time `u`, the
fee contribution is:

```text
PV_F(u) = sum_paths weight * sum_{t > u}
          cash(t) * DF(t) / DF(u) * exp(-(fee_spread + bump) * (t-u))
PV_total = PV_loan(original_loan_OAS) + PV_F
DV01_total = (PV_down - PV_up) / (2 * bump_bp)
```

The existing paired down/up loan histories and corresponding fee histories are
used together. The same parallel bump enters each discount calculation once.
No credit state, path probability or fee cash is weighted a second time. A
reviewed negative fee obligation retains its sign throughout pricing and risk;
there is no asset-only or positive-DV01 clamp.

Finite exercise uses one joint native Bellman owner. Loan and fee values roll
back separately at original loan OAS and supplied fee spread. Every call/put
optimizes the same root total: at a node time `t` its loan-numeraire score is
`loan_value(t) + DF_fee(0,t)/DF_loan(0,t) * fee_value(t)`. Node continuation/value
receipts use that same numeraire, and `joint_fee_values.decision_criterion`
identifies it explicitly. Adding independently rebased local components would
give inconsistent decisions when the discount spreads differ. The independent
oracle instead compares the two time-zero component contributions directly.
The unequal-spread hand control uses loan OAS 10% and fee spread 50% with a
day 10 call cash amount 104.90. The independently rebased local continuation
sum 104.932401752 would incorrectly select the call. The consistent
loan-numeraire continuation 104.880100524 selects continuation instead:
root continuation 104.593151043 is below root call 104.612996075. Separate loan
and fee root components still reconcile their original quotes.
Information groups include the observed loan/fee prefix and
gross rights. Future fee earning, collection, default and coupon fixing outcomes
enter conditional averages within that group, never its decision key. The
receipt retains `joint_fee_values` with separate loan and fee root values.

Fee policies must explicitly select
`native_exit_follows_native_exercise_timing-1`. One genuine native stop supplies
both stopped loan cash, including complete pending post-exit coupons, and fee
dispositions. Before-scheduled-cash exercise precedes coincident fee service and
settlement; after-scheduled-cash exercise retains both before disposing of the
remaining rights. Original Physical acquisition proof uses this same policy.
The resulting sealed loan/fee owners are reused by acquired coefficients and
the finite candidate daily Sim, rather than appending fee PV to a loan-only stop.

Original OAS calibration prepares bounded immutable stopped cash once and runs
the existing bounded trials over that cash. Source identities are checked on
reuse; graph admission reserves stopped cash, both preparation passes and
repeated discount/information reductions before generation. The final original
policy independently prepares and verifies its separate quotes. Generated
exercise still rejects fees until its training/evaluation owner has this same
combined economics and actual stopped-source adoption.

## Acquired basis, income and retained coefficients

The shared native nonintegral fee owner supports explicit `acquire` and
`release_acquired_cost` events. Acquisition occurs at the actual original
purchase, before service earning/cash, and only once per fee. The cost does not
consume the service entitlement or change the loan's carrying value. In
debit-positive terms:

| Event | Positive acquired fee asset | Negative acquired fee obligation |
|---|---|---|
| Acquisition `C_f` | Debit acquired fee asset; credit cash. | Debit cash; credit acquired fee liability. |
| Reviewed signed release `R_f` | Debit fee acquisition expense; credit acquired asset. | Debit acquired liability; credit fee acquisition expense. |

Unreleased signed cost is `C_f - sum R_f`. Positive and negative balances have
separate gross asset/liability accounts. Native fee snapshots retain original
acquired cost and cumulative releases even after the balance becomes zero.
Service receivables/payables and deferred/prepaid balances remain separate.

The existing ten-metric coefficient tensor retains its existing `nii` meaning:
authoritative Physical loan `InterestIncome` only. An additive
`fee_income_values` tensor has shape `[scenario, unit, reporting_period]` and
contains, per unit face:

```text
service fee income + recovery income - fee writeoff expense
    - reviewed acquired fee cost release
```

Earned-service refunds reverse fee income under their existing explicit terms.
Purchase-day fee income/releases enter the first held reporting period. Fee
income is not renamed as loan NII or fed into the existing worst-case external
NII optimization objective. Retained commercial evaluation reports it separately
and also reports `incremental_nii_plus_fee_income`. The public library keeps
separate `nonintegral_fee_income` and keyed `fee_income_vectors` cells.

Purchase-cost limits and funding use `total_purchase_cost`; face limits continue
to use face. The purchase cash event occurs once. The service cash tape excludes
the fee owner's `acquire` posting because that amount is already in the total
purchase event. It includes complete actual service/refund/recovery tails after
the reporting cutoff. Native actor evaluation reduces retained coefficients and
funding vectors; it does not regenerate credit, price cashflows or refit yield.

## Finite candidate daily adoption

`commercial-candidate-journal-1` attaches the graph's very same sealed Physical
fee owner to the corresponding already prepared loan owner in one daily Sim.
Loan acquisition pays only the original loan cost; the fee owner pays/receives
its separate allocation once. Source-authoritative service and acquired-cost
groups then enter the ordinary ordered commercial fee phase. The original
fractional loan exit times remain retained, and complete fee tails extend the
replay horizon without moving cash into the requested report date.

Protocol 7 retains fee lineage, daily gross snapshots and full closing snapshots.
Existing fee-free finite candidates continue to use protocol 6. The explicit
native-to-ledger position mapping can rename a candidate without rewriting its
raw native fee source. Allocation and source probability are applied once by
the same parent loan binding, including zero participation.

The explicit complete-origin
[`commercial-candidate-holding-journal-1`](commercial-fee-holding.md) route
retains both original candidate loan/fee anchors, exactly verifies the observed
ordered prefix and derives gross cut stocks and prior P&L before replaying
future postings once. Historical raw loan-only holding requests keep their
existing schema and receipts. The finite acquired fee capital/FTP bridge is
documented separately in [commercial-fee-funding.md](commercial-fee-funding.md).

This does not yet attach fee sources to generated candidate streaming,
discretionary draw tranches or secondary conveyed rights. Those routes
require their own complete ownership and independent replay. A generic mixed
backbook, marks, taxes and collateral overlays are not inferred from finite
candidate acceptance.

## Validation, admission and publication

`commercial_fee_adoption_reference.py` independently recomputes contractual fee
cash, actual native credit-exit binding, fee cost events, gross rights, separate
quotes, acquisition releases, pricing and income. Existing graph/unit
publication verification invokes it for every selected source. Retained raw fee
history, acquisition allocations and bound fee source identity are compared
exactly; numerical results keep the existing final tolerances.

`commercial_fee_reference.verify_tables` independently replays the combined
ordered loan/fee journal, acquired basis, gross reports and snapshots for both
original and purchased sources, including purchase offsets and explicit ledger
mapping. Durable candidate verification compares the full persisted output to
these raw-source calculations. Existing immutable source/artifact hashes,
original deadline and final cancellation/lease/token/revision/attempt fences
remain required. A serialized projection does not become a sealed native owner.

Admission charges complete role/path coverage, retained fee owners, allocation
and release groups before loan generation. Existing bounds remain: graph
32 million work/128 MiB, complete fee source within 1,080 days, at most 64 fees
and 512 supplied events per native policy, and complete joint finite journal
8 million work/32 MiB component admission. Joined requests may be refused even
when each component alone fits. Component capacities are not aggregate RSS caps.
Joint finite exercise separately bounds its prepared cash/continuation work to
8 million/128 MiB within the graph's unchanged aggregate admission. Retained
source identity material, no-option fee owners and all possible stopped cash
are charged before generating that tape.

Targeted controls are in:

- `portfolio-model-core/tests/nonintegral_fee.rs`: separate signed acquisition,
  no reacquisition/over-release, and Physical/Pricing entrypoint separation.
- `portfolio-risk-native/tests/commercial_graph_units.rs`: fee marks/DV01,
  original loan anchor identity, separate income, purchase cash and complete
  funding, zero allocation and malformed acquisition rejection; joint finite
  before/after stopping and cached original OAS calibration with separate quotes.
- `portfolio-decision-native/tests/commercial_fee_candidate.rs`: shared finite
  acquisition/release journal, signed gross balances, explicit position mapping
  and zero participation.
- `portfolio-risk/tests/test_commercial_fee_adoption.py`: independent raw
  graph/units/candidate replay, unequal and zero path weights, purchase-day and
  delayed cash, adapter cells, and source/basis/cash/coefficient/report corruption;
  joint signed fee stopping, original calibration and conditional averaging of
  late-published post-exit coupons within one observed information group.
- `apps/api/tests/test_commercial_fee_candidate_jobs.py`: actual durable joint
  fee-exercise candidate publication/reopen and rehashed acquired basis, cash,
  exercise decision and before/after clock corruption rejection.

The raw runnable example is
[`commercial-fee-units-1.json`](../../examples/models/commercial-fee-units-1.json),
whose actual schema remains `commercial-units-1`. Native release/installed,
durable persistence and full repository validation must be recorded separately
from targeted debug checks.

## Remaining full-issue requirements

Generated fee exercise/candidate fees, cut-started mixed holdings and
draw/secondary fee rights, remaining book/policy capital/FTP coverage, complete saved/cohort/mixed
strategy adoption and whole-book scale remain required. Observed histories,
chronological holdouts, policy review and complete 60,000-position combined
graph/credit/solver/journal acceptance remain separate evidence gates. See
[`commercial-completion-plan.md`](commercial-completion-plan.md) and the complete
[`implementation-plan.md`](implementation-plan.md). Matching binaries and
explicit selected-library/session rebuilds are required; local implementation
and synthetic controls do not establish deployment or close issues #15/#25.
