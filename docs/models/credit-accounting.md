# Explicit physical credit-accounting policy

`portfolio_ledger_native::credit_accounting::evaluate` applies one immutable
`credit-accounting-1` day step and returns balanced **journal intents**, closing
state and complete recovery claims. Adapters own account mapping, persistence,
independent journal replay and publication. This integration foundation does not
replace the historical daily credit engine or close the shared/product credit
issues.

The economic transition operator and accounting decisions remain separate.
`credit_operator` transports physical state probability, principal, undrawn
commitment, recognized interest rights and signed basis. The coordinator supplies
actual default-by-origin monetary deltas from its default-origin slots. This layer
does not rerun transitions, apply another survival ratio, infer a coupon allocation,
or convert a default CCF into ordinary scheduled commitment utilization.

## Scope and required identities

The request must specify `measure="physical"`, `monetary_unit="currency_units"`,
an uppercase currency code, position identity, today's `recognition_ordinal` and a
bounded `reporting_end_ordinal`. Pricing and contractual measures are rejected.
The supplied schedule represents modeled physical expected recovery cash; it is
not proof that an observed borrower actually paid. Recovery intents are explicitly
tagged `modeled_physical_recovery_cash`.

Opening live state has three origins in order performing, watch, delinquent:

```
live[3]: {principal, undrawn, accrued_interest, book_adjustment}
allowance: positive contra-asset balance
claims: complete conserved default/recovery states
```

Principal and undrawn amounts are nonnegative actual currency units. Recognized
`accrued_interest` is signed: a positive receivable or a negative deferred/prepaid
right supplied by the actual-date accrual owner. It is not reconstructed from a
whole contractual coupon. `book_adjustment` is signed premium/cost positive or
discount/deferred net fee negative. Principal plus basis cannot be negative.
Allowance is a positive balance; its corresponding debit-positive GL balance is
negative. No opening amount is silently recalculated.

The accounting policy includes immutable `{id,revision}` and explicit choices:

| Policy | Choices and effect |
|---|---|
| Accrued right | Reverse against interest income at default, or transfer until a separately reviewed reversal |
| Signed basis | Transfer until reviewed chargeoff, or recognize in earnings at default |
| Insufficient allowance | Reject a reviewed chargeoff, or recognize its uncovered amount as expense at that reviewed date |
| Cash above remaining booked default principal | Credit allowance, or recognize recovery income |

Claims retain the **full policy configuration plus identity**. Resuming a claim
under a changed treatment, even with a reused policy ID, fails. Configuration
choices are supplied research accounting policies; support for a choice is not a
claim that it is appropriate under a particular accounting standard.

## Economic default and cash-funded default utilization

Each `DefaultInstruction` supplies unique `claim_id`, economic-source identity,
reviewed recovery-schedule identity and three origin deltas:

```
by_origin[3]: {principal, undrawn, contingent_draw,
              accrued_interest, book_adjustment}
economic_principal_loss
complete_recovery_tail: [{payment_ordinal, amount}, ...]
```

Every delta must fit its supplied opening origin balance/right. Signed accrual and
basis allocations must have the same sign as, and not exceed the magnitude of,
their opening rights. Contingent draw must not exceed the explicitly defaulted
undrawn commitment. The entire defaulted undrawn amount leaves live availability;
the unused portion is terminated without a GL asset posting.

Let `D` be defaulted existing principal, `C` contingent draw and `G=D+C` gross
default EAD. Economic principal loss `L` must lie in `[0,G]`; all nominal recovery
tail points must sum to `G-L`. This loss definition excludes accrued interest and
premium/discount. No second LGD loss is applied to those balances.

Contingent utilization emits a cash-funded intent **before** default transfer:

```
Dr LivePrincipal C; Cr Cash C
Dr DefaultPrincipal G; Cr LivePrincipal G
```

The combined live-principal movement is `-D`, not `-G`; the new drawn exposure
enters the default claim after cash funding. CCF/default utilization is therefore
not an unrecorded EAD increase or an ordinary scheduled draw.

The default itself transfers gross principal to a separate default asset. Expected
loss does not automatically reduce that asset, debit allowance, expense a loss or
create an expected-recovery receivable. Accrued and basis rights follow their
explicit policies independently.

## Reviewed allowance and chargeoff timing

`ReviewedChargeoff` supplies review identity, claim identity, today's exact
recognition date, principal amount, signed basis amount and signed accrued-interest
reversal. Future reviews cannot be recognized today. A principal chargeoff is a
supplied accounting decision; it need not equal modeled expected principal loss.

With transferred basis, the reviewed net principal/basis chargeoff is:

```
W = reviewed_principal + reviewed_signed_basis
covered = min(W, available_allowance)
uncovered = W - covered
```

`W` must be nonnegative. The intent credits default principal and signed basis,
debits allowance for the covered amount and, only under the explicit shortfall
policy, debits credit-loss expense for the uncovered amount. Removing a negative
discount/deferred fee reduces net chargeoff; removing a positive premium increases
it. Reviewed accrued-interest reversal separately debits interest income and
credits its default accrued-right account, including the appropriate reversed
sign for a negative recognized right. It is not included in principal LGD or the
principal allowance calculation.

