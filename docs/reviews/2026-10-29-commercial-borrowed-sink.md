# Borrowed daily output frames and explicit cut Sink capacity

This continues commercial issues #15 and #25 from local `45eceae`. It removes
an actual duplicate owner on the failing cut route. The selected request's
reported memory component now fits 128 MiB; complete source/private-allocation/
Sim/codec proofs and positive persisted publication remain open.

## Output ownership and compatibility

Daily `Sink::flush` previously constructed a new `serde_json::Value` frame from
borrowed rows. That deep-cloned the complete row array, nested objects and strings
while the original rows stayed retained. `RowMessage` now borrows the rows and
table name directly. Its declared `rows, table` order preserves the historical
sorted JSON object order. Journal frames already borrowed their columns and
dictionary. The shared 16,384-record flush threshold, reduction order, dictionary
suffixes, row clearing and write-error ordering are unchanged.

Frozen pre-change controls compare complete wire bytes, state and errors for
empty/nested rows, signed zero, numeric categories/extremes, escaped/Unicode/long
strings, nine write limits, repeated flushes and two automatic mixed row/journal
threshold crossings. No financial formula, OAS/EIR, tolerance or source/history
selection changes.

## Versioned logical component accounting

New output inventories select `commercial-cut-output-inventory-2` and include
`commercial-borrowed-sink-memory-1`. Historical inventory version 1 retains its
original `40E` clone reservation and refusal. Its actual pre-change native census
is frozen as a fixture and independently checked. The outer request/receipt
contracts and historical public omitted-policy refusal are unchanged.

Let E be the complete prefix plus visible decoded-table encoded upper, N each
table's complete row upper, D the resend-expanded dictionary string-cell upper,
and S its escaped string bytes. All arithmetic is checked before returning an
inventory. Version 2 retains the existing `20E` logical-Value convention and adds:

- `32 * sum(3 * min(16384, N) + 8)` for nonempty ordinary row buffers, separately
  for prefix and visible tables. Duplicate names conservatively reserve both.
- `64 * (3 * min(16384, total journal rows) + 8)` for nonempty journal columns.
  This reserves transient relocation of all eight columns simultaneously.
- `2S` for the separately owned dictionary and hash-map string payloads, plus
  `24 * (3D + 8) + 64 * (8D + 32)` for vector/hash-table buffers and relocation.
- `4096 * (prefix tables + visible tables + 1)` plus twice the nonjournal table
  name UTF-8 bytes for table nodes, headers and retained/temporary names.
- 64 KiB writer/frame workspace, including the pinned 8 KiB writer buffer.

The pinned 64-bit layouts (`usize=8`, `Value=32`, `String=24`) are required; other
layouts refuse this selected census. The growth tests exercise the pinned vector
and hash-map paths through 16,384 elements and retained high-water clears.
`3N+8` includes old and new buffers during relocation; merely halving `40E`
would omit those owners. The implementation references the pinned
[Rust RawVec](https://raw.githubusercontent.com/rust-lang/rust/1.93.1/library/alloc/src/raw_vec/mod.rs),
[B-tree layout](https://raw.githubusercontent.com/rust-lang/rust/1.93.1/library/alloc/src/collections/btree/node.rs)
and [hashbrown growth](https://raw.githubusercontent.com/rust-lang/hashbrown/v0.16.1/src/raw/mod.rs).
The Python reference independently reconstructs these formulas, rejects component
and total overflow, and retains historical version-1 arithmetic. The native helper
returns its paid table-scan work for the unticketed diagnostic fallback as well;
ticketed selected admission uses the actual charged work and never scans names
again just to count that traversal.

These are logical component allowances. `20E` is not a new proof for arbitrary
Value allocator padding; writer workspace is not a complete native stack bound.
The component does not include all private simulator/report/source allocations
and does not establish an aggregate RSS guarantee or publication authority.

## Genuine selected fixture result

The complete short secondary-transfer request retains all 27 source roles, both
named runs, original P/Q sources and anchors, surviving and appended candidates,
zero-weight controls, fee and loan groups and complete cash/rights tails.

| Sink component | Bytes |
|---|---:|
| Existing single row-tree convention | 19,420,400 |
| Ordinary row-vector capacities and relocation | 89,216 |
| Journal vector capacities and relocation | 99,584 |
| Dictionary string payload owners | 45,008 |
| Dictionary containers and relocation | 565,216 |
| Table containers and names | 107,138 |
| Writer/frame workspace | 65,536 |
| **Total** | **20,392,098** |

The reported coexisting source phase falls from 150,532,884 to **132,084,182
bytes**, below 134,217,728 by 2,133,546 bytes. This is a census result, not measured
RSS. Complete output table shapes, dictionary/flush shapes, source bounds,
preparation receipt sizes and ledger component dimensions remain exactly equal
to the historical fixture. Added explicit descriptors and paid structural visits
raise root work slightly from 65,033,611 to **65,036,571**. The independent ledger
guard remains **11,343,032** work / **2,876,652** bytes. Both work gates still
refuse before Sim execution. No former work is deducted or cap increased.

## Validation and remaining work

Preliminary evidence is retained in `artifacts/commercial-cut-batch-2026-10-29/`,
including the original and revised actual native censuses, frozen byte/error
controls and setup/Clippy failures. The `-v2/` run was invalidated by the subsequent
fallback accounting fix; it is not final evidence. The matching final gates,
source inventory, debug standalone binary identity and validation receipt live in
`artifacts/commercial-cut-batch-2026-10-29-v3/`.
Changed ledger and dependent decision/workflow source gates cover formatting,
all targets, strict all-feature Clippy and doc tests: **464 fresh native tests**
(ledger 242, decision/workflow 222) and all eight gates pass. **84 focused cut,
source, raw-census and metadata controls** pass against the fresh standalone debug
executable, including positive candidate-source journal replay and the cut's
required refusal. Architecture/eleven fixtures, native/HTTP/MCP contracts and all
forty MCP tests pass. Validation receipt:
`808033f3a52a98c8f252b5eb57bdb40a5bb30737efd6b0605e59020f60d60ad4`.
Core, pricing and independent verifier sources are unchanged from the separately
retained 1,618-test receipt;
their exact source hashes are checked rather than rerunning those suites.
Standalone cut transport uses an explicitly selected fresh debug executable;
this is not installed release acceptance.

Remaining engineering gates are complete raw-source/private-allocation/Sim/codec
admission within all unchanged caps, removal of actual duplicated receipt/Value
and canonical comparison work with complete replacement proofs, exact published
predecessor binding, and positive raw/persisted cut publication. Complete installed
engine/API/browser and combined mixed-product scale acceptance remain required.
Invented histories remain synthetic, example policies unreviewed, and the full
issues remain open. No push, deployment or worker restart is part of this stage.

The current typed prefix diagnostic is not a drop-in canonical comparison
replacement: it rejects reviewed credit snapshots and source strings longer than
1,024 UTF-8 bytes which the existing canonical route can compare. Those explicit
subset refusals and full work/capacity obligations must be resolved before using
that diagnostic to replace production comparison work. The completed immutable
specification subgate does not authorize deducting the full Compiled comparison
reservation. Root transport still repeats complete preparation/Value scans;
retaining complete original source/proof lifetimes in a bounded stream remains
the larger work-budget task.
