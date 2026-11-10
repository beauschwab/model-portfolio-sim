# Verified fixed commercial cut predecessors

`POST /models/cut-mixed-journals` binds a completed fixed
`commercial-cut-mixed-journal-1` job in the current authorized workspace to a new
native cut run at the published cut or a later date within its published complete
cashflow tail. The predecessor may itself have a verified predecessor. The request contains
`expected_revision`, `source_job_id`, integer `cut_ordinal`, the complete
`revised_candidate`, and optional `timeout` (at most 1800 seconds). Unknown fields,
duplicate JSON keys, nonfinite values and bodies above 2 MiB reject.

The server copies the predecessor's executed `revised_candidate` into the new
`completed_candidate`. Callers cannot supply completed candidates, opening money,
object descriptors or verification flags. The existing native source-prefix rules
apply to future changes and candidate additions. Both complete requests must still
fit the existing 32-million-work and 128-MiB requested-heap limits; prior acceptance
does not grant a larger allowance for a later combined request.

Before native execution, the worker rereads the predecessor's scoped SQL authority,
immutable request, actual executed envelope and persisted native result. The
independent fixed-cut verifier reconstructs raw resource allowances, every source,
completed monetary prefixes, original anchors, prospective journal and closing
balances. Producer resource counters or stored verification flags cannot authorize
execution. All consumed objects are retained in provenance and reread at publication.

At the same date, the new native result must exactly match the predecessor's ordered opening journal
(including prior P&L in its original GL), day-cut reports and unweighted loan states.
Zero participation does not remove loan principal, coupon lots or gross rights
from that comparison. The completed raw candidate is also exactly server-bound to
the previously executed revised candidate; matching net balances alone is insufficient.
The native cut still moves its own reconciled state and retains its original calendar.
No Python financial calculation enters production execution.

For a later cut, independent verification derives the expected opening from the
predecessor's verified complete journal. The target must exist in the published
daily path and precede its complete tail end. Native execution constructs and
resumes the actual cut state. Calendar, journal ordering/lineage, scalar types,
day-cut snapshots and gross loan state remain exact comparisons. Only accumulated
journal money uses the existing commercial reference comparison tolerance; no
financial tolerance is widened. Advancing beyond the published tail or backwards
rejects, including when the new candidate would otherwise extend that tail.

Separate `execution.prior_opening_verification`, schema
`commercial-cut-prior-opening-verification-1`, records verified modeled publication
and exact cut matching. Native producer authority flags remain false. The receipt
retains the prior job ID, attempt, token, owner, revision, request/result descriptors,
queued identity, original deadline and every consumed object. The source must have
finished within its original deadline. Later expiry of that historical deadline
is provenance; all new reads, replay, execution and publication obey the new
request's original deadline without a reset.

The original same-cut, ordinary-predecessor receipt remains version 1. Chains and
advancing cuts use `commercial-cut-prior-opening-verification-2`, retaining each
ancestor's recomputed evidence, published and matched dates, and the number of
verified predecessor publications. An advancing receipt explicitly reports
`exact_published_cut=false`, `published_cut_opening_matched=false` and
`published_journal_rolled_forward=true`; its opening matches the independently
verified journal at the later date. Stored receipts never grant authority: every
ancestor's raw source, native result and receipt are independently checked again.

The ordinary fee extension uses version 3 when the independently replayed predecessor
contains separately owned service/acquired-cost fee rights. It retains the version 2
chain/date evidence when applicable and reports `separate_fee_adoption=true` and
`gross_unweighted_fee_cut_matched=true`. All selected fee proofs must be objects;
null is not an optional verification shortcut. Ordinary no-fee versions 1 and 2
retain their historical meaning. This receipt does not grant transferred/generated
fee ownership or accounting-policy approval.

One chain admits at most eight predecessor publications, with no repeated job ID,
and at most 1,024 distinct source objects. Initial replay reads, decoded Parquet
bytes and logical Arrow bytes each have a cumulative 64 MiB allowance across all
ancestors; retained comparison prefixes have a cumulative 32 MiB allowance.
Read admission precedes object I/O, and decoded/logical checks precede whole-table
materialization. Each ancestor is replayed before loading its child's result,
retaining bounded comparison evidence rather than nested complete native results.
Final object rechecks retain the per-object bound and original job deadline.
These component allowances are not a process RSS guarantee.

The shared final SQL guard rechecks every ancestor's authority and objects alongside
the current worker's lease, token, attempt, cancellation, revision, request bytes,
source/executable identities and deadline. Restart and artifact reachability use
the existing immutable storage contracts. The endpoint is a durable compute
operation in the generated MCP catalog and has a generated HTTP request contract.

## Supported scope and remaining gates

This tier binds fixed sequential cut publications, including bounded verified
chains and advancing dates as described above. The ordinary fee-bearing extension
passes targeted rebuilt-artifact publication and advancing-chain checks; full
regression remains required. See [its capacity and validation record](commercial-fixed-fee-cut.md).
Indexed/generated/exercise and transferred fee owners still require separate admission. General mixed books, saved/cohort/strategy/accounting/FTP adoption and the complete
60,000-position joint acceptance remain separate work. Synthetic fixtures and
modeled journal reconciliation do not establish observed accuracy, observed bank
reconciliation or reviewed accounting/regulatory policy approval.

The raw `/models/runs` route remains available with its unverified predecessor
flags. Source-bound publication requires the explicit endpoint above. No deployment,
worker restart or saved-library rebuild is implied by local validation.
