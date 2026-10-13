# Quote-aware dated funding contract

The frozen `aged-units-1` fixed-contract result retains principal in metrics 8/9
to preserve the current ten-metric solver layout. `strategy-aged-1` now adopts
separate native funding rows, purchase-cost limits and allocated cash-event replay.
Nonpar quotes are admitted by that versioned route. The legacy solver contract
retains its historical economics. All 15 public aged tests pass against rebuilt
native artifacts, including independent allocated cash replay and actual quote
limits. The 23 native producer analytical tests also pass. Public release evidence
and its measured coefficient-only timing are recorded in [aged-units.md](aged-units.md).

## Native producer extension

Keep the ten financial metrics and add separately produced purchase/settlement
cash from the same `advanced_product::project_fixed` projection:

* `purchase_costs[N]`: actual positive quote in currency per face unit, with stable
  unit index and actual purchase date. Bank cash is `-side * quote`.
* `cash_events`: native unit index, ordinal, event/source identity, signed bank
  principal and coupon cash. Bank cash is `side * contractual_cash`. Preserve
  complete tails; these events are not NII recognition or market value changes.
* `funding_ordinals[F]`: sorted union of purchase, reporting and settlement dates,
  through the **full contract tail**, not only reporting horizon. Empty dates are
  unnecessary; same-day events net once under explicit end-of-day cash policy.
* `funding_demand[S,N,F]`: at date `d`, zero before purchase; otherwise
  `side * (quote - sum contractual_cash paid through d)`. Positive values consume
  starting cash, negative values are retained cash resources. Fixed contracts
  give identical cash under scenarios; keep explicit scenario dimensions for
  subsequent state-dependent products.

All these amounts must be native-produced. No Python financial construction,
caller-generated proxy, mark-to-market cash or double principal settlement is
allowed. Bound dense funding values/event transformation separately before
allocation, and preserve source identities/cancellation. A result lacking these
fields requires rebuilding before dated evaluation or solving. The explicit
funding policy declares gross pretax contractual cash retention, end-of-day
netting and full-tail coverage. Taxes, distributions, fees, collateral requirements, intraday order,
funding costs and discretionary reinvestment need additional explicit sources;
they must not be inferred from pretax NII or the existing regulatory weights.

## Native solver and evaluation adoption

1. Retain coefficient NII as the external objective and direct aged DV01 as risk.
   Metrics 8/9 remain principal exposure for existing KPI reporting; do not relabel
   them as purchase cash or silently change historic `strategy-library-1`.
2. Native cash-resource rows cover every admitted `funding_ordinal`, with
   `sum allocation_i * funding_demand[s,i,d] <= cash_budget`.
   The current incremental, baseline-free contract uses constant starting cash.
   Full saved-book adoption requires separately source-linked base cash.
   Do not let a future deposit issuance fund an earlier purchase.
3. Check all full-tail cash dates, or require an explicit source-linked terminal
   funding policy. Reporting-period checks alone can miss an early CD maturity
   funding a longer loan. A generic rollover assumption is insufficient.
4. For a purchase-cost asset cap, the row is
   `sum(asset allocations * actual_purchase_quote) <= max_purchase_cost_assets`.
   Keep notional-based limits explicitly separate. Report allocated face,
   initial purchase cash and principal exposure distinctly; changing the meaning
   of the existing `max_total_assets` field requires a versioned contract.
5. Replay allocated bank cash independently from native event/purchase streams,
   in chronological order, and reconcile it to every constructed funding row.
   Validate complete policy/source/grid/capital dependencies before a solve.
   Missing/stale native cash tensors require rebuilding. Constraint-only edits
   consume already prepared tensors and invoke no product models.
6. The additive version-1 funding coefficient FFI evaluates only retained
   `[S,N,F]` data; the existing ABI 8 remains unchanged. Interactive output
   separates actual `funding_gap` and full-tail ordinals from reporting-period
   `principal_exposure_gap`. Missing funding vectors require rebuilding.
   Daily candidate journal acceptance remains separate from coefficient funding
   feasibility, and saved snapshots require compatible dated source identity.

## Independent fixtures for the follow-through

* **Discount purchase:** face 1, quote .9, zero coupon, maturity cash 1. Starting
  .9 cash can buy exactly one unit. Initial bank cash is -.9, terminal +1; realized
  retained cash .1 reconciles original-yield income. The purchase date is not a
  new unexplained par gain. A premium 1.1 example reverses this result.
* **Actual future origination:** quote .9 on day 30 cannot use a day-60 CD receipt
  on day 30. Resource constraints before and at each purchase catch this.
* **Coupon settlement versus earning:** face 1, coupon .1 paid at maturity has
  zero funding benefit before payment, although frozen-yield NII accrues earlier.
  After maturity the .1 cash resource appears exactly once.
* **Funding tail:** issue par CD for 1 at day 0, repay 1 at day 365; purchase par
  zero-coupon asset paying 1 at day 730. A reporting horizon at day 180 must not
  hide the day-365 cash deficit. Explicit permitted refinancing requires its own
  dated cash-source event/policy; terminal inventing is rejected.
* **Matched maturity:** asset receipt and CD repayment on the same day net once
  under supplied end-of-day policy. Shift either settlement one actual day to
  expose the resulting liquidity deficit. This is not an intraday acceptance test.
* **Amortization/reinvestment:** native scheduled principal cash from a first unit
  funds a later actual purchase at its quote. Include coupon and principal
  separately, and verify no duplicate income or principal removal.
* **Signed CD cash:** issuance is positive bank cash; coupons and principal
  repayment are negative. Negative note rates remain explicit signed contractual
  amounts; original-yield expense and cash do not become two receipts.
* **Public adoption:** native full replay equals prepared cash-resource rows;
  a coefficient-only allocation evaluation invokes no pricers. Test discount and
  premium prices, a purchase-cost cap binding independently of face/cash limits,
  stale capital and missing funding data. Deadline/failed-publication and daily
  candidate journal gates remain distinct. Historical snapshots retain their
  existing economics.
