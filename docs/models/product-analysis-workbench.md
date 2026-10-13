# Explicit product analysis and dated allocation adoption

`GET /models/contracts` lists versioned native analytical contracts.
`GET /models/examples/{schema}` returns synthetic, editable examples. No saved
book is automatically upgraded. `POST /models/runs` requires the current
workspace revision and a known schema, and queues a durable worker job.

The request is bounded to 32 MiB, the native output to 128 MiB and the deadline
to 1,800 seconds. The worker invokes `portfolio-lifecycle` or the explicit
`strategy-product-1`, `product-candidate-journal-1` and `product-mixed-dated-1`
contracts' `portfolio-strategy` in a separate process
with source and executable identity fences. Cancellation, ownership loss or
deadline terminates that process. Tables are immutable Parquet artifacts, paged
and downloaded using existing job report endpoints. The manifest retains the
queued raw-input artifact, hashes, executed schema and explicit analytical scope.
Failure publishes no successful result. Numerical fixtures and examples do not
establish empirical predictive accuracy.

The Product Models panel supports editing assumptions, running these jobs and
reopening stored analyses. Its tables include native source/state details. This
route does not automatically adopt analytical output into the daily mixed book.

The explicit [candidate journal](product-candidate-journal.md) contract retains
the actual native preparation and allocated face, posts acquisition at the
supplied purchase date, and recognizes original-yield income over complete
physical cash and rights tails. Its independent verifier reconstructs raw
product sources, ordered postings and closing GL before storage and again from
persisted tables. The native result keeps `independent_persisted_verification`
and `dynamic_mixed_acceptance` false; the separate publication receipt records
only the checks actually performed. Opening identity is supplied provenance.
Existing mixed positions, defaults, fees, taxes and collateral owners reject.
Source/executable identities, ownership, cancellation and the original deadline
are checked again after final progress reporting and before durable publication.

The explicit [performing mixed replay](product-mixed-dated.md) combines an
existing supplied dated backbook and exact-date candidate purchases in one
native chronological cash and general ledger. Independent reconstruction checks
raw backbook recognition and candidate original-yield sources, acquisition,
ordered postings and complete closing rights before and after persistence.
It does not select the monthly saved book or invent a joint stochastic process.
Opening provenance and backbook recognition policies remain supplied inputs.
Credit/default, modification, collateral and unsupported reverse run ownership
require separate integration; incomplete contractual tails reject.

A completed independently verified mixed report can supply `kind="ledger"`
to the existing closing-capital bridge. The template retains its complete-tail
closing date, which the worker requires unchanged. Signed protocol-3 postings
are independently replayed anew before Rust prepares capital from the closing
trial balance. This is expected complete-tail closing capital, separate from
report-date capital, dynamic credit acceptance or modeled funding FTP. Regulatory
eligibility, weights, deductions and requirements remain explicit reviewed inputs.

`POST /models/aged-library` explicitly selects `aged-units-1` and publishes a
prepared dated library through the existing lease/token/revision transaction.
Worker recovery restores those coefficients outside request handlers. Synchronous
`/strategy/eval` consumes them without repricing; a missing purchase grid requires
a rebuild. Actual reporting dates, admitted template purchase dates and the
native fixed-OAS sensitivity method are exposed by `/state` and Strategy Lab.
The dated lane currently reports incremental allocations without mixing the
monthly saved-book KPI baseline into its actual-date grid.

`strategy-aged-1` / `optimize_aged` accepts raw dated templates/markets and supplied
scenario base constraints. Rust prepares the coefficients, constructs and invokes
HiGHS and replays the allocation. Retained capital rows require exact preparation
dependencies; changes raise `CAPITAL_REFRESH_REQUIRED`. Funding rows use quoted
purchase cash and principal/coupon settlement through the full contractual tail.
They retain gross contractual cash, end-of-day netting and explicit available
starting cash. Face-notional and purchase-cost asset caps are separate. Rust
independently replays allocated source cash events against every funding row.
Dynamic journal validation remains separately required.

