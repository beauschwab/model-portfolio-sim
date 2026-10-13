# Native actual-purchase performing product units

`product-units-1` prepares reviewed effective-yield income, actual aged risk,
principal exposure and scenario-specific funding coefficients in Rust from raw
consumer installment, card, mortgage and deposit graphs. It accepts explicitly
supplied purchase-date contracts and histories. It does not manufacture future
origination, borrower, promotional, ARM, burnout, deposit or market state by aging
today's template. This is an additive coefficient-preparation boundary; it does
not replace historical fixed units, authorize a solver allocation, or establish
candidate daily-ledger acceptance.

## Admitted product and accounting boundary

The initial boundary is performing repayment-only consumer/card/mortgage and
deposit runoff. Consumer steps can be payments or payoff. Cards require no initial
purchase grace or grace reinstatement, no opening billed/unbilled/unrecognized or
contingent interest and no fees; payment and explicit zero-limit retirement are
allowed. Unbilled interest produced by the admitted native earning policy remains
a real right and can settle after principal has retired. Existing statement/due
ordering and payment feasibility are checked by the card engine.

Mortgages require fixed rates, performing-origin exposure, full performing
collection and recognition, zero credit/migration/default/turnover hazards and
zero refinancing incentive slope. Original borrower payment, origination,
seasoning, burnout and agency versus whole-loan economics remain explicit in the
native source. Actual remittance lags and already entitled settlement rights
remain in the full tail. Deposit migration and external inflow are rejected.
Supported withdrawal/closure runoff remains owned by the native deposit source.

All products require zero purchase-date accrued/fee opening rights. Same-asof
actions or settlement cash are rejected: this route has no separately reconciled
after-action purchase opening. Native deposit runoff emitted at the purchase date
therefore also rejects; runoff starting on later supplied dates is admitted.
Nonzero starting accrued rights, draws, default, nonaccrual/cures, migration,
new inflows, fees and explicit rate/payment/yield modifications require a later
reviewed source/accounting ownership implementation. ARM resets and mortgage
early-retirement basis policy remain open. An otherwise admitted early payoff or
runoff path can still fail retained-yield closure rather than receiving an
automatic premium/discount disposal entry.

These are strict initial admission limits. They do not narrow the remaining issue
23 product-unit, stochastic-credit, daily candidate/category acceptance, model
validation or combined 60000-position acceptance requirements.

## Raw request, original anchors and scale

`ProductUnitsRequest` contains `schema`, `identity` (model/grid/CRN/input
revisions), explicit `currency`, `monetary_unit="currency_units"`, increasing
actual `reporting_ordinals`, `base_scenario_id` and ordered `units`. A unit contains
`unit_id`, `template_id`, `purchase_ordinal`, positive `unit_face`, positive
`quote_price_per_face_unit`, accounting category, capital-policy revision, five
supplied regulatory weights, its complete `purchase_graph`, reviewed
`income_policy`, and all `age_branches`. Unknown fields reject.

The tagged graph uses `kind` (`consumer_installment`, `card`, `mortgage`,
`deposit`) and its entire native `request`. It identifies one position equal to
`unit_id`, with as-of equal to the actual purchase date and graph reporting dates
equal to the request dates at/after purchase. Purchase is a reporting boundary
excluding the final date. Units share ordered scenario/run/severity identities;
each future purchase can supply its own explicit purchase-date market curves.
These curves are not projected internally. A unit's continuation branches must
match its complete supplied market, bump, currency, report, original OAS and
pairing/source contexts.

The original purchase OAS is mandatory `locked`, with source/revision. Native
base pricing cash must value to `quote_price_per_face_unit*unit_face` within
`1e-10*max(quote,1)`; supplied OAS is verified and retained, never solved or refit.
The income policy separately supplies immutable `original_annual_effective_rate`,
`original_yield_anchor` and reviewed policy identity. Each complete physical path
in every scenario must discount at that original annual effective yield to the
same actual purchase quote within that tolerance. It is not inferred from a Q
mark or OAS. This deliberately restrictive slice excludes physical paths needing
unimplemented estimate-change/retirement accounting. It does not claim that
pathwise reconciliation is a general bank accounting rule or empirical fit.

