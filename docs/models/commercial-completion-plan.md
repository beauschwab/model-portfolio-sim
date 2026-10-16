# Remaining commercial acceptance work

## Current development reconciliation (2026-10-16)

This section supersedes the older implementation exclusions below without
removing their wider acceptance requirements. The last accepted installed stage
is 0.29.19: 1,299 native tests, 2,998 engine tests (two skips), 1,311 API tests
(one skip), 24 browser journeys, strict native checks and contract checks passed.
Its source and eight installed artifacts are sealed separately in
[the acceptance review](../reviews/2026-10-16-commercial-source-adoption.md).
Skips, deployment, empirical evidence and complete joint-scale acceptance are not
covered by those results.

Subsequent committed components below have contained native and independent
checks. They have not yet passed a combined installed release regression. Work
still being implemented is identified explicitly.

**Current release blocker:** the expanded 48-cell stochastic-coupon, credit and
call/put family retains 36 passes and 12 independent native/reference failures.
Some failures change exercise decisions and path cash; the largest observed path
difference is about 0.544876 per face100. Three declared failures reproduce on
the optimized development lifecycle binary. Exact native matrix/target capture
and independent reconstruction identify an invalid SVD, including a deterministic
16-by-3 collinear control. Explicit iteration settings do not correct it. A
bounded native correction with decomposition and solve checks, followed by the
unchanged benchmark families and combined installed regression, is required.
The initial combined acceptance attempt is retained and invalidated; full engine
and API acceptance has not been claimed. See
[the numerical family](commercial-credit-gaussian-exercise.md). This is distinct
from the earlier exact-constant metadata correction, whose paired financial
outputs remain identical.

A separate 28-case current-coupon-shaped history family confirms four incorrect
fits returned by the actual native calibration routine, with fitted-coupon error
up to about 13.4 basis points and some adjustment speeds changing from one to
0.02. Two additional cases fail reconstruction checks while fitted-output errors
remain small. The earlier 29 representative caller controls still pass their
declared cases. A shared checked numerical module must preserve the historical
caller's epsilon, floating cutoff and accepted tall/wide shapes; a correction
limited to commercial continuation does not clear this additional release gate.
See [the caller audit](../reviews/2026-10-16-native-svd-secondary-audit.md).

| Work | Implemented bounded scope | Remaining acceptance or engineering |
|---|---|---|
| Separate fees | Explicit loan/fee graph valuation, acquired cost allocation, joint finite exercise, retained unit fee income, finite same-journal candidate purchases, fee-specific closing capital and daily FTP; later conveyed fee capital/FTP, signed prepaid refund acquisitions and generated fee stopping/journals | Combined installed acceptance; draw-tranche/generated secondary fees, wider mixed/saved/cohort adoption and reviewed policies remain |
| Candidate holdings | Complete-origin finite loan/fee replay, identical completed raw/monetary prefixes, prospective gross rights/basis and future postings; exact-cut ordinary modeled predecessor binding; later complete-origin secondary acquisition and coupon entitlement100/live principal80 | Seasoned graph/cohort/strategy coefficients, broader acquired/default/draw rights and observed bank reconciliation |
| Native cut replay | The native component moves the actual Sim through a positive cut into a distinct resumed Sim on the original calendar; component controls cover future acquisitions, gross signed fees, cancellation and historical parity. Complete finite source-tail census is also integrated | The omitted public raw specimen still refuses at 36,844,632 work /223,261,940 bytes before generation; the new short selected specimen refuses at 35,324,604 /147,721,280. Coordinator receipt lifetimes and cut admission are under implementation. Positive raw/persisted publication, predecessor binding and complete union-tail adoption remain open |
| Observable indexed earning | Published-fixing prefixes, separate signed loan/coupon owners, actual settlement rights and reviewed late default/waiver/recovery claims; later separate dated principal and retained coupon nominal | Alternative legal true-up policies and delayed-publication generated loadings |
| Generated paths | Explicit complete Physical path catalogs, canonical source dimensions and paired original anchors; compact candidate journals, positive reviewed actions, counted actual draw-source construction and generated loan/fee streams | Final combined regression is pending; generated draw fees, secondary generated rights and wider joint/mixed scale remain |
| Source capacity | Explicit finite transfer authority counts the true origin and both actual buyer builds, canonical scans and source operations. Complete carry/claim/recovery/body census and before-growth guards now retain separate economic/book roles; selected prepaid exercise and no-option dual holdings have positive independent/persisted component controls | With the new complete-tail census, joint exercise plus dual holding still refuses at 49,586,912 work /54,466,096 bytes. Caps, samples, all actual source passes and complete tails remain unchanged |
| Numerical evidence | Training-only linear/quadratic/cubic bases, exact normal/one-date controls, two-date independent learned-policy integration; 24-cell credit-loss/34-control and 60-cell credit/exercise/73-control benchmarks pass. Exact-constant classification is corrected with unchanged paired financial output | The expanded stochastic-coupon/credit family exposes 12 genuine financial failures in 48 cells. Certified native SVD correction and unchanged full replay are release blockers; broader contract/driver/seed/step/basis convergence and observed validation remain |
| Application | Durable generated stream, fee treasury/holding routes, source/persisted/final SQL fencing, retained fee coefficient evaluation and workbench reopening passed the accepted 0.29.19 checkpoint | Matching release/full regression and browser acceptance for the subsequent combined changes; deployment/restarts and explicit saved-library/session rebuilds |

