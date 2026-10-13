# Native consumer graph to dated daily ledger

`graph-dated-ledger-1` connects the admitted scheduled consumer graph to the
existing native dated store and streamed daily runner. It supplies complete
physical event sources and explicit recognition segments. It does not change
saved monthly books, manufacture accounting policy, publish a manifest or adopt
consumer defaults into credit accounting.

## Raw input and native APIs

The request contains `graph: advanced-product-graph-1`, `owners`,
`capture_policy`, `run_mapping`, `monetary_unit`, `recognition_policy` and a raw
`balance-stress-2` `specification`. All are required. Unknown typed fields reject.
Owners and capture policy use the graph's existing source contract; the ledger
recognition policy must explicitly select `supplied_calendar_segments-1`, with
its policy identity/revision and `immediate_earnings` fee policy. Capture admits
no fees, so this latter ledger convention has no financial effect on these
sources. The policy remains recorded rather than silently inferred.

`advanced_ledger::prepare(&Request) -> Result<Prepared, String>` builds a native
graph, captures the complete scheduled source, maps it into a typed
`DatedLedgerRequest` and calls `CompiledDatedLedger::compile`. It returns that
fully validated request plus `SourceReceipt`. `run(&Request, Box<dyn Write +
Send>) -> Result<SourceReceipt, String>` prepares the same source, invokes
`daily::run_dated` and returns the source receipt only after successful streaming.
A failed validation or writer returns an error; the component publishes nothing.

The synthetic request is
[`examples/models/graph-dated-ledger-1.json`](../../examples/models/graph-dated-ledger-1.json).
Its zero-change run is explicitly supplied because the daily specification
requires at least one scenario in addition to baseline. No identity scenario
is invented by the adapter.

## Ownership, measures and run identity

Capture requires one realized physical history of exact weight one for every
position/run. The graph's pricing histories and fixed base OAS remain separate
valuation evidence. No pricing-measure cashflow is posted. Native generation
consumes supplied paired histories; it does not claim fitted behavior or shared
stochastic draws merely because histories have matching path IDs.

`run_mapping` explicitly maps every graph execution `run_id` to a daily
`ledger_scenario_id`. The graph base alone must map to `baseline`, with severity
one. Other mappings preserve each graph scenario's supplied severity. Different
graph economic scenario IDs can explicitly map to one daily scenario at distinct
reverse severities. The daily compiler checks that the resulting economic run
set is exactly baseline, every named daily scenario and every requested reverse
severity. Missing coverage, extra coverage and duplicate economic executions
reject; no reverse-stress interpolation or severity fallback occurs.

The source schedule and accrual-plan selectors retain execution run IDs.
`RunIdentity` retains the mapped ledger scenario name and graph input revision.
The source receipt preserves the original graph economic scenario IDs, curves,
P/Q histories, original loan/cohort identities, raw ledger input, both policies,
owner mapping, native graph results and complete capture.

## Opening anchors and accounting admission

Every graph position must already name an explicit raw ledger position with
`kind="loan"` and the owner's account. Accounts, currency, classification,
research weights, costs, tax, policies and equity remain raw ledger inputs.
The adapter adds omitted `balance`, `opening_accrued`, `book_adjustment` and
`source_id` from the captured native opening. An explicitly supplied value must
match exactly. It neither rescales principal nor plugs equity. The raw request
retains the omitted or supplied original fields; the prepared dated request
contains the derived anchors. Other raw specification defaults and all opening
reconciliation execute in the existing native ledger compiler.

The supported policy is `consumer_simple_interest_par-1` with
`book_basis="par_note_simple_interest"`, `fees="reject"`, performing opening
state, zero opening earned/unrecognized interest and zero fees. Principal and
gross carrying value are par. This is explicitly selected contractual simple
interest recognition, not an effective-yield claim for arbitrary purchase
prices, fee deferral or credit-impaired loans. Full native interest recognition
and settlements are retained for scheduled payments and admitted penalty-free
payoffs. Default, recovery, nonaccrual, delinquency, unrecognized income, fees,
day-zero cash or ambiguous rights reject the entire source. No invalid event
is pruned to make capture succeed.