`strategy.aged.AgedSession` adds `create_aged`, `update_aged`, `publish`, `abort`,
`eval` and `status` on the existing bounded native actor registry. Constraint-only
edits invoke no pricer; template edits selectively prepare affected units. Full
market/calendar/model changes rebuild the required dependencies. A candidate
retains its old live state until the caller verifies it and publishes the token.
Failed, stale and aborted edits preserve the original economics. See the
[native cache contract](aged-cache.md) for exact identity and component bounds.
The public optimizer dispatches dated libraries through this dated financial
contract and rejects mixed monthly/dated source libraries.

`credit-path-accounting-1` explicitly couples realized physical credit events
to reviewed accounting policies and complete recovery tails. Its durable model
analysis is a source report; independently verified streamed journal publication
remains a separate route. See [path accounting](credit-path-accounting.md).

`consumer-credit-accounting-1` uses one supplied physical installment history as
the principal/coupon/default/recovery owner, with a retained original-yield prefix
and reviewed allowance, chargeoff and gross recovery/cost policies. Its independent
verifier reconstructs the complete raw source and ordered intents before storage
and again after lossless Parquet decoding. This analysis source emits no acquisition
cash journal and does not automatically adopt a loan into the mixed daily book.
See [acquired installment accounting](consumer-credit-accounting.md).

See [complete issue gates](implementation-plan.md), [native contracts](native-contracts.md)
and [aged units](aged-units.md) for the remaining mixed product and empirical scope.


Reviewed derivative accounting and realized credit-accounting model analyses are
independently checked against raw source economics before storage, then decoded
from persisted Parquet cells and checked again before durable publication. Their
receipts explicitly retain `persisted_journal_verified=false`: these are analysis
source checks, separate from the complete protocol 5 daily journal publication.
Missing receipts, changed source/artifacts, corrupted source cells and expired or
cancelled publication fail without a completed result. Nested tagged data use
lossless canonical JSON rather than an inferred union of Polars struct variants.

Every model analysis and dated-library build also checks the original executable,
source identity and publication deadline after persistence. Cancellation and
lease/token fencing still apply at the final repository transaction.

Persisted verification also binds every published scalar summary to the decoded,
independently verified native source. Verification receipts and summary values
retain exact JSON types: numeric zero/one cannot substitute for a boolean flag.
This applies to both analysis reports and prepared product-library reports;
changing an eligibility declaration without changing its source blocks publication.

The explicit [consumer graph](advanced-product-graph.md) and [card graph](card-product-graph.md)
retain separate physical/pricing histories, complete native product sources and
original base OAS. Their physical metrics report declared contractual recognition
and cash components; they do not invent an effective-yield income policy or
establish empirical behavior. The separately verified modeled daily routes are
documented in [consumer capture](graph-dated-ledger.md) and
[frozen credit drivers](driven-credit-ledger.md).

The [term/CD graph](term-product-graph.md) retains contractual amortization,
fixing provenance and explicit supplied CD elections. Gross cash components and
net after-penalty settlement diagnostics stay separate. Each future receipt or
liability payment and renewal tail is priced once with its original base OAS.
This analytical route does not infer optimal election, effective yield, borrower
credit or a daily journal.

`POST /models/aged-library` also explicitly selects `product-units-1` to publish
[performing product coefficients](product-units.md). Source request, purchase
dates, true template/unit identities, original yield and full scenario funding
stay retained. Strategy Lab consumes the selected scenario's exact coefficients;
off-grid selections require a rebuild. These results do not mix in the monthly
saved-book baseline. The broader native [product allocation](product-strategy.md)
route constructs HiGHS rows and supports versioned staged actor updates; raw
product changes selectively rebuild changed units when shared identities and
the ordered grid are unchanged. Constraint-only changes retain native preparation.

Both product coefficient builds and product allocation reports require independent
raw-source reconstruction before storage and after persisted source decoding.
Consumer, card, fixed-mortgage and deposit performing sources retain their original
yield/OAS anchors and full tails. Library publication additionally compares the
persisted adapter's coefficient arrays, funding vectors and source metadata with
the independently verified source receipt. A source-only report cannot authorize
a different cached library. The original deadline, cancellation and durable
ownership fences remain active inside independent replay and through publication.
Unsupported credit, draw, ARM and modification ownership still require separate
adoption; no numerical receipt establishes empirical accuracy or candidate-journal
acceptance. See [product allocation](product-strategy.md) for exact scope.
