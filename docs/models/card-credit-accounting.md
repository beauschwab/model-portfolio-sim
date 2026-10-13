# Acquired card runoff and reviewed credit intents

`card-credit-accounting-1` is an explicit native source contract for one acquired,
initially performing card account with a supplied physical runoff history.
`consumer_card::simulate` owns all principal, contractual interest, billed state,
payment/default and recovery economics. The adapter uses frozen original-EIR
income and the same reviewed `credit_accounting` operator as the installment
owner. No second principal/default projection or probability scaling runs.

Public Core export, native lifecycle dispatch and transport-only adapter are
implemented and pass 15 public native fixtures plus 93 independent/public DEBUG
process checks (55 pure source checks and 38 actual lifecycle cases). The official
product release is rebuilt, and all 93 checks pass against its default lifecycle.
Ten durable primitive publication/reopen/failure-fence API checks also pass.
The tagged installment/card mixed owner also passes independent source/ordered
journal, durable publication and full-tail closing-capital checks. The rebuilt
bundle passes 1,627 engine tests (two reference-only skips), 445 API tests and
37 browser cases, including primitive/mixed save and reopen. These local checks
do not establish deployment; the wider gates below remain required.

## Admitted source and explicit closure

The strict request retains raw `CardRequest`, path/economic-source identity,
purchase date/principal/cost/original yield and anchor, review policies, report
date, allowance and chargeoff instructions, and an explicit `Termination`.
Economic-source revision equals the raw contract revision. Aggregate opening
principal is positive and exactly equals purchase principal; purchase occurs at
as-of. The source must have zero opening billed/unbilled/unrecognized interest,
fees and grace shadow. Non-PCD gross acquisition cost and an independently
reviewed original annual effective yield are supplied, never inferred from Q.

The first lane accepts `Payment`, `Default` and one actual final `SetLimit0`
for reviewed nondefault closure. It rejects new draws, fees, rate/state changes,
other limit edits and as-of actions. Both purchase-grace switches are false.
ACT/360 or ACT/365 fixed contractual accrual, class rates, genuine promo expiry,
minimum/excess allocation, statement cycles and billed-only compounding remain
the card projector's supplied conventions. Unsupported events reject the whole
request; they are not pruned from the result.

`SourceDefault` requires the exact native source default. A nondefault
`ReviewedZeroRightsClosure` requires a reviewed date, one raw zero-limit action
on that date, no later actions, and zero principal/billed/unbilled/fees/limit/
undrawn at closure and full source end. Limit zero is not itself an inferred
legal closure; the review declares closure and the source proves financial rights.
Accounting status is retained separately from source `ConsumerState`: a zero
card balance is not relabeled as an installment `PaidOff` state.

## Interest, statements and original income

Native daily earned non-grace recognition is accounted once. A statement moves
earned unbilled interest into billed charges without recognizing income again.
Payments settle posted finance charges under the original card waterfall; they
do not settle unbilled charges prematurely. Recognition indices plus the retained
source identify lot/date intervals, because a card lot origin ID repeats across
days. `StatementReceipt` retains native index/date/due and explicitly records
`interest_recognized_again=false`.

The original EIR prefix uses purchase gross carrying value and the existing
ACT/365 fixed recurrence `C * expm1(log1p(y)/365)` on actual dates. Source coupon
day count remains separate. Principal and cash interest reduce carrying value;
earned interest updates the accrued right, and `effective - contractual earned`
updates basis. For reviewed closure, its day's final cash is excluded from the
prefix recurrence so actual ordered cash then reviewed signed basis retirement
can execute explicitly. Original EIR continues through an admitted interest-only
tail and stops at default or reviewed closure. The income operator's carrying
guards and shared accounting policy guards are unchanged.

For example, principal 100 at 36% ACT/360 earns 1 through day 10. A payment of 100
before the day 10 statement pays principal while leaving unbilled interest of 1,
which is subsequently billed and paid on day 11. Reviewed cost 101/original EIR 0
retains principal 0, accrued interest 1 and basis 0 between those dates, then
closes with total cash 101 and no income plug. Paying principal to zero on day 10
does not retire that accrued right.

**Current basis limitation:** cost 100/original EIR 0 in that same valid card history
leaves principal 0/accrued interest 1/basis -1 and total carrying 0 during the interest tail.
The shared accounting operator rejects negative principal-plus-basis. This source
retains an explicit rejection regression rather than skipping that guard. Positive
EIR can likewise leave a discount allocated to an interest-only asset; reviewed
basis-on-interest ownership is a required future extension. No asset/right is
silently zeroed, and an original EIR is not changed to force admission.