`CompiledDatedLedger` also rejects overlap with legacy cashflows, credit/default
fields, draw/runoff/floating overlays, sale/collateral policies and unsupported
mark conventions on dated owners. Its existing daily route suppresses legacy
coupon/maturity processing for owned positions. Other unowned ledger positions
can retain their existing processing in the same book.

## Actual-time recognition, rights and lineage

Each captured segment carries an explicit actual ordinal interval and interest
amount. Native simple note interest is `principal × note_rate × selected
day-count fraction`. Cash and recognized interest are separate: a paid interest
amount is assigned FIFO to complete native recognition origins. Partial interest
payments split an origin amount across event plans without duplicating total
income or moving the original earning interval. The dated store distributes
each explicitly supplied segment uniformly within its original calendar
interval, and settlement cash is posted once on the event date.

`CaptureSegment.origin_id` is preserved in `SourceReceipt.capture`, along with
position, run and settlement event identity. The numeric dated-store segment
layout omits that text; the immutable receipt is the retained lookup for origin
lineage. A reused origin ID across runs or positions must be interpreted with
those identities, not globally deduplicated. Full raw sources and policy
revisions must be published alongside generated journals and reports.

The final reporting ordinal minus valuation ordinal must match the raw ledger
horizon. Full contract schedules remain intact beyond that cutoff. Closing
rights distinguish accrued interest receivable and future interest/principal
settlements. Reporting dates are not converted to 30-day product periods;
legacy daily report cadence remains the existing ledger convention.

The existing runner emits detailed postings/lineage for baseline and ordinary
scenarios. Reverse-severity executions retain full source coverage and produce
the runner's existing summary output rather than a new detailed reverse journal.
This adapter does not broaden that reporting contract.

## Bounds, reproducibility and validation

Canonical raw input is admitted with a counting writer before additional owned
copies and is limited to 32 MiB. Prepared request plus full source receipt uses
a conservative four-times serialized retained charge within 128 MiB. Native
graph run/event/work limits and dated-store capacity limits remain additional;
the existing daily tier remains 60,000 positions and 90 million combined work
units. These are component budgets, not an aggregate RSS cap. `run` validates
again when building its executable compiled store. Cooperative checkpoints
remain active in graph paths, source anchoring, compilation and daily execution.

Source regeneration is deterministic for the same complete typed request.
Publication can hash the canonical serialized raw request and receipt; the
adapter supplies complete content rather than a label-only identity. It does
not substitute hashes for collision-safe raw model identity.

Focused tests independently calculate the leap fixture: 1,000 principal at
36.5% ACT/365F earns 29 over January 31–February 29; payment 529 retires 500.
The remaining 500 earns 15.5 over the next 31 days, with final cash 515.5.
Closing cash is 1,044.5, principal is zero and recognized interest is 44.5.
At a day-30 reporting cutoff, cash is 529, principal is 500 and accrued interest
is 0.5, while the complete final settlement remains in the tail. Tests also
check partial-interest origin conservation, explicit scenario/reverse mapping,
zero-change identity, equal-date positions, mixed legacy ownership, conflicting
anchors, policy/domain/overlap rejection and failed streaming. An independent
test decodes and sums emitted journal postings to every trial-balance key and
checks transaction balance. This proves posting/report consistency separately
from the hand-calculated raw source economic fixtures.
The focused native suite passed all 14 fixtures; targeted Clippy passed with
warnings denied. This is source/component validation. Matching release binaries
and public worker/API publication require their separate integration checks.

Full issue acceptance still requires public worker/API immutable publication
with independently replayed journals and an independent raw source economic
oracle, consumer state-aware default/impairment/recovery adoption, effective-yield
accounting where selected, fitted source histories/held-out evidence, all other
product dispatches and combined scale acceptance. See
[advanced product graph](advanced-product-graph.md),
[consumer installments](consumer-installment.md),
[dated store](dated-store.md) and the production execution contract.
