# Mortgage complete-tail closing capital

The explicit `mortgage-closing-capital-1` route separately prepares complete-tail
closing capital from a completed `mortgage-mixed-accounting-1` publication.
Its source journal retains its original research scope. This permission does
not grant interim capital, FTP, accounting-standard approval, empirical accuracy,
or regulatory approval of supplied eligibility or risk weights.

The API queues `mortgage_capital.run` through
`POST /treasury/mortgage-closing-capital`. It captures an immutable compatible
completed-source binding and a new finite original deadline before dispatch.
The completed source's earlier deadline is retained as provenance; the new
deadline covers reopen, source verification, native execution, output writes,
persisted verification and final publication. Retry does not extend it.

Rust rebuilds the complete raw mortgage source and its actual ordered journal.
It admits complete unweighted seven-slot principal, interest, signed basis,
borrower rights and default/economic claim tails, including zero allocations.
It checks authoritative native journal/trial/closing coverage and exact dates
before using the Risk crate's existing treasury bridge. No saved trial balance,
closure boolean or Python financial option supplies native authority.

Reviewed per-account/scenario capital inputs, monetary multiplier, risk weights
and negative net-carrying exposure policy remain explicit. Signed accounting
balances, including tiny negative basis, remain unchanged. The reviewed
nonnegative-group policy changes only the separate exposure used for supplied
risk-weight calculations. Research signed contributions are disclosed and an
aggregate negative credit-RWA denominator is rejected. The reject-negative
policy rejects negative groups rather than silently treating them as zero.

The independent Python verifier reopens original source, completion and bounded
partitions, reconstructs source economics and ordered GL, then verifies the
native capital result. It encodes normalized DTO types independently under
`typed-json-binary64-identity-1`: UTF-8 sorted keys, binary64 float bits, signed
zero and distinct integer/float/boolean domains. Original queued input identity
is retained separately from typed normalization.

The selected native operation reparses original request bytes after the
historical CLI peek. Its body limit is 2 MiB; historical whole input admission
remains 128 MiB, so this is not a preparse 2 MiB or aggregate RSS guarantee.
Source work retains its independent 8M admission and the bridge separately
reserves up to 2M work. Complete serialized source/control limits are unchanged.

Publication seals queued inputs, source and code bundle, executed binary,
owner/token/revision, original deadline and independent receipt. It rereads
checksummed source and capital-output bodies after replay and inside the final
SQL guard. The guard rechecks source metadata, current job, lease and workspace
revision in the publishing transaction. Failed native rebuild, cancellation,
expiry or changed evidence publishes no completed result reference.

Whole-number policy amounts and rates may arrive with integer JSON spelling.
The independent verifier retains their exact original reviewed input, then
constructs a private copy with only the declared native f64 policy fields typed
as floats. This covers weights, exclusions, capital inputs and minimum/buffer/
management requirements; it does not infer eligibility or requirements. Null
advanced RWA remains unavailable. Boolean money, unknown policy fields and
changed output weights reject; metadata, inclusion booleans, signed zero and
financial tolerances remain unchanged.

The reviewed route is applied locally in 0.29.8; matching official native builds
and component gates pass. Earlier setup/transport failures and their immutable
receipts are retained. The latest full engine/API gates pass 2,220/589 cases,
while browser acceptance exposes the financial policy spelling defect described
above. Its repair passes 18 focused engine and 11 durable API/source/capital/
publication-fence cases. Fresh full acceptance runs under
`product-adoption-0298-capital-browser-repair` now passes 2,227 engine tests
(two reference-only skips), 590 API tests and 45 scoped browser checks. Contained
proof, focused process checks and complete installed acceptance are distinct.
Local builds do not deploy this route or restart production workers.
