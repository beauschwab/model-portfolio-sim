# Commercial closing capital, daily FTP and purchased journals

These are explicit additive contracts. Rust prepares the source, credit, coupon
book, journal and treasury amounts. Python transports immutable inputs and runs
independent raw-source and persisted verification. Compatible native artifacts
and explicit selection are required. Local fixtures do not establish observed
accuracy, accounting approval, deployment or complete issue acceptance.

## Source-linked capital and daily FTP

`commercial-treasury-1` takes `raw_commercial` containing the complete
`commercial-credit-ledger-1` request. Optional `capital` and `ftp` branches select
separate reviewed policy inputs; at least one branch must be configured. The
capital branch supplies `treasury`, account/scenario `policies`, an
`amount_multiplier`, reviewed identity and `exposure_policy`. The FTP branch
supplies `as_of`, policy identity, curves and explicit per-path reviews.

Rust first executes the commercial source and its protocol-6 journal. Closing
capital uses the complete-tail closing trial balance and requires the treasury
date and every account policy period to equal that exact closing date. It does
not substitute the requested earlier reporting date. Reviewed provisions,
chargeoffs, recovery income and retained P&L enter through actual postings once.
Economic modeled LGD does not become a second capital deduction. Legal memo
accounts are balanced and independently verified before being excluded from
regulatory amounts. Eligible capital, regulatory weights and adjustments,
denominators, requirements and management buffers remain supplied policies.

Daily FTP uses actual unweighted path stocks, actual cash dates and posted income,
with participation applied once. Separate channels retain live/default principal,
basis, signed recognized interest, gross legal coupon rights and actual undrawn
commitment. Reviewed paths additionally retain legal principal and allowance.
The supplied funding, channel-change and income reviews determine permitted
ownership. Remaining channel stocks require complete explicit termination;
neither a short reporting horizon nor a zero aggregate net right closes them.

Original effective-interest income, reviewed basis realization, reviewed credit
expense and recovery income use distinct posted earnings identities. Contingent
charges use actual undrawn commitment once. Internal transfers do not change
external NII or regulatory capital. Opening-day P&L with FTP is refused until a
separate opening earnings owner is supplied. Realized paths are not reinterpreted
as expected-flow FTP inputs. Supplied curves, reset tenors, option spreads,
operating costs, capital allocations and cost of capital remain assumptions.

The current bridge admits at most 16 paths and 64 complete FTP days, a separate
2-million bridge-work bound, 2 MiB raw envelope and 32 MiB output. Source and
journal admission limits remain in force. Capital-only requests retain the full
native ledger calendar. These component limits are not aggregate RSS promises.

The transport is `core.commercial_treasury_native.run`. The durable
`/models/runs` route independently recomputes sources, signed ordered postings,
closing reports and treasury outputs before and after persistence.

## Actual-purchase commercial candidate journal

`commercial-candidate-journal-1` takes a raw `commercial-units-1` request,
ordered `allocation`, actual `as_of_ordinal` and `reporting_end_ordinal`,
explicit `owners`, `run_mapping`, zero-placeholder ledger `specification` and
`verified_opening_identity`. The identity is supplied provenance; the native
receipt retains `opening_identity_verified: false`.

Unit preparation retains the original base OAS, separate Physical and Pricing
histories, actual purchase cash and original effective yield. Exercised
acquisitions require complete original Physical expectations and an independently
reconstructed stopped-cash acquisition proof. Performing draw tranches retain
their own conditional activation, original source, cost and EIR. Allocation
replays prepared coefficients and full-tail funding cash without repricing.

The native coordinator maps each exclusive source to an actual account and
position. Purchase offsets remain in source lineage and daily/closing rows;
local credit time and global credit time are distinct retained fields. Commercial
loans use `commercial_loan-1` and a loan placeholder. Issuer-credit assets use
`issuer_ac_security-1` and an AC security placeholder with zero duration and unit
opening market-price anchor. Issuer securities reject loan facilities, CCF and
performing draws. The selected candidate owner supports AC; AFS/HTM marks,
institution funding liabilities and arbitrary mixed owners need separate gates.

Opening account cash/equity is posted once, candidate acquisition uses actual
purchase cost, and source groups enter the existing shared daily Sim in stable
global chronological order. The minimum Sim calendar is 30 days; the caller's
earlier reporting date remains immutable. A separate full calendar covers every
contractual, recovery and reviewed action tail. No terminal payoff is invented.
Existing pooled cash-floor checks remain active.

The result retains complete native unit preparation, allocation replay,
normalized specification, protocol-6 tables and explicit reporting/full dates.
`shared_daily_sim_adoption` and `graph_exercise_source_adoption` disclose this
selected ownership. Independent persisted verification, opening evidence,
arbitrary mixed adoption and empirical validation are separate flags. Generated
policy graphs do not imply candidate adoption when complete native Sim retention
exceeds admission; such requests must refuse without increasing existing caps.

The candidate envelope/output are bounded to 32 MiB; native admission retains the
32-million aggregate work and 128 MiB component-capacity boundaries. The transport
is `core.commercial_candidate_native.run`.

## Immutable publication

Both durable routes preserve the original scalar deadline through raw parsing,
native execution, independent replay, persistence and final SQL publication.
Publication checks the exact queued raw request, native executable and source
identities, immutable artifact bytes, cancellation, input revision, lease and
attempt token. Nested monetary, integer and boolean identities are checked before
table conversion. Failed final checks remove only fresh owned unpublished output.
The API does not execute financial calculation in its request handler.

Focused tests cover raw cash/book/journal replay, reviewed capital without a
second loss deduction, incomplete FTP termination, mutation rejection and durable
publication adversaries. Their current execution evidence belongs in
[the implementation ledger](implementation-plan.md); rebuilding and restarting
deployed workers is a separate operator action. Full observed holdouts and complete
60,000-position joint graph/credit/solver/journal acceptance remain required.
