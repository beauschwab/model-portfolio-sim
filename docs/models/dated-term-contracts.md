# Actual-date fixed term contract projection

`portfolio-risk-native::dated_term` adds the `dated-term-1` producer for issue
#12. It generates bounded contractual principal and interest events for fixed,
nonoptional corporate contracts and CDs, using the existing native contract
normalizer, calendar/day counts and shared amortization routines. It is an
incremental producer stage. The pricing graph, saved-book ledger capture and
daily publication path do not yet consume its events automatically.

## Input contract and explicit scope

`DatedTermRequest::build()` accepts:

```json
{
  "schema": "dated-term-1",
  "product": "corporate",
  "asof": 738916,
  "horizon_days": 366,
  "calendar": {"name": "NONE", "extra_holidays": []},
  "bdc": "NONE",
  "accrual_dates": "unadjusted_scheduled_dates",
  "scenarios": ["base", "zero_shock"],
  "positions": [{
    "position_id": "asset-1",
    "currency": "USD",
    "source_id": "contract-terms-1",
    "source_revision": "immutable-revision-1",
    "accrual_anchor_ordinal": 738916,
    "direction": "receipt",
    "contract": {
      "maturity": 738976,
      "freq_months": 1,
      "daycount": "ACT/360",
      "coupon": 0.06,
      "notional": 1000.0,
      "price": 100.0
    }
  }]
}
```

The dates in this example are January 31 and March 31, 2024. The producer
returns February 29 and March 31 payments, with 29 and 31 actual accrual days.
`direction` is mandatory: receipt produces positive cashflows; payment produces
negative ones. A bank-held asset and a bank-issued CD liability therefore need
their appropriate explicitly supplied direction. The producer does not infer
owner perspective from product type.

`contract` is the strict existing `term_deck::Contract` schema. Its coupon is an
annual decimal rate and notional is actual currency units. The price target has
no economic role in this contractual protocol: changing a valid price alone
leaves output exactly unchanged. The existing raw contract validator still
requires finite, positive price and valid auxiliary fields. No OAS, present
value, spread, stochastic market path or discount calculation occurs.

The source ID/revision identifies the supplied terms and selected contractual
conventions, not an observed historical payment. Output has contractual measure
and contractual provenance because it is a deterministic projection under those
explicit inputs. Callers must persist the immutable raw request, including date
policies and source revisions, to preserve reproducibility. A source label alone
does not prove that the terms match an external contract.

## Current coupon anchor and remaining balance

`asof` is the portfolio valuation date. `accrual_anchor_ordinal` is the start
of the remaining contractual coupon schedule and can precede as-of. This keeps
the full coupon paid at the next settlement rather than discarding interest
earned before valuation. Notional is the remaining principal immediately before
any included settlement, and must be unchanged between anchor and as-of.

The producer rejects any generated payment strictly before as-of. It does not
infer historical amortization, use original face as current face, subtract past
cashflows or infer a beginning-of-day balance after an included day-zero
settlement. If a supplied anchor generates a past payment, provide the actual
remaining schedule anchor and compatible remaining balance, or use a historical
contract lifecycle capable of reconstructing them. Matured positions are
rejected instead of silently omitted. Zero-notional contracts retain zero-valued
events so their supplied identity and conventions remain visible.

## Original dates, accrual dates and actual settlement

The original schedule uses the existing backward maturity-anchored calendar
roll and a short initial stub when needed. Frequencies are calendar months;
monthly boundaries are not thirty-day offsets. Backward rolls clamp invalid
days from the original maturity anchor. No hidden explicit EOM, forward-roll,
long-stub or user-supplied coupon-date convention is inferred. Contracts needing
different schedule rules require an extended, versioned schema.

The producer first uses `Calendar::schedule` with `BDC=NONE` to obtain the
original scheduled endpoints, then `Calendar::adjust` to obtain actual payment
dates. It never reconstructs dates from monthly grid indices or `t_pay`. Each
result contains an audit row with scheduled start/end, selected accrual start/end,
actual payment, daycount, year fraction, principal fraction, remaining balance
and `settlement_lag_days = payment - accrual_end`.

The caller must select one accrual policy:

| Policy | Meaning |
|---|---|
| `unadjusted_scheduled_dates` | Accrue between original scheduled endpoints; adjust settlement separately |
| `adjusted_payment_dates` | Preserve the existing native term model's accrual between adjusted dates |

For a single-payment CD with omitted/nonpositive coupon frequency, existing
adjusted semantics accrue from the original anchor to adjusted maturity. Regular
coupon schedules also adjust the initial effective date under adjusted policy.
These choices can change interest and level-payment amortization; the selected
policy is part of result identity and is never inferred from product type.

A payment moved before an unadjusted accrual end by preceding/modified-following
has a negative settlement lag. This is valid and does not imply a negative
interest period. Core `DatedSchedule` validates chronological accrual endpoints
and their horizon independently of settlement. For example, March 31, 2024
falls on Easter Sunday; US modified following pays March 28 because March 29
is Good Friday. The unadjusted coupon still accrues through March 31.

Business-day conventions and accrual calculation are separate contractual
choices. ARRC documents actual/360, modified following, and the importance of
aligning loan and derivative conventions rather than assuming one universal
SOFR convention. This producer does not implement floating SOFR cashflows. See
the [ARRC bilateral loan conventions](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2020/ARRC_SOFR_Bilat_Loan_Conventions.pdf).