`unit_face` must reconcile to the source's contractual opening principal before
pricing or source reuse: consumer principal, card class totals, mortgage investor
principal, or deposit segment balances in every P/Q history and age branch.
The existing amount-replay bound `1e-9 + 1e-12*abs(principal)` allows summation
roundoff. Reciprocal quote/face edits cannot disguise a different actual face
under a face cap. The independent raw-source verifier enforces the same domain.

All raw monetary state is in currency units. Numeric coefficients divide source
amounts by explicit `unit_face` once. Physical path weight is applied once to
each physical coefficient and cash event. Expected mortgage monetary states
already contain their native exposure mass; no extra survival factor is applied.
Purchase cost is one quote per unit/scenario, independent of path weights. Asset
bank purchase cash is negative; deposit liability issuance cash is positive.

## Physical age conditioning and pricing

For every reporting left boundary at/after purchase, scenario and physical path,
an `AgeBranch` supplies `ordinal`, `scenario_id`, `physical_path_id` and a full
purchase-origin native graph. It contains one actual physical path, retaining the
complete original physical input with its weight changed to 1 solely for this
conditional branch. It supplies its own pricing/down/up futures with normalized
conditional weights. Native paired-bump admission applies.

The branch replays the original purchase contract and exact physical prefix;
it does not restart from caller-supplied scalar balances. Native full replay
retains card lots/grace/statements, mortgage payment/remittance entitlements,
original payment and burnout, and deposit rate/retention source state. Known
contracts and past actions/drivers/policies must match. Independently generated
prefix cash, state points, earned accrual and entitlements must also match.
Installment raw known contract and exact past actions prove its event-grouped
state; recognition is integrated through the actual age rather than requiring
future events to have identical interval grouping. Changes to any actual prefix
fail. Complete native results and all source tags remain retained.

At purchase, conditional Q/down/up source rights, weighted once by the physical
path distribution, must reconcile the original purchase graph's full Q/bump cash
by role/scenario/date/component at `max(1e-9,1e-12*amount)` tolerances. This prevents
a branch from silently replacing inception pricing. Later supplied Q futures can
reflect the explicitly replayed actual age information. There is no P-to-Q
transformation or inferred future behavior.

For a cash right at date `t>age`, the native mark uses
`cash(t)*D((t-purchase)/365)/D((age-purchase)/365)` times
`exp(-(original_OAS+bump)*(t-age)/365)`. Actual curve ratios, log-DF interpolation
and explicitly declared curve tails reuse the native supplied-curve implementation.
Cash on the boundary is already settled and excluded from the remaining mark.
Down/up futures use the same conditional pairing and +/- native curve spread
bumps; DV01 is `(PV_down-PV_up)/(2*bump_bp)`, not origination DV01 scaled by balance.
Conditional Q weights apply once, followed by physical path weight once. Deposit
marks and DV01 are bank-perspective signed values; complete native graph reports
also retain positive liability PV for source comparison.

## Reviewed effective-yield external income

Native `dated_income::evaluate` starts gross carrying value at actual purchase
cash and retains the supplied original EIR `y`. On each actual interval of `d`
days it earns `B*expm1(log1p(y)*d/365)`; principal and interest cash reduce carrying
value, principal receipts separately reduce outstanding principal, and native
recognized contractual interest updates accrued rights. Its basis adjustment is
`carrying-principal-accrued`. Integral/nonintegral fees and yield changes are absent
in this admitted slice. Producer recognition endpoints and actual report/cash
dates form the complete event grid. Uniform native recognition intervals are
split by actual elapsed days; contract coupon day-count remains separately native.

The income coefficient is weighted effective interest divided by unit face,
positive for assets and negative for liability expense. It is external earned
interest, not an internal FTP transfer or a gross contractual coupon relabel.
Principal repayments and purchase cash are never income. Carrying value continues
for explicitly unpaid rights after principal ends, including card unbilled
interest and mortgage remittance tails. Full nominal source cash remains separate
from income and can extend beyond reporting. Principal, accrued interest and gross
carrying value must retire within `1e-10*max(total_absolute_cash,1)` at the complete
native cash/accrual endpoint; native negative-carry/domain rules also apply.
No residual book value is automatically written off by this module.

