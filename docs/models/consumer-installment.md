# Native auto and personal installment paths

`portfolio_model_core::consumer_installment::project` accepts strict
`consumer-installment-1` inputs and returns a deterministic, versioned account
path. This is the contractual and supplied-policy calculation stage for issues
#17 and #18. It is not a calibrated consumer behavior model, an accounting-policy
approval, or evidence that cohort, public graph, unit-library, and journal
publication adoption are complete.

## Identity and source contract

Every request carries position, original loan, cohort, currency, source revision,
model version, assumption identity, and run identity. Currency is a label; there
is no conversion. A caller explicitly supplies a **physical** or **pricing**
measure. Identical cashflow mechanics can be run under either supplied policy
path, but the engine never converts a physical path into a risk-neutral one.
Pricing paths must not post physical GL. A contractual-only label is rejected
because this schema admits borrower and credit actions.

Original principal and original payment remain in the result. Current opening
principal cannot exceed original principal. Recognized accrued interest,
unrecognized contract interest, and unpaid fees are distinct supplied opening
balances. The model does not infer earlier payments, previous nonaccrual, or
origination fee economics from a current tape balance.

`payment_dates` are exact, already selected contractual ordinals. They are
strictly increasing after valuation and contained in the full horizon. The model
does not infer payment frequency, infer a holiday calendar, shift dates, or treat
months as 30 days. Users must retain the contract's original/adjusted date policy
in source inputs. All actions and recovery dates must fit this full horizon;
short reporting horizons belong downstream. Recovery tails are never truncated.

## Payment and interest calculation

This version supports fixed note-rate daily **simple** interest on the remaining
principal, with explicit ACT/360 or ACT/365F. For a half-open interval `[s,e)`:

`interest = opening interval principal * note_rate * (e-s) / denominator`.

