# Supplied funding and repo product graph

Rust `funding_graph::analyze(&Request)` and immutable `FundingGraph` own the
`funding-product-graph-1` analysis stage of #21. They replay whole-book supplied
P/Q/Q-down/Q-up `funding_contract::FundingRequest` histories, price native cash,
retain the original whole-book OAS and report physical funding/collateral flows.
Whole-book replay preserves the component's exclusive global owned-collateral
inventory; pricing individual contracts through isolated inventories would not.
Public methods are `new`, `request`, `snapshot`, `base_anchor`, `reprice` and
`repriced`. No Python financial execution or backend fallback is introduced.

## Known state and supplied history boundary

`known` is immutable contractual opening/term metadata using `FundingRequest`:
explicit actual currency units, asof/tail, owned inventory, original contracts,
and their initial collateral agreement/pledges/snapshots. It has contractual
measure, maturity exits without elections and empty future margin actions. It
is an anchor template, not an implied financial projection. Indexed conditional
observation slots can retain explicitly Physical/Pricing source metadata; every
executable history supplies its own compatible declared measure and rate sources.
No future observed fixing is synthesized for that template or any history.

Each history includes the full executable native request and its probability and
path ID. Graph scenario IDs and native run IDs are separate retained identities;
history `scenario_id` equals the graph scenario's `run_id`. Measures must be
explicitly P for physical and Q for all pricing roles. No measure is converted
from observed/default behavior. P IDs/weights can differ from Q. Ordered Q path
IDs/weights are identical across all scenarios and both bumps; each role's
probabilities sum to one within `1e-12`, without normalization or path pruning.
Zero-weight paths are still validated and executed. The randomness domain is
`supplied_paired_histories-1`; this graph consumes supplied paired histories and
generates no draws or fitted funding-access/default/rollover behavior.

Every history preserves known static contract identity, opening principal,
origination terms, entity/counterparty, currency, lending/borrowing direction,
funding kind, actual period/payment dates/daycounts/fees, fixed coupon rates and
initial collateral policies/inventory/pledges/marks. Explicit future margin
snapshots/transfers, exits and one separately specified rollover replacement
generation can vary. Indexed conditional observations preserve geometry,
selection/reset convention, descriptor, margin/bounds and calendar policy while
their declared conditional rate/source/path can vary. Observed fixing content
is immutable. Snapshot and observed fixing source ID/revision pairs cannot be
reused with different content anywhere in the graph. Fixed coupons cannot be
quietly treated as scenario-dependent floating rates.

The reused component admits fixed and explicit indexed coupons with observed
publication or conditional information cutoffs. An indexed business-day mapping
is supplied and retained as unverified; arithmetic acceptance does not prove a
calendar/fixing mapping. See [funding-contracts.md](funding-contracts.md) and
[term-enhancements.md](term-enhancements.md) for exact coupon, collateral,
settlement, default and supported-policy conventions.

## Signed funding cash and immutable OAS

PV includes only native Q schedule cash `principal + cash_interest + fees`,
strictly after asof, discounted by supplied collateral DF and
`exp(-base_oas * actual_days / 365)`. Lending receipts are positive; borrowing
liability payments negative. Signed and negative-rate coupon cash remains signed.
The required `after_opening_actions` timing retains asof advance/fee/security
actions in a separate opening report and excludes them from future PV and
`(left,right]` cash reports. Opening contracts are never re-advanced.

The graph's single `book_id` identifies one whole-book base spread.
`oas_target=locked` is required for mixed-direction books or signed directional
future cash. For a homogeneous lending or borrowing book,
`calibrate_positive_cash` targets the positive **magnitude** of future cash in
that direction; borrowing still produces negative signed PV and DV01. The
solver rejects any future event whose net contractual cash opposes the book
direction, even on a zero-weight path. It does not net an old repayment and new
advance into an artificial positive calibration instrument. Bounded log-sum-exp
bisection uses the original base Q histories/curve and [-5,5] OAS interval.

The retained base key contains the complete known dependency and base Q raw
requests, original target/source, base scenario/curve, book/model/paired-history
identity and explicit cash/tail policies. Repricing checks exact dependency
equality, reports `BASE_ANCHOR_CHANGED` when it changes, and never solves a
scenario OAS. Nonbase scenarios, P/down/up histories, input revision and report
boundaries can change while preserving that anchor. The raw request's final
report date must match the graph; report dates are excluded from calibration
identity. `supplied_history_dv01` is a signed central finite difference using
paired Q-down/Q-up cash and native +/- curve bumps with original OAS. It is
conditional supplied-history risk, not an empirical funding spread or legal
collateral-option valuation.

## Cash, collateral, default and full tails

