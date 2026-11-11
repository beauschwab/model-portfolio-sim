# Fixed commercial cut predecessor binding - 2026-11-08

The explicit `/models/cut-mixed-journals` endpoint now binds an ordinary completed
fixed commercial cut publication at its exact published cut. The server selects
the completed candidate from the prior executed request. The worker independently
replays the predecessor and matches the new ordered opening journal, day-cut reports
and unweighted loan states, including zero participation. Native producer authority
flags remain false; separate execution evidence records the modeled binding.

The existing private final SQL guard rechecks prior source rows and immutable
objects together with current request/source/executable, lease, attempt, token,
revision, cancellation and original deadline. Existing holding routes retain their
behavior. No Rust financial source or native binary changed.

## Validation

- 33 new API cases pass: 32 source-binding/replay/publication/intake cases and one
  separate advanced fee admission refusal. Normal and zero-allocation cases publish,
  retain object reachability and reopen after restart; future candidate acquisition
  changes the prospective journal while the exact prior cut remains unchanged.
- 93 adjacent holding/candidate/cut/envelope regressions pass in 304.99 seconds.
- 12 original cut publication and mutation regressions pass in 33.87 seconds,
  including the unchanged positive worker example.
- All 40 MCP tests, 11 architecture fixtures, Python/Rust/TypeScript boundary checks,
  native/HTTP/MCP generated contracts, TypeScript compilation and Vite build pass.
- The 12 installed native executables match the November 5 full-suite checkpoint.
  This change did not repeat that full engine/native/browser/API matrix. It validates
  the changed application boundary against those unchanged native artifacts.

The API tests ran on temporary SQLite workspaces. PostgreSQL was not exercised in
this checkpoint. The API suites report an existing Starlette/httpx deprecation
warning; the web build retains its existing large-chunk warning.

An exploratory fee-bearing positive fixture refused the existing advanced joined
admission before generation (31,881,320 work / 211,818,272 bytes). It is outside the
fixed route's supported domain. That refusal is retained as a separate negative
regression; the implementation and receipt explicitly do not claim fee adoption.
The initial root-directory pytest invocation failed import collection; all actual
API validation ran from `apps/api`. No successful result is inferred from either
initial failure.

Evidence: `artifacts/cut-prior-2026-11-08/validation-receipt.json`.
SHA-256: `19082a5c8c464645562b12a84a203e89b34a667252a8a87638f8b7410243270f`.
The receipt includes changed-file hashes, unchanged native artifact hashes, JUnit
receipts and captured validation logs. The 32-case terminal run has a retained
summary; the separate added fee case has its own JUnit/log.

## Remaining acceptance

See [the API contract](../models/commercial-cut-prior-opening.md). This tier is
exact-cut fixed-loan modeled publication. Advancing cuts, recursive predecessor
chains, advanced/fee owners, wider saved/cohort/strategy/accounting/FTP adoption,
observed behavioral/credit validation, reviewed policies and complete 60,000-position
joint graph/credit/solver/journal acceptance remain open under #15/#25.
No deployment, worker restart, saved-library rebuild or observed-bank reconciliation
is claimed. The branch includes main `01d0254`; local integration is separate from
pushing or deploying these changes.
