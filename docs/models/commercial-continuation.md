# Complete-origin commercial holding continuation

Local 0.29.13 adds the explicitly selected `commercial-holding-continuation-1`
analysis contract. `analytics.model_contracts.continue_commercial_holding` sends
raw inputs through the native C ABI; `/models/runs` executes the same contract in
the durable standalone Rust process. Python performs independent verification
and storage. Neither transport computes opening money or restarts credit.

The request supplies `completed_source`, `revised_source` and `cut_ordinal`.
Both sources use the complete commercial compiler contract from original
acquisition through the complete contractual, reviewed-action and recovery tails.
The native owner replays each full history, requires identical completed atomic
groups and carry records through the cut, and constructs a non-deserializable
opening. It retains original acquisition cost and effective yield independently
of cut carrying value, live/default principal, undrawn commitment, tranche yields,
canonical coupon entitlement, signed recognized/legal lots and reviewed credit
state. It selects subsequent groups by their unchanged native indices. Summaries
reduce only subsequent cash, effective interest and basis realization, in source
order. Historical earnings and acquisition are excluded from that window.

There is no monetary checkpoint input. The revised CTMC starts at original
acquisition on the original stream and paths. Its latent elapsed waiting hazard
survives the cut and supplied calendar boundaries. For example, a supplied
exponential threshold corresponding to day 20 at hazard 10, with a cut at day 15,
defaults at day 17.5 when the future hazard becomes 20. Restarting the same waiting
draw at the cut would produce a different time. Defaults already incurred retain
their original complete recovery claims and reviewed book/legal state.

Original source/cost/yield, credit paths/stream, factor policy, contractual dates,
principal plan, draw terms and reviewed credit actions cannot change. Periods
already begun and observations with fixing dates through the cut cannot change.
Recognition, rates and hazards can change only in wholly future supplied intervals.
Future indexed projections can change when existing contractual observability
and original/current identity gates permit them. General contractual modification,
policy replacement and partial-interval editing require separate ownership.
A live holding cut requires an exact native earning boundary; a frozen default
owner can retain its latest action before the cut. No partial income is invented.

The example is a synthetic two-period100 loan. The original yield is 5%, its
purchase quote discounts actual day 35/day 65 cash, and the cut is day 15. An unpaid
legal coupon right of100*10%*15/365 survives the cut. Only the future period's
conditional projected coupon changes to 15%; original acquisition economics remain
retained. Other controls cover a funded draw with its 7% original tranche yield,
reviewed nonaccrual, reviewed chargeoff/waiver/recovery and signed rights after
principal has settled. These are numerical controls, not observed performance.

Admission charges both complete owners, prefix comparisons, opening copies,
suffix indices and raw transport before either source is generated, within 8M
work and 128 MiB retained capacity. Raw input is bounded to 1 MiB and output to 32 MiB.
These component bounds do not guarantee aggregate process RSS. Original native
execution controls cover decoding, generation, comparison and serialization;
durable jobs retain the original queue deadline through independent raw and
persisted replay, source/executable/object checks and final SQL authority fences.

This contract supplies a reusable holding-window foundation. It does not yet
consume a verified prior journal publication, run a cut-started mixed daily Sim,
prepare seasoned saved/cohort/strategy coefficients, or adopt option-stopped
acquisition proofs. `verified_prior_publication`, `mixed_ledger_adoption` and
`secondary_acquisition_adoption` remain false. Secondary midcoupon purchases,
conveyed earned rights and clean/dirty quote allocation remain distinct required
work. Nonintegral fees, daily in-arrears indexed earning, generated streaming,
empirical histories/policies and full mixed scale acceptance remain open under
issues 15/25. Historical schemas and receipts remain readable unchanged. New
execution requires matching rebuilt artifacts and explicit selected-session and
library rebuilding; this local change does not restart deployed workers.

Matching official local builds pass 1,216 native tests, 2,579 engine checks with two
reference-only skips and 1,091 API checks. Strict, contract and whole-output identity
checks pass. See [the accepted review](../reviews/2026-10-12-commercial-holding-continuation.md)
for exact source/artifact/log identities and the preserved superseded runs.
