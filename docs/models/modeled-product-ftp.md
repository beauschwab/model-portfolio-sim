# Source-linked modeled product FTP

The explicit `modeled-product-ftp-preparation-1` contract reconstructs selected
Physical product sources, prepares reviewed daily funding balances, and invokes
the existing native Treasury owner. It keeps actual cash, earned income, carrying
adjustments, credit accounting and internal transfers separately traceable.
This is one implementation stage of issue #24; all original issue #25 gates remain
required. Applied source and successful acceptance of a particular source bundle
are separate facts.

## Owners and inputs

`portfolio-model-core::modeled_product_funding` owns the common state-channel,
cash-assignment, reset and daily-averaging contract. The shared
`modeled_product_ftp_policy` module contains typed bindings and review identities.
`portfolio-decision-native::modeled_product_ftp_source` rebuilds raw sources through
their existing product/journal owners, then maps their evidence into that contract.
`modeled_product_ftp` admits the combined problem and invokes native Risk Treasury.
`portfolio-risk-native::modeled_ftp_policy` owns the two explicit comparative-cost
formulas. Python transports inputs, stores tables and independently verifies raw
sources and persisted output; it does not generate the production financial result.

Each source has an immutable `source_id`, declared scope and review identity.
The selected variants are acquired mortgage mixed accounting, acquired installment
or Card2 mixed accounting, revised deposit income, and performing deposit source
continuation. The native source journal and actual producer completion survive
preparation. Reviews bind exactly to `(source_id, owner_index, run_index, path_index)`;
missing, duplicate and surplus bindings fail. Inherited dated backbook owners are
retained as evidence but do not automatically become funded product exposures.

The first durable API route accepts explicit raw sources. Selection of completed
Card2 or deposit jobs needs a separate source-binding integration; a job identifier
alone cannot supply the original raw source, policy or journal authority.

Every path requires entity, currency/unit, source/anchor identity, full start/end
dates, physical weight, unit face, allocation, reviewed funding policy and reset
schedule. Paid rates, original effective yield, reviewed allowance/credit policy,
recovery cash and collection cost remain owned by the source model. No source
permission becomes accounting-standard or regulatory approval by passing through FTP.

## Funding state and complete rights

The common state retains eighteen ordered channels:

- Active, pending-default, pending-prepay and default-claim principal.
- Live and default interest receivables/payables.
- Principal, live-interest, default-principal and default-interest basis.
- Borrower and economic principal/interest memos, undrawn commitments and allowance.

Each channel has complete source-origin lineage and an explicit reviewed coefficient
in `{-1, 0, 1}`. Borrower/economic memos, undrawn memo and allowance require coefficient
zero: they cannot be converted into funding exposure. Gross channels stay nonnegative;
the four carrying-basis channels retain their signed source amounts. The selected
funding balance at checkpoint `d` is

`F_d = sum(channel_coefficient_i * source_channel_i,d)`.

Ordered cash assignments and separately reviewed noncash reallocations must reproduce
every subsequent checkpoint. An assigned cash event changes funding by
`-side * bank_cash`, where asset side is `+1` and liability side is `-1`; an event
instead excluded from funding requires an explicit exclusion review. A cash event
cannot be assigned twice or both assigned and excluded. Noncash reallocations
cannot invent cash.

Source-weight and allocation scaling occurs once:
`scale = allocated_face / unit_face * source_weight`. Funding, cash and earnings
are scaled after the complete unweighted source has been checked. Zero participation
does not excuse missing tails or contradictory source state.

Policies explicitly choose opening-daily or trapezoid-daily averaging:
`average_d = F_(d-1)` or `(F_(d-1) + F_d) / 2`. This tier uses ACT/365F, with daily
fraction `1/365`. Supplied reset schedules independently retain reference/repricing
and liquidity/funding tenors; the last reset strictly before the ending day applies.
Tenors are not inferred from repayment cash or account labels.

Principal reaching zero does not terminate interest rights, basis, recoveries or
commitments. The terminal checkpoint must close every unweighted channel and each
individual origin. Complete tails cannot be replaced with synthetic maturity cash.
The common research contract can retain signed funding under an explicit policy;
the existing Treasury-position bridge rejects negative funded exposures.

## Earnings, acquisition and internal transfers

Seven distinct signed bank-profit channels are retained: original EIR, reviewed
basis, incurred credit, recovery income, recovery cost, initial allowance and deposit
revision. Original EIR is earned income supplied by the accounting source, not
coupon cash. Cash repayment, gross recovery and a carrying-basis realization cannot
silently become a second copy of recurring income.