The admitted fee-free installment adapter has a different payment waterfall:
recognized and unrecognized coupon settle before principal. Its material
principal-zero/unpaid-coupon state is therefore unavailable in the current lane.
This distinction does not authorize relaxing the shared accounting guard for cards.

## Default, recovery and commitment

Prior-day recognition occurs before supplied same-day actions. A payment before
default reduces the exact source principal under the card waterfall. Default
then consumes actual remaining principal plus all recognized billed/unbilled
interest and current basis. The shared operator reverses or transfers accrued
interest and recognizes or transfers basis under the immutable reviewed policy.
Payment after default is rejected by the card projector. All balance classes
map to the performing credit origin in this lane; purchase/cash/promo classes
are never misidentified as performing/watch/delinquent credit states.

Gross principal-only recovery and cost are separated exactly as for the installment
adapter: shared claim loss is `P-gross`, gross cash reduces that claim, and a
separate reviewed same-date expense pays cost. Their net equals native source
recovery. Economic total loss is a retained diagnostic, not an extra provision.
The first policy requires `0 <= cost <= gross <= P`; it rejects surplus proceeds
and uncovered cost. If gross equals cost and the native model emits no net-cash
event, lineage retains the actual default coordinate rather than inventing a
recovery cash ID. Reviewed chargeoffs and allowance updates remain separate.

`Daily` retains source billed/unbilled arrays, limit and undrawn alongside accounted
monetary state. Undrawn is an unfunded source commitment under
`retained_source_undrawn_no_ccf_no_draw-1`; shared monetary undrawn/contingent draw
is zero. Its receipt retains the default/closure retirement coordinate and policy.
There is no implied CCF, additional funded draw, unused-line pricing or full
off-balance-sheet EAD claim. The current source lacks a predefault event-phase
commitment snapshot; wider CCF ownership needs that source and booking contract.

Reporting is an actual-date end-of-day snapshot. Full replay keeps all economic
recovery and reviewed chargeoff dates. Unpaid default claims retain principal,
accrued and signed basis instead of being written off automatically. The primitive
seeds acquisition monetary state but emits no acquisition cash/equity journal.

## Bounds, tests and adoption

Limits are 1080 days, 64 review instructions, 1 MiB raw input, 64 MiB retained
result, 2 million conservative adapter work units and the shared 1e15 monetary
bound. Native card's existing 250,000 lot/day/cycle admission also applies. Cash and
recognition are indexed by actual date, avoiding day×all-recognition scans.
Cooperative native control checks surround bounded projection/income stages and
daily accounting; failure returns no result. These are component limits, not an
aggregate process-RSS cap. Existing final/ownership tolerances are unchanged.

The focused native fixtures cover prestatement default, billed versus unbilled
recognition, interest-only rights, premium/discount/negative original EIR,
same-day payment/default/recovery/chargeoff order, class/promo behavior, gross/net
cost, retained claims/undrawn memo, unsupported ownership, cancellation and the
negative principal-basis rejection. Fifteen native fixtures and focused strict
Clippy pass for the public source, along with 93 independent/process checks and
10 durable primitive API gates. Shared mixed-owner and wider adoption still
require their separately documented publication and acceptance checks.

An exclusive mixed owner must acquire once, recognize initial allowance once
under its reviewed policy, apply allocation and physical weight once, post these
intents to the same pooled daily GL, and bypass duplicate legacy/monthly/default
ownership. Independent raw and persisted ordered journal/closing verification
remain publication gates. The primitive has no implicit mixed adoption or empirical claim.
General grace, fees, draws/CCF, nonaccrual, stochastic/empirical behavior,
solver/capital and combined scale scope under #19/#25 remain open.

## Primary context

[CFPB §1026.53](https://www.consumerfinance.gov/rules-policy/regulations/1026/53/)
distinguishes excess-payment allocation from issuer minimum allocation; the
projector's supplied conventions remain authoritative for this supported policy.
[CFPB §1026.54](https://www.consumerfinance.gov/rules-policy/regulations/1026/54/)
and [its interest explanation](https://www.consumerfinance.gov/ask-cfpb/how-does-my-credit-card-company-calculate-the-amount-of-interest-i-owe-en-51/)
make purchase grace and transaction APR distinctions material. This first lane
rejects the broader grace/fee boundary instead of treating shadow amounts as book
income. These references inform contract design; this supplied-source contract establishes
neither universal issuer terms nor regulatory/accounting compliance.
