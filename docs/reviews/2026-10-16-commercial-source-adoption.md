# Commercial sources and adoption: local acceptance

This local 0.29.19 stage includes main `93ef58a99689ee0dd28ffc10e9bd0ac3dada1e20` and starts from
`54833c162b020fdcb3886ed3356fcb952e923d9c`. It completes the bounded implementation listed below.
**Issue 15 and tracker 25 remain open.** Original scope and acceptance criteria
remain in [the implementation plan](../models/implementation-plan.md) and
[completion ledger](../models/commercial-completion-plan.md).

## Behavior

Separate service fees retain their own quote, discount spread, acquired cost,
gross rights and earnings outside original loan OAS/EIR and external NII. Joint
finite exercise compares values in one common loan numeraire; independent
unequal-spread controls prevent a locally summed value from changing the root
optimal decision. Unit evaluation reduces retained fee coefficients natively
without repricing or changing the external-NII solver objective.

Finite candidate fees enter the existing daily Sim once. Complete-origin holding
windows preserve raw and ordered monetary prefixes, gross loan/fee stocks and
prior P&L. The source-ID binding independently verifies an ordinary completed
modeled predecessor at its exact published cut and retains final SQL/source/
artifact authority. This is modeled provenance, not observed bank evidence or a
Sim initialized at the cut. Reviewed fee capital and daily FTP retain separate
GL weights, channels, complete tails and supplied regulatory policies.

Published fixing prefixes drive indexed earning at actual information clocks.
Pending legal/recognized coupon claims and reviewed late default/recovery tails
remain separate. Explicit generated Physical path catalogs retain canonical
draw sources, original path anchors and reviewed actions without fitting Q from
P outcomes. The compact generated candidate route retains one expanded source
at a time and publishes bounded partitions through independent raw and persisted
replay. Admission refusals remain part of its contract.

Explicit linear/quadratic/cubic continuation bases fit training only. Historical
quadratic order, omitted fields and historical admission receipts remain intact.
Synthetic normal-integral controls distinguish learned-policy approximation,
evaluation sampling and exact financial replay. They do not establish general
convergence or observed predictive accuracy.

Browser-shaped requests retain opaque raw ledger specifications exactly, while
declared supplied-curve and financial DTO fields use their native scalar types.
Integer clocks, booleans, signed zero and mutated source money remain strict
identity checks. Strategy browser checks decode the actual Arrow envelope.

## Accepted evidence

All matching official native artifacts were rebuilt from the retained native
freeze. Native tests: **1299**, with strict all-target/
all-feature Clippy for all four financial crates and decision documentation.

| Gate | Result |
|---|---|
| Full engine | 2998 passed, 2 skipped in 1029.78s (0:17:09) |
| Full API | 1311 passed, 1 skipped, 1 warning in 2810.88s (0:46:50) |
| MCP | 40 passed in 9.03s |
| Real MCP/API/worker | 2 passed, 1 warning in 9.14s |
| Durable browser journeys | 24 passed (3.3m) |

Architecture and its checker tests, native/HTTP/MCP generated contracts, web
build and exact operator package bytes pass. Four complete historical native
outputs are byte-identical; two complete historical fee results match canonical
JSON and explicit zero/false settings retain historical fee economics/admission.
All eight artifact hashes remain unchanged through acceptance. Logs retain any
disclosed skips and dependency warnings. No fast-math or tolerance relaxation
was introduced. Component/phase bounds are not an aggregate RSS guarantee.

The first full API run exposed an expired lease in a synchronous test helper
which bypasses the production worker heartbeat. Only that fixture's reservation
changed from60s to300s, followed by a complete API rerun. An exact source delta
proves all production, native, engine, browser and contract bytes unchanged;
their passing runs remain applicable. Production leases, heartbeats, deadlines
and publication ownership checks are unchanged.

## Remaining

Generated fees, draw/secondary fee rights, secondary acquisition and seasoned
coefficients, cut-started mixed owners, full saved/cohort/backbook adoption,
broader generated convergence and complete 60,000-position combined acceptance
remain required. Observed histories and reviewed policies have not been supplied.
Local synthetic/parity evidence cannot close those gates. Deployment/restarts
and saved-library/session rebuilding are separate operational actions.

The receipt, exact source archive, commands and logs are retained locally under
`artifacts/issue15-commercial-integration-validation/`. No push or deployment is
part of this acceptance.