The denominator remains 365 in a leap year for ACT/365F. Shared Gregorian/date
and accrual helpers perform date validation. There is no interest capitalization
into principal. Fees and accrued interest do not earn interest. Note rate and
disclosed APR are separate fields. Disclosed APR is retained lineage and does
not replace the note rate or compute a fresh disclosure APR from fees. The
[CFPB distinction between loan interest rate and APR](https://www.consumerfinance.gov/ask-cfpb/what-is-the-difference-between-a-loan-interest-rate-and-the-apr-en-733/)
supports keeping those concepts separate.

Each modeled date first accrues from the previous date using the previous
principal and credit state. It then applies at most one explicit action. An
explicit payment or payoff replaces the automatic payment on that date. Other
actions take effect before the automatic payment, when one is due. Only
performing accounts make automatic scheduled payments. Delinquent/nonaccrual
accounts require explicit collections or a supplied cure state.

The supported allocation order is unpaid fees, recognized interest,
unrecognized contract interest, then principal. It is an explicit versioned
policy, not a claim that all lenders use this order. Agreements with a different
waterfall require a different supported policy. The
[CFPB repossession bulletin](https://www.consumerfinance.gov/compliance/supervisory-guidance/cfpb-bulletin-2022-04-mitigating-harm-from-repossession-of-automobiles/)
emphasizes that actual payment application must follow the disclosed agreement.

Before the final due date, automatic cash is the current payment capped by total
payoff. At the final due date a performing account pays the full remaining
obligation: this includes any final adjustment or catch-up balloon caused by
earlier low/missed payments. It does not silently retain the original installment
when that would leave an unpaid balance. Delinquent accounts are not fabricated
as paid at maturity; unpaid principal and interest persist to closing. An
explicit partial final payment can also leave a supplied unresolved balance.

`ChangePayment` changes subsequent scheduled payments while retaining the
original payment. `Fee` adds supplied unpaid fees. `Payment` permits partial or
extra collections without recasting the original contract. `Payoff` pays accrued
obligations and all remaining principal, adds an explicitly supplied fraction of
principal as a prepayment penalty, and exits the account. Automatic or ordinary
extra principal payments do not receive that payoff penalty. If an agreement
penalizes all extra principal, this version's policy is unsuitable.

Precomputed-interest loans and unearned-interest refund methods are unsupported
and unknown fields fail, rather than being routed through this model. The
[CFPB simple-versus-precomputed explanation](https://www.consumerfinance.gov/ask-cfpb/whats-the-difference-between-a-simple-interest-rate-and-precomputed-interest-on-an-auto-loan-en-841/)
describes why early payment behavior differs between these contracts.

## Credit, nonaccrual, repossession, and collection

Performing, delinquent, and nonaccrual are live states. Supplied state changes can
represent delinquency and cure. Default and payoff require economic actions;
setting a terminal state directly is rejected. The engine does not fit cure,
default, or prepayment probabilities from FICO, DTI, vintage, or cohort labels.
These inputs need upstream calibrated or explicitly assumed scenarios. A path
cannot both default and pay off on the same date, and an action after terminal
exit fails. Original principal equals principal collected **before default**,
closing principal, plus defaulted principal. Recovery cash is tracked separately
and never counts a second time as pre-default principal amortization.

Nonaccrual suppresses accrual-basis recognition. The caller explicitly chooses
whether simple contract interest continues to accumulate as unrecognized debt.
Cash collected against that debt produces a cash-basis recognition record at
the collection date, with equal start/end ordinals. Cure does not automatically
recognize the uncollected nonaccrual balance or apply an allowance/accounting
reversal. Those decisions belong the reviewed credit accounting policy.

Auto inputs require new/used status, vintage, original LTV, channel, as-of
collateral value, annual depreciation, sale haircut, repossession cost, and an
integer recovery delay. Vintage and LTV are supplied domains, not fitted causal
coefficients. The recovery sale value is:

`gross = collateral_value_as_of * (1-depreciation)^((sale_date-as_of)/365) * (1-sale_haircut)`.

Depreciation runs through the **sale date**, including repossession delay. The
annual multiplicative decay with ACT/365 elapsed time is a disclosed supplied
assumption, not a vehicle-price forecast. Gross collateral proceeds are not
capped before deducting costs. Thus surplus collateral can cover repossession
cost before principal is capped, avoiding a false shortfall on an oversecured
loan. Principal recovery is `min(principal, max(gross-cost,0))`. Surplus belongs
outside the lender's principal recovery and is not invented as lender fee
income. Actual lawful repossession, redemption, notices and deficiency recovery
are jurisdiction/contract inputs, not automatic triggers. The
[CFPB repossession discussion](https://www.consumerfinance.gov/ask-cfpb/what-happens-if-my-car-is-repossessed-en-865/)
explains the distinction between vehicle proceeds, expenses, and remaining debt.

Personal inputs require vintage, DTI and its definition, channel, a supplied
principal collection fraction, collection cost, and recovery delay. There is no
collateral value or mortgage behavior proxy:

`gross = defaulted principal * collection_fraction`.

For both products, recovery net of cost is a principal-only policy. Default
recognized interest, unrecognized contract interest, and fees are reported
separately and are not silently included in principal LGD. Costs beyond gross
proceeds produce a negative fee/expense cash component at recovery, so they do
not vanish under a zero-recovery floor. Principal loss remains principal minus
principal recovery; the excess expense is additional economic cost. Multiple
collections, deficiency interest/fees, collateral liquidation uncertainty,
collection curves and recovery receivable recognition require explicit further
policies.

## Outputs, bounds, and independent evidence

The result retains dated lender-perspective cashflows, exact recognition
intervals, state/balance points, original due dates and payment lineage, closing
rights, and default/recovery records. Default principal removal is **not cash**.
Recognition is not another settlement. The graph/ledger adopter must retain
separate origins when allocating collections and defaults, map signed expense
cash, reject pricing GL, and prevent a second legacy credit/prepay overlay.
Allowances, effective-yield fee amortization, tax and regulatory classification
remain separate accounting functions.

Admission is at most 4096 total contractual dates/actions, 36600 horizon days,
bounded 256-byte nonblank identities, nonnegative finite amounts and aggregate
obligations/cash within `1e15`, note/APR rates in `[0,2]`, fractions in `[0,1]`,
and supplied DTI/LTV domain checks. Dates use shared ordinal validation. Requests
deny unknown fields, including tagged empty actions. Duplicate action dates,
payments larger than payoff, terminal-state shortcuts and truncated recovery
tails fail. There is no financial Python callback or silent fallback.

`tests/consumer_installment.rs` supplies 17 independent analytical tests,
including zero-rate principal conservation, leap-day actual interest, separate
APR, penalty payoff, delinquency/cure, cash-basis nonaccrual, default/accrual
separation, unsecured and auto cost/recovery, collateral surplus, depreciation
through sale, preserved tails, original-payment changes, monetary/domain
failures, and a direct payment/rate grid. Identical supplied paths serialize
identically. These synthetic/analytical checks are separate from empirical
validation. Auto and personal calibration need chronological observed borrower
histories, validated variable definitions, training-only fit, account/cohort
stress error reports, and held-out evidence; no such claim follows from these
fixtures or from cohort grouping.
