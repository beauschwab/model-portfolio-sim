# Verified modeled commercial prior openings

Local0.29.15 adds durable `POST /models/holding-journals`. It resolves an
authorized completed `commercial-holding-journal-1` job in the current repository
scope. The caller supplies `source_job_id`, the exact published `cut_ordinal`,
`revised_sources`, `expected_revision` and an optional timeout. The caller cannot
supply an opening ledger, balances, permission flags or object descriptors.

The API captures bounded source metadata. It materializes the new native request
from the stored source's original account/participation/allocation specification
and its actually executed revised sources. Native financial calculation uses the
existing `commercial-holding-journal-1` owner and daily Sim. No Python financial
generation, caller checkpoint, original-yield refit or credit-clock restart is
introduced. Before native execution, the durable worker reopens and independently
recomputes the complete raw prior source and its persisted full journal. A stored
verification flag alone grants no authority.

The new complete native journal must match every prior posting through the cut,
the exact published opening stocks and retained prior P&L. This verifies the
published modeled day-cut opening, not an arbitrary later closing or bank/custodian
statement. Currency/account identity is retained in the server-materialized native
specification and independently verified source. Future revisions remain subject
to the existing raw-prefix and economic-prefix guards. Complete cash, gross rights
and recovery tails and original acquisition anchors remain retained.

The result retains the producer's `prior_publication_verified=false`: the native
producer cannot grant external storage authority. Separate
`execution.prior_opening_verification`, schema
`commercial-prior-opening-verification-1`, records
`modeled_prior_publication_verified=true` only after server-scoped source resolution,
full independent replay and exact current-prefix/opening matching. It retains the
source SQL identity, attempt, owner/token, revision, created/finished timestamps,
queued/raw/result descriptors and all consumed immutable object hashes. The source
must have completed within its own original deadline. Its expired deadline is
retained provenance; all current reads/replays obey the NEW job's original budget.
The source's completed SQL row remains a published-history GC root. Restart and
reachability tests verify its retained objects.

The source's tables are admitted before decoding through the shared bounded
completed-source reader, also used by existing Card2/deposit FTP source selection.
Per-frame rows, cumulative compressed/decoded/logical bytes and distinct object
counts are explicit. The prior route retains at most1,024 object descriptors and
64MiB source object bytes, with a32MiB prefix envelope. Client/native raw limits
remain2MiB and native output32MiB; inner8M/128MiB and joined32M/128MiB finance
admission remain unchanged. These are component capacities, not an aggregate RSS
claim. Recursive prior-bound source chains are rejected in this tier.

Both holding schemas now participate in the shared commercial publication guard
inside the final SQL transaction. The0.29.14 worker had omitted them from that
selection; its focused gates did not exercise this last transaction boundary.
Current source and output bytes, original deadlines, current executable/bundle,
workspace revision, request identity, attempt, token, lease and cancellation are
rechecked after independent persisted replay and again under the SQL lock. Prior
source SQL identity and every consumed source object are also rechecked there.
Rehashed prior/current money, evidence, order, summaries or input identities cannot
authorize publication. No financial tolerance or existing admission cap is relaxed.

Executed-envelope reconstruction uses the saved integer milliseconds directly.
Converting1001ms through floating seconds can otherwise floor it to1000ms and
falsely reject an unchanged command hash. Typed clock bounds remain enforced;
this repair does not change actual native execution budgets or extend a deadline.

This stage verifies a modeled published opening used by a complete-origin native
replay. It does not start a new mixed Sim at the cut, verify observed bank balances,
prepare seasoned saved/cohort/strategy coefficients or allocate secondary conveyed
rights. Nonintegral fees, daily in-arrears earning, generated draw/review streaming,
general convergence, observed histories/reviewed policies and complete60000-position
mixed graph/credit/solver/journal acceptance remain required under issues15/25.
The source adoption is additive and explicit; historical receipts remain unchanged.
Deploy matching application/native artifacts, restart workers and explicitly rebuild
selected sessions/libraries before use. Local validation is not deployment.

Matching official local builds pass1,220 native tests, strict Clippy and doc checks,
210 commercial engine and184 commercial API checks,2,598 full engine checks with
two existing reference-only skips and1,157 full API checks. The API environment's
optional MCP import skip is separately covered by the dedicated real-API/worker
integration pass. MCP29 and the web build pass. See
[the acceptance review](../reviews/2026-10-12-commercial-prior-opening.md)
for retained scope, exact identities and the unchanged native financial source.
