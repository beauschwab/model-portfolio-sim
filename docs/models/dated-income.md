# Actual-date effective-yield income

`dated-income-1` is an explicit amortized-cost analysis contract for lending
receipts. `portfolio-model-core::dated_income::evaluate` owns the financial
arithmetic; `analytics.model_contracts.evaluate_dated_income` transports it.
It does not automatically replace monthly accounting or the daily credit route.

Inputs retain contract, original-yield anchor, reviewed policy and cash-source
identities; physical/contractual measure, actual currency units and complete
contract dates are mandatory. Opening principal, signed accrued interest, gross
carrying value, original annual effective yield and reviewed current yield are
supplied separately. A seasoned loan is not refit from its remaining term.

For an interval of `d` actual days and current annual effective yield `y`, native
earned effective interest is `B * expm1(log1p(y) * d / 365)`, where `B` is opening
gross carrying value. Contractual interest earned is a separate supplied source
amount for this interval. Interest cash settles the accrued right. Principal cash
reduces contractual principal; integral fee cash reduces gross carrying value;
nonintegral fee cash is separately recognized as fee income. Basis amortization
is effective interest minus contractual interest earned minus integral fee cash.
Closing book adjustment is gross carrying value minus principal minus accrued
interest. The result preserves cash, effective interest, contractual interest,
basis amortization and reviewed modification gain/loss as separate values.

Settlement follows recognition, then an explicitly reviewed yield change:

- `prospective` supplies the reviewed new annual effective rate. It applies to
  subsequent intervals and does not reprice today's recognition.
- `catchup_original` supplies complete remaining revised cash rights. Their dated
  amounts must reconcile to the same admitted remaining source cash schedule.
  Native arithmetic discounts them at the immutable original yield, recognizes
  the carrying-value change separately, and retains that yield thereafter.

The model does not decide whether a change is a modification, derecognition,
floating-rate reset or credit impairment. The caller supplies that reviewed
economic policy. It does not generate an allowance, infer chargeoffs, introduce
an OAS into income, produce CECL/IFRS9 compliance, or convert pricing cashflows
into physical outcomes. Draws, borrowing directions and net-of-allowance interest
bases require separate explicit ownership and are rejected/not provided here.

Future cash is not a current recognition event. All supplied dated events are
returned, including tails, and residual principal/accrued/carrying values are
explicit. Missing terminal cash is not replaced by an invented maturity payment.
Event IDs are unique; dates are ordered (same-date phases remain in source order)
within the declared full tail. The model admits at most 4,096 events, 36,600 days,
32 MiB metadata, finite currency amounts up to `1e15`, and yields `-0.99..10`.
Exact terminal repayment can cancel machine-roundoff residuals only when both
principal and accrued rights have retired. These bounds are component admission,
not an aggregate RSS guarantee.

Independent fixtures cover a 100 principal/10 coupon one-year loan at 10% yield,
actual 29-day stub compounding, integral/nonintegral fees, historical anchors,
reviewed prospective and catchup changes, negative yields and invalid sources.
These analytical fixtures do not establish empirical predictive accuracy.

The dated [IFRS9 text](https://www.ifrs.org/content/dam/ifrs/publications/pdf-standards/english/2021/issued/part-a/ifrs-9-financial-instruments.pdf)
supports separating effective interest, contractual cash and modification
recognition as modeling concepts. The current [IASB amortised-cost project](https://www.ifrs.org/projects/work-plan/amortised-cost-measurement/)
contains tentative proposals as of this research pass; they are not treated as
effective requirements. A bank's applicable reviewed accounting policies are
required for production use. Full issue #22 still requires integrated yield/reset,
modification, fee, credit, reporting and independently verified journal adoption.