Recognized calendar names are exactly `US`, `NONE` and `WEEKEND`. `NONE` and
`WEEKEND` supply no named holidays; Saturdays/Sundays still adjust when BDC is
not `NONE`. `US` preserves existing research holiday rules, including their
year-scoped treatment of a next-year New Year observation in December. Extra
holiday ordinals can add missing dates; they cannot remove built-in holidays.
Unknown calendars fail instead of falling back to weekends. This is not an
exchange-specific or complete legal settlement calendar certification.

Supported daycount labels retain their existing native meanings: ACT/360,
ACT/365F, ACT/ACT using calendar-year denominators, and the existing 30/360
algorithm. In particular, ACT/ACT here is not an ICMA reference-coupon-period
algorithm, and 30/360 does not silently become another variant. Exact Treasury
or other specialized instrument conventions require their own implementation:
Treasury regulations describe full semiannual coupon amounts independently of
the half-year's day count, with a reference-period calculation for fractional
periods. See [Treasury's auction regulations, Appendix B](https://www.treasurydirect.gov/files/laws-and-regulations/auction-regulations-uoc/31-cfr-part-356.pdf).

## Principal and interest mathematics

For period `j`, with opening balance `B_j`, selected year fraction `tau_j`, and
coupon `c`, signed period interest is `direction * B_j * c * tau_j`.

Corporate principal supports:

- `bullet`: no principal until final settlement.
- `annuity`: historical equal-principal behavior; equal fractions over remaining
  periods, not equal total payments.
- `level_payment`: equal total payments under the selected accrual fractions,
  using the shared stable fixed-rate annuity helper.

For level payment, the normalized payment is
`1 / sum_j product_(k<=j)(1 + c*tau_k)^(-1)`. Interest follows opening principal;
principal is the payment less interest. The production helper uses a stable
backward annuity recurrence. Unadjusted level payments are built from unadjusted
taus; the producer does not reuse principal calibrated to adjusted taus. All
period growth factors must remain positive, and unsupported negative
amortization fails. The last principal settles the exact remaining balance,
without inventing a payment beyond maturity.

CD principal is always bullet, even though the existing CD deck's principal
coefficient vector is empty-valued for kernel use. An omitted/nonpositive CD
frequency means one maturity payment, not a rollover or compounded deposit
model. CD interest is simple annual-coupon-times-daycount; periodic coupons do
not become interest-bearing new principal.

`cash_interest` and `accrued_interest` both contain the whole contractual period
amount. The former is cash settlement, while the latter records the period's
contractual accrued amount for projection/audit. It is not a daily recognition
schedule and must not be treated as incremental post-as-of NII without a
separate daily accrual adapter. `cash_total` excludes accrued interest. Fees and
book amortization are zero because no fee recognition or effective-yield policy
is supplied. They are not inferred from market price.

## Explicitly rejected behavior

Floating coupons, active call/put schedules, sinking schedules and incompatible
CD amortization fail before result construction. Existing sinking schedules
use approximate nearby-date snapping; this producer does not represent that as
exact contractual principal. CD early withdrawal must be disabled (`ew_mult=0`),
or the existing brokered-channel convention must disable it. Otherwise a fixed
nonoptional projection would silently discard a real behavioral option.

This producer has no default, recovery, prepayment, credit migration, reinvestment,
renewal, continuation-value exercise or economic-yield model. All declared
scenarios receive the same `all` contractual events. A zero-change scenario is
exactly identical. A shock requiring different credit/behavioral cashflows must
use an appropriate scenario generator, retaining shared CRN and fixed base OAS
when those flows enter valuation.

## Admission, validation and remaining integration

A request admits at most 256 positions, 4,096 periods per contract, 250,000 total
events, 1,024 extra holidays and the core date/scenario limits. Event plus audit
string metadata is bounded to 32 MiB before retaining additional rows. Shared
monetary admission limits notional, components and cash totals to `1e15` absolute
currency units. Accrual anchors and future horizons are bounded separately to
100 years. Actual maturity, accrual end and adjusted settlement must all fit the
declared horizon; no terminal-grid clamping or partial truncation is allowed.
Transport must separately enforce its request/output byte and runtime limits.

The existing `DeckRequest` is used for contract validation and non-level-payment
principal normalization with a synthetic grid ending strictly beyond the full
date horizon. It never supplies output dates. Core schedule validation gates the
completed result. Stable payment-date sorting preserves source position order
and within-position period order for equal-date events; audit rows remain aligned.
Failure returns no partial result and does not publish artifacts.

`cargo test --manifest-path packages/portfolio-risk-native/Cargo.toml --locked
--test dated_term` covers independently hand-calculated leap/month-end dates,
holiday following/modified following, negative settlement lag, current coupons
before valuation, stubs, equal principal, CD bullet/sign semantics, unequal-stub
level payments against an independent discounted annuity, zero-notional rows,
price-inert and zero-change identities, unsupported features and horizon/domain
rejections. Core tests separately cover the updated accrual/settlement invariant.

Issue #12 remains open for full product date semantics, floating/optional and
behavioral scenario producers, pricing graph capture, source-linked daily
accrual/ledger integration, independent persisted journal verification,
large-book acceptance and publication/cancellation gates. Contractual-event
fixtures alone do not establish those outcomes.
