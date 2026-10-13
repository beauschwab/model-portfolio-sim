# Native revolving credit-card statement state

`portfolio_model_core::consumer_card::simulate` accepts strict `consumer-card-1`
requests. The model is distinct from installment amortization: purchase, cash
advance and promotional principal can grow with supplied draws and shrink with
supplied payments while an open commitment remains. It is the state mechanics
stage of #19; it does not by itself complete public product/graph/cohort,
unit-library, allowance, daily journal, or empirical validation acceptance.

## Source, state, and measure

The shared consumer identity retains account/original loan, position, cohort,
source revision, model/assumption/run identity and currency. Physical and pricing
policies are explicit; there is no inferred measure conversion or synthetic
reverse stress. Pricing state paths must not post physical GL. Spend/payment
behavior and credit transitions are supplied actions. An assumption identity
does not turn synthetic actions into fitted behavior.

All openings are explicit: three-class principal, billed interest, earned
unbilled interest, unrecognized billed interest, fees, pending statement rights,
minimum remaining and optional due date, plus contingent purchase grace
interest. Pending statement balances cannot exceed current billed debt. Missing
opening due dates with pending balances fail. Opening purchase principal is a
single aggregate lot: its supplied contingent interest is allocated
proportionally when that lot is paid. Accounts whose opening transaction lots
need distinct dates/rates require upstream lot-level state rather than claiming
this aggregate opening is exact for them.

Each new draw or fee creates a stable lot. Equal-day actions execute in input
order; draw-then-payment differs from payment-then-draw. Opening/draw lots are
FIFO at equal APR/class. The model does not infer spend from utilization or
infer borrower transactor/revolver type from a label. A transactor appears as
supplied full statement payments; a revolver carries unpaid principal.

## Daily accrual, statement posting, and payment

Dates are actual Gregorian ordinals. At each date the prior day's interest is
accrued before current transactions. A rate change on date `d` applies to
`[d,d+1)` rather than repricing the preceding day. APR inputs are periodic
accrual rates expressed annually, using explicit ACT/365F or ACT/360:

`daily_interest = (principal + optional previously billed interest) * APR / denominator`.

