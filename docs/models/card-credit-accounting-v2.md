# Acquired card interest-basis ownership, schema 2

`portfolio_model_core::card_credit_accounting_v2` is a separate bounded native
component. Explicit `card-credit-accounting-2` selection is required. Schema1's
financial formulas, negative principal-basis rejection and wire contracts remain
unchanged. No fallback, mixed-ledger adoption, empirical fit or accounting-standard
approval is implied: the result's three corresponding flags are false.

## Raw source and reviewed assumptions

`Request` retains the actual native `CardRequest`, `Review` acquisition cost/face/date,
position/currency/source revision and immutable original-yield/review anchors,
`RightsPolicy`, termination, claim identity, reporting date and raw actual reviewed
interest-claim collections. Review explicitly requires
`accounting_basis="supplied_non_pcd_gross_cost-1"`. No financial state, journal,
fitted yield, probability weight or discount override is accepted.

Policy rule strings are mandatory:

- `actual_terminal_principal_transfer_to_unpaid_interest-1`;
- `frozen_original_yield_on_whole_live_carrying-1`;
- `proportional_basis_realization_separate_from_original_eir-1`;
- `stop_at_source_default_no_claim_eir-1`.

The admitted slice is acquired performing, physical, fee-free, non-grace, no draws,
zero opening interest rights and no allowance/PCD. Only exact full principal cash
is supported; subsequent default must have principal zero and a recognized unpaid
interest right. The native source's collection fraction and collection cost must
both be zero: actual interest collections use separately retained reviewed inputs.
Partial principal, default before principal retirement, negative contractual
coupon, early cash creating negative accrued rights, nonaccrual/cure, modifications,
fees, grace, CCF and new draws need separate policy owners and are rejected here.
Negative **original EIR** is distinct and can be admitted when carrying remains
valid. Supplied review identities are assumptions, not proof of outside approval.

## Channels, finance and actual source order

Let P be principal, A gross earned billed/unbilled interest, Bp principal basis and
Bi basis owned by the unpaid interest right. Carrying is `C=P+Bp+A+Bi`. Opening is
`Bp=cost-P`, `A=Bi=0`. The unchanged native card projector is the sole contractual
economic owner, preserving class/promo rates, statement/minimum/payment waterfall,
actual recognition intervals, source default and commitment evidence.

Each live actual-date interval uses unchanged native `dated_income::evaluate` with
one event and the original supplied annual effective rate y. Effective interest is
`e=C*expm1(log1p(y)*actual_days/365)`. Gross A increases by actual earned a; Bp while
P>0, or Bi after P reaches zero, changes by `e-a`. No complete future cash arrays,
re-fit, yield-change input or cash flow approximation is used.

Full actual principal cash atomically transfers remaining Bp to an **existing**
interest right with zero transfer cash/income, then settles P. The original shared
`P+Bp` guard is checked before and after; Bi is never hidden in principal basis.
Transferred `A+Bi` must be admissible. Actual interest q releases `Bi*q/A` (all exact
Bi for full cash), decreases gross A by q, receives q once and recognizes the
negative of released Bi as **reviewed disposal income**, separately from EIR.
Exact native cash is retained; rounded excess collection does not become owned cash.

Order is acquisition opening, live recognition, source interest-before-principal
cash, source default, then reviewed same-date claim collection. Every date index
preserves input order. Source-default recognition includes the actual default date,
then stops original EIR. Default claims receive no implied EIR. Gross A and Bi must
be paired under either transfer/transfer or reversal/recognition default policy.
Default and collection shared credit intents plus supplemental basis postings form
one atomic action. Individual gross claims and basis remain visible after default,
including when their net carrying is zero.

Reviewed closure requires the source's exact P, billed/unbilled interest, fees,
limit and undrawn to be zero, and owned channels/claims closed under existing
unchanged reconciliation tolerances. A source default can retain incomplete claims;
reporting does not prune them. No principal-zero payoff inference is made.

For each committed state, total live/default carrying equals original cost minus
cash received plus original EIR, reviewed disposal income and shared default income.
Postings use the existing debit-positive convention and balance within unchanged
`1e-9 + 1e-12*abs(reference)` reconciliation semantics. Interest carrying checks
retain the frozen proof's existing `-1e-9` bound. Nothing zeroes residual rights or
relaxes the shared principal-basis rule.

## Compact result and exclusive downstream owner