The accounting distinction follows the effective-yield concept of reconciling
estimated receipts/payments with carrying value; the
[September 2025 IASB staff paper, paragraphs 7 and 10](https://www.ifrs.org/content/dam/ifrs/meetings/2025/september/iasb/ap11a-determining-effective-interest-rate.pdf)
also discusses estimation judgments for conditional terms. That discussion paper
is a modeling reference, not a binding decision or an endorsement of this
pathwise policy. Reviewed policy identity and raw provenance remain required;
the module makes no IFRS/GAAP compliance or automatic allowance claim.

## Coefficient and cash contracts

The result retains the whole `raw_source`, all complete purchase and age native
results, locked OAS, reviewed income requests/results and report opening states.
`shape=[scenario,unit,10,report_period]`; its metric order is unchanged from aged
units: NII, principal balance, signed DV01, HQLA L2A, 30-day outflow, ASF, RSF, RWA,
asset principal and liability principal. The five supplied weights multiply
principal outstanding after boundary cash, including unpaid principal remittance
rights. They remain supplied research/policy coefficients, not inferred regulatory
eligibility or carrying-value RWA. Category labels do not establish HTM feasibility.
Before purchase all metrics and cash demand are zero.

`ProductFunding` retains scenario-tagged source events, their unit/path/event IDs,
weights, purchase/principal/interest/fee components, bank cash, sorted full-tail
date union and `[scenario,unit,date]` demand. Demand is the negative cumulative
bank cash after all events at the date. Stable same-date order follows original
unit/path/source event order. Actual per-scenario source events, rather than one
shared contractual schedule, independently determine each scenario demand.
Fee cash remains zero under this policy. Gross pretax end-of-day netting excludes
unsupplied taxes, distributions, reinvestment and intraday rules.

Existing aged funding types assume scenario-invariant settlement events. This
module intentionally exposes a separate source contract. A later coordinated
strategy route must pass these coefficients into native HiGHS construction,
independently replay every scenario's allocated cash and all capital/category/
constraint amounts, preserve transactional immutable library/cache identity and
keep interactive evaluation coefficient-only. Those integrations are not claimed
by coefficient preparation or its tests. Historical aged/fixed units are untouched.

## Bounded execution and validation

Admission accepts at most 256 units, 32 templates, 13 scenarios, 120 reporting
periods, 16384 conditioned branches per unit, 32 MiB raw input, 32 million aggregate
work units, one million coefficient/funding values and 250000 cash events.
Every graph's native domain/work helper runs before the first financial generation.
Additional cost charges all source inspection, native branch replay, prefix and
cash comparisons, income's `dates*(recognition_rows+cash_rows)` scans, and dense
coefficient/full-tail funding reductions. Upper row/date bounds include possible
zero events. Aggregate funding admission uses a conservative event/date upper
bound, so it can reject a book whose eventual sparse realization would be smaller.
Retained output is bounded conservatively at four times JSON size within 128 MiB.
These ceilings are not aggregate RSS or 60000-position scale evidence. Cooperative
checkpoints apply across serialization, admission, graphs, paths, income dates,
marks, funding and retained-output completion.

[`examples/models/product-units-1.json`](../../examples/models/product-units-1.json)
is a synthetic two-day 1000 deposit with coupon .365, EIR .02, quote
`1002/(1.02^(2/365))`, and supplied locked OAS `log(1.02)-.05` against a .05
collateral curve. Daily coupon expense is 1; effective interest is about .054
currency units, with premium amortization retained explicitly. Native hand tests
cover this distinction, negative coupons/yields, independent consumer annuity and
mortgage remittance math, post-principal card interest rights, exact path weights,
conditional Q versus actual P prefix, scenario-specific runoff funding, future
purchase zeros, strict unsupported transitions, immutable anchors and aggregate
quadratic-work rejection before financial generation.
