# Native supplied mortgage product graph

`mortgage-product-graph-1` explicitly selects the bounded native
[`mortgage-state-1`](mortgage-state.md) model for pricing and physical source
analysis. It retains original payments, borrower/investor state, competing exits,
ARM fixings, investor entitlements, actual remittances, default/recovery claims
and supplied guarantee advances. Historical monthly mortgage economics are not
silently replaced. This stage does not add cohort publication, fitted histories,
an effective-yield income policy or mortgage journal adoption.

## Input and ownership

The request reuses `GraphIdentity`, `Scenario`, `OasTarget`, `Role` and
`BaseAnchor` from the advanced consumer graph. It requires `as_of_ordinal`,
currency, strictly increasing `reporting_ordinals` beginning at valuation,
`base_scenario_id`, `bump_bp`,
`terminal_policy="complete_generated_investor_cash_tail"`, supplied collateral
curves and positions. The [synthetic example](../../examples/models/mortgage-product-graph-1.json)
is a controlled short whole loan with supplied zero credit/turnover hazards.

Each position contains its original known `MortgageRequest`, target/locked OAS
and complete scenario-ordered history sets. The known template requires
contractual measure, empty `path` and empty ARM resets; it is not projected
directly. Every history explicitly supplies `path_id`, weight, assumption ID,
`MortgagePathPoint` records and `arm_resets`. Native generation declares physical
measure for P and pricing measure for central/down/up Q, then invokes the shared
mortgage state model. Fixed-rate histories must have no ARM resets. Index values,
publication timestamps and observed/projected source declarations remain inputs;
the graph does not derive future ARM fixings from the collateral discount curve.

Path weights must sum to one without normalization fallback. Every Q scenario
and bump side has identical ordered path IDs and exact weights. P is explicitly
separate. The required randomness domain is `supplied_paired_histories-1`; these
are supplied deterministic state recipes and expected population transitions,
not native sampled borrower paths. Matching IDs do not establish stochastic CRN
generation. Native shared-draw/model-policy integration remains an open gate.

Refinance, seasoning and burnout coefficients, state collection/recognition
fractions, LGD, complete recovery weights, guarantee policy and ARM contractual
terms remain supplied known-contract assumptions. P/Q paths supply their own
refinancing rates, turnover and credit intensities and selected ARM reset values.
This boundary does not infer HPI/geographic recoveries, pricing risk premia or
empirical coefficients. Changing contract recovery/agency assumptions requires
an explicit new base anchor rather than silently stressing a scenario parameter.
Unsupported securitization tranches, waterfalls, servicing-profit valuation,
guarantor default and implicit agency legal eligibility have no schema fallback.

## Full tails and unsupported interest rights

The projection horizon must include the complete remaining contractual payment
calendar. For agency ownership it must additionally extend through the final
contractual payment plus supplied default-buyout lag: the core generates complete
buyout cash but clips earned recognition at its projection end. This conservative
admission prevents omission of post-default investor coupon recognition.

Every generated path must finish with zero contractual reference principal,
live investor principal and live investor accrued interest. Full investor cash
principal plus explicit whole-loan principal loss must conserve opening investor
principal. No survivor scaling, terminal market value, fabricated recovery,
tail truncation or balancing cash is introduced. The state model's supplied
final contractual maturity payment remains explicit; it is distinct from a
reporting-horizon balloon.

Whole-loan default contractual/recognized accrued interest has no collection or
disposal policy in the shared state model. Such claims, or remaining borrower
interest rights, reject the entire graph rather than being assigned zero value
or silently omitted. The admitted whole-loan credit fixture therefore has zero
coupon and explicit principal recoveries/losses. Agency borrower arrears and
default interest remain issuer-side source evidence; supported guaranteed
investor coupon/buyout rights are separately generated and conserved. These
borrower rights are not added to investor valuation or discarded from results.

All actual remittance dates and complete borrower recovery cash remain in
sources even after reporting and projection. Valuation and physical-tail reports
include those dates. The supplied collateral curve's declared reject/flat-zero/
flat-forward tail applies; no undisclosed discount extension occurs. Supplied
calendar arithmetic is retained with `calendar_mapping_verified=false` from
the core; this is not a certified agency/index holiday mapping.

## Pricing, original anchors and risk

For each supplied Q path, investor cash after valuation is discounted as

```
PV = sum[(principal + interest + fees) × collateral_DF(t) × exp(-base_OAS × t)]
t = (actual remittance ordinal - valuation ordinal) / 365
```

The OAS target and source identity are explicit. Positive-future-cash targets
use bounded 100-step bisection in the original `[-5,5]` decimal OAS bracket with
log-sum-exp endpoint pricing. Signed future cash would require explicitly locked
OAS; it cannot enter that monotone positive-cash calibration. The shared state
model's present admitted investor cash is nonnegative. Zero-asset rights can
use locked OAS; a positive quote without positive future Q cash rejects.

