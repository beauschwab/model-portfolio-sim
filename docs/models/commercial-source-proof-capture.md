# Commercial source proof capture component

This native Rust component captures complete source projection records during
the existing finite commercial preparation. It is a prerequisite for removing
the repeated preparation/final-Value transport in the mixed-cut route. It is
**unpublished staging**, with no JSON request selector, API route, cut execution
permit or production adoption.

## Original proof lifetime

`commercial_graph::analyze` previously retained current physical/pricing sources
in its result but dropped its local original Pricing sources after calibration.
The final original proofs include original/current factor projections, carry and
complete cash/recovery tails, with separately reconciled original fee projections.
They must survive in a future bounded stream even though the graph no longer
needs their financial owners.

`analyze_with_source_observer` observes those already-created proofs after both
original loan and fee quotes reconcile, including final stopped sources when the
graph has an option. It then observes current sources after each scenario's
unchanged metric calculation. `prepare_with_source_observer` adds unit identity.
The original graph/unit entrypoints continue through the same implementation
without an observer. No new projection, calibration, pricing or financial query
is performed to produce an observation.

`SourceProof` has a private borrowed source reference and implements serialization
only. It has no public constructor, clone, deserializer, dereference or financial
query interface. The higher-ranked callback lifetime prevents retaining its
borrow beyond the invocation. Observers distinguish original Pricing, original
Physical and current scopes independently of scenario/role labels. Position,
scenario, history and unit indexes identify every use; zero weights are retained.
The observer entrypoints reject generated policies before financial preparation;
the historical generated graph remains available through its existing entrypoint.

Original stopped typed economic owners can already have been dropped inside the
stopping implementation. The observation borrows the complete existing
`NativeSource.projection` Value proof. It does not claim to retain or expose that
earlier typed owner. Original acquisition/exercise receipts remain distinct proof
families rather than being implied by a source observation.

## Selected cut capture

`commercial::cut_proof_capture::capture` composes the observer with the existing
finite candidate and Compiled preparation. It requires the explicit counted
finite-transfer source authority, a positive cut, a complete shared horizon and
a future appended candidate. Its initial subset rejects generated/optional
sources and original Physical acquisition proof families before staging. Loan
and separate fee sources remain complete; none is replaced by a summary.

The completed preparation remains live while the revised preparation is built.
Their source admission charges are unchanged. The revised reservation also
includes capture work already executed for the completed preparation. A native
remaining-work ticket begins before either financial graph is built.

Each JSONL source record carries the completed/revised side, native use identity
and the complete source request, cash, projection and selected fee projection.
A small footer records complete source coverage with `publication_eligible=false`
and `cut_sim_executed=false`. No complete Preparation or result Value is encoded
and parsed just to transport these source records.

The staging writer limits output to 4 MiB and checks the remaining work ticket
before forwarding bytes or growing the external buffer. Its logical charge is
two units per encoded byte, covering emitted bytes and a cell-count upper, plus
sixteen units per record. Footer serialization reserves a further 2,048 units
before its bounded 1 KiB encoding. A successful component receipt therefore has:

```text
capture_work = 2 * encoded_bytes + 16 * source_records + 2048
reported_work = unchanged_source_preparation + 8 * typed_request_bytes + capture_work
```

The component retains the existing source/compiled allowances and an additional
staging growth/workspace reserve, refusing reported work above 32 million or
reported retained capacity above 128 MiB. These are component dimensions.
Serde has performed some traversal/formatting/escape scanning before calling a
writer: this byte/cell charge is **not** a complete pre-traversal serializer CPU
or allocator proof. Arbitrary caller transport/writer ownership and aggregate
RSS are not certified. Full source/private-allocation/Sim/codec flags remain false.

Cancellation, failed writes, exhausted tickets, byte-limit errors and failed
flushes return no capture receipt. Unpublished partial or completed staging bytes
can remain with the caller; the footer alone cannot authorize publication. A
future durable route still needs exit/status checks, immutable source/artifact
identity, independent persisted verification and all publication fences.

## Genuine fixture evidence and remaining work

The full short secondary-transfer fixture captures **27** records: three original
Pricing, six current Physical and six each of Pricing/down/up. All source groups,
zero allocations, original loan/fee anchors and complete cash/rights/recovery
tails remain represented. The stream is 1,070,233 bytes. Logical capture work is
2,142,946; unchanged source preparation is 24,846,612 and typed-prefix reserve
1,954,344, for 28,943,902 reported work. Reported retained component capacity is
115,257,808 bytes. This is source capture only; no cut Sim runs.

The independent analysis checker validates exact raw role/order/identity and
footer/counter/false-claim metadata before recomputing every source with the
existing independent credit, cash, accounting and fee references. Original loan
and separate fee quotes are independently reconciled. This establishes captured
source economics, not empirical accuracy or completed source/Sim/resource proofs.
The retained specimen has no realized defaults or recovery receipts; complete
declared policies and empty recovery tails are replayed, and inserted recovery
money is rejected. It is not a positive realized-default/recovery control.

Fresh native validation, independent tampering and zero-weight controls, artifact
identities and acceptance limits are recorded in
[the checkpoint review](../reviews/2026-10-30-commercial-source-proof-capture.md).

Next gates remain complete raw-derived/private allocation and pre-traversal
serializer accounting, actual repeated transport removal, complete native
prefix/Sim/output work under the unchanged cut limits, exact published
predecessor binding, positive independently persisted cut publication, matching
installed-artifact acceptance and every original requirement under issues #15
and #25. Invented histories remain synthetic and example policies unreviewed.
