# Dated cashflow event model

`portfolio-model-core::cashflow` provides a Rust-owned, versioned event contract
for issue #12. It separates actual contractual settlement dates, accrual
endpoints, economic components, scenario identity and provenance. It is a shared
foundation for product generators and daily accounting. Existing monthly product
capture and daily replay have not yet been replaced by this contract: introducing
the type does not establish scenario-specific product repricing or end-to-end
publication acceptance.

## Dates and time

All dates are proleptic Gregorian ordinals with `0001-01-01 = 1`, through
`9999-12-31 = 3652059`. `payment_ordinal` is the actual settlement date supplied
by the source or product generator. `day_offset` is the exact number of calendar
days from the schedule's `as_of_ordinal`, and validation requires both fields
to agree. An event on the as-of date has offset zero. The horizon is inclusive.
There is no conversion from months to multiples of thirty days.

An optional accrual period is half-open `[start_ordinal,end_ordinal)`. Its end
must be inside the declared horizon, independently of settlement. It may follow
settlement when a preceding/modified-following convention moves payment before
an unadjusted contractual accrual endpoint; this is a negative settlement lag,
not a negative accrual fraction. Its start may precede as-of, as for a coupon
already accruing at the valuation date. Accrual endpoints are metadata; the type does
not smear accrued interest across a reporting month or automatically post
recognized income. The event date specifies when each explicit component is
recognized or settled by the downstream accounting policy. A producer needing
separate daily accrual recognition and later cash settlement emits separate,
uniquely identified events.

Calendar helpers implement Gregorian dates and actual month increments only.
`add_calendar_months` requires an explicit policy:

| Policy | Example from 2024-01-31 | Meaning |
|---|---|---|
| `clamp_day` | 2024-02-29 | Clamp an invalid day to the destination month's last day |
| `preserve_end_of_month` | 2024-02-29; 2024-03-31 from the original January anchor | Preserve month-end if the original anchor is month-end |
| `reject_invalid_day` | Error for February | Require the contractual day to exist |

Generate periodic dates from the original anchor plus the cumulative month
offset. Repeatedly adding to a clamped February date can introduce drift. These
helpers do not adjust business days or assume US holidays. A producer must
apply the actual contract's calendar, business-day convention and payment lag,
then provide the final payment date. No missing convention is inferred.

## Typed event and amount semantics

Each event carries `position_id`, stable `event_id`, scenario selector, currency,
date, optional accrual period, components, measure, provenance and adjustment.
The owner perspective is explicit: cash receipts are positive and cash payments
negative. Currency is a three-letter uppercase code, without FX conversion or
an embedded monetary multiplier. Currency-code validation is structural; it does
not infer settlement eligibility or validate against a changing ISO registry.

| Component | Meaning |
|---|---|
| `principal` | Cash principal receipt/payment; not an independently inferred outstanding balance |
| `cash_interest` | Cash interest receipt/payment |
| `fees` | Cash fees, separate from interest and principal |
| `accrued_interest` | Signed noncash interest-recognition change in the owner perspective |
| `book_amortization` | Signed noncash change in carrying value attributable to premium/discount/fee amortization |

`cash_total()` sums only principal, cash interest and fees. Noncash amounts are
never included in cash settlement. An accounting adapter must map signed
recognition changes onto asset/liability accounts; the event contract does not
assert that cash interest equals recognized interest or recompute a book yield.
All components and each event's signed cash total must be finite with absolute
value at most `1e15` actual currency units, using the shared monetary admission
constant. This is a numerical input bound, not a policy capital or exposure
limit. Financial calculations retain f64 precision without internal invoice
rounding.

## Scenarios, identity and provenance

`scenarios` declares the complete supported scenario names. The tagged selector
`{"kind":"all"}` makes an event apply unchanged to every declared scenario;
`{"kind":"named","scenario_id":"stress"}` selects one. All events must
be ordered by nondecreasing payment date. Same-day order is retained exactly.

Identity is `(position_id,event_id,scenario)`. The same economic `event_id`
may have separate versions in different named scenarios. It cannot occur
twice in one scenario, and an `all` version cannot coexist with any named
version of that identity, even if their dates differ. This prevents implicit
overrides and double-counting. Distinct IDs on the same date remain valid;
for example, one coupon and one fee. Distinct `all` and named IDs may combine
as explicitly supplementary events. Producers must preserve economic event
identity across scenario versions: the validator cannot discover two unrelated
IDs that describe the same payment.

Provenance is mandatory and uses one of:

- `contractual`: source ID plus immutable source revision; this describes an
  exact supplied contract event and requires contractual measure.
- `modeled`: model ID, model version and immutable input revision.
- `assumed`: assumption ID and a nonempty rationale.

