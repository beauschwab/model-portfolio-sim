# Fixed-rate contractual amortization

This is the contractual schedule stage of corporate debt and commercial-loan
enhancement issue [#15](https://github.com/beauschwab/model-portfolio-sim/issues/15).
It introduces explicit `amort_type="level_payment"` in the Rust term-deck request.
The existing `"annuity"` value retains its historical **equal-principal** meaning.
Saved instruments are never reinterpreted or automatically converted.

## Contract and scope

`level_payment` produces a constant scheduled total payment of principal plus
fixed-coupon interest, per unit of opening principal. The contractual frequency,
business-day adjustment and day count determine its actual accrual fractions.
The first stub receives the same total payment as the remaining periods; its
interest and principal split reflects its shorter accrual. An input must describe
the remaining fully amortizing contract starting at the valuation date. The
calculated payment is not a reconstruction of a seasoned loan's original payment.
If that payment must be preserved, a future explicit contractual-payment and
remaining-balance input is required.

The new value is supported for fixed-rate corporate/commercial instruments only.
Floating-rate contracts are rejected: a schedule prepared at valuation cannot
predict path-dependent reset rates or their payment-reset conventions. CD
contracts and simultaneous sinking schedules are also rejected. Corporate call
and put rules continue to operate on the resulting contractual balance, with their
existing threshold-exercise assumptions unchanged.

The actual adjusted payment dates must fit within the supplied market grid.
If any exceeds `months/12 - 1e-9` years from valuation, construction fails and asks
for a longer grid. This is deliberate for the new contract value: compressing
later payments into the terminal grid would change the contractual economics.
Historical bullet, equal-principal and sinking contracts keep their existing
terminal-grid behavior to preserve saved results.

## Financial equations

Let opening principal be `B_0=1`, annual fixed coupon be `c`, and actual accrual
fraction of payment period `i` be `tau_i`. Interest follows the existing term
kernel's simple-accrual convention:

```
g_i = 1 + c * tau_i
interest_i = c * tau_i * B_(i-1)
B_i = g_i * B_(i-1) - P
principal_i = B_(i-1) - B_i
```

For full payoff at the last payment, the level amount is:

```
D_i = 1 / product(g_k, k=1..i)
P = 1 / sum(D_i, i=1..n)
```

For equal `tau`, this reduces to `P=r/[1-(1+r)^(-n)]`, with `r=c*tau`.
At zero coupon it reduces continuously to `P=1/n`; interest is zero and
principal is equal across periods, including stubs. A negative coupon is permitted
when **every** `g_i > 0`; the coupon itself is not silently floored at zero.

The implementation calculates remaining payment annuities backward:

```
V_n = 0
V_(i-1) = (1 + V_i) / g_i
P = 1 / V_0
B_i = V_i / V_0
```

Logarithmic annuity values avoid overflow in a direct compounded-factor product
and avoid forward error accumulation on long amortizing contracts. `ln_1p` retains
accuracy near zero coupon. Principal is the difference of successive normalized
balances. The last remaining annuity is zero, so final principal pays exactly the
remaining numerical balance. No tolerance or fast-math relaxation is introduced.

Positive, finite accrual fractions and representable positive payment amounts are
required. The public shared helper `portfolio_model_core::amortization::
level_payment_principal(coupon, taus)` admits 1..=4096 periods independently of the
term-deck caller. It returns normalized principal repayments and owns validation
of the coupon, accrual factors and numerical domain. Extreme coupon/stub
combinations that imply increasing principal are
rejected as unsupported negative amortization; the existing principal kernel is
not a negative-amortization contract engine. A 64-machine-epsilon comparison
handles rounding at effectively zero principal without admitting an economically
increasing balance.

## Model interpretation and remaining assumptions

The CFPB describes a typical fixed-rate amortizing loan as a constant combined
principal-and-interest payment whose principal share increases over the term,
and a fully amortizing schedule as paying the loan off at the end of its term.
These primary sources inform the payment/payoff invariants:

- [How does paying down a mortgage work?](https://www.consumerfinance.gov/ask-cfpb/how-does-paying-down-a-mortgage-work-en-1943/)
- [How do mortgage lenders calculate monthly payments?](https://www.consumerfinance.gov/ask-cfpb/how-do-mortgage-lenders-calculate-monthly-payments-en-1965/)

The generalized irregular-period formula above is derived from the balance
recurrence, rather than asserted as a universal market stub convention. Real
contracts can instead have separately prorated stub payments, original seasoned
payments, cents rounding, interest-only periods, balloons, recasts, or negative
amortization. None of these is inferred by `level_payment`. Currency rounding is
not applied to intermediate normalized cashflows. Market discount factors and
OAS do not enter schedule construction.

This stage does not change credit/default/recovery, prepayment, floating-index
selection, exercise optimization, calendar conventions or accounting treatment.
It does not close issue #15. Rust remains the production calculation runtime;
the schedule helper calls no Python financial backend. Public application
validation and UI integration are separate rollout work.

## Validation

`packages/portfolio-risk-native/tests/amortization.rs` checks:

- Regular quarterly schedules against independent closed-form payments and a
  hand balance recurrence for positive, negative and near-zero coupons.
- Zero-coupon equal payments and exact full-payoff conservation.
- An irregular first stub and unequal calendar quarters using independently
  specified dates, compounded-discount payment calculation and forward balances.
- A 30-year monthly schedule, final residual payoff and constant total payments.
- Independently calculated rejection of a long, high-coupon ACT/365 contract
  whose first period would increase its principal balance.
- Exact preservation of historical `annuity` principal values and grid clamping.
- Explicit rejection of floating/CD/ambiguous-sinking terms, invalid negative-rate
  accrual factors, nonfinite inputs and uncovered new-contract payment dates.

Run `cargo test --manifest-path packages/portfolio-risk-native/Cargo.toml --test
amortization`. Existing term ownership and financial parity suites remain required
before production rollout; these schedule tests alone do not validate end-to-end
OAS, risk, income accounting or observed borrower behavior.

`packages/portfolio-model-core/tests/amortization.rs` additionally tests the public
shared helper's standalone admission/input validation, arbitrary accruals, and
all 4096 supported periods against a closed-form payment for zero, near-zero and
positive coupons. Run `cargo test --manifest-path
packages/portfolio-model-core/Cargo.toml --test amortization`.
