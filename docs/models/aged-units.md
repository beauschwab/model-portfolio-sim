# Actual dated fixed-contract aged coefficients

`aged-units-1` prepares a bounded dense coefficient tensor in Rust. The initial
supported slice is fixed, nonoptional corporate contracts and fixed CDs with
disabled early withdrawal. `advanced_product::project_fixed` constructs their
actual calendars through the same `dated_term` producer used by standalone
contract analysis; `advanced_product` also owns remaining-right valuation,
purchase OAS solving and immutable-yield book replay. Future graph/cohort/aged
consumers can reuse those functions. There are no caller-prepared cashflow,
opening-state, risk or income tensors in the input.

The synthetic request is `examples/models/aged-units-1.json`. It contains a unit
zero-coupon asset purchased January 31, 2024, with reporting boundaries January
31, February 29 and January 31, 2025. These are actual Gregorian ordinals; there
is no 30-day month approximation. The native module does not automatically
replace historical `strategy-library-1` or the existing approximate forward
program path. Root integration uses the same dense vectors in a versioned dated
strategy route; publication/evaluation acceptance must be validated separately.

## Input and identity contract

The request contains a strict schema, model/input/grid/CRN revision labels,
currency, curve-as-of ordinal, `H+1` reporting ordinals, explicit base scenario
ID, ordered scenario collateral curves, fixed term templates, prepared purchase
nodes, optional off-grid validation purchases, a positive bump in basis points,
and supplied absolute/relative interpolation tolerances. Dates must increase;
purchases occur at reporting boundaries excluding the final boundary. Each
template's prepared purchase dates increase in input order, and `(template,date)`
identities cannot repeat across prepared and validation purchases. Stable input
scenario/unit ordering determines `[S,N,10,H]` tensor ordering.

Each template declares corporate/CD product, side, term in calendar months,
frequency, coupon, amortization, day-count, explicit calendar and payment/accrual
adjustment policies. Corporate side is `+1`, CD side `-1`. Native construction
sets unit face, rolls maturity from the **purchase** date with the shared clamp-day
calendar helper, preserves complete accrual/settlement tails, and disables calls,
puts, floating resets and withdrawal behavior. Corporate equal-principal legacy
`annuity` and true `level_payment` policies are distinct. Sinking schedules and
CD principal amortization are rejected. Frequency zero is only admitted for CD
single-payment conventions. Calendar and day-count assumptions retain the
limitations disclosed in [dated contractual terms](dated-term-contracts.md).

Each purchase supplies actual unit cash price, original annual effective yield
and its immutable source anchor. Price is an inception total-right quote with
zero opening accrual; it is not a percentage quote. The native yield must discount
the complete contractual cash schedule to purchase price within
`1e-10 * max(price,1)` at inception, and separately retire native book replay
within `1e-10 * max(total absolute contract cash,1)` at the terminal date. An inconsistent yield
fails instead of being refit. Original-yield anchor, quote, fixed base OAS,
actual purchase/full-tail dates, and replayed opening states accompany each unit.
Results retain full scenario curves and template policies, source revisions,
reporting dates, bump and native projection model `advanced-fixed-contracts-1`.
Consumers must hash/compare the immutable full request as well as labels for
collision-safe cache/session identity; a caller's label alone is not equality.

Accounting category and capital-policy revision are explicit dependencies;
the five supplied weights remain research/policy inputs. This module does not
infer regulatory eligibility, HTM sale rules, tax policy or credit RWA. A category
label in prepared output is not daily replay acceptance.

## Separate valuation and original-yield income

The supported valuation policy is explicitly `contractual_discounting`.
Contractual nonoptional rights are deterministic and measure-neutral. This slice
does not translate a physical deposit/state model into a pricing-measure model,
infer risk-neutral default/prepay hazards, or claim those behavioral models are
supported. Future state products need separate supplied physical/pricing inputs
with native validation of purchase-to-age prefix states before conditioning.
Clipping a pricing projection made at purchase does not establish an actual aged
state under a different physical history.