`measure` distinguishes `contractual`, `pricing` and `physical` cashflows.
Contractual cashflows have no probabilistic measure. Pricing-model expected
cashflows belong to the declared pricing measure and are suitable for valuation
only with a consistent market/discount model and fixed base OAS. Physical or
conditional stress expectations are for forecasting or accounting under their
declared model, and cannot be silently substituted into OAS, EVE or derivative
valuation. Merely labeling a flow does not transform its measure or validate the
model producing it. Scenario identifiers are labels, not new random seeds;
product generators must preserve the shared-CRN risk contract.

## Actual accrual and overnight compounding

`actual_year_fraction` supports ACT/360, ACT/365 fixed and ACT/ACT ISDA. ACT/ACT
splits at calendar-year boundaries and uses 365 or 366 for each piece. ACT/365
fixed always uses 365. No helper silently selects a convention, and this helper
does not implement ACT/ACT ICMA or a 30/360 convention.

`overnight_compounded_factor(start,end,intervals)` returns the accumulation
factor, not an annualized rate or cash interest:

```text
F = product_i (1 + r_i * (end_i - start_i) / 360)
interest for unchanged principal P = P * (F - 1)
annualized period rate = (F - 1) * 360 / (end - start), when end > start
```

Rates are decimal annual values: `0.05` is 5%. Intervals must exactly partition
`[start,end)`, with no gaps, overlaps or zero-length pieces. A Friday observation
applying until Monday is one factor `1 + r*3/360`, not three daily compound
factors. The New York Fed methodology compounds on business days and applies
simple interest over intervening nonbusiness days using the preceding
business-day rate and actual/360 weighting. Observation/value dates differ
from publication dates. See the [New York Fed reference-rate methodology](https://www.newyorkfed.org/markets/reference-rates/additional-information-about-reference-rates).

The caller supplies interest-application intervals and the already-selected
rate. It must implement the contractual observation calendar, lookback,
observation shift, lockout, floor and spread policy before calling this helper.
The helper performs none of these choices, and has no implicit fixing fallback.
It supports negative rates when each accumulation factor remains positive.

The constant-principal rate-product identity does not establish correct
interest for principal changing within the accrual period. ARRC distinguishes
compounding the balance from compounding the rate, and notes qualifications for
rate compounding when principal is paid down. Loan lookbacks and derivative
payment delays may differ; floors and fallback spreads also require contractual
treatment. Those belong in dedicated product generators, which must emit exact
cash and accrual events. See the [ARRC bilateral business-loan conventions](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2020/ARRC_SOFR_Bilat_Loan_Conventions.pdf).

## Surviving-principal overlay

`proportionally_adjust(fraction,rationale)` scales all five components by one
explicit fraction between zero and one. Fractions below one attach the
`proportional_surviving_principal` approximation marker and rationale. Applying
a second overlay to an already adjusted event fails. A factor of exactly one
returns the original event unchanged, including provenance and adjustment;
the zero-shock serialized identity is exact.

This is an optional approximation, not a credit or prepayment model. It may
misstate fees, interest already accrued, recovery timing, cure paths, option
exercise and effective-yield amortization. Full scenario integration must
regenerate product cashflows with joint credit/behavioral state and preserve
fixed OAS and CRN. An exact contractual source with this marker remains
traceable to its source but no longer asserts an unmodified contractual amount.

## Admission and validation

Schedules admit at most 250,000 events, 62,000 distinct positions, 256 declared
scenarios and 36,600 horizon days. Total supplied string metadata across the
schema, declared scenarios and events is bounded to 32 MiB; shared labels still
count each time supplied, and enum labels/JSON syntax are transport overhead.
Text fields also have explicit per-field byte limits and reject
empty/control-character values. Validation does not expand common events
across scenarios. `for_scenario` first validates and then creates one bounded
event vector, preserving order, amount and date. These are component limits,
not an aggregate process RSS guarantee. JSON transport must separately admit
request/output bytes before allocation and persist immutable request identity.
Structs and tagged enum payloads reject unknown JSON fields; unsupported schema
versions fail rather than reinterpret data.

## Validation and remaining delivery

`cargo test --manifest-path packages/portfolio-model-core/Cargo.toml --locked
--test cashflow` covers independently known Gregorian ordinals, leap centuries,
February/end-of-month policies, year-boundary ACT/ACT, a hand-computed coupon,
weekend SOFR accumulation, negative rates, interval gaps, strict JSON decoding,
duplicate/common-specific identities, scenario resolution, numeric overflow,
signed monetary bounds, aggregate metadata admission and exact zero-shock
overlay identity.

These tests validate the event foundation. They do not certify existing product
models, empirical behavioral fit, new production ledger integration, independent
persisted-journal replay, large-book acceptance or publication cleanup. Issue #12
still needs each product's actual-date scenario generation, transport/replay
integration, independent daily-output fixtures and removal of monthly-grid
mapping assumptions where contractual data is available.
