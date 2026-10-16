# Verified modeled candidate prior openings

`POST /models/candidate-holding-journals` binds an ordinary completed
`commercial-candidate-holding-journal-1` publication in the current authorized
workspace. Its strict request is:

```json
{
  "expected_revision": 0,
  "source_job_id": "completed-candidate-holding-job-id",
  "cut_ordinal": 738901,
  "revised_candidate": {"schema": "commercial-candidate-journal-1"},
  "timeout": 1800.0
}
```

The abbreviated `revised_candidate` must contain the complete finite candidate
request; the existing candidate holding example supplies its full shape. Only the
source's exact published cut is admitted. The server copies the source's actually
executed `revised_candidate` into the new native `completed_candidate`. The caller
cannot supply completed sources, opening balances, object descriptors or authority
flags. A predecessor with its own prior-publication binding is rejected; recursive
chains and advancing the published cut are outside this tier.

Before native execution, the worker reopens the immutable historical raw request,
original executed command (including its exact integer millisecond budget), native
result and every consumed persisted table. The independent candidate holding
reference replays both complete candidate loan/fee sources, original purchase
OAS/EIR/acquired fee basis, complete ordered journals and prospective window.
The new complete-origin native run must retain the same opening, signed prior P&L
and every ordered dated source/report/posting row through the cut. Unweighted
loan and fee snapshots and lineage remain bound when participation is zero; net
GL equality alone cannot prove gross rights or acquired fee basis equality.
Future-only revisions still obey the existing native raw/economic prefix rules.

The native `prior_publication_verified` flag stays false. Separate
`execution.prior_opening_verification`, schema
`commercial-candidate-prior-opening-verification-1`, records modeled provenance
only after independent replay and exact matching. It records the predecessor SQL
row, owner/token/attempt/revision, immutable request/result and consumed object
hashes, queued identity and original source deadline. The historical source must
have completed within its original budget; an expired historical deadline remains
provenance. All new reads, materialization, computation, independent replay and
publication use the new HTTP request's original deadline.

The shared private final SQL guard checks the current request, output, worker
lease/token/attempt/workspace revision, cancellation, executable/source identities
and original deadline. It rechecks predecessor authority and every consumed source
object under that same completion transaction. Rehashed monetary source changes,
forged evidence, late source-row changes or late object replacement cannot authorize
publication. Source objects remain reachable through retained descriptors and the
completed source job; reopening results after restart preserves both receipts.

No existing bound or tolerance is widened. HTTP and materialized native inputs
remain bounded to 2 MiB, native output to 32 MiB. The existing shared source reader
retains its 32 MiB object and cumulative 64 MiB serialized/decoded/logical bounds;
prior evidence retains at most 1,024 unique descriptors and 64 MiB referenced
objects, with a 32 MiB prefix envelope. Native candidate holding admission remains
32 million combined work units and 128 MiB retained financial capacity, with
separate 8 MiB completed/revised/window components. These are component/phase
limits, not an aggregate RSS guarantee. The historical commercial prior-opening
route keeps its original receipt, limits and financial behavior.

This verifies a modeled publication used by complete-origin replay. It does not
claim observed bank reconciliation, a cut-started mixed Sim, secondary conveyed
rights, empirical accuracy, accounting approval or regulatory eligibility. Those
flags remain false, and broader saved/cohort/mixed adoption, observed/reviewed
evidence and complete 60,000-position acceptance remain required under issues
15/25. Matching artifacts, application/worker restart and explicit saved-library
rebuilds remain deployment requirements; local checks do not deploy services.