P reports expose signed principal/interest/fees, new advances, maturity principal,
retained/residual recoveries, explicit default claims and reviewed separate
economic principal/interest losses. Margin call/unmet values and rollover
requested/executed/access-shortfall amounts are separate diagnostics. Security
quantity movements are grouped by asset, owner and disposition; quantities in
different assets are never added into a single monetary cash balance.

Ordinary pledge, margin, substitution and release never generate cash/PV. Native
collateral realization cash already contains the retained principal/interest
recovery once. Gross sale proceeds, sale cost, net proceeds and surplus returned
are explanatory amounts, never a second sale cash flow or a second haircut.
Physical reports preserve those audit explanations distinctly from recovery cash.
Failed management-access transfers remain pending, retain the margin breach and
do not erase maturity or collateral release. Rollover requires complete separately
identified replacement terms; unavailable access does not invent new funding or
remove old maturity principal.

The required `complete_economic_cash_collateral_tail` policy retains all explicit
maturity, replacement, recovery and collateral disposition through the supplied
tail, independent of reporting horizon. The native component reconciles each
default's principal and accrued claim separately to recoveries plus supplied
economic loss; it preserves report-date gross default claims rather than
pretending an economic loss is legal/accounting extinguishment. Inventory remains
conserved and tail encumbrance is released/disposed; outside encumbrance is never
released by these contracts. The graph retains full raw/native sources and a
separate physical tail beyond the final report boundary.

Default is inherited as **Physical cash-lender counterparty default only**.
Pricing-measure default requests, own borrowing default and unsupported legal
netting/closeout fail closed. Supported P defaults can be paired with explicitly
different Q maturity histories; their empirical or risk-neutral probability
relation is never inferred. This restriction is not full credit-risk pricing.

Cash interest is truthful native settled cash. Full coupon audits and original
intervals are retained. Uneven overnight compounding and default supplied accrued
claims are not prorated into a claimed daily earned-income measure. Gross daily
recognition plans, effective-yield NII, reviewed allowance/charge-off/journal and
treasury/liquidity/FTP adoption remain separate integration gates.

## Bounds and independent fixtures

Preflight charges every supplied full book before projection: up to 256 old plus
replacement contracts, 4,096 owned assets, 4,096 periods/observations/margin steps/
recovery points per supported component field, 13 scenarios, 128 paths per role,
120 actual report periods and 4,096 runs. Worst-case native cash/audit/source rows
are bounded to 250,000; coupon/observation work, registry scans, action/asset
sorting, calibration passes and ordered per-period security aggregation are
charged against 8 million graph work units. Input byte counting precedes canonical
dependency allocation at 32 MiB; retained raw request/source/result/base identity
uses a 128 MiB capacity estimate. These are component admission bounds, not an
aggregate process RSS or 60,000-position combined workflow gate. Native source
domain/chronological feasibility remains authoritative and failures publish no
successful result.

The [raw synthetic example](../../examples/models/funding-product-graph-1.json)
uses initial repo principal 100, ten-day ACT/360 rate 0.36 and a separately
supplied 60-principal replacement at rate 0.72 for another ten days. Initial
advance -100 is asof; old maturity 101 and replacement advance -60 are distinct
events, followed by final replacement payment 61.2. Collateral discount rate
0.05 and locked OAS -0.05 cancel, so future PV is 102.2. First-period margin call
is `100.2 - 112*0.9*0.8 = 19.56`; delivery of 40 produces report pledge 152.
Both maturity releases remain in the complete tail. This is a hand fixture,
not observed market access, a quote or an empirical calibration.

Focused tests cover cash/sign/fees, original opening anchors, closed-form OAS,
fixed-anchor reprice, separate P default/Q maturity, final-date recovery, sale
cost/surplus once-only cash, margin outage, explicit rollover/no access, supplied
index measures/reset cutoffs, Q pairing, immutable sources and preprojection
work/tail admission. Reference tolerances stay `1e-12` relative and `1e-9` absolute.

[ICMA's haircut explanation](https://www.icmagroup.org/market-practice-and-regulatory-policy/repo-and-collateral-markets/icma-ercc-publications/frequently-asked-questions-on-repo/21-what-is-a-haircut/)
distinguishes collateral-value haircuts from initial-margin ratios; the reused
component chooses the former explicitly and does not apply it again to sale cash.
The [New York Fed reference-rate methodology](https://www.newyorkfed.org/markets/reference-rates/additional-information-about-reference-rates)
distinguishes publication/observation and revisions, motivating exact cutoffs and
source identity. Those sources do not establish private repo enforceability,
reuse/netting rights or empirical scenario probabilities. Full #21 saved-book,
verified daily journals, liquidity/FTP, legal scope and joint scale acceptance
remain open until independently integrated and validated.