See [fee adoption](commercial-fee-adoption.md),
[fee funding](commercial-fee-funding.md), [candidate holdings](commercial-fee-holding.md),
[candidate prior openings](commercial-candidate-prior-opening.md),
[observable coupons](commercial-observable-coupons.md),
[generated Physical sources](commercial-generated-physical-sources.md),
[generated candidates](commercial-generated-candidates.md),
[publication](commercial-generated-publication.md) and
[policy refinement](commercial-policy-refinement.md),
[conveyed fee treasury](../reviews/2026-10-16-commercial-conveyed-fee-treasury.md),
[signed acquired prepaid rights](commercial-secondary-prepaid.md),
[principal and coupon entitlement](commercial-principal-entitlement.md),
[finite source work](commercial-finite-source-work.md),
[complete-tail census](commercial-finite-tail-work.md),
[native cut coordinator](commercial-cut-mixed.md) and
[credit sampling diagnostics](commercial-credit-convergence.md).

Original OAS/EIR, separate Physical/Pricing ownership, shared random streams,
final-output tolerances and historical omitted paths remain required. Component
and phase admission bounds are not a process RSS cap. Observed histories and
reviewed accounting/regulatory policies have not been supplied; synthetic controls
cannot complete those evidence gates. Issues 15 and 25 remain open.

This plan preserves the original scope of issues 15 and 25. It describes work
that remains **not implemented or accepted** after the bounded local stages. The 0.29.11 supported
stage and its validation are recorded in
[the follow-through review](../reviews/2026-10-11-commercial-followthrough.md).

The local0.29.13 [holding-window foundation](commercial-continuation.md) implements
complete-origin raw replay, sealed openings and selected subsequent groups. Verified
prior publication, cut-started mixed journal, seasoned coefficients and secondary
acquired rights below remain required; the original acceptance scope is unchanged.

Local0.29.14 additionally implements [complete-origin native daily holding journals](commercial-holding-journal.md)
with a derived balanced cut opening, prior P&L rolled into equity and ordered
prospective postings. Matching installed regression and immutable receipt gates
pass. This completes the complete-origin prospective journal stage; verified prior
publication, a cut-started mixed Sim and broader seasoned/secondary adoption below
remain required. No original acceptance criterion is removed.

Local0.29.15 additionally verifies an authorized prior modeled holding publication
at its exact published cut through [the source-ID route](commercial-prior-opening.md).
Full independent source/journal replay, matching completed postings/opening and
source/object/current-job fences inside the final SQL transaction are implemented
and locally validated. Arbitrary closing or observed bank evidence, cut-started
mixed owners and broader seasoned/secondary integration below remain required.

## Seasoned holdings and acquired rights

Extend the native source/compiler and shared credit/coupon owners with a sealed
complete-origin continuation. Replay acquisition through the cut date, then
select the subsequent posting/reporting window from that same owner. Retain
original cost/EIR, canonical gross legal and recognized lots, P/U/B/G, draw-specific
anchors, nonaccrual/default/recovery progress and reviewed allowances/waivers.
Preserve the original latent waiting hazard and shared credit stream; restarting
CTMC at the cut changes paths. Derive opening from raw replay, rather than accept
caller monetary checkpoints or infer yield/payment from remaining balance.

An existing holding must not post a second acquisition or historical earnings.
Require identical observed cash, fixings, credit events and reviews through the
cut. Only explicit future revisions can change. Reuse complete-prefix comparison
and original-yield/opening-carry patterns while retaining commercial gross rights.
Admission includes prefix, continuation and every surviving recovery/rights tail.

Secondary midcoupon acquisition is a distinct required case. Separate contractual
origin from acquisition and supply conveyed earned rights and clean/dirty cost
allocation. Coupon entitlement can remain 100 when acquired live principal is 80;
pre-acquisition income must not become current income. Full versus cut replay
must match ordered cash, states, lots and recovery clocks. Test post-principal
rights, opposite signed lots, default/nonaccrual/draw/review prefixes, prefix
tampering, missing history and independent persisted opening/posting replay.

