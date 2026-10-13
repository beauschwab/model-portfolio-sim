# Commercial loan and separate fee journal: installed local validation

Local 0.29.17 implements the next fee integration stage under issue #15 and tracker #25.
The explicit `commercial-fee-journal-1` controller reuses each generated physical
loan owner, binds separate service-fee dispositions to its actual native exit, and
posts loan and fee money in one existing daily Sim. This review accepts that bounded
stage. It does not close the full issue or establish observed/accounting approval.

## Implemented behavior

- Separate original-tranche fee sources retain service/classification reviews,
  earning, cash, gross receivable/payable and deferred/prepaid ownership. Loan
  principal, original cost/EIR, credit draws and carrying value remain unchanged.
- Default/prepayment and explicit reviewed maturity events own fee cancellation,
  unpaid-earned waiver, prepaid refunds and independently supplied recovery tails.
  Loan LGD and coupon-recovery fractions cannot supply fee recoveries. The binder
  recognizes sealed exercise identities, but this public loan request does not yet
  adopt an exercise/candidate/holding source.
- Actual native clocks and complete tails survive principal settlement. Exit
  dispositions precede coincident service/cash; loan groups precede fee groups at
  the same clock. Source weight and allocation are applied once. Zero participation
  retains complete unweighted source policy.
- Integer-date attribution checks the retained native year directly. A multiplication
  that rounds a just-later year onto an integer cannot post that future event today.
  The correction covers commercial daily, fee, unit, candidate and treasury mappings.
- Protocol 7 adds fee lineage and gross daily/closing snapshots. Historical protocol 6
  and earlier routes retain their schemas. Independent raw loan/fee, ordered shared
  journal, gross/report and persisted replay gate durable publication, with original
  deadline, source/executable/object and final SQL token/lease/revision/attempt fences.

See [the source contract](../models/commercial-fee-journal.md) for formulas, schemas,
limits and exact exclusions. Joined admission uses the final complete loan recovery
calendar when charging fee reconciliation. Component budgets remain separate from
aggregate RSS guarantees.

## Accepted evidence

The immutable local receipt is
`artifacts/issue #15-fee-journal-validation-final/receipt.json` and the accompanying
`validated-source.zip`. Its1193-file source freeze is
`a00e49eb00feab174240c72b357039700bf1fc2bdbce65cb149cf0ce6e1697b2`.
Receipt SHA256 is `e19729816fce76da59831e11ff4e188d6021b0c01b8da15af72bbd3f86704964`;
archive SHA256 is `1248063880a703bf92474e0baf6fe69bb873f09829ef8999ae982448d3a1d2f5`.
The source archive includes existing shared-checkout dependencies and inputs; it is
not a scoped commit change set.

Fresh installed checks pass:

| Gate | Result |
|---|---|
| Native tests |1,236: core 389, product 503, ledger 197, decision 147 |
| Strict native Clippy | All four crates, all targets/features; no suppressed warning |
| Decision documentation tests | Pass |
| Dedicated fee engine/API |73/20, no skips |
| Full engine |2,671 passed; two existing reference-context skips |
| Full API |1,185 passed; one optional MCP module skip and existing Starlette warning |
| MCP and real MCP/API/worker tests |40 and 2 passed; dedicated integration has no skips |
| Architecture | Python/Rust/TypeScript checks and 8 checker tests pass; 110 existing cycle edges |
| Generated contracts | HTTP/native ABI/MCP verified |
| Web | Build passes; existing 500kB chunk warning |
| Historical output controls | Four complete commercial public outputs byte-identical |
| Final source/artifact checks |1,193 source files, eight release artifacts, logs and packaged operator bytes verified |

The zero-EIR par-loan control retains G100 while a separate fee 5 adds 5 to closing
cash/equity. Integer money JSON retains original raw identity and independent replay.
Fractional recovery splits 0.1/0.2/0.7 recover exactly the separately waived 0.3 in the
accepted control. Other controls cover positive/negative rights, real fractional
default clocks, independent loan LGD, prepaid refunds, coincident maturity/service,
full loan recovery calendars, source-order multiple paths, zero/half allocation,
fresh native ownership, invalid original source and rehashed persistence tampering.

The preliminary freeze in `artifacts/issue #15-fee-journal-validation` contains the
strict ledger lint failure for its staging tuple signature. The final successor
introduces a named type without changing financial statements/runtime fields. All
affected artifacts and every native gate were rebuilt/rechecked; the final complete
engine/API suites use the corrected matching bundle. Preliminary evidence is retained.

## Remaining issue #15 acceptance

Fee Pricing graphs, original acquisition allocation, acquired units, capital/FTP,
exercise/candidate/holding and draw/secondary fee rights, and earned refunds remain.
Daily in-arrears indexed earning, generated draw/review/streaming and convergence,
complete saved-book/cohort/mixed strategy adoption, observed histories and reviewed
accounting/regulatory policies, and 60,000-position joint graph/credit/solver/journal
acceptance remain required. The complete criteria in
[the implementation ledger](../models/implementation-plan.md) are retained.

No commit, push, deployment, live database rewrite or API/worker restart was performed.
Additive execution requires matching artifacts and explicit selected-library/session
rebuilding; deployed services require their normal restart procedure. Synthetic
mathematical controls do not establish empirical accuracy or regulatory compliance.
