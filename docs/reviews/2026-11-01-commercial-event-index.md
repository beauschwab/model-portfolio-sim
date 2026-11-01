# Immutable commercial event scheduling

This local stage removes repeated daily scheduling work from native commercial
loan/fee replay and cut-prefix checks. It does not admit the public cut example,
verify a published predecessor or complete issues 15 and 25.

## Implementation and exact order

`Compiled` now retains immutable per-scenario loan and fee timelines. Scenario
identities borrow the same source strings already retained by its bindings.
Each event caches the original posting day, fractional global year and existing
binding/group identity. No financial group, cash amount, snapshot or native
financial source is copied into the index.

Preparation sorts each metadata timeline once using an explicit bottom-up merge
sort. Daily lookups use bounded binary day searches. A borrowed exact-size fused
iterator merges the two already ordered daily slices without allocating lookup
Strings or rebuilding/sorting a daily scheduling vector. Its comparison key is
unchanged: original fractional year, Loan before Fee, binding index, group index.
The existing actual-date mapping and all posting/accounting logic are unchanged.
Repeated queries are independent and do not consume the stored index.

The sort makes at most `n * ceil(log2(n))` key comparisons for `n >= 2` entries.
Metadata copies, including its initial scratch copy, are bounded by
`n * (1 + ceil(log2(n)))`. Each numeric day boundary search takes at most the bit
length of `n` comparisons. The daily merge consumes every event once. Sort
cancellation checks run between merge ranges and at most 256 metadata copies.
All scratch records contain only Copy metadata; they cannot clone financial
groups or construct financial authority.

The diagnostic inventory reports actual sort counters, closed sort/search
bounds and observed vector capacities. It explicitly does not certify allocator
overhead, complete source/Compiled/Sim/codec work or process RSS. Historical
source, preparation, prefix and ledger charges remain unchanged; no budget,
tolerance, recovery policy, source history or publication fence is relaxed.

## Validation

All 248 ledger-native and 237 decision-native tests pass. Both crates pass
formatting, strict all-target/all-feature Clippy, doc tests and debug-binary
builds. A frozen former daily collector/comparator independently reads source
years and verifies the cached order through complete day ranges, including the
existing signed-fee and zero/half/full-allocation native cut controls. Synthetic
index controls cover fractional/signed-zero clocks, ties, Unicode scenarios,
permutations, empty days, iterator exhaustion, sealing, cancellation and the
closed sort/binary-search operation bounds.

Ten complete previous-binary journal/treasury results remain exactly identical
under canonical JSON, including numeric representation. Cases include ordinary
and secondary candidates, acquired prepaid fees, holding windows, loan/fee
journals, candidate capital/FTP, commercial treasury and zero allocations. Targeted
independent engine/adoption tests pass 276 cases, including two new complete
native candidate controls with supplied synthetic default/recovery paths at
zero and full allocations. Independent raw-source and ordered-journal replay
checks those actual modeled default/recovery events and complete tails.

Genuine complete preparation emits these metadata observations:

| Input | Indexed ledger events | Setup comparisons / bound | Metadata copies | Retained metadata vector capacity |
| --- | ---: | ---: | ---: | ---: |
| Original full public example | 68 | 186 / 314 | 382 | 3,280 bytes |
| Short secondary/fee example | 56 | 100 / 180 | 236 | 3,360 bytes |

Both examples retain exactly the same entire index inventory at zero allocation.
There are no owned scenario Strings or daily scheduling-vector allocations in
either index. The largest observed two-vector sort capacity is 2,120 bytes for
one full-example timeline and 1,040 bytes for one short-example timeline. Those
component observations omit other coexisting timelines/owners and allocator
overhead; they are not aggregate peak-memory bounds or source admission proof.
No cut Sim executes in this inventory capture.

Evidence is retained under `artifacts/commercial-event-index-2026-11-01/` in
`native-final-v2/`, `final-census-v2/`, `journal-parity/` and the v2 targeted/API logs.
Initial diagnostics are preserved separately. An initial zero-allocation test
incorrectly compared raw-byte admission charges as well as indices; reducing
`100.0` to `0.0` changes serialized raw length. The corrected control requires
exact equality of every index field/counter/capacity while retaining the actual
raw-byte admission calculation. No financial expectation or tolerance changed.
Final v2 validation uses a single `<` or `<=` numeric comparison per boundary
probe, matching the stated search bound; earlier diagnostics remain separate.

## Remaining acceptance

The required positive API cut-publication test still fails before dual graph
generation at work36,844,632 and bytes223,261,940 against the unchanged
32M/128MiB caps. All nine dedicated refusal/source/executable/deadline/ownership
controls pass. The positive requirement is not skipped, xfailed or changed into
a refusal expectation. The earlier full-suite failure remains authoritative;
these scoped checks do not constitute a full engine/API/browser rerun.

The new metadata bounds alone cannot deduct from the root census. Complete
raw-derived financial source/private-allocation/lifetime/Compiled/Sim/output
bounds, remaining canonical comparison work, exact verified predecessor binding
and independently verified persisted cut publication remain open. Every other
original empirical, reviewed-policy, adoption and full-scale issue gate remains.
This local debug validation installs no release, deploys no worker and establishes
neither observed model accuracy nor accounting/regulatory approval.