The selected acquisition phase retains actual source-owned purchase cash and initial
allowance postings once. These opening amounts are separately reconciled to the
native ordered journal; deposit continuation does not receive an invented purchase
cash flow. The mandatory policy is
`actual_owned_purchase_and_initial_allowance_once-1`.

The existing native Treasury owner computes matched-tenor reference and liquidity
charges, entity/currency/scenario/period elimination, profitability, economic profit
and annualized RAROC. Its linear curve interpolation and disclosed flat tails remain
unchanged. Reviewed operating costs and allocated capital are explicit inputs.
Daily Treasury positions carry original EIR; the accompanying accounting-scope
reconciliation adds the other six earnings channels exactly once. In that complete
scope, `business_profit + treasury_profit = source_earnings - operating_cost`.
Internal transfers do not alter external earnings, OAS or regulatory capital.

## Explicit comparative charges

This stage implements reviewed Physical-scenario incremental costs. They are not
pricing-measure option fair values. Each selected path requires compatible complete
benchmark paths, reviewed annual rates and the exact rate vintage.

`option_amount_d = max(funded_average_d - benchmark_average_d, 0) * rate * 1/365`.

`contingent_amount_d = max(native_undrawn_d - benchmark_undrawn_d, 0) * rate * 1/365`.

Undrawn amounts come from reconstructed source commitments, not caller-supplied
surrogate balances. The positive part belongs only to the stated cost methodology;
source cash, carrying amounts, income and regulatory exposures are not clipped.
The option amount converts to the existing Treasury spread only against a positive
funding/accrual basis. A nonzero amount at zero basis rejects. Annual comparative
rates and derived spreads are bounded to one.

Optional closing capital is currently the separately reviewed mortgage complete-tail
bridge. Its raw mortgage source must match the FTP source exactly, with explicit
capital policy, monetary multiplier and closing date. Interim/report-date capital,
automatic eligibility and jurisdiction-specific regulatory inputs are separate work.
Dirty retained unit dependencies require refreshed capital preparation; this route
requires `clean_frozen_units-1`.

## Durable execution and verification

`POST /treasury/modeled-product-ftp` queues `modeled_product_ftp.run` on the durable
worker. The example is synthetic. The request retains its original raw JSON and
finite queue-time deadline, workspace revision, code/source identity and executable
bundle. Selected JSON admission rejects duplicate keys, including nested variants.
Missing or mismatched native artifacts fail without fallback.

Independent verification reconstructs raw source state, source journals, complete
funding/earnings channels and comparative charges before storage. Publication retains
the complete native result plus strict typed FTP reconciliation, source-journal and
actual completion tables. Persisted output is reread and independently compared,
including exact source/metadata identity and unchanged financial tolerances.
Object-body and partition hashes, original deadline, cancellation, revision, attempt,
lease and token are fenced through the final scoped SQL transaction. Reopening,
paging and downloading use immutable published artifacts.

The separate reviewed successor adds
`POST /treasury/modeled-product-ftp/sources` with an explicit
`modeled-product-ftp-completed-selection-1` specification. It requires one or two
distinct completed source jobs, exact tagged scopes/reviews and the existing FTP
policy template with source records omitted. The server captures raw source records
from completed Card2 mixed or performing-deposit jobs; the worker independently
replays those records and reconciles fresh native FTP source preparation. Original
completed-job deadlines remain provenance; the new job's original deadline begins
before metadata reads and covers reopening, native execution and publication.
Completed source status or a saved verification flag never supplies funding policy
or eligibility. Source request/result descriptors, executed identities, owner/token/
attempt and raw state are rechecked during persistence and inside final SQL completion.

The successor separately checks original typed input clones exactly. A monetary
comparison is not input identity: a preserved predecessor case accepted an original
yield clone changed by `1e-16`. New declared DTO normalization rejects that change
while retaining the original raw JSON types, signed zero and unchanged computed
amount tolerances. The helper's own file hash joins the queued/current code bundle.

## Bounds and current acceptance

The selected raw FTP body is at most 1 MiB; native complete output and joined stored
output are each bounded to 32 MiB. The shared channel tier admits up to 64 complete
daily intervals, 4,096 cash/change/earnings events, 65 resets and 128 origins per
checkpoint. Source composition admits at most four sources and sixteen selected
reviews. Preparation reserves source generation, channel conversion, acquisition
scan, charge work and optional capital replay before generation. Its combined work
ceiling is 62 million units and charged retained-capacity ceiling is 384 MiB.
The retained journal transport admits 250,000 rows. These component reservations
are not aggregate process-RSS guarantees or 60,000-position acceptance.

