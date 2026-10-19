# Nonfinancial component cash flow hedges (`nonfinancial-hedge-1`)

Issue [#88](https://github.com/beauschwab/model-portfolio-sim/issues/88), for #35 stage 6. Cash flow
hedges of forecasted purchases and sales of nonfinancial assets by commodity swaps, with the
component eligibility of ASU 2025-09 Issue 3 (815-20-25-22C).

## Eligibility

The hedged risk is the variability of one price component. The request names its basis:

| `component.kind` | Rule | Adoption |
|---|---|---|
| `contractually_specified` | Explicitly referenced in the agreement's pricing formula (former 815-20-25-22A; 22C(b)(1)). | not required |
| `clearly_and_closely_related` | No pricing formula; the component is clearly and closely related to the asset in a spot or forward market (22C(a)). | ASU 2025-09 adoption on or before designation |
| `subcomponent` | Clearly and closely related to an explicitly referenced component (22C(b)(2)). | ASU 2025-09 adoption on or before designation |

The clearly-and-closely-related judgment (815-10-15-32) is supplied, reviewed evidence. The owner
never infers it from price data.

## Request

- `dates`: increasing market dates. Each has a discount curve from that date and forward price
  curves by index and delivery date. A delivery date's own price on that date is the fixing.
  Nothing is interpolated.
- `exposures`: a purchase or sale of an asset with its component, probability evidence and
  deliveries (date and quantity).
- `instruments`: commodity swaps on one index. `receive_index` hedges purchases and `pay_index`
  hedges sales. Each has a fixed price, a trade date and settlement legs.
- `relationships`: a swap, an exposure, documentation, a designation date, and a one-to-one map
  from legs to later deliveries on the same date with no more quantity.
- `events`: `consumption` of a delivered purchase by a fraction, `forecast_status` for a
  delivery (`not_probable` or `probable_not_to_occur`) before it occurs, or `dedesignate`.

## Recognition

- Swap legs are valued as `sign * quantity * (forward - fixed) * discount`. A leg settles in cash on
  its date, and an off-market value at trade is a cash premium.
- Designation takes effect at the end of its date. The hypothetical derivative is a swap on the
  hedged component's own index for each mapped delivery, with its fixed price set to that
  component's forward at designation.
- Each date compares the cumulative designated instrument change with the cumulative hypothetical
  change. When the swap index differs from the component, basis risk shows in this ratio.
  - Within the bounds, each leg's change goes to `cash_flow_hedge_oci` attributed to its delivery.
  - Outside the bounds, the period's change goes to earnings and the relationship is discontinued.
- Undesignated legs post to `pnl:commodity_swap_mark`.
- A sale releases its delivery's OCI to `pnl:revenue_hedge_reclassification` at delivery. A
  purchase keeps it until the delivered asset is consumed. Each consumption releases the matching
  share to `pnl:cost_of_sales_hedge_reclassification`.
- `probable_not_to_occur` releases that delivery's OCI to `pnl:missed_forecast_reclassification`
  at once (815-30-40-5). `not_probable` stops designation prospectively and keeps the OCI until
  the transaction affects earnings.
- A relationship whose mapped deliveries have all occurred is `completed`.

## Output and verification

The result holds the ordered balanced journal, leg values, per-period assessments, OCI by delivery
(recognized, reclassified, balance, consumed fraction), relationship closings with the component
basis, and the closing trial balance. The independent reference `nonfinancial_hedge_reference`
recomputes all of them. Durable jobs run through `/models/runs` and publish only after that replay
matches.

## Boundaries

- The purchase or sale cash and the inventory itself belong to the product ledger
  (`purchase_and_inventory_cash_included` is false). This owner posts hedge effects only.
- Swaps only: no commodity options, and no hedging instrument in another currency.
- Not an accounting-standard approval or observed effectiveness. Capital and FTP are not fed.

## Evidence

- Native: `tests/nonfinancial_hedge.rs` checks the example's hand values (8,000 and 2,000 of
  purchase OCI released on consumption, 3,200 of it in April, and 500 for the sale), basis risk
  passing and failing, a missed forecast, and eligibility refusals.
- Engine: `tests/test_nonfinancial_hedge.py` compares native output with the reference over eight
  variants and catches tampering.
- API: `tests/test_nonfinancial_hedge_jobs.py` covers durable publication and tamper fences.
