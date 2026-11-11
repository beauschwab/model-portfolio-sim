# Commercial main integration and borrowed cut comparisons

This is an incremental engineering checkpoint for issues #15 and #25. It does
not close either issue or replace their complete acceptance criteria.

## Source integration

The isolated `codex/commercial-oct25-sync` branch integrates commercial checkpoint
`c70058628b427751baa9d6389cb5f8f73b6b7deb` with main
`01d0254aede575be00e49f7dc8d3368439cc2e98` at merge commit
`77965dd7d0152c2ac1e3ca97976a38ce21606a19`. The generated MCP reference conflict
was resolved by regeneration from the combined documents. Current main's native
service, lifecycle, independent verifier, hedge and scale work is retained.

The new lifecycle owner adds an argument to the shared daily simulator. The
commercial cut constructor explicitly supplies `None` for that owner and keeps
the commercial owner in its existing argument. The first native compilation
identified this integration error; it was repaired before the recorded native
validation run. The shared primary checkout's unrelated `.serena/project.yml`
and `PRODUCT.md` edits were left untouched.

## Cut comparison change

`replay/daily/cut.rs` previously converted both complete specifications to JSON
Value trees, cloned the original position JSON into the revised tree, and cloned
the retained typed positions for the separate prefix comparison. It now uses a
private borrowed specification view and a borrowed position slice. The view
retains all nine specification fields. Exhaustive `Spec` destructuring requires
review when a field is added; nested DTOs use their complete existing serializers.

The immutable comparison still substitutes the original positions into the revised
specification and separately compares every retained position. Future positions
remain subject to the existing ownership, zero-opening, acquisition and calendar
checks. The existing comparison-size guard and bounded canonical encoders remain.
No source-work deduction, capacity increase, financial calculation, tolerance,
original OAS/EIR, journal order or public schema changes are introduced.

Four regression tests retain the previous Value/clone comparison as a test-only
oracle. They cover 117 independent scalar mutations across all populated field
families, escaped and Unicode labels, full wire content, borrowed container
identity, appended and missing positions, container changes, array ordering,
empty inputs, signed zero, finite floating extremes, the unchanged component
capacity refusal and cancellation. Existing complete-state cut replay tests also
remain in the full ledger suite.

## Validation

Evidence lives in `artifacts/commercial-oct25-sync/` in the isolated checkout.
The native runner hashes its tracked native sources, examples and model fixtures
before and after every gate. Its logs and receipt distinguish this debug source
checkpoint from an installed release.

- Ledger: 225 tests, strict all-target/all-feature Clippy, formatting and doc tests.
- Shared model core: 448 tests, strict Clippy, formatting and doc tests.
- Pricing: 698 tests. Decision/workflow: 213 tests. Independent lifecycle verifier:
  15 tests. All three also pass formatting, strict Clippy and doc-test gates.
- Total: 1,599 native tests and twenty native gates. The native receipt hash is
  `5be3faf2240046777669aaf85bb104f55ad0047a6ad8d477396d8f4618ab6fd0`.
- Architecture: Python/Rust and TypeScript boundaries, ten independent fixtures.
- Native ABI, HTTP and MCP generated contracts: pass, with 94 MCP operations.
- MCP: 40 tests. API cut raw admission and legacy/native worker routing: six tests.
- Web TypeScript/Vite build: pass, with the existing chunk-size warning.

The first architecture attempt lacked TypeScript dependencies in the isolated
checkout. The successful run reads existing matching-lock dependencies. The first
MCP collection lacked MCP/jsonschema dependencies; the successful run uses the
declared MCP package and its test dependencies. Failed setup logs are retained.

## Remaining acceptance

The public raw cut publication requirement is still positive and still open.
This change removes actual comparison copies but establishes no new complete
raw-source/private-memory/Sim/codec bound. Existing selected census flags and
conservative admission charges are unchanged. No refusal is reclassified as a
successful financial run or an expected-publication pass.

Required follow-through remains complete source and runtime admission within the
existing limits, exact predecessor binding, positive raw and persisted cut
publication with all final SQL/source/artifact/deadline guards, and a fresh matching
installed release with the complete engine/API/browser gates. Main's liquidity
scale acceptance remains a separate lane from the complete mixed product
graph/credit/solver/journal acceptance required by #25. Broader product adoption,
observed holdouts and reviewed policy evidence remain required. The invented
histories remain explicitly synthetic and their example policies unreviewed.
This checkpoint does not deploy, restart workers or close either issue.