The reviewed 49-file public integration is applied under
`modeled-product-ftp-public-adoption-0298`. Its frozen 851-file source set has aggregate
SHA-256 `998d3fee404fde63dcfe3c27f801b8e2afa143d273c8774f6fb7b93a9d7874bd`.
All eleven matching official native build/check gates pass, with 923 native tests
and four strict crate checks. Fresh installed-default engine acceptance passes
2,237 tests with two reference-only skips. The API gate is terminal with 676
passing tests and one initial API-health startup failure in the actual restart
test; browser acceptance was not reached. Its original temporary process log is
no longer available, and the cause remains unproven. A retained reproduction
exposed a real local Windows long-path publication/read failure. The subsequent
852-file `modeled-product-ftp-public-adoption-0298-windows-storage-repair` has
aggregate `b82dd0a2458459ca3783faf9b48967ea8f5ea0b463d8acce395719e3a465ca2b`.
Its storage helper preserves relative object keys, checksum and scope; the exact
preimage fails and the helper passes on the same 415-character object path.
Twenty-five focused checks and the exact actual process restart case pass with
the existing timeout. Its API gate passes 679 tests and the existing browser scope
passes 47 cases. The required FTP browser failed on two full-result requests during
saving/reopening a paged report, so the storage source set is not accepted. Native/engine
results are explicitly reused because their source and artifacts are unchanged.
The missing original process log prevents a confirmed original-cause claim. The earlier
accepted deposit bundle does not establish acceptance of
this new financial source set. Evidence lives in the distinct application, native
gate and acceptance receipts under that prefix; failed receipts must be preserved.

The coherent 17-file selector/identity/UI correction is now applied under
`completed-product-ftp-ui-public-adoption-0298`. Its 859-file freeze aggregate is
`db900cf5b82d2371fa0ce9346b6049e37bb482ec0323e2d8849100a53d93ce9b`.
It retains the storage repair, reads strict real Codec descriptors, preserves
opaque raw Value fields, bounds logical table expansion, independently binds
declared source DTO clones and uses status-only polling for modeled reports.
All 22 official-default source-worker/metadata cases pass; architecture and
HTTP/native contract checks pass. Complete fresh validation passes 2,252 engine
tests with two reference-only skips, 701 API tests, 47 existing browser cases and
four FTP browser cases. The application receipt is complete. The unchanged
native financial source/artifacts explicitly reuse the 923-test build/check gate;
these results belong to the 859-file source set. The subsequent applied CD/source2
successor is undergoing separate fresh builds/default gates and inherits no
financial acceptance from these counts. Source2 multi-owner deposit funding/FTP remains a
separate additive native contract rather than a relabeled historical source1.
The contained successor now passes two actual completed-source browser flows and
two late-private-SQL refusal browser cases, with complete source comparison and
independent persisted replay. Final successor source freeze/application/default
acceptance remain required. See [joint funding validation](joint-funding-validation.md)
for the primary-source rationale, separate reset/funding tenors, exclusive owner
identity and required full-tail, collateral and empirical acceptance evidence.

The completed-source successor's frozen seven-file delta passes 105 contained
native/source/Codec/Parquet/SQL/HTTP checks and exact isolated patch roundtrip.
Manifest SHA-256 is `eb00338cffc9de01c22af7380afba083c33742b5eb97583da87814f97886bd11`.
It changes no Rust source, artifacts or numerical tolerances. At that predecessor
freeze, four actual default source-worker/selected-publication tests were authored
but unexecuted. Actual UI
source selection, installed acceptance and PostgreSQL evidence are separate gates.
The root application review passes, but application is held for matching predecessor
acceptance and a separate actual-source transport/identity repair. Real source-worker/
browser checks found normalization applied to opaque raw Value fields, missing
declared numeric DTO fields and unsupported Codec ObjectReference nodes. Preserved
standalone-versus-nested source receipts also require exact separately declared
raw/DTO clone binding. The operational-fixture suite does not establish that missing
actual source-worker/browser coverage. No financial tolerance or Rust calculation
change is authorized by these repairs. The unapplied delta is not evidence of
installed completed-source support.

Remaining full issue gates include completed-source bindings, broader product and
long-tail coverage, product/cohort/saved-book/strategy adoption, report-date capital,
observed behavioral/credit holdouts, reviewed regulatory policies and combined
60,000-position graph/credit/solver/journal acceptance. Supplied histories and
synthetic numerical checks cannot establish empirical predictive accuracy.
