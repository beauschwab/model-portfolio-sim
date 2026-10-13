# Bounded dated schedule store

`portfolio_ledger_native::dated_store::DatedStore::build` compiles an immutable
`dated-store-1` input. It owns storage, actual Gregorian date admission, explicit
run coverage and supplied recognition intervals. It does not estimate behavior,
rescale surviving principal, choose an allowance, or post a journal.

## Identity and economic ownership

Every owner declares position, account, currency, cash direction, source ID and
revision, and one of `contractual_dated_native_state` or
`fully_scenario_generated`. Contractual owners accept contractual events only.
Scenario-generated owners accept contractual or physical events; pricing measure
and already proportionally adjusted events are rejected. The shared contract
also rejects contractual provenance relabeled as a physical forecast.

Explicit runs contain unique run IDs and scenario/severity cases, plus input
revision. Every source schedule uses the same valuation date and exact run-ID
set. Each owner/run requires `complete_event_set` or `no_events` with a rationale.
An absent row is not a zero-flow assumption. All-run and named declarations
cannot overlap. Same-day events retain supplied ordering and distinct event IDs;
day-zero settlement is supported.

Source container identities are distinct from original event provenance. Both
are preserved. String and provenance interning reduces repeated metadata without
substituting a mutable global revision for economic input identity.

## Recognition and rights

A nonzero interest or basis-recognition amount requires an explicit accrual plan.
Its segments exactly partition the event's accrual period and reconcile the
event totals. Each segment declares actual start/end ordinals and signed amounts.
Uniform recognition **within each supplied segment** is the declared policy;
whole-coupon recognition is never inferred from a payment date.

For segment amount `A`, dates `s < e`, and cutoff `t`, cumulative recognition is
`A * clamp((t-s)/(e-s), 0, 1)`. Interval recognition is the difference of the two
cumulative endpoints. Adjacent intervals telescope; a complete segment returns
its supplied total. Producers can provide unequal segments or daily segments for
day-count splits, rate resets, varying balances and modeled recognition.

`rights_at` reports cumulative earned interest, recognized basis, whether cash
settlement occurred, cash interest settled, and the signed difference between
earned interest and cash. `BeforeEvents` versus `AfterEvents` makes day-zero and
same-date interpretation explicit. Early settlement can leave deferred income or
prepaid expense; delayed settlement can leave a receivable or payable. These
rights do not disappear when principal reaches zero.

`settlements_on`, `accrual_boundaries_on`, `opening_accrual_events`,
`pending_accrual_events` and `tail_events` support a bounded native coordinator.
The 1,080-day reporting horizon is separate from a full contract horizon of up
to 36,600 days; payment and recognition tails remain retained.

## Bounds and validation

Admission caps are 62,000 owners/sources, 256 runs, 250,000 total events,
250,000 coverage rows and 250,000 recognition segments. Input metadata is bounded
to 32 MiB; retained numeric schedules, dictionary references and boundary indices
are charged to a 128 MiB capacity budget. Construction temporaries, the caller's
request, active simulation state and GL/output buffers are additional; these
component budgets do not constitute an aggregate RSS cap.

Independent tests cover pre-as-of recognition, leap dates, early/late cash,
day-zero/same-day identity, full tails, nonuniform segments, explicit empty
coverage, source/scenario/measure conflicts and malformed plans. Store tests do
not establish behavioral calibration or full mixed-book scale acceptance.