`ResultData` retains raw input and full native economic source once, then flat
`events`, `shared_intents`, `income_steps` and `actions` arenas. `Daily` holds only a
current `Snapshot` and action range. An `AtomicAction` holds event/intent/income
ranges, immutable source-recognition/cash/collection lineage and committed state.
Snapshots retain separate live Bi and claim-ID-to-Bi ownership alongside unchanged
`AccountingState`. Card balance classes are source classes; no new credit-state
origins are inferred.

**The event arena is the complete posting stream.** Shared intents and income
receipts are evidence referenced by action ranges; replaying their postings again
would double accounting. Source indices plus the retained raw/source and review,
yield, claim, date and atomic-action identities make replay deterministic. Atomic
staging clones only current state and newly proposed bounded receipts; failure
preserves all prior balances, earnings and arena cursors.

Downstream adoption still needs the existing Sim to own supplemental signed live
and default interest-basis accounts. Negative Bi is a contra receivable adjustment,
not deferred income. Gross rights cannot be netted away across paths. Allocation
and physical path weight belong to that exclusive outer owner and must apply once.
Treasury net-carrying mappings, capital verification, persisted independent replay,
publication/cancellation/lease fences and public transport remain separate gates.

## Admission and controls

`audit_request` returns full end date, conservative work, raw bytes, reserved bytes
and maximum action/event/income counts without card projection. It validates every
raw source action/review, including zero-allocation callers. `audit_request_with_check`
and `evaluate_with_check` accept caller-native cooperative control checks. Checks
surround the bounded source projector and run between serialization chunks, dates,
cash/default/collection actions and final retained verification. The existing
projector itself is unchanged and is limited here to 64 days and 64 source actions.

Limits are 64 actual days, 64 steps+cycles, 16 collections, 256 conservative events,
1MiB raw input, 16MiB component reservation/result and 2 million conservative work
units. Monetary values retain the shared 1e15 limit and EIR range -0.99 through10.
All byte/work/count admission arithmetic uses checked addition/multiplication.
Arena capacity is reserved/charged before projection. Every staged action and daily
snapshot is charged before retention. Final charging includes actual vector
capacities, conservative map-node space and full JSON-escaped numeric/identity
representation, then rejects overflow of the predeclared reservation. This is a
component capacity contract, not an aggregate process RSS guarantee.

Date/statement/source indices are built once. Income steps have exactly one native
input event and result row. Current states retain at most one default claim with16
settled IDs. Storage is linear in dates, source and flat evidence, with explicitly
bounded current claim snapshots; no daily cumulative history cloning remains.

## Validation and context

The tracked Core fixture contains 18 frozen typed proof cases, independently
raw-verified by 67 artifact reference checks. Twelve accepted cases expand exactly
to the same complete native card source, 492 daily states, ordered posting events,
dated-income inputs/results, default claims and action lineage; six invalid cases
remain rejected. Additional hand/bounds/cancellation/escaped-metadata/flat-retention
and atomic failure tests cover the compact representation. The evidence is
synthetic and does not establish empirical issuer fit or complete #19/#22/#25 scope.

The installed independent source suite passes 120 checks (79 pure and 41 actual
public-process checks) against the rebuilt official lifecycle. Explicit durable
primitive publication and final execution fences pass ten API checks. In the
[tagged mixed owner](product-mixed-product-credit.md), complete actions enter the
existing Sim and ordered GL; gross rights and signed basis close independently
before weights apply. The component result's own mixed/adoption flags stay false;
the outer mixed owner supplies its separate journal and verified closing evidence.
That outer lane passes 97 Ledger and 74 Decision tests with strict Clippy and 89
independent mixed-source checks against the rebuilt default workflow. The complete
installed bundle passes 1,842 engine tests (two reference-only skips), 493 API
tests and 40 browser cases. The final gates select no DEBUG override; identities
are retained in `artifacts/card-credit-v2-model-identities.json`, with
`deployed=false`. Wider issue scope remains required.

Signed live/default interest-basis adjustments are explicit asset accounts in
closing-capital preparation. Ten native and six independent journal/bridge hand
calculations preserve signed assets/RWA and distinct original-EIR/basis income.
Separate matching `closing_capital_eligible:true` native/independent/persisted
permission is required at the complete-tail date with supplied policies/weights.
`capital_ftp_eligible:false` remains; no interim capital or modeled FTP is inferred.

[CFPB payment-allocation rules](https://www.consumerfinance.gov/rules-policy/regulations/1026/53/)
distinguish excess-payment allocation from minimum allocation. [CFPB finance-charge
rules](https://www.consumerfinance.gov/rules-policy/regulations/1026/54/) address
grace-related charges. They motivate retaining actual card conventions and rejecting
unsupported grace ownership; neither source prescribes the supplied basis-transfer
or original-yield disposal policy in this component.
