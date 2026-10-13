# Fully-bound supplied indexed-book analytical publication

Proposed `POST /models/runs`, `schema_name=market-indexed-books-1`. Matching
default runtime, durable worker and browser acceptance remain pending. This
does not opt saved `/run` pricing, what-if, strategy or cohort books into a new
owner. The example is synthetic.

The full native raw book DTO remains the request. Every loan row is explicitly
bound, in its original order, to a reviewed original quote, original supplied
OAS and named original/current collateral and index curves. The current native
binding supports floating bullet loan assets only. Nonempty debt and unbound
rows reject. Native generation supplies complete conditional coupons, terminal
principal and paired collateral/index/parallel gradients. Current displayed
quote changes never refit original OAS or replace original source parents.

One bounded native graph/cache actor exists per job, then closes through its
captured library. Raw source/normalized retained DTO identity and financial
calculations are independently verified; exact identities include types and
signed zero, whereas calculated money retains 1e-12 relative/1e-9 absolute
tolerances. Nested source JSON and Parquet are checksummed and reread before
publication; the independent source/price/risk replay runs again on that reread.
No Python financial implementation executes the job or feeds its native control.

The deadline is accepted once before queue writes, including queue waiting.
Retry/restart cannot renew it. Fresh per-call native deadlines are only the
remaining portion of that original deadline. IO lease checks are bounded and
fresh at readiness and handoff; pure loops check the absolute deadline on each
checkpoint and refresh lease IO at most 50 ms apart. Broken/unbounded lease IO
fails without publication. Source/DLL/bundle changes, cancellation and lease/
token loss refuse execution/publication. A private publication closure rechecks
sealed queued input and output objects within the scoped SQL transaction, then
freshly tests token, attempt, lease, revision and original deadline. Metadata
returned by a producer cannot authorize a deadline extension or a foreign result.

The actor retains no Python callback or expired creation deadline. Drop is
bounded resource cleanup through its originating library; u64 handles/tokens
are library-scoped and can collide across different loaded libraries. Synchronous
OS/SQL/Codec operations and individual allocations are checked before/after,
not forcibly interrupted. Worker watchdog termination is separate from native
cooperative cancellation evidence. There is no hard real-time or aggregate RSS
promise.

Native component limits remain sixteen bindings, 2 MiB raw, 32 MiB source output,
128 MiB joined admission and 60 million work. The job cache is 8 MiB/1024 entries
within native 16 MiB/4096 caps. Persisted manifest and all table objects together
are capped at 64 MiB. The supplied native grid is not shortened to fit admission;
large grids/mixed inventories may refuse. The joined bounds include native raw,
nodes and transient projection/risk arrays, not the entire Python/storage process.

This is deterministic supplied Pricing. Observed fixings, supplied-calendar
verification, Physical income, joint market/borrower dynamics, accounting
journals, saved workspace warm actors, capital/FTP, empirical and broader product
adoption remain false. A complete analytical source is not a regulatory or
accounting approval. Original issue #10 and tracker #25 acceptance remain open.
