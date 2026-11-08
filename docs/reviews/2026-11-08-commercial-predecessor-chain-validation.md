# Bounded commercial predecessor chains: validation checkpoint

The local source checkpoint `c358e45` adds server-bound, independently replayed
predecessor chains and later cut dates to the original fixed commercial route.
Every ancestor retains raw input, persisted journal, gross opening, artifact and
SQL authority checks. Eight predecessors share cumulative input and prefix bounds.
The independently checked later opening comes from the actual predecessor journal.
Original source tails, zero allocations, OAS/EIR and financial tolerances remain.

## Complete regression and explicit supplements

The frozen source and twelve installed artifact hashes were checked before and
after every gate. These artifacts were unchanged from the November 5 release;
this checkpoint does not claim a rebuild or deployment.

| Gate | Result |
|---|---|
| Full engine | 4,147 passed; three skips in the initial run |
| Counted-work supplement | 13 native controls generated the missing receipt; all nine independent counted-work tests passed, including the initially skipped case |
| Remaining engine skips | Two Python parameterizations explicitly excluded by native ownership contracts |
| Full API | 1,517 passed, no skips; one dependency deprecation warning |
| Native crates | 1,734 tests and three doc tests passed; format and strict all-feature checks passed for six crates |
| Rust API | 30 tests, format and strict checks passed |
| MCP | 40 unit tests and ten API/integration tests passed |
| Boundaries/contracts | Architecture, eleven checker fixtures and native/HTTP/MCP generated checks passed |
| Web | Build passed; 105 browser cases passed initially, one timeout passed on an unchanged retry |

The streamed browser's `commercial_generated_fees` case initially timed out
waiting for its verified-publication badge. The isolated test database records a
successful job, with 119.332 seconds from enqueue to publication against the
120-second UI wait. A subsequent single-case run passed without source, financial
tolerance or timeout changes. Scheduling/render timing is a supported explanation,
not a proven root cause. The original failure and its log remain intact.

The initial 41-gate receipt therefore deliberately retains `all_passed: false`.
The separate retry and counted-work receipts document resolution of those two
validation gaps without rewriting the initial result.

Evidence is local under `artifacts/cut-chain-2026-11-08/`:

- `full-suite-receipt.json`: `84dd7f95cdf38ef6e526dc8b4419e6c2b999c4387059d62ba48cbe96673cf60e`.
- `browser-retry-receipt.json`: `627ace95f3efd0c6c3cd1ff46cdb854bb2d9b804c9284465a269f8240e4f4ce3`.
- `counted-validation-receipt.json`: `be4debe48d4d8b6156a2a08f1ef3dcaea1e07b070b9e82d5558e0d35387365cc`.
- `focused-validation-receipt.json`: `0834602cb27e83a8b2664208caddf6238e50d40cd58db173811518547edd19c7`.

## Scope still open

This checkpoint excludes the subsequent ordinary-fee capacity extension. It
exercises SQLite; it does not establish PostgreSQL, hosted or deployed acceptance.
Issues #15 and #25 remain open for their wider product adoption, complete mixed
scale, observed-data and reviewed-policy gates. Synthetic fixtures establish
implementation and independent parity, not observed predictive accuracy or policy
approval. No push, merge, deployment or production worker restart is implied.