`MortgageGraph::{new,request,snapshot,base_anchor,reprice,repriced}` holds immutable
`Arc` snapshots; `analyze` is the raw convenience route. Complete canonical base
content includes original known contract/state/policy, base Q path/fixing/reset
recipes, curve source/content, model/paired-history identity and OAS target.
Content equality is collision-safe and is not replaced by label equality or
hash equality. Repricing validates that anchor and holds the original OAS.
Changed base economic content requires an explicit new calibration. Current
non-base P/Q history edits and input revision changes can produce a new immutable
snapshot while preserving the original anchor.

`audit_admitted_work(&Request) -> Result<usize, String>` returns the exact
conservative structural work charge from the same graph admission, without
generating any path or solving OAS. After admitting aggregate bounds, it builds
each explicit measure/history request and calls shared
`mortgage_state::validate_request` for the existing core contract/fixing domains.
That helper forwards the projection's existing validator; it changes no model
economics. Output-dependent terminal/interest-rights rejection still requires
projection, and the audit does not prove that a requested quote is attainable.

`supplied_history_dv01` is the central difference of explicitly supplied Q down/up
histories with native negative/positive parallel curve bumps, divided by twice
the bump in basis points. It includes supplied ARM/index/behavior changes only
when those raw histories provide them. It is not evidence of a fitted joint
rate/HPI/credit law or a full native stochastic effective-DV01 model. Investor
remittance delays are discounted on the same supplied curve and fixed OAS as
other cash; no OAS-only delay shift is used in this versioned path.

## Physical reports and retained sources

`NativeSource` retains position/scenario/run/path/assumption identity, weight,
role and the entire `MortgageResult`. The graph request itself remains accessible;
publication must persist that complete raw request alongside its result. Each
result preserves original-loan/cohort identity, original payment, actual cash,
entitlements, ARM selected index/fixing revisions, state points, credit/default
claims, complete recoveries, agency buyouts and total advances.

Physical periods use actual `(left,right]` boundaries and report borrower versus
investor principal/interest cash, investor entitlements, principal defaults/loss,
separate recovery/buyout cash and `gross_recognized_investor_interest`. Explicit
positive-length recognition intervals are allocated uniformly when a reporting
cutoff splits them; instant cash-basis recognition remains on its original date.
This uniform report allocation does not reproduce intrainterval CTMC occupation
timing and is disclosed separately from the core's integrated reward amount.
Behavioral exits/default recovery clocks retain the core's interval-end expected
cash approximation. Supplied path refinement can change event timing.

`physical_tail` reports all generated dates after the final reporting boundary.
`physical_totals` separately retains scheduled/turnover/refinance principal and
aggregate guarantee principal/interest advances. The core returns aggregate
advances without individual origin dates; the graph does not invent period
allocations. Gross recognized investor interest is not frozen-yield NII,
servicing profitability, provisions or regulatory accounting income. No ledger
capture is exposed by this component.

## Bounds and evidence

Admission allows 256 positions, 13 scenarios, 128 paths per measure role, 120
reporting periods, 4,096 aggregate native projections, 250,000 conservatively
admitted/generated rows and eight million structural work units. It charges
combined payment/path/reset boundaries, worst-case default claims, complete
recovery/default-coupon tails, bounded shared operator evaluations, source
selection scans, OAS iterations and every physical reporting pass before
projection. The shared state/operator bounds remain additional. Canonical raw
input is limited to 32 MiB; retained request/result/base content uses a conservative
four-times serialized charge within 128 MiB. These component bounds do not claim
aggregate RSS or full graph/solver/daily scale acceptance. Checkpoints run across
native paths, scenario metrics and OAS iterations.

Independent fixtures calculate three ten-day scheduled payments: interest 0.1,
0.0661 and 0.0321661; remittance occurs two days after each entitlement. They
check actual-date PV/DV01 and full tail, distinct P refinance versus unchanged
Q, fixed original OAS through +/-200 bp, explicit Q bump histories, zero-coupon
whole-loan default/recovery conservation, agency advances/default buyouts without
duplicate borrower recoveries, ARM caps/original payment and annuity recast,
weighted zero-change identity and invalid domain/measure/tail/work admission.
The graph suite passed all 16 fixtures and targeted Clippy with warnings denied;
all 12 existing mortgage-state fixtures also passed after the validation-helper
addition. Tests include equal core/preflight invalid-contract/fixing errors and
issuer default-loss tails even when investor cash is entirely zero.
Source-component fixtures do not establish public/API publication or empirical
fit. Issue #13 retains dated empirical calibration/holdout/source hashes,
HPI/geographic recoveries, stochastic shared drivers, stressed loan/cohort audits,
effective-yield income, source-aware daily default/advance journals and complete
native runtime acceptance.

The CFPB's [index and margin explanation](https://www.consumerfinance.gov/ask-cfpb/for-an-adjustable-rate-mortgage-arm-what-are-the-index-and-margin-and-how-do-they-work-en-1949/)
supports retaining contractual index/margin inputs separately from market
discounting. Fannie Mae's [remittance description](https://singlefamily.fanniemae.com/servicing/principal-and-interest-remittance)
distinguishes payment activity reported by servicers from funds passed to
investors. These references motivate separate roles; they do not establish this
generic supplied guarantee model's legal or empirical accuracy.
