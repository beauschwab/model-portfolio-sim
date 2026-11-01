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

## Prepaid source serialization (2026-11-01)

`capture_prepaid` adds receipt version 2 without changing version-1 execution,
receipts or the complete JSONL source stream. A borrowing serializer adapter
funds each built-in JSON cell before the downstream serializer sees it. A string
or object key prepays one cell plus its UTF-8 length before escape scanning;
a finite native-domain number prepays one cell plus a 32-byte formatting upper.
Arrays and objects each prepay one cell before traversal. The existing writer
charge remains in full, including output escaping/forwarding and buffer growth;
the fixed bounded footer keeps its separately prepaid 2,048-unit allowance.

Transparent wrappers and option values retain their original representation.
Opaque RawValue/Number serialization, unbounded Display, nonfinite numbers,
integers outside the native JSON domain and depth above 128 reject. This covers
the audited built-in source record serializers. It cannot certify arbitrary work
hidden in caller-supplied Serialize implementations or private allocator/RSS
capacity. No API selector, Sim execution or production publication is added.

For version 2 the independently reconstructible formula is:

```text
source_traversal_work = sum(record JSON cell/string/numeric uppers)
capture_work = 2 * encoded_bytes + 16 * source_records + 2048 + source_traversal_work
```

The complete fixture's stream remains byte-identical: 27 records and 1,070,233
bytes, with all original loan/fee quotes and zero-weight sources retained.
Additional prepaid traversal is 1,510,239 units, capture work 3,653,185 and joined
reported component work 30,454,141. Retained component capacity remains
115,257,808 bytes; all caps and existing source charges remain unchanged.
Independent full-source replay, sixteen tampering controls and all 24 zero-weight
current sources pass for the version-2 capture as well. Sixty portable reference
tests retain version-1 behavior and independently verify the additional counter.

This closes pre-traversal funding for the selected source record encoder. The
current cut's complete source/private-allocation/Sim/output codec bounds,
repeated preparation/Value scans, predecessor binding and persisted publication
remain open. This component still has `complete_source_sim_codec_bounds=false`
and `publication_eligible=false`.

## Finite typed proof ownership (2026-11-01 follow-through)

Finite graph sources now serialize their complete sealed Physical or Pricing
owner directly. A Physical source no longer retains a second JSON projection;
Pricing bundles and their source proofs share the same immutable native result.
Original Pricing owners are released after their complete observation and anchor
checks, before current scenario owners are retained. Generated metadata/recipes
keep their existing separate representation. Factor-segment evidence is emitted
as a borrowed sequence rather than collecting a temporary descriptor vector.

The public JSON fields, numbers, cash, fee and recovery tails remain the same.
Direct struct serialization can change JSON object key emission order, so a new
encoded capture may have a different byte hash. Historical immutable artifacts
and their hashes are not rewritten. Independent verification must continue to
check the exact new bytes and complete decoded source, rather than substitute an
older capture's hash.

The refusal-only projection inventory may create one transient JSON view of a
typed source. Its cell/key/scalar construction is prepaid, it is dropped before
the next source, and the unchanged historical graph allowance still covers that
view together with its sealed owner. Observed Value capacities describe this
diagnostic temporary; they do not certify the typed source's allocator layout.

All ordinary and selected admission bounds remain unchanged. This removes a
retained duplicate representation without claiming a smaller complete source,
Sim or codec bound. The original public cut example still refuses at
36,844,632 work units and 223,261,940 retained-byte upper bound. Complete private
allocation/lifetime admission, predecessor binding and successful independently
verified cut publication remain required under issues 15 and 25.
