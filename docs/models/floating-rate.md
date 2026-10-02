# Supplied-fixing floating coupons

`portfolio_model_core::floating_rate::evaluate` implements the bounded,
versioned `floating-rate-1` arithmetic stage of
[corporate/commercial issue #15](https://github.com/beauschwab/model-portfolio-sim/issues/15).
It calculates contractual interest from immutable **supplied observed fixings**.
There is no default index, hidden three-month mapping, rate projection, inferred
missing fixing, benchmark fallback or Python financial backend.

This stage does not integrate the new model into the existing corporate graph,
strategy unit library or ledger. Future forward-rate emission remains work under
[market-model issue #10](https://github.com/beauschwab/model-portfolio-sim/issues/10).
Existing saved floating contracts retain their historical model until explicitly
converted and rebuilt; this analysis helper never reinterprets them.

## Inputs and provenance

`FloatingRateRequest` contains `schema="floating-rate-1"`, `position_id`, an
uppercase three-letter `currency`, `monetary_unit="currency_units"`, and
`direction="receipt"|"payment"`. Principal amounts are nonnegative actual currency
units. The direction determines the interest cashflow sign; negative interest
rates can reverse that cashflow economically.

Required identities include `reference_calendar:{id,revision}` and
`fixing_snapshot:{id,revision}`. The index descriptor has `index_id` and one of:

| Kind | Required descriptor | Supported calculation |
|---|---|---|
| `term` | Explicit `tenor_months` | `term_simple` |
| `published_average` | Explicit `average_calendar_days` | `term_simple` |
| `overnight` | Neither tenor nor averaging window | `daily_simple` or `daily_compounded` |

Index tenor and the interest/payment period are independent. The supplied fixing
determines the rate; the descriptor identifies what that fixing represents. This
helper cannot certify that a supplied benchmark rate actually corresponds to its
declared tenor or administrator.

Each fixing has `fixing_id`, `revision`, matching `index_id`, `value_ordinal`,
`publication_unix_seconds` and a decimal annual `rate` (`0.05` means 5%). The
request supplies an exact `publication_cutoff_unix_seconds`. Future value dates,
unavailable publications, duplicate fixing IDs, wrong indices and missing fixing
references fail. Only one selected immutable revision may exist per fixing ID.
The result retains the snapshot, calendar, cutoff, index descriptor and each
selected fixing/revision in observation order.

Each `FloatingPeriod` has `period_id`, half-open accrual dates `[start_ordinal,
end_ordinal)`, an independent `payment_ordinal`, a calculation, actual day count,
margin, margin treatment and optional index/all-in bounds. Payment may precede an
unadjusted accrual end under a supplied preceding/modified-following convention;
the arithmetic does not move either date or change interest for that reason.

For `term_simple`, `reset_cutoff_unix_seconds` is mandatory. It must be at or before
both period start and the valuation cutoff, and the selected fixing must have been
published by that exact reset time. A fixing published at 08:00 is unavailable for
a 07:00 reset on the same day. Overnight periods must leave the term reset cutoff
null and use their individual observed fixing publications.

The principal path and observation mapping are separate:

```
principal: [{start_ordinal, end_ordinal, amount}, ...]
observations: [{interest_start_ordinal, interest_end_ordinal,
                observation_start_ordinal, observation_end_ordinal,
                fixing_id}, ...]
```

Principal intervals and interest intervals must each fully and contiguously cover
the period without overlaps. Overnight fixing value dates match observation-start
dates. Observation dates cannot be after their corresponding interest-start date.
The caller supplies `lookback_business_days`, `observation_shift`, and
`complete_observation_coverage=true`. Shifted observation intervals must themselves
be contiguous. Weekend-start periods can use the preceding business day's fixing.

**Calendar/lookback mapping is supplied, not verified here.** The helper checks
dates, identity, availability and coverage. It does not generate government
securities business days, verify calendar content, or prove that a declared
business-day lookback produced the supplied observation dates. The result
explicitly returns `calendar_mapping_verified=false`. Production calendar mapping
needs its own native validation and source identity before integration.

## Arithmetic and contractual choices

Use `act360`, `act365_fixed` or `act_act_isda` explicitly. The shared actual-day
fraction helper handles real dates and ISDA year-boundary splitting. No reporting
month is interpreted as thirty days.

For each observation, select the annual rates in this order:

```
index = apply(index_floor, index_cap, supplied_rate)
all_in = apply(all_in_floor, all_in_cap, index + margin)
adjusted_index = all_in - margin
```

Bounds apply to annual interval rates; they do not constrain the final compounded
effective period rate. No bound is inferred when its value is null. This ordering
keeps index bounds distinct from all-in bounds and makes the treatment of margin
under a binding all-in cap/floor explicit.

`term_simple` holds one selected in-advance term/average fixing over the complete
period. `daily_simple` applies each selected daily fixing to the principal actually
outstanding within its interval, including principal changes inside a weekend.
Without observation shift, both methods calculate:

```
interest = sum(principal_on_subinterval * all_in_rate * interest_year_fraction)
```

With observation shift, daily simple uses constant principal and:

```
interest = principal * sum(adjusted_index * observation_year_fraction
                           + margin * interest_year_fraction)
```

For `daily_compounded`, principal must remain constant within the period. Only
**rate compounding** is supported. The `balance_compounded` calculation is
explicitly rejected. Rates are compounded over each business-day interval; a
three-day weekend interval has one factor `1+r*3/360`, rather than three separate
one-day factors. No implicit capitalization of accrued interest or compounding
across payment periods occurs.

Let `w_i` be interest-date weights without shift and observation-date weights with
shift; let `a_i` always be actual interest-date weights. Then:

```
simple margin:     interest/principal = product(1 + adjusted_index_i*w_i) - 1
                                       + margin*sum(a_i)
compounded margin: interest/principal = product(1 + all_in_i*w_i) - 1
```

The simple margin remains uncapitalized and accrues over actual interest dates,
even when the index's observation weights shift. Each compound factor must be
positive and finite. The implementation accumulates `ln_1p(rate*weight)` and
uses `exp_m1` for the resulting interest factor, preserving interest near zero
where subtracting one from a rounded product would erase it.

Changing principal is rejected for compounded rates and shifted daily-simple
calculations because allocating shifted/cumulative interest across changing
principal requires further contractual rules. Separate balance compounding and
noncumulative rate implementations can be added under explicit model identities.

`effective_annual_rate` equals unsigned-direction interest divided by the sum of
principal times actual interest year fractions. It is null for zero principal.
Cashflow direction affects `interest_amount`, not the reported annual coupon.
Internal calculations are unrounded; invoice/ledger rounding remains a downstream
explicit policy.

## Admission and failure

Standalone validation applies before result calculation:

- 1..=4096 periods; 1..=250000 fixings; at most 250000 combined principal and
  observation intervals across the request.
- Actual dates in Gregorian years 1..9999; each interval and payment displacement
  is bounded to 36600 days. Lookback metadata admits 0..=366 business days.
- Each identity is nonempty, control-free and at most 1024 bytes; aggregate checked
  metadata is at most 32 MiB. Supplied UTF-8 IDs/revisions retain their exact text.
- Principal and each result interest amount are finite and at most `1e15` actual
  currency units in absolute value; no hidden monetary scaling is applied.
- Publication cutoffs are UTC Unix seconds in 1970..9999. Nonfinite rates/margins,
  inverted bounds, nonpositive compound factors and numerical overflow fail.

These are component/work limits, not an aggregate process RSS guarantee.
Calculations intersect two ordered interval lists in linear work; no path-by-day
tensor or new random draws are created. Failure returns no successful result.
The surrounding native transport retains its own JSON/input and deadline bounds.

## Independent validation and sources

`core/tests/floating_rate.rs` checks hand-calculated simple and compounded coupons,
Friday/weekend weights, observation shifts, distinct index/all-in caps and floors,
simple/compounded margin, principal changes inside an observation interval, term
tenor/provenance retention, intraday reset availability, payment before accrual end,
negative/near-zero rates, zero principal, coverage/source failures and independent
metadata/monetary/work admission. Run:

```
cargo test --manifest-path packages/portfolio-model-core/Cargo.toml --test floating_rate
```

Primary sources informed the explicit conventions and limitations:

- [ARRC in-advance term/average conventions](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2021/Term_SOFR_Avgs_Conventions.pdf)
  distinguish forward-looking term rates from published averages and explain
  in-advance observation and independent interest-period/benchmark tenor choices.
- [ARRC syndicated-loan arrears conventions](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2020/ARRC_SOFR_Synd_Loan_Conventions.pdf)
  distinguish simple, compound-balance and compound-rate methods, describe
  lookbacks without observation shift, daily floors and principal-change limits.
- [ARRC technical appendices](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2020/ARRC-Syndicated-Loan-Conventions-Technical-Appendices.pdf)
  describe interest/observation date weights and alternative compounding methods.
- [NY Fed SOFR methodology](https://www.newyorkfed.org/markets/reference-rates/additional-information-about-reference-rates)
  distinguishes value/publication dates and specifies business-day compounding,
  weekend/holiday simple weights and ACT/360 for its published averages/index.

Conventions are contractual choices, not a claim that every supported combination
is an ARRC recommendation. These tests validate arithmetic, not empirical rate
projection, administrator data authenticity, full graph/ledger behavior or issue
#15 completion. New production coupon models require explicit cache/model identity,
library rebuilds, historical compatibility, fixed base OAS, shared CRN and separate
end-to-end ownership/publication validation.
