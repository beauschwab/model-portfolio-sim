# Explicit ASC 815 interest-rate hedge accounting (initial lane)

`portfolio_risk_native::hedge_accounting::evaluate` consumes an immutable
`HedgeAccountingRequest` (schema and model version `hedge-accounting-1`) and
returns a `HedgeAccountingResult`. It is an additive, explicitly selected
component of issue #35. It does not reinterpret saved economics, upgrade a
saved derivative, or close the issue's saved-book, portfolio-layer,
net-investment, treasury or 60,000-position gates.

The owner binds formal fair-value and cash-flow hedge relationships to one
complete reviewed Physical derivative source (`derivative-accounting-1`) and
to explicit hedged-item or forecast-exposure sources. It produces dated
assessment evidence, an append-only relationship lifecycle, and one ordered
journal in which every derivative mark, coupon, collateral and hedge entry is
recognized exactly once. A legacy `fvh`/`cfh` saved-book label is not an input.

## Rule matrix and activation

| Rule area | Treatment in `hedge-accounting-1` |
|---|---|
| Designation and documentation (815-20-25) | Relationship identity, risk-management objective, documentation, instrument, hedged item/exposure, designated proportion, hedged risk and inception date are mandatory retained inputs. Missing or unsupported values reject. |
| Hedged risk | `benchmark_interest_rate` only. The benchmark coupon component of a fixed-rate instrument (FVH) or the variability of forecast index-linked interest (CFH). Credit, FX, price-component, total-cash-flow and prepayment risks reject. |
| Effectiveness assessment (815-20-35) | `cumulative_dollar_offset` with explicit reviewed `lower_bound`/`upper_bound`. The band is policy input; no universal band is coded. Regression, qualitative and shortcut methods reject. CFH without a supplied hypothetical requires reviewed `matched_critical_terms_evidence` and exact native term matching. |
| Fair-value hedge recognition (815-25-35) | Instrument marks stay in earnings. The hedged item's benchmark-component clean value change is posted daily as a basis adjustment with the offset in earnings. A partial-term hedge (`hedged_term_end_ordinal`, 815-25-35-13B) measures the benchmark cash flows as if the item matured at that coupon period end. Discontinuation freezes the basis; it is amortized straight-line by actual days to the item's principal date (the assumed maturity for a partial-term hedge), or derecognized on sale. |
| Cash-flow hedge recognition (815-30-35, post ASU 2017-12) | The designated share of every instrument mark change goes to OCI while qualified; nothing is split as ineffectiveness. OCI is reclassified to earnings when each hedged forecast period affects earnings, by the hypothetical settlement for that period. After discontinuation frozen OCI is released pro rata by period accrual weight; `probable_not_to_occur` releases all remaining OCI immediately. |
| Discontinuation (815-25-40, 815-30-40) | Reviewed `discontinue` events (dedesignation, hedged item sold, instrument terminated) and failed assessments discontinue prospectively. Partial discontinuation removes a fraction of the remaining designated proportion. Redesignation is a new relationship with `predecessor_relationship_id`; history is append-only. |
| ASU 2022-01 portfolio layers | Not implemented; `asu_2022_01_portfolio_layer` must be `false`. |
| ASU 2025-09 (prospective, 815-20-65-7) | With `adopted=true` and an `adoption_ordinal`, the `similar_risk_grouping` election admits groups of forecast exposures with a documented similar-risk method (815-20-55-23A through 55-23D) and the `selectable_rate_debt` election admits choose-your-rate liability debt with documented index options and rate elections (815-30-35-37I through 35-37M, 815-20-25-79B). The `swap_written_option_exception` election designates a swap combined with a written cash-settled European swaption of matching notional as one hedging instrument without the net written option test (815-20-25-88). All apply only to relationships designated at or after the adoption date; existing hedges are not migrated. `dual_hedge_basis` rejects as unimplemented; nonfinancial components are out of scope. See [ASU 2025-09 elections](#asu-2025-09-elections). |
| Seasoned relationships | A relationship whose `inception_ordinal` precedes the derivative opening date must carry a reviewed `opening` block (and only such relationships may): the after-payment hedged value at the opening, cumulative instrument and hedged changes, and the retained basis or OCI balance. Balances are posted once against opening equity before the first node; measurement continues from the derivative opening clean values. See [seasoned openings](#seasoned-openings). |
| Net investment, nonfinancial components, standalone options as instruments, counterparty default | Rejected explicitly. |

`policy.rule_version` must equal `asc815-asu2017-12`. The implemented rule
matrix is this document; accounting policy approval, auditor judgment and
entity-specific qualification are outside the engine.

## Request

```json
{
  "schema": "hedge-accounting-1",
  "model_version": "hedge-accounting-1",
  "run": {"id": "...", "revision": "..."},
  "entity": "bank", "currency": "USD", "monetary_unit": "currency_units",
  "policy": {
    "identity": {"id": "...", "revision": "..."},
    "rule_version": "asc815-asu2017-12",
    "adoption": {
      "asu_2025_09": {"adopted": false, "adoption_ordinal": null,
        "elections": {"similar_risk_grouping": false, "selectable_rate_debt": false,
                      "swap_written_option_exception": false, "dual_hedge_basis": false}},
      "asu_2022_01_portfolio_layer": false
    },
    "assessment": {"identity": {...}, "method": "cumulative_dollar_offset",
                   "lower_bound": 0.8, "upper_bound": 1.25},
    "fvh_basis_amortization": "straight_line_actual_days_to_principal",
    "cfh_reclassification": "hedged_period_hypothetical_settlement"
  },
  "derivative": { ...complete derivative-accounting-1 request... },
  "hedged_items": [{
    "item_id": "loan-1", "source": {...}, "carrying_source": {...},
    "kind": "fixed_rate_instrument", "direction": "asset",
    "notional": 100.0, "contractual_rate": 0.06,
    "periods": [{"period_id": "p1", "start_ordinal": 738886, "end_ordinal": 738896,
                 "payment_ordinal": 738896, "day_count": "act360"}],
    "principal_ordinal": 738896
  }],
  "forecast_exposures": [{
    "exposure_id": "float-1", "source": {...}, "probability_evidence": {...},
    "direction": "payment", "notional": 100.0, "index_id": "term-1m",
    "periods": [{"period_id": "e1", "start_ordinal": 738886, "end_ordinal": 738896,
                 "payment_ordinal": 738896, "day_count": "act360"}]
  }],
  "relationships": [{
    "relationship_id": "fvh-1", "predecessor_relationship_id": null,
    "designation": "fair_value_hedge",
    "hedging_instrument_position_id": "swap", "designated_proportion": 1.0,
    "hedged_item_id": "loan-1", "hedged_notional": 100.0,
    "hedged_risk": "benchmark_interest_rate", "benchmark_rate": 0.05,
    "inception_ordinal": 738886,
    "risk_management_objective": {...}, "documentation": {...},
    "hypothetical": null, "matched_critical_terms_evidence": null
  }],
  "events": [{
    "event_id": "assess-1", "relationship_id": "fvh-1", "ordinal": 738896,
    "sequence": 0, "kind": "assess", "review": {...}
  }]
}
```

A `cash_flow_hedge` relationship names `forecast_exposure_id` instead of
`hedged_item_id`/`hedged_notional`/`benchmark_rate`, and either a `hypothetical`
(`evidence` plus one `nodes[]` entry `{ordinal, request}` holding a complete
`SwapRequest` for every path node from inception) or
`matched_critical_terms_evidence`.

Events are `assess`, `discontinue` (`reason` in `dedesignation`,
`hedged_item_sold`, `instrument_terminated`; optional `proportion` in (0,1]
of the remaining designated proportion, default 1), `forecast_status`
(`status` in `no_longer_probable`, `probable_not_to_occur`; CFH only) and,
under the ASU 2025-09 election, `rate_election` (`index_id`). Group
assessments may carry `effective_ordinal`. Events are globally ordered by
`(ordinal, sequence)` with unique ids and reviews.

## Admission

- The derivative request is a complete `derivative-accounting-1` Physical
  netting lifecycle with one path, no default policy, matching entity,
  currency and unit. Every hedging instrument is a mapped `swap` trade whose
  raw review designation is `undesignated_economic_hedge`; CFH instruments use
  `settle_derivative_carrying` coupon recognition. Options, expired options and
  defaulted paths reject.
- Hedged item periods are contiguous and chronological; `principal_ordinal`
  is at or after the last payment. `hedged_notional` is positive and at most
  the item notional; the sum over relationships cannot exceed it. The sum of
  designated proportions per instrument over relationships cannot exceed 1.
- `inception_ordinal` is a path node at or after the opening date, or an
  earlier date with a reviewed `opening` block, and in both cases an
  accrual-free boundary: a `start_ordinal` of some fixed and some floating
  period of the instrument and (FVH) of the item, or (CFH) of the exposure.
  Item and exposure periods may start before the opening date (within the
  explicit 36,600-day domain); hedged payments before the opening need no node.
- `assess` requires an accrual-free node: `end_ordinal` of some fixed and
  floating period of the instrument and of the item/exposure, with
  `after_scheduled_payments` timing, after inception and before the
  instrument's final payment date. Other events can occur on any node at or
  after inception. Events on discontinued relationships reject except
  `forecast_status` for CFH frozen OCI.
- Every item payment date and principal date after a node must have an exact
  discount point in the instrument's node discount curve; no interpolation.
- A supplied hypothetical covers every node from inception to the tail with
  `asof_ordinal` equal to the node, the node's timing, the exposure's currency
  and exactly the exposure's periods on its floating leg; its clean value at
  inception must be zero within the shared tolerance. Matured hypothetical
  floating periods must carry observed or Physical observations.
- A `predecessor_relationship_id` must name an earlier relationship on the
  same instrument that is fully discontinued at or before the new inception.
- Bounds: 64 relationships, 1,024 events, 64 items/exposures, 4,096 periods
  per item/exposure, 32 MiB raw input, 32 MiB output, saturating 1,000,000
  combined hypothetical work units in addition to the derivative owner's own
  admission. Component bounds are not aggregate RSS guarantees.

## Valuation and recognition

Let nodes be the derivative path nodes `t_0 < ... < t_n`. For a relationship
with designated proportion `p`:

**Instrument change.** `dV_t` is the instrument's clean mark change at node
`t` as posted by the derivative owner (`clean_mark` intents, excluding coupon
settlements); it drives recognition. For assessment the instrument change is
measured clean of accrued periodic interest: `D_t = sum(p_s * (V_s - V_{s-1}))`
over nodes after inception through `t`, where `V_s` is the after-payment
carrying value at node `s`. At an accrual-free date this is the clean fair
value change since inception; settled net coupons are excluded on both sides.

**FVH hedged item.** With `N_h` the hedged notional, `r_b` the benchmark
rate, `tau_i` the period year fraction and `D_t(d)` the exact discount factor
from the instrument's node-`t` curve:

```
B_t = sum_{active i} N_h r_b tau_i D_t(pay_i) + [principal active] N_h D_t(principal)
A_t = N_h r_b * yearfrac(start_j, t)   for the period j containing t (zero after
      its end with after-payment timing, the full coupon with before-payment timing)
C_t = B_t - A_t
```

A period is active with before-payment timing when `pay_i >= t`, otherwise
when `pay_i > t`. With `hedged_term_end_ordinal = T` (an item period end after
inception and before the principal date) only periods ending at or before `T`
enter the sum and the principal is discounted at `T` instead of the principal
date, so the hedged benchmark cash flows are those of an item maturing at the
end of the hedged term. `C_t` is continuous across coupon payments. The daily basis
adjustment is `dC_t = C_t(before payments) - C_{t-1}(after payments)`, starting
from the inception node's after-payment `C`, so settled coupons and the
principal repayment are excluded exactly as the derivative owner excludes
settled coupons from marks. Posting (direction `dir` = +1 asset, −1 liability):
`hedged_item_basis_adjustment` debit `dir * dC_t`, `hedged_item_basis_gain_loss`
debit `-dir * dC_t`. The hedged item's contractual interest is not posted here.

**CFH hypothetical.** `H_t` is the hypothetical swap's after-payment clean
value at node `t` (native `swaps::value_swap`), or the instrument's own
after-payment carrying value when matched critical terms are elected.
Cumulative `E_t = sum(s * (H_t - H_{t-1}))` after inception, the same clean
measure as `D_t`. Matched critical terms additionally require the instrument's
fair value at inception to be zero; a supplied hypothetical must itself be at
market at inception.

**OCI.** While qualified, each node's `p * dV_t` moves from
`mark_gain_loss` to `cash_flow_hedge_oci`: the derivative owner's `clean_mark`
intent is re-owned with the `mark_gain_loss` posting split into
`cash_flow_hedge_oci` (`p` share) and `mark_gain_loss` (`1-p` share). The
derivative asset/liability postings are unchanged.

**Reclassification.** At each exposure period payment date on or after the
inception node, `R = p * S_t` where `S_t` is the hypothetical's net
settlement due at `t` (the instrument's Physical cash right when matched).
Posting: `cash_flow_hedge_oci` debit `R`, `hedged_forecast_reclassification`
debit `-R`. After discontinuation the frozen OCI balance `F` is released at
each remaining period `k` by `F * w_k` with `w_k = N tau_k / sum N tau` over
periods remaining at discontinuation; the last release clears the balance
exactly. `probable_not_to_occur` posts the entire remaining balance at the
event node.

**Assessment.** At an `assess` event, `ratio = D_t / E_t` where for FVH
`E_t = -dir * sum(dC)` since inception (the clean benchmark-component change,
with the principal settlement excluded). `qualified` iff
`lower <= ratio <= upper`. Assessment dates must precede the instrument's
final settlement; at or after it nothing remains to measure.
`E_t = 0` with `D_t = 0` rejects the event (nothing to assess); `E_t = 0` with
`D_t != 0` fails. Failure discontinues the whole relationship at that node
with reason `failed_assessment`.

**Discontinuation.** For fraction `q` of the remaining proportion: FVH freezes
`q * basis_balance`; `hedged_item_sold` derecognizes it at once
(`hedged_item_basis_derecognition`), otherwise it amortizes per node by
`frozen * (t - t_disc) / (principal - t_disc)` cumulatively
(`hedged_item_basis_amortization`), fully at or after the principal date.
CFH freezes `q * oci_balance` for pro rata release. Proportion becomes
`p * (1 - q)`; zero means fully discontinued and later marks stay in earnings.

**Node order.** For each node: the derivative owner's intents for that node in
their original order (with CFH mark re-ownership using the status at the end
of the previous node); then hedge postings (FVH `dC`, amortization, CFH
reclassification/frozen release); then that node's events in sequence order;
then the daily closing. Relationships become active at the end of their
inception node, so inception-node marks stay in earnings.

## ASU 2025-09 elections

Both elections are additive request fields admitted only when
`policy.adoption.asu_2025_09.adopted` is `true`, the election flag is `true`
and the relationship's `inception_ordinal` is at or after `adoption_ordinal`
(prospective application, 815-20-65-7). Elections that no relationship uses
leave the result unchanged. The elections are limited to the cash-flow hedge
of interest-rate risk already in scope; the hypothetical derivative method is
required for both.

**Similar-risk groups (815-20-55-23A through 55-23D).** A cash-flow
relationship carries `group` instead of `forecast_exposure_id`:
`member_exposure_ids` (2 to 16 distinct exposures, none choose-your-rate),
`similar_risk_method` and `hypotheticals` (one `{exposure_id, hypothetical}`
per member, in member order, each mirroring its member exposure on that
member's `index_id` from inception to the tail and at market at inception).
The group's cumulative hedged change is the sum of member hypothetical changes;
reclassification posts per member at that member's payment dates by the
member's hypothetical settlement. Similarity is assessed at every `assess`
event (815-20-55-23) in addition to the relationship's dollar offset:

- `instrument_highly_effective_each_risk` (55-23A(a)): for member `j`,
  `ratio_j = (D_t * N_j / sum N) / E_t,j`, the instrument's cumulative change
  pro rata by member notional over the member's cumulative hypothetical
  change, must lie within the reviewed band.
- `hedged_risks_similar_to_each_other` (55-23A(b)): with per-unit changes
  `u_j = E_t,j / N_j`, every pair `u_j / u_k` must lie within the same band;
  the reported member ratio is `u_j / u_1`.

A zero member change makes that member not similar. When a group is not
similar the relationship is dedesignated as of the last date all hedged risks
were assessed similar (the previous qualified assessment, or inception) unless
the event supplies a specific `effective_ordinal` between those dates
(55-23D): instrument changes recognized in OCI after that date return to
earnings (`similar_risk_earnings_catch_up`, OCI against `mark_gain_loss`),
hedged-period reclassifications after that date are reversed
(`similar_risk_reclassification_reversal`), the OCI through that date is
frozen over the member periods paying after it (weights `N_j tau`), and the
entries already due are released at the event. OCI through the effective date
therefore remains until the forecast transactions affect earnings. The
lifecycle record reports `effective_ordinal`, `earnings_catch_up` and
`reversed_reclassification`; the assessment reports `similar_risk` with the
method, verdict and member rows. Similarity failure takes precedence over a
failed overall dollar offset. Qualitative similarity (55-23B/C) remains a
reviewer assertion outside the engine.

**Choose-your-rate debt (815-30-35-37B(b), 37I through 37M).** A forecast
exposure of liability interest payments (`direction = payment`) carries
`choose_your_rate = {evidence, documented_index_ids, debt_classification:
"liability"}` listing 2 to 16 documented index options including its initial
`index_id`. The relationship supplies `hypothetical` for the initial index and
`rate_hypotheticals` with exactly one hypothetical per documented alternative,
each with terms as of inception (at market at the inception node, 35-37M) and
covering every node from inception to the tail on that index. A
`rate_election` event at an accrual-free assessment node selects a documented
index different from the current one (35-37I): the engine performs the final
retrospective dollar-offset assessment on the previously selected rate (35-37L;
recorded with `index_id` of the prior rate); if it fails, the instrument
changes recognized in OCI since the last assessment return to earnings
(`rate_election_earnings_catch_up`) and the relationship continues under the
reviewed expectation in the event's review; then the hypothetical for the new
index governs with cumulative change `E_t = s * H_new,t` (its inception value is
zero) and later reclassifications use its settlements. Assessments report the
rate measured in `index_id`; daily rows and the closing report the selected
index. Changes to the number or timing of payments (tenor changes), the
forecast-issuance documentation update of 35-37H and replacement debt
designation are not modeled: periods are fixed per exposure, and a probable
non-documented rate or fixed-rate replacement is recorded by the existing
`probable_not_to_occur` status (37E/37K). Assets and analogies reject (37B).

**Swap plus written option (815-20-25-88).** A relationship may name
`written_option_position_id`: a mapped European swaption that is `short`
(written) whose terminal-state underlying notional equals the swap notional,
present on every path node as a live `european_swaption` or, from expiry, an
`expired_european`. Cash settlement settles carrying at expiry; physical
delivery (with the option's reviewed `delivery_review`) continues as the
delivered swap under the same mapped position, whose notional must also match,
so its later marks and coupons stay in the combination. The option carries
the raw `undesignated_economic_hedge` review and, for cash-flow hedges,
`settle_derivative_carrying`, so expiry cash and delivered coupons settle
carrying without earnings. The combination is one instrument: the designated clean value and
cumulative change `D_t` sum the swap and the option, cash-flow mark
re-ownership covers both positions' `clean_mark` intents, and designated
proportions are tracked on the option as on the swap. The hypothetical
derivative method is required (matched critical terms cannot describe a
combination); the hypothetical still mirrors the forecast exposure's
contractual terms, so a written option without a corresponding feature in the
exposure is measured as dollar-offset mismatch, not excluded. Options expiring
before inception, caps/floors as separate contracts and optioned hedged
items/hypotheticals remain open.

**Jointly designated swaps (815-20-25-45).** A relationship may name
`additional_instrument_position_ids`: further mapped swaps, each with the raw
`undesignated_economic_hedge` review (and `settle_derivative_carrying` for
cash-flow hedges), distinct from the primary instrument and the written
option, with designated proportions tracked per swap and the accrual-free
inception boundary required on every leg. The combination is one hedging
instrument: the designated clean value and cumulative change sum every
component (plus any written option), cash-flow mark re-ownership covers every
component's `clean_mark` intents, assessments require accrual-free dates on
every component, and the hypothetical derivative method is required.

**Refused elections.** `dual_hedge_basis` rejects; other option instruments
still require their own lane. Nonfinancial price components (Issue 3) are
outside the interest-rate scope of this owner.

## Seasoned openings

`opening = {evidence, hedged_value, cumulative_instrument_change,
cumulative_hedged_change, basis_balance, oci_balance, scale?, frozen_basis?,
frozen_oci?}` is required exactly when the relationship was designated before
the derivative opening date. The
reviewed amounts are those through the opening: `hedged_value` is the
after-payment benchmark-component value per unit of hedged notional (FVH) or
the hypothetical's after-payment clean value (CFH; with matched critical terms
it must equal the opening instrument value); a fair-value opening carries
`basis_balance = -dir * cumulative_hedged_change` and no OCI, a cash-flow
opening carries `oci_balance` and no basis. `scale` (default 1, in (0, 1])
is the remaining fraction of the designated proportion after earlier partial
discontinuations. Frozen tranches retained at the opening are explicit:
`frozen_basis[] = {amount, start_ordinal, amortized}` (FVH; amortization
continues straight-line from `start_ordinal` to the item's principal date,
and `basis_balance + sum(amount) = -dir * cumulative_hedged_change`) and
`frozen_oci[] = {amount, released, entries[] = {payment_ordinal, weight}}`
(CFH; the remaining entries are hedged exposure payment dates at or after the
opening whose weights sum to the unreleased fraction, and the last entry
clears the tranche exactly). Roll-forwards start from `basis_balance +
sum(amount)` adjusted and `sum(amortized)` amortized, or `oci_balance +
sum(amount)` recognized and `sum(released)` reclassified. The instrument
anchor is the derivative owner's opening clean value of the swap (plus any
combined written option).

Before the first node the engine posts one `hedge_opening` intent per
seasoned relationship: `hedged_item_basis_adjustment` debit `dir * (basis +
unamortized frozen basis)` against `opening_equity` (FVH), or
`cash_flow_hedge_oci` credit `oci + unreleased frozen OCI` against
`opening_equity` (CFH), so the hedge journal's opening equity differs from the
derivative owner's by exactly those amounts (the daily reconciliation accounts
for them). The relationship is qualified from the first node: its marks are
re-owned from the opening, the first node's basis change measures the
opening-node value against `hedged_value`, cumulative changes start from the
supplied amounts, and roll-forwards start from the supplied balances. The
lifecycle records an `opening` entry with the evidence identity and the
unamortized/unreleased frozen amounts; the closing row reports
`opening_evidence`. Groups, rate options and redesignation
predecessors are not combined with seasoned openings. Supplied opening
balances are reviewed inputs, not verified prior-publication evidence.

## Output

`HedgeAccountingResult` retains `schema`, `model_version`, `run`, entity,
currency, unit, the policy, the complete `derivative` result (evidence, not a
second journal), `relationships` (closing status, proportion, balances,
frozen schedules), `lifecycle` (ordered inception/assessment/discontinuation/
forecast events with proportions before and after), `assessments` (cumulative
amounts, ratio, bounds, outcome), `daily` (per node, per relationship:
instrument designated value, hedged clean value or hypothetical value,
cumulative changes, basis and OCI balances, status; plus the financial trial
balance), `intents` (the single ordered journal), `closing` roll-forwards and
explicit flags: `accounting_standard_approval=false`,
`empirical_validation=false`, `persisted_journal_verified=false`,
`saved_book_adoption=false`, `portfolio_layer_supported=false`,
`net_investment_supported=false`, `asu_2025_09_similar_risk_supported=true`,
`asu_2025_09_selectable_rate_supported=true`,
`asu_2025_09_swap_written_option_supported=true`,
`seasoned_openings_supported=true`. Election-only fields
(`similar_risk`, assessment and lifecycle `index_id`, `effective_ordinal`,
`earnings_catch_up`, `reversed_reclassification`, daily `members`,
`member_exposure_ids`, `selected_index_id`, `written_option_position_id`,
`opening_evidence`, `hedged_term_end_ordinal`,
`additional_instrument_position_ids`) appear only for relationships
that use an election, so results without elections are unchanged.

Journal accounts are the derivative owner's accounts plus
`cash_flow_hedge_oci`, `hedged_item_basis_adjustment`,
`hedged_item_basis_gain_loss`, `hedged_item_basis_amortization`,
`hedged_item_basis_derecognition` and `hedged_forecast_reclassification`.
Every intent balances; hedge intents carry `relationship_id`. Each node's
trial balance equals the derivative owner's balances for every derivative
account except `mark_gain_loss`, which is reduced exactly by the OCI share.

## Independent verification

`portfolio_risk.analytics.hedge_accounting_reference.verify(raw, result)`
independently reconstructs the derivative result with the existing raw
derivative reference, the item benchmark values, hypothetical values, the
relationship state machine, assessments, intents, daily balances and closing
roll-forwards, then compares with the existing 1e-12 relative / 1e-9 absolute
financial tolerance and exact identity for every nonfinancial field. It uses
no native callbacks. A receipt is returned only when the whole result passes.

## Scope statement

This lane admits single-currency fixed/floating swaps against fixed-rate
assets/liabilities and forecast index-linked interest, with the ASU 2025-09
similar-risk group, choose-your-rate and swap/written-option elections above.
It does not establish accounting-standard compliance, observed effectiveness,
portfolio-layer, net-investment, standalone option or nonfinancial coverage, saved-book or shared daily journal adoption, or
complete-scale acceptance. Those remain open under issue #35 and are listed in
`implementation-plan.md`.

## Closing capital feed (`hedge-treasury-1`)

`portfolio_risk_native::hedge_treasury::evaluate` re-executes a complete
`hedge-accounting-1` request natively and maps its complete-tail closing
balances to the existing treasury bridge (`treasury-bridge-1`, ledger kind,
protocol 0). The request carries the raw hedge request, the ledger `account`
label, a reviewed GL `mapping` identity, a `treasury` request whose `as_of`
equals the closing date and whose capital/FTP inputs are empty, one to sixteen
bridge `policies` for the hedge scenario and account whose `capital.period`
equals the closing date, and a positive `amount_multiplier`. An explicit
`interim_report: true` selects report-date capital instead: `as_of` (and every
policy period) must then be a hedge path node strictly before the complete
tail; rows carry the journal through that node and reconcile to that day's
trial balance, the embedded hedge result keeps the complete tail, and the
result reports `interim_capital=true` with `closing_capital_selected=false`.
Without the flag an `as_of` before the tail still refuses.

Rows are per instrument from the single ordered journal plus the opening
anchors, reconciled to the owner's closing trial balance before mapping:

| Hedge journal account | Bridge GL |
|---|---|
| cash | `cash` |
| derivative asset/liability (signed per position) | `derivative_value` |
| posted VM/IM, received VM | `posted_margin`, `received_margin` |
| opening equity | `opening_equity` |
| mark, contractual income, collateral interest, closeout losses | `pnl:derivative_*` |
| cash-flow hedge OCI | `cash_flow_hedge_oci` (AOCI with its own eligibility) |
| hedged-item basis adjustment | `hedged_item_basis_adjustment` (asset item) or `hedged_item_basis_liability` (liability item) |
| basis gain/loss, amortization, derecognition, forecast reclassification | `pnl:hedged_*` |

Rows within the 1e-9 comparison tolerance of zero are omitted; unpaid
derivative rights at the closing date refuse. A negative asset-item basis
cannot carry a nonzero risk weight because the item's own carrying rows are
owned outside this journal. The bridge policy field
`cash_flow_hedge_aoci_eligible` (default `false`) decides whether
`cash_flow_hedge_oci` enters eligible AOCI when `include_aoci` is true.

Every policy on this route must also carry the reviewed `aoci_eligibility`
treatment `{identity, deferred_tax_rate, opt_out_elected, phase_in_fraction}`
(#30/#72 inputs): included AOCI is the book AOCI admitted by `include_aoci`
and the cash-flow-hedge flag; `deferred_tax_on_aoci = -included * rate` is
disclosed, not posted (the DTA/DTL and any threshold deduction stay with the
item owners and tax policy); eligible AOCI is zero under an opt-out election,
otherwise `included * (1 - rate) * phase_in_fraction`. The phase-in fraction
is a supplied schedule value, not a hard-coded rule. The bridge audit then
discloses `cash_flow_hedge_aoci`, `included_aoci`, `deferred_tax_on_aoci`,
`aoci_opt_out_elected`, `aoci_phase_in_fraction` and the policy identity;
policies without the treatment keep the historical audit unchanged. The result retains
the hedge result as evidence, the rows, policies, the bridge output (prepared
capital, thirteen ratios, audit, contributions) and explicit flags for the
closing/interim selection and false flags for FTP, accounting-standard and
regulatory approval. The
Seasoned relationships contribute their retained basis/OCI rows and the
matching `opening_equity` rows under the item/exposure identity; a combined
written option contributes its own `derivative_value` and OCI rows under its
position. The
independent `hedge_treasury_reference` replays the hedge journal, the rows and
the capital arithmetic. Regulatory eligibility, requirements, risk weights and
the monetary unit remain supplied inputs; deferred-tax and AOCI opt-out policy
(#30/#72) are not inferred.
