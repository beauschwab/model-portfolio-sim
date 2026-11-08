# Verified fixed commercial cut predecessors

`POST /models/cut-mixed-journals` binds a completed ordinary
`commercial-cut-mixed-journal-1` job in the current authorized workspace to a new
native cut run at the same exact published cut. The request contains
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

The new native result must exactly match the predecessor's ordered opening journal
(including prior P&L in its original GL), day-cut reports and unweighted loan states.
Zero participation does not remove loan principal, coupon lots or gross rights
from that comparison. The completed raw candidate is also exactly server-bound to
the previously executed revised candidate; matching net balances alone is insufficient.
The native cut still moves its own reconciled state and retains its original calendar.
No Python financial calculation enters production execution.

Separate `execution.prior_opening_verification`, schema
`commercial-cut-prior-opening-verification-1`, records verified modeled publication
and exact cut matching. Native producer authority flags remain false. The receipt
retains the prior job ID, attempt, token, owner, revision, request/result descriptors,
queued identity, original deadline and every consumed object. The source must have
finished within its original deadline. Later expiry of that historical deadline
is provenance; all new reads, replay, execution and publication obey the new
request's original deadline without a reset.

The shared final SQL guard rechecks predecessor authority and objects alongside
the current worker's lease, token, attempt, cancellation, revision, request bytes,
source/executable identities and deadline. Restart and artifact reachability use
the existing immutable storage contracts. The endpoint is a durable compute
operation in the generated MCP catalog and has a generated HTTP request contract.

## Supported scope and remaining gates

This tier binds ordinary fixed sequential cut publications. Fee-bearing,
indexed/generated/exercise owners retain their existing separate admission and
are not upgraded by this binding. The receipt explicitly reports separate fee
adoption false. Recursive predecessor chains, advancing to a different cut,
general mixed books, saved/cohort/strategy/accounting/FTP adoption and the complete
60,000-position joint acceptance remain separate work. Synthetic fixtures and
modeled journal reconciliation do not establish observed accuracy, observed bank
reconciliation or reviewed accounting/regulatory policy approval.

The raw `/models/runs` route remains available with its unverified predecessor
flags. Source-bound publication requires the explicit endpoint above. No deployment,
worker restart or saved-library rebuild is implied by local validation.