`ReviewedAllowance` has a review identity, today's date, explicit `target_amount`
and `before_credit` or `after_credit` phase. The target changes allowance through
credit-loss expense/release. A closing review cannot retroactively fund an earlier
chargeoff under the reject-shortfall policy. No allowance target is inferred from
PD, LGD, expected economic loss, remaining balance or a scenario.

Daily event order is deterministic:

1. Optional reviewed allowance target before credit.
2. Explicit cash-funded contingent draws, economic default transfer and independent
   accrual/basis policy intents, in supplied default order.
3. Today's reviewed chargeoffs/reversals, in supplied review order.
4. Due cash from supplied reviewed modeled-physical recovery schedules.
5. Optional reviewed allowance target after credit.

## Recovery claims and horizon treatment

An opening claim retains initial gross principal, principal-only economic loss,
initial accrual/basis, outstanding book balances, cumulative reviewed removals,
cumulative recovery cash and the full uncollected nominal recovery tail. Validation
independently enforces:

```
initial_principal = outstanding_principal + principal_charged_off
                    + recovery_applied_to_principal
initial_accrued = outstanding_accrued + accrued_reversed
initial_basis = outstanding_basis + basis_recognized
initial_principal - economic_principal_loss = recovery_cash_collected
                                             + remaining_recovery_tail
```

Due recovery cash first reduces remaining booked default principal. Excess cash
then follows the explicit allowance/income choice. No previously removed principal
is rebooked simply because a future recovery is expected. Fully charged-off loans
retain their economic recovery tails even when booked principal is zero.

If reviewed chargeoff differs from expected loss, the accounting principal can
remain after the last modeled recovery; it remains visible as an unresolved booked
default asset requiring further review. Basis/accrued balances can similarly remain
until their supplied recognition decisions. Nothing is silently cleared to make
the recovery schedule appear complete.

Same-day zero-lag recoveries execute after today's chargeoffs. Points after the
reporting end stay in `closing.claims[].remaining_recoveries` and contribute to
`recovery_tail_after_reporting`. No horizon truncation or terminal-date compression
is permitted. Passing a day that leaves unprocessed past-due schedule points in
opening claims fails rather than silently dropping cash. Resuming with today's
due points is supported; replay ownership must prevent applying the same state
twice.

## Balanced intents and validation

Posting amounts are debit-positive. The typed `AccountCode` also exposes stable
`numeric_code()` values for generic adapter mapping: cash, live/default principal,
live/default accrued right, live/default basis, allowance, interest income,
credit-loss expense, basis-recognition expense and recovery income. Adapters must
not treat intent account codes as a hardcoded production chart of accounts.

Every intent retains policy/source identity and position/claim identity. Inputs
are cloned; failure returns no result and never mutates caller opening state.
Final claims are checked against the original economic anchors. Numerical guards
allow f64 rounding only; existing engine/ledger tolerances are unchanged.

Standalone admission is at most 62000 opening-plus-new claims, 256 new default
events, 4096 reviewed chargeoffs and 32 points per complete recovery tail. Dates and
tails admit at most 36600 days; monetary components and reported totals admit
finite magnitudes up to `1e15` actual currency units. Identity fields admit at most
1024 control-free bytes each and 32 MiB aggregate metadata. These component limits
do not assert an aggregate RSS cap or regulatory exposure/chargeoff limits.

Ten independent native fixtures cover covered/uncovered allowance, rejected and
explicitly funded shortfalls, deferred reviews, full writeoff then recovery,
contingent funding before default, accrued reversal, signed premium/discount,
negative prepaid rights, closing allowance targets, same/final-day recoveries,
zero-credit exact state identity and invalid measures/units/tails/rights/policies.
They check hand-computed balanced postings and closing states. Run:

```
cargo test --manifest-path packages/portfolio-ledger-native/Cargo.toml --test credit_accounting
```

## Primary accounting guidance and limits

The [OCC Bank Accounting Advisory Series, August 2026](https://www.occ.treas.gov/publications-and-resources/publications/banker-education/files/pub-bank-accounting-advisory-series.pdf)
provides relevant distinctions: topic 2B separates accrued-interest reversal from
principal chargeoff decisions; topic 2D question 11 discusses deferred fees and
cost basis; topic 2F treats recovery recognition separately from hoped-for
collections and describes recoveries through ACL after chargeoff. These examples
informed the explicit separation and conservative default policy boundaries.

The [OCC allowance resources](https://www.occ.treas.gov/topics/supervision-and-examination/bank-operations/accounting/allowance-for-credit-losses/index-allowances-for-credit-losses.html)
identify allowance as a valuation account and link its measurement, documentation
and review guidance. This module executes supplied reviewed amounts; it does not
implement CECL, IFRS 9, regulatory chargeoff rules, collateral-dependent measurement,
off-balance allowance, return-to-accrual rules, tax, OREO, acquired-loan accounting
or observed borrower settlement verification.

Full native integration still requires coupled transition/accrual ownership,
product/default/prepayment replay, adapter state mapping, independent persisted
journal verification, production immutable publication, fixed base OAS/shared CRN
for pricing and the remaining issue acceptance criteria.