## Nonintegral fees

The supplied-history [fee component](nonintegral-fees.md) is locally implemented
and validated in0.29.16. Native credit binding, mixed GL, fee pricing/acquisition
allocation/units/capital/FTP and the full acceptance below remain open.

Add a separate source-owned fee entitlement/service/settlement/default-tail model.
Supply fee identity, sign/refund terms, earning event/period, payment date,
state/tranche eligibility and reviewed exercise/default/recovery dispositions.
Earned unpaid fees create receivables and fee income; prepaid unearned fees create
deferred liabilities released on the service event. Keep signed gross rights and
complete fee tails after loan principal settlement. Coupon recovery fractions or
modeled LGD cannot silently extinguish a fee claim.

Nonintegral fee cash stays outside loan G/EIR and enters total economics/funding
once. Integral fees retain their existing loan owner. Add explicit fee GL accounts,
snapshots, daily reconciliation, treasury lineage and independent replay. Purchase
allocation must distinguish fee rights from loan quote/OAS/EIR; do not silently
price a fee right inside the loan acquisition anchor.

Hand controls: a zero-EIR loan100 plus fee5 earned day10/paid day20 retains loan
G100 and recognizes a receivable/income5 once; fee paid day5/earned day10 retains
deferred liability5 before service. Cover opposite signed fees, reviewed fee
default/recovery tails, exercise with earned/unearned rights, mixed fee types and
graph/unit/persisted reconciliation. Additive GL/protocol changes must preserve
historical readability and require explicit compatible native rebuilding.

## Indexed coupons earned while fixings arrive

The present commercial credit owner requires every fixing influencing a cash or
partial-earning amount to be available by that payment or earning-segment start.
This prevents future information from entering exercise and recognized income.
It does not provide general daily earning for an in-arrears overnight coupon
whose fixings become available during its accrual period. The broader standalone
indexed cashflow calculation does not establish this credit/journal adoption.

Introduce native prefix coupon entitlement from the actually available fixing
sequence, with supplied lookback, observation shift, lockout, publication clocks,
day-count and compounding policy. Update earned/legal lots at those clocks and
retain the separately complete contractual settlement amount. Do not recognize
the final full-period coupon pro rata before its required observations exist.
Default/nonaccrual and exercise must act on the same observable prefix owner;
original acquisition EIR and current contractual reset policy remain separate.

Hand controls require changing daily fixings, unequal calendar weights,
simple/compounded conventions, publication delay, default before the last fixing,
and rights surviving principal settlement. Early cash that requires unavailable
fixings must reject or use a separately reviewed settlement policy. Independent
source, graph, income, ordered posting and persisted replay must agree without
weakening observability guards or financial tolerances.

## General generated original OAS

The bounded retained-Q graph and acquired-unit implementation is now present in
local0.29.12. Its [contract](commercial-generated-oas.md) and
[validation review](../reviews/2026-10-12-commercial-generated-oas.md) retain the
separate installed acceptance and convergence boundaries. The requirements below
remain the original design/acceptance contract; this stage does not establish
broad model convergence, empirical accuracy or all requested mixed adoption.

Introduce a sealed native calibration sample owner. Generate original Q training
and distinct evaluation tapes, observable features and native no-option cash once.
Every bounded OAS trial recomputes discounted backward targets and refits
continuation on the identical retained training sample; evaluate on the identical
distinct evaluation sample. Reusing stops fitted at a different OAS changes the
economics. Preserve reduction order and common random numbers.

Policy switches can make the finite evaluation objective discontinuous or
nonmonotone. A deterministic bounded bracket solver must accept only when final
actual native stopped Q cash reproduces the original quote at unchanged tolerance.
Unbracketed, ambiguous or unresolved quotes reject. Generate the original P
cost/EIR proof only for the accepted Q policy. Retain exact selected OAS bits in
all current scenarios/bumps; P never supplies the Q calibration target.

Preflight charges retained samples, all allowed trials, final stopped replay and
coexisting graph/proof receipts within the existing generated 4B work/128 MiB
limits. Test deterministic known-spread recovery, actual policy-boundary exercise,
unreachable/discontinuous residual refusals, repeatability, immutable original P
proof, independently rejected tampering and cancellation/no-publication.

## Generated candidate streaming and opening evidence

