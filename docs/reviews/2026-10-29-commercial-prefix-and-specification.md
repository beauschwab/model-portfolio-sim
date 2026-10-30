# Borrowed commercial prefixes and complete specification diagnostics

This continues issues #15 and #25 from local `43ec508`. It preserves their full
acceptance criteria and the positive durable cut-publication requirement.

## Raw history comparison

The holding and mixed-cut paths now share private borrowed comparison views in
`candidate_prefix.rs`. They avoid cloning the complete commercial graph, round
tripping both candidate requests through JSON, cloning the folded future candidate,
and copying fee policies/events just to substitute retained histories. Native
continuation exposes a borrowed-source validation entrypoint so the joint caller
also avoids copying both complete loan sources into a temporary request.

The views exhaustively destructure each mapped DTO and use its complete nested
serializers. Original sources, pricing proofs, histories, weights, quotes, costs,
loan/fee anchors, review identities and original field omission policies remain
represented. The original reporting-grid/horizon fold and future acquisition
boundary remain unchanged. Future changes are still checked against the complete
observed loan/fee prefix. Interactive evaluation and financial calculations are
unchanged. No admission charge or resource limit is reduced.

A test-only oracle freezes the previous holding, mixed-cut fold and continuation
implementations from `43ec508`. Three fixture matrices exercise 3,608 supported
DTO mutations: holding 1,158, public cut 809 and secondary cut 1,641. They select
representative declaration paths while retaining each named P/Q lane. Exact
results/errors agree, including 235 accepted prospective/irrelevant mutations and
3,373 refusals. Additional controls retain all-zero weights, immutable anchors,
acquisition/source clocks, actual future fee events/releases, omitted defaults,
JSON number categories, signed zero and active cancellation. These are raw
comparison controls, not financial publication acceptance.

## Immutable specification diagnostic

`cut_spec.rs` maps every field of Spec, Account, Position, NettingSet, Policy,
Scenario and Cashflow. It checks complete immutable header and existing-position
identity, finite f64 bit patterns, ordered containers and the existing raw future
AC/zero-opening/collateral conditions. Visits, field discriminants and UTF-8 scans
are charged before traversal under the existing remaining-work ticket. String
comparisons check cancellation in bounded chunks. The implementation borrows its
inputs and retains fixed-size counters without a JSON tree or copied positions.

The composed prefix diagnostic additionally checks exact compiled position
coverage, unchanged existing ownership and exclusively owned appended positions.
Its `immutable_specification_verified` subgate is now true only after these
checks. Standalone specification reports explicitly retain
`compiled_ownership_verified=false`; only composition with actual Compiled owners
sets it true. Source binding, fee identities, future source dates and ordered
observed groups retain their existing checks. Complete Sim and raw-source capacity
flags remain false; the source-inventory completion function still refuses.

## Source-view lifetime

`SourceAdapter::views()` now returns a borrowing iterator instead of allocating a
temporary vector at every earned-factor, entitlement, pending/canonical, terminal
and cash lookup. It preserves base-first order and every index offset contributed
by inactive draw sources. The test-only old eager builder checks source pointer,
audit/cash/segment offsets and tranche identity for all active subsets.

The index control joins independently admitted source objects solely to inspect
their index spaces. Its earlier combined two-draw financial request hit the
existing carry preflight and that failure is retained. It does not establish
two-draw financial admission or waive a source-cost bound.

For the earned/entitlement component, the existing prepared bound gives
`Q = 2 * P * (I + B + J + 1)` queries and `A <= Q * S` emitted atoms. The old code
therefore constructed at most `Q + A <= Q * (S + 1)` temporary view vectors in
this component alone. Even selected transfers, which reject draws and have D=0,
allocated their base view on every call. The iterator removes those allocations.
This does not bound all callback strings, factor atoms, carry/book output or
allocator overhead, and grants no source-work deduction or aggregate RSS claim.

## Validation and remaining work

Native source/fixture hashes, complete gate logs and the final receipt are retained
in `artifacts/commercial-oct29-native-v2/`. Preliminary failures and targeted logs
remain in `artifacts/commercial-oct29-completion/`. All twenty native formatting,
all-target test, strict all-feature Clippy and doc-test gates pass. Counts:
ledger 234, shared core 448, pricing 699, decision/workflow 222, independent
verifier 15: **1,618 native tests**. Native receipt:
`e80b6e2eb0a7b6a47961fbc7113d2fc2b86ec2f3aefaf8150544815613cf0514`.
The tested base is `43ec508`; its additional staged native files are included in
the recorded source/fixture hashes. These are debug source gates.

The Windows architecture checker also now reads Rust source/manifests as UTF-8
explicitly. A Unicode fixture still detects forbidden application imports. The
architecture checks and eleven fixtures, native/HTTP contracts and six selected
API transport/routing tests pass. The 94-operation MCP contract and all forty MCP
tests also pass. No fresh installed engine/API/browser or 60,000-position combined
product acceptance is claimed.

Complete raw-source/private-allocation/Sim/codec bounds, exact published
predecessor binding and positive independent raw/persisted cut publication remain
open. Matching installed artifacts and full engine/API/browser acceptance remain
required. The invented histories remain synthetic and their policies unreviewed;
observed model accuracy, professional approval, wider product adoption and combined
product-scale acceptance have not been established. No deployment, worker restart
or issue closure is claimed by this source checkpoint.
