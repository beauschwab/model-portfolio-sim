# Corporate exercise source verification and publication

## Local acceptance: 0.29.9, 2026-10-10 UTC

The final matching-source checks pass **409 native product all-target tests**,
strict all-target/all-feature product Clippy, **88 affected engine tests in
17.18 seconds**, and **285 affected API/worker/persistence tests in 113.80
seconds**. The API slice includes complete model-worker restart coverage, scalar
transport, exercise corruption/type/identity gates and source/persisted/final
publication failures. The engine slice includes native corporate/CD ownership,
enhanced contracts and the thirteen independent exercise-oracle cases.

All three official release artifact builds succeed; the unchanged ledger build
is current. Product rebuild takes 6m20s and dependent decision rebuild 8m26s;
these are compilation times, not model performance measurements. Official
decision release Clippy also passes. Architecture, seven boundary-script tests,
native ABI and HTTP contracts, version/lock consistency and packaged operator
mirrors pass. The version is synchronized at 0.29.9 and financial schemas/ABI
are unchanged.

The initial combined Python collection used an incorrect working root and
failed to import the API package. The suites were rerun separately. An initial
release attempt failed because concurrent tests held the Windows DLL; tests
then exited and the rebuild succeeded. An intermediate API rerun was stopped
before dependent builds finished; the final 285-case run is complete. None of
those failed/interrupted predecessors is counted as passing evidence.
The broader 2,378-engine/920-API checkpoint in the implementation ledger is
previous-turn evidence, not a fresh full-suite run for this stage. Changes remain
local and uncommitted. #15 is open and its complete original acceptance is retained.

This #15 follow-through applies to the existing `conditional-exercise-1` route.
It changes neither `term-contracts-1` economics nor historical saved contracts.
Rust performs conditional exercise valuation. The durable worker now requires
an independent complete raw-tree replay before formatting and again after
reopening its immutable Parquet result, using the original publication deadline.

## Financial verification

`analytics.term_exercise_reference` independently constructs values from the
immutable raw request. It calls no native callback or production pricer. The
only shared arithmetic helper is the independent actual-day-count reference.
For each node it computes scheduled cash plus conditional discounted child
values, applies the fixed original OAS exactly once per edge, and compares
continuation against permitted immediate redemption. Issuer call minimizes
holder value; holder put maximizes it. Simultaneous priority, exact tie behavior,
before/after scheduled cash, signed accrued rights, clean premiums and fees
remain explicit supplied policies.

Forward stopped-probability propagation independently reconstructs every
expected cashflow. Recombining nodes sum incoming live probability; exercised
branches retain no future rights. Reporting principal is separate from the full
maturity tail. Complete node audits, including unreachable-under-policy nodes,
cash components, accrual periods, actual dates, measure, provenance, policy,
original OAS and shared-draw identity must match. Unknown/missing fields, wrong
counts, nonfinite amounts, boolean amounts and floating-point dates reject.
The financial comparison uses the existing independent-reference tolerances
of 1e-12 relative and 1e-9 currency units absolute; it does not alter production
financial tolerances. Disclosure text is required but is not a financial source.
The original supplied OAS must match exactly. Probability comparisons use
1e-12 relative and 1e-14 absolute tolerance, separately from currency amounts.

The receipt records independently recomputed node/edge/event counts and explicitly
retains `measure="pricing"`, `physical_journal_eligible=false` and
`empirical_validation=false`. Persisted publication requires the same recomputed
receipt and scalar summary, plus the existing raw-input, source, executable,
deadline, cancellation and lease/token fences. A receipt alone is insufficient.

## Cooperative execution

`value_exercise_with_check` checkpoints during admission, every node and branch
in backward valuation and forward propagation, principal conservation and final
audit construction. The existing public wrapper uses native compute control.
The dispatcher explicitly transfers the caller's control across Rayon and
preserves `COMPUTE_CANCELLED` and `COMPUTE_DEADLINE_EXCEEDED`. Failed execution
returns no partial policy. Bounded schedule validation and JSON conversion have
fences around them; their inner library operations are not separately interruptible.
The standalone process watchdog remains in force. Component work/transport
bounds are unchanged and are not an aggregate RSS guarantee.

## Validation and limitations

Tests compare raw replay against Rust for simultaneous rights, both cash timings,
receipt/payment signs, signed accrued rights, original OAS and recombining states.
Option-free, called and puttable supplied-state refinements at 16/64/256 steps
are compared with independent normal-integral lognormal expectations. The
option-free value remains 100; the call-capped limit is
`100 * erfc(0.15 / sqrt(2))` and the put floor is its complementary `200 - cap`.
This synthetic terminal-right fixture verifies supplied-tree numerical
convergence, not a calibrated corporate rate model. Existing native checks for
actual dates, call/put ordering, no perfect foresight and complete tails remain.

Fault tests alter policy, values, all stopped cash components, identities,
dead-node audits, shape and numerical domains. Every native checkpoint is tested
for both failure reasons; real delayed Rayon dispatch checks inner failure
independently of the outer scope. Durable fault tests cover corruption and
cancellation before and after persistence, missing receipts and final fences.

The min/max exercise and explicit coupon ordering agree with the financial
operations in [QuantLib's callable-bond implementation](https://github.com/lballabio/QuantLib/blob/master/ql/experimental/callablebonds/discretizedcallablefixedratebond.cpp).
That implementation's nearby-date snapping is not part of this exact-date
contract. No QuantLib code was copied into the independent oracle.

This closes an exercise-result publication gap. It does not generate a calibrated
tree, joint credit/market states, notice-delayed settlement, commercial draws,
nonaccrual, recoveries, ledger postings, library coefficients or saved-book/cohort
adoption. Those #15 gates and observed validation remain required. Rebuild native
product and dependent ledger/decision artifacts before use; deployed workers and
saved sessions need their existing explicit restart/rebuild procedure. Local
validation is not deployment or empirical acceptance.