The current compiled journal borrows every full accounting owner. Materializing
all generated paths recreates the rejected capacity footprint. Add a sealed
compact journal source: materialize one genuine P recipe, copy native atomic
postings and absolute financial endpoints, intern identities, then drop its full
owner. Retain gross lots and snapshot fields as absolute f64 values; avoid
reconstructing money from repeated deltas. Finite and compact sources feed the
existing daily Sim posting/staging/reconciliation in the same total order:
scenario/day, global native year, binding index, native group index.

Keep ordinary candidate admission unchanged. An explicit generated route must
charge joined work within the existing 4B tier while retaining separate graph and
journal phase caps and combined 128 MiB capacity. Refine structural bounds for the
actual compact event/state passes within ledger 8M/32 MiB limits. Results beyond
the buffered envelope require bounded immutable partitioned emission, rather
than a larger JSON limit.

Compare compact versus borrowed ordered postings, gross daily/closing balances
and trial balances. Admit a minimum128-path control with at most one live full P
owner; cover real exercise/default/recovery/signed rights, zero allocation,
purchase offsets, foreign proofs, missing/duplicated/out-of-order records,
persisted tampering and cancellation at materialization/stream/publication.
Interactive evaluation remains coefficient-only.

Resolve opening evidence from an authorized immutable completed journal: replay
its raw source and persisted postings, bind exact closing date/currency/accounts,
reconcile signed opening, and retain all source/partition hashes through final
SQL publication. Cash/equity-only prior closing is a bounded first stage. Surviving
backbook principal/basis/allowance/rights need native prefix and future ownership;
closing scalar balances alone are insufficient. Modeled-journal verification is
not observed bank/custodian reconciliation; observed evidence remains separate.

## Empirical and integrated completion

Observed issuer/commercial histories and reviewed accounting/regulatory policies
must be supplied. Retain chronological fitting/holdouts, source lineage,
uncertainty diagnostics and independent verification. Synthetic mathematical
controls do not establish predictive accuracy. Complete saved-book/cohort/mixed
strategy and product capital/FTP adoption, original dependency gates and full
60,000-position mixed graph/credit/solver/journal acceptance remain required,
including deadlines, cancellation, immutable source/artifacts and final authority
fences. Completing the bounded stages above does not close these criteria.


Local0.29.16 adds the supplied-history [separate fee owner](nonintegral-fees.md).
It implements earning, signed gross rights, cash, prepaid refund, reviewed waiver/
cancellation and recovery intents with full supplied tails and independent replay.
The native event binding, mixed GL, graph/unit/acquisition allocation/capital/FTP,
observed and full-scale fee acceptance above remains required.


## Native loan and separate fee journal (local0.29.17)

`commercial-fee-journal-1` binds reviewed original-tranche fee earning/cash and
separate exit dispositions to one already generated physical commercial loan
owner. Native default/prepayment and explicit reviewed maturity events determine
waiver/refund/cancellation/recovery; fee LGD is not borrowed from loan policies.
Separate signed gross rights enter the same existing daily Sim once, with complete
fee tails, unchanged loan cost/EIR/carry and retained unweighted zero allocations.
Protocol7 adds fee lineage/daily/closing snapshots; historical6 is unchanged.
Actual fractional clocks cannot round into the preceding day. Independent raw
source, ordered shared journal, gross/report and persisted replay plus original
deadline/executable/object/final SQL authority fences gate durable publication.

This bounded lane does not yet adopt exercise/candidate/holding fee sources,
draw/secondary fee rights, fee acquisition allocation, Pricing graphs/units or
capital/FTP. In-arrears earning, generated streaming, saved/cohort/mixed strategy,
observed/reviewed histories and full60000 combined acceptance remain required
under15/25. See docs/models/commercial-fee-journal.md and the complete ledger.
Rebuild compatible native artifacts, restart deployed workers and explicitly
rebuild selected saved sessions/libraries before use; local validation does not
deploy or close the full issues.

## Reviewed earned-fee refunds (0.29.18 follow-through)

The standalone fee owner and native loan/fee journal now implement explicitly
permitted refunds of earned AND collected service, with separate cumulative
refund ownership, reviewed exit fractions, gross signed snapshots and reversed
fee income/cash. Refunds cannot recreate service/collection capacity or use
prepaid cash or waived recovery as collected earning. Loan original cost/EIR and
credit paths remain unchanged. Omitted selections retain historical layouts and
admission. Independent raw-source and persisted replay remain required.

This removes the earned-refund subtask from the earlier exclusion lists. All
other remaining engineering, data/policy and complete combined acceptance work
above stays open, including fee Pricing/acquisition/unit/capital/FTP adoption,
exercise/candidate/holding and draw/secondary rights, in-arrears coupons,
generated streaming/convergence and full saved/cohort/mixed adoption. It does
not establish observed accuracy, policy approval, deployment or issue completion.
