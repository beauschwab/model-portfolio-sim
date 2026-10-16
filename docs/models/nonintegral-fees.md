# Reviewed nonintegral fee ownership

The explicit `nonintegral-fee-accounting-1` native analysis contract separates
reviewed service fees from loan principal, gross carrying value, integral fee
amortization, original effective yield and OAS. It is an additive supplied-tape
component of issues15/25; the existing commercial credit compiler continues to
reject nonintegral cash without an integrated fee owner. This stage does not
close either issue or silently replace the historical cash-based income schema.

## Source and clocks

Each fee supplies a unique identity, source/revision, classification review,
service identity, position/tranche, signed entitlement and prepaid refund terms.
The source supplies currency/monetary unit, reviewed policy, actual opening,
report and complete-tail dates. A receipt-positive entitlement earns income;
a payment-negative entitlement earns expense. Integral/nonintegral classification
is a reviewed input, not a rule inferred by this model.

Events retain separate identity, exact ordinal, explicit within-day sequence,
source, review, reason and signed amount. Events must be globally ordered;
duplicate IDs/clocks, foreign sources, unknown fields/checkpoints, wrong signs,
invalid scalar categories and excess rights reject. Service earning, ordinary
cash and refunds have explicit service/settlement reasons. Waivers, future service
cancellations and recovery of waived amounts require explicit default, exercise,
maturity or other-reviewed reasons. These reasons are supplied history; a native
loan default or exercise does not automatically create a fee disposition.

## Money and journal

For one fee let T be signed total entitlement, E cumulative earning, C cancelled
unearned entitlement, W waived unpaid earning, R recovered waived amounts and K
net ordinary service cash (receipts less refunds). Local 0.29.18 additionally
retains F, cumulative reviewed earned-fee refunds. The signed unsettled
service balance is E-F-W-K and remaining earning capacity is T-E-C. With s=sign(T),
n=s*(E-F-W-K), the positive-fee owner retains receivable=max(n,0) and deferred
income=min(n,0). The negative-fee owner retains payable=-max(n,0) and prepaid
expense=-min(n,0). Separate lots never net opposite obligations.

Earning posts fee income/expense and gross rights; ordinary cash posts only cash
and rights. Payment can cross a receivable/payable into prepaid ownership in one
balanced intent. A reviewed waiver can remove only earned unpaid rights and posts
separate fee write-off expense. Recovery cash is bounded by waived amounts less
prior recovery and posts separate recovery income without recreating the waived
receivable. A prepaid refund is bounded by unearned prepaid cash and explicit
refund terms. Cancellation removes only future earning capacity; existing prepaid
obligations survive until actual reviewed refund/earning. There is no default LGD,
coupon-recovery fraction, interest waiver, allowance or invented terminal settlement.

Every action retains an ordered balanced intent and its exact next gross snapshot.
Reporting snapshots include actions on the report date; complete-tail closing and
all subsequent cash/recovery events remain separately retained. Unsettled closing
claims stay visible. `full_source_tail_replayed` means every supplied event was
processed; it does not certify that an omitted legal event never exists.
Opening/secondary fee acquisition checkpoints remain unsupported.

## Explicit earned-fee refunds (0.29.18)

A fee can explicitly select `earned_refunds_allowed=true`; omission/false retains
historical refusal. A `refund_earned` event requires a default, exercise, maturity
or other-reviewed reason and a separate review identity. It is bounded by
`min(s*(E-F-W), s*K)`: both remaining recognized earning and actual collected
service cash must exist. Prepaid cash and waived-claim recoveries cannot enlarge
that right. The signed amount increases F and decreases K, posting cash -amount
and fee income +amount. Gross earning E, service capacity, loan cost/EIR and
recovery claims remain unchanged. An earned refund never grants a right to earn
or collect that amount again. Prepaid refunds remain a separate action and policy.

For a receipt-positive fee5 earned and paid, refund2 retains E5/F2/K3 and zero
unsettled rights; net income and cash are3. A fee5 prepaid with only2 earned can
refund1 of earned service and3 of prepaid cash independently, leaving net income
and cash1. Negative fee entitlements mirror the signed postings. No tolerance
grants extra collected or earned rights.

The additive permission and `refunded_earned` state omit false/zero values from
typed output. Historical omitted requests retain their source, snapshot and
admission layouts. New fields/actions require matching binaries. Both standalone
and native loan/fee journal routes retain independent raw and persisted replay.

## Admission and transport

Input is bounded to2MiB, output to32MiB,2048 fees,8192 events and36600 days.
A structural retained bound charges2048 bytes per fee and4096 per event before
calculation. Identity strings are nonempty/control-free and at most128 UTF8 bytes.
Amounts and derived balances are finite within the existing1e15 currency-unit cap.
No tolerance grants additional legal entitlement; journal balance retains the
existing1e-9/64-epsilon check without plugs. Component caps are not aggregate RSS.

`analytics.model_contracts.account_nonintegral_fees` transports the contract to
standalone Rust. `/models/runs` exposes the same explicit analysis through the
existing durable worker, lossless paged Parquet sources and final SQL authority
guard. The original HTTP/queued deadline spans source verification, persistence
and final publication. Native cooperative checks and the process watchdog govern
execution; integer native timeout identity is retained without a float roundtrip.
Independent Python source and ordered posting replay is a publication gate, before
and after persistence. Rehashed money/date/review/evidence changes cannot publish.

Monetary JSON fields accept finite integer or float inputs and use the native
f64 DTO representation. Independent replay normalizes only those money fields;
actual-date integer clocks, booleans, identities and reviews retain exact scalar
categories. Original queued raw JSON and its execution hash remain retained.

## Required follow-through

The result explicitly reports `native_credit_event_binding`, `mixed_ledger_adoption`,
`pricing_unit_ftp_adoption`, `empirical_validation` and `accounting_approval` as false;
`loan_carrying_value_changed` is false. Bind this sealed fee owner to native credit/
exercise/tranche events, integrate separate fee GL/daily reconciliation and closing
capital, and price/integrate fee cash once in graph, original acquisition allocation,
aged units and funding/FTP. Fee rights must never be implicitly included in the
loan quote/EIR anchor. Saved-book/cohort/mixed strategy adoption, reviewed/observed
histories and full60000-position joint acceptance remain required.

Historical protocols and ABI8 are unchanged. Additive execution requires matching
product, ledger and dependent decision/workflow binaries; restart deployed workers
and explicitly rebuild selected libraries/sessions before adoption. This local work
performs no deployment and establishes no accounting-standard compliance.


The separate local0.29.17 [native loan/fee journal](commercial-fee-journal.md)
binds original service fees to native default/prepayment/reviewed maturity and
posts them in one existing Sim. The standalone component flags above remain
unchanged; exercise/candidate/holding, Pricing/units, acquisition allocation and
capital/FTP fee adoption remain required.