The `compound_billed_interest` switch is explicit. Newly earned **unbilled**
interest does not compound before posting, and fees do not earn interest. For
non-grace balances, daily interest is recognized as earned and retained as
unbilled until statement close. At statement close, unbilled interest becomes
billed without being recognized twice. Payments settle billed charges, not
earned unbilled charges that have not yet posted. These conventions must match
the selected issuer agreement; they are not universal card rules. The
[CFPB interest calculation explanation](https://www.consumerfinance.gov/ask-cfpb/how-does-my-credit-card-company-calculate-the-amount-of-interest-i-owe-en-51/)
distinguishes rates by transaction type and purchase grace.

The caller supplies statement and due ordinals. Each statement must precede its
due date; the next statement follows the previous due date. This version admits
ordered, nonoverlapping statement/due windows and rejects timelines it cannot
represent. It does not create a mailing/notice date or independently establish
regulatory compliance. In a date's order, transactions occur before due checks
and statement snapshots. Thus a payment on a due date is timely; a payment on
statement date occurs before that statement creates the new minimum.

At statement close, the configured minimum is:

`min(statement debt, max(floor, principal_fraction * principal + optional billed interest + optional fees))`.

The minimum remaining is reduced by supplied payments. The caller supplies a
permutation of the three classes for allocating the minimum portion. The excess
portion uses descending APR on the **payment date**, with stable FIFO ties.
Within each selected lot, payments settle fees, then billed interest, then
principal. This lot-based waterfall is a declared supported issuer policy;
different policies require a versioned extension. The minimum/excess
distinction and payment-date rate selection are consistent with the supported
case in [Regulation Z §1026.53 and its official interpretation](https://www.consumerfinance.gov/rules-policy/regulations/1026/53/).
Deferred-interest exceptions and disputed/protected balances are not silently
modeled as ordinary APR balances.

## Purchase grace and promotions

Eligible purchase interest accumulates as **contingent shadow interest**, not
earned NII or billed debt. Cash advances and promotional balances have no
purchase grace. Principal paid while eligible proportionately waives that
lot's accumulated shadow interest. Consequently, full payment before the due
check waives the interest, and paid principal is protected even when a statement
is paid only partly. Lot-specific transaction dates prevent an early purchase
payment from incorrectly waiving a later purchase's interest history.

If pending statement rights remain at due, purchase grace is lost. The
remaining shadow amount on still-unpaid eligible purchase lots becomes billed
interest and is recognized **on that determination date**. The version supports
the supplied policy in which loss also applies to other outstanding eligible
purchase lots; issuer terms must permit that policy. Paid principal's shadow
amount has already been waived. There is no two-cycle back-billing. The
[paid-portion restrictions in §1026.54](https://www.consumerfinance.gov/rules-policy/regulations/1026/54/)
are why the engine tracks shadow balances and waiver separately rather than
applying a blanket charge to original statement principal.

`regain_grace_after_full_statement_payment` explicitly determines whether a fully
paid due check restores eligibility for subsequent new purchase lots. Existing
non-grace lots are not relabeled retrospectively. Before a due check, closing
contingent interest remains visible in the result; a short analytical horizon
does not silently turn it into interest income.

Promotions are genuine temporary APRs. Before the explicit expiry ordinal,
promotional APR applies; at expiry and thereafter, post-promotion APR applies
to future daily accrual. Zero APR can therefore degenerate to no promo interest.
There is no deferred-interest retroactive catch-up. Unknown deferred-interest
fields fail. Supported supplied APR resets affect class rates going forward;
the model does not infer an index fixing or establish whether a rate change is
legally permitted. That permission and notice policy are source inputs.

## Limits, credit, recovery, and reconciliation

A draw requires a performing account and available limit after current
principal, posted finance charges and fees. Unbilled/contingent interest is
retained as rights but does not reduce this version's available credit before
posting. Limit reduction may leave existing debt above the line without
inventing a payment or negative draw. Utilization is billed debt/limit and can
exceed one after reduction. Zero limit returns unavailable utilization (`None`),
with zero available undrawn. The model does not create a CCF/default draw from
unused commitment. Upstream jointly modeled behavior must supply such a draw
before default, within the explicit limit policy, or introduce a reviewed
contingent-default-draw policy.

Delinquency/cure/nonaccrual are supplied live-state changes. Nonaccrual suspends
new daily contract interest in this version. If previously contingent purchase
interest becomes billed while nonaccrual, it remains unrecognized until cash
collection. Previously recognized accrual is not automatically reversed on
entry: that is the separate credit accounting policy's responsibility.

Default removes all principal once, closes the line, stops subsequent accrual,
and separately reports recognized accrued interest, unrecognized contract
interest, fees, and principal loss. Recovery is a supplied principal-only
collection fraction at an explicit lag, less supplied collection cost.
Costs exceeding gross collections produce negative expense cash, not a dropped
cost. An action after default, including a same-day later payment, fails.
Contingent grace interest not yet billed is extinguished at default under this
version's principal-only recovery policy; it is not fabricated as earned NII.
Agreements requiring collection of those contingent claims need a further
explicit default/grace policy. Mere zero balance is an open card account, not
an installment payoff exit.

The conservation identity is:

`opening principal + funded draws = principal payments + defaulted principal + closing principal`.

Recovery cash is outside that identity and never causes a second live-principal
reduction. Cash draws are negative principal from the lender's perspective;
payments/recoveries are positive. Fees, cash interest, and recognition are
distinct outputs. A downstream ledger must own draws/default/collections once,
preserve the account/run/lot lineage, reject legacy duplicate overlays, use
recognized-only accrued interest for GL, and retain full recovery and pending
statement/grace tails. Allowances, economic-versus-accounting recovery claims,
fee effective-yield amortization, capital/FTP and tax remain separate policies.

## Admission and validation evidence

There are at most 4096 supplied actions plus cycles, 36600 horizon days, and
250000 admitted daily lot/cycle work units. This bound includes the potential
lot/day recognition output, not merely input row count. Opening/closing total
rights, cash/principal totals and component amounts are finite and bounded by
`1e15`; rates lie in `[0,2]` and fractions in `[0,1]`. Each shared identity is
nonblank and at most 256 bytes. Unknown fields and terminal-state shortcuts
fail. Recovery dates must lie inside the full contract horizon; no tail is
clipped. A caller's transport/process budget remains additional to these
per-model bounds.

`tests/consumer_card.rs` provides 19 independent analytical fixtures. They
cover transactor/revolver limits, actual leap dates, issuer minimum versus
highest-APR excess, paid-portion grace, FIFO dated lots, promo expiry, statement
posting, billed-only compounding, pending opening rights, nonaccrual collection,
draw/limit/fee/default conservation, recovery costs/tails, zero-change/measure
identity, duplicate/unknown convention rejection and work/money admission.
These are controlled synthetic mechanics tests, not calibration evidence.
Chronological observed account panels, training-only fitting, forward holdouts,
payment/spend/utilization/default diagnostics, account-versus-cohort error
reports and stressed PV/DV01/NII/runoff validation remain required before any
empirical model claim or full #19 acceptance.