For curve anchor `a`, age date `v`, cash date `t` and continuous OAS `s`, the mark
is `sum cash(t) * D((t-a)/365)/D((v-a)/365) * exp(-s*(t-v)/365)` over `t>v`.
Log-DF interpolation and explicit tails reuse the native supplied market curves;
see [market model definitions](market-models.md) and the primary
[Strata log-linear definition](https://strata.opengamma.io/apidocs/com/opengamma/strata/market/curve/interpolator/CurveInterpolators.html).
Discount time uses ACT/365 independently of contractual coupon day-count.
Changing collateral curves changes marks, not already issued coupons or original
yield. Curves must be collateral-role, common-currency curves.

Base OAS is solved once at **purchase**, against the explicitly named base curve,
to the actual unit quote. All ages, scenarios and bumps retain that OAS. The
root solve uses positive remaining cash rights, log-sum-exp and a unique monotonic
`[-5,5]` spread bracket with 100 bisections. Signed intermediate coupon cash is
rejected by this initial solver; negative-rate single-payment CDs are supported
when their total final right remains positive. No scenario OAS refit occurs.
Age DV01 is the native remaining mark difference
`(PV(-bump_bp/10000)-PV(+bump_bp/10000))/(2*bump_bp)`, in unit currency per
basis point. It is recomputed at each reporting date, including full tails,
and is never origination DV01 multiplied by a balance ratio.

Book replay starts from actual purchase cash price. Between settlements, carrying
value compounds at frozen annual effective yield `y` using
`B_new=B_old*exp(log1p(y)*actual_days/365)`; contractual cash then reduces carrying
value and principal cash reduces unsettled principal rights. Same-day source
settlements retain stable order. A negative carry beyond numerical residual
admission fails. Once complete contractual rights retire, a roundoff-size terminal
residual alone can be cleared. Each reported opening state is generated by this
native purchase-to-age replay, not accepted from a provenance string.

The effective interest for an interval is
`carrying(right)-carrying(left)+cash_paid(left,right]`. Purchase cash is external
funding, not income; principal receipts are not income; integral/nonintegral fees
are absent because this fixed contract supplies no such policies. The same
fixed yield applies under all market scenarios. The
[dated IFRS 9 text](https://www.ifrs.org/content/dam/ifrs/publications/pdf-standards/english/2021/issued/part-a/ifrs-9-financial-instruments.pdf)
supports separating original effective yield and contractual cash as modeling
concepts; this simplified unit recurrence is not an automatic accounting policy
decision or standards compliance determination for a bank or CD liability.

## Reporting convention and vector layout

Each reporting boundary is **closing after settlement**. Risk excludes cash paid
exactly on the boundary; state includes its retirement. Column `m` contains the
state/risk at boundary `m` and income over `(boundary_m,boundary_(m+1)]`.
Before purchase every coefficient is zero. At actual principal settlement its
remaining balance and risk retire. A following convention can leave a payment
right after unadjusted maturity; a preceding convention can settle before the
contractual accrual end. Complete tails and actual settlement dates govern those
rights. This model publishes carrying/principal/cumulative cash states, not an
invented accrued-interest opening or whole-coupon daily NII. Full daily contractual
accrual/right posting remains the separate dated-store accounting responsibility.

For dense `values[((scenario*N+unit)*10+metric)*H+period]`:

| Metric index | Meaning |
| --- | --- |
| 0 | Side-signed frozen-yield effective income |
| 1 | Unsettled contractual principal magnitude |
| 2 | Side-signed native aged DV01 |
| 3–7 | Supplied HQLA L2A, 30-day outflow, ASF, RSF, RWA weights times principal |
| 8 | Asset principal magnitude, zero for liabilities |
| 9 | Liability principal magnitude, zero for assets |

The layout matches the existing solver's ten metrics. Quote price and cash replay
remain separate. `strategy-aged-1` replaces principal funding rows with explicit
full-tail cash-demand rows, accepts discount/premium purchase quotes and reports
allocated face separately from actual asset purchase cost. Its optional
`max_purchase_cost_assets` cap uses quotes; `max_total_assets` remains a face cap.
The historical solver route retains its existing contract. Source-linked saved-book
cash resources and category-aware candidate daily replay remain additional work.

The additive `funding` result now preserves native `purchase_costs`, stable
purchase/settlement `cash_events`, explicit `scenario_ids`, a sorted full-tail
union of reporting/purchase/settlement `ordinals`, and dense `[S,N,F]` cash-demand
values. Before purchase demand is zero; thereafter it is
`side * (quote - cumulative contractual cash settled)`. Positive demand consumes
starting cash; negative demand is retained cash. Purchase costs are bank cash
`-side * quote`, settlements are `side * (principal + coupon)`. These arrays
deliberately preserve coupon settlement independently of earned NII and include
cash obligations after the reporting horizon. There are no inferred taxes,
distributions, discretionary reinvestment, intraday phases, fees or rollover
sources. The returned policy explicitly states gross pretax contractual cash
retention and end-of-day netting.

Funding data is produced from the same native projection as risk/income, rather
than supplied by an adapter. Contract source per unit, whole input revision,
event IDs and unit indices retain lineage. Native dated solving consumes the
same prepared cash tensor, then independently replays allocated source events.
Coefficient-only evaluation reports actual funding demand and full-tail dates,
with principal exposure reported separately. Missing funding tensors require
rebuilding. The 15 public aged-strategy tests pass against the rebuilt native
artifacts with no skips. [The funding contract](aged-funding-design.md) specifies the native
rows and their full-tail/resource assumptions. Component funding bounds admit
one million dense demand values, 250,000 purchase/settlement events and the shared
32 MiB metadata cap; these are additional to ten-metric and pricing-work limits.

## Interpolation diagnostics, bounds and acceptance

No interpolation modifies prepared native nodes. Off-grid validation purchases
must supply their own actual quote/yield anchor and must lie strictly between
prepared purchases of the same template. Native full construction/replay at each
such purchase is compared with linear interpolation by actual ordinal of the
two dense coefficient rows. This intentionally exposes errors around purchase,
amortization and maturity boundaries rather than promising interpolation accuracy.
For each scenario/metric it reports maximum absolute error, peak period,
maximum relative error where native value is nonzero, and whether every cell meets
`absolute_tolerance+relative_tolerance*abs(native)`. Relative errors that cannot
be represented finitely fail; zero native values use the absolute test. Outside
grid tails fail instead of extrapolating flat. Interactive consumers use immutable
prepared coefficients only; no pricer is part of evaluation.

Admission allows 32 templates, 13 scenarios, 512 prepared purchases, 128 validation
purchases, 120 reporting periods, one million total dense scalar values including
validation work, and eight million aggregate event-pricing/replay/root work units.
Terms are at most 1,200 calendar months and full horizons at most 36,600 days.
Dates, bounded nonblank identities, directions, money up to `1e15`, finite yield
`[-0.99,10]`, coupon `[-1,2]`, weight `[0,100]`, positive quote and bump domains,
roles/currencies, scenario coverage and unsupported semantics are validated.
Inherited curve/calendar/schedule limits remain additional. Root and cash replay
are bounded; cooperative checkpoints run between rows/scenarios/marks. This is
component admission, not an aggregate RSS or combined 60,000-position guarantee.

The 23 analytical fixtures check actual purchase/maturity/leap dates, unchanged
principal with declining aged risk, scenario marks at frozen base OAS, original
effective income and native state conservation, equal-principal and true
level-payment behavior, CD signs and negative yields, preceding/late settlement,
stable scenario identity, actual off-grid error, strict admission, cancellation,
and persisted JSON against independent closed-form values. Quote-aware cash cases
cover discount/premium cost without a par gain, coupon cash after earning,
full-tail short-CD funding gaps, future issuance not funding an earlier purchase,
matched maturities, scheduled and partial amortization funding later origination,
cash/income conservation and large-quote yield rejection. Public engine tests
separately gate native solver/evaluation ownership and coefficient-only timing.

On October 2, 2026, the 15 public tests passed against rebuilt native artifacts:
discount/premium purchase cost and NII conservation, a cost cap binding separately
from face/cash caps, coupon settlement timing, future CD issuance rejection,
short-CD/long-asset full-tail infeasibility, same-day cash netting, input-derived
cash-event replay, missing-grid/metric/funding rejection, exact capital-refresh
dependencies and fresh-process Rust-default ownership. Pricers and coefficient
rebuilding are forbidden during interactive evaluation. The warmed one-unit,
two-period median across 128 evaluation samples is **41,500 ns (0.0415 ms)**.
This is measured fixture evidence, not a portable speed or large-book guarantee.
The product DLL SHA-256 is
`94841147112e7814e8db6c203eb058c9a4c3e2ffcc12957b87bad04cffac3da0`.
The decision DLL SHA-256 is
`6bc037bc8d9e5a02fd61c4042fd7899f4a17fd6f04fe2838532fd1a22e386b86`.
Actor/durable publication and independently verified candidate journals retain
their separate acceptance gates.

Full #23 remains open for mortgage/deposit/indexed/consumer physical versus pricing
state conditioning, quote/coupon rules from native projected indices, observed
behavior and credit, source-linked saved-book/reinvestment cash conservation,
state-dependent quote-aware solver limits, durable library/cache/dirty-row and
capital-refresh acceptance, coefficient
evaluation performance with no callbacks/pricers, solver allocation and independently
verified immutable candidate journals. Exact fixed-contract kernels do not close
those gates or establish empirical accuracy.
