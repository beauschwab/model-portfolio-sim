# API and worker contracts

## Production execution contract (0.29.4; supersedes historical defaults)

The authoritative policy is `../../docs/rust-production-contract.md`.
Follow its deployment, compatibility and validation rules. Rust owns built-in
pricing, graph/coordinator, simulation, capital/FTP and solver financial logic.
HiGHS remains C++; Rust constructs/invokes/validates its financial problem.
Python API/storage adapters and independent verification remain application services.

Rust is the new/omitted backend default. Historical Python inputs/results remain
readable unchanged but need an explicit Rust selection and library/session rebuild
before execution. Missing/mismatched native artifacts fail without fallback.
Production pricing/what-if enters the native graph without a custom discount override.
Never add Python/NumPy/Numba financial options to production requests or UI.

Preserve fixed base OAS, shared CRN, unchanged final-output tolerances, immutable
verified publication and coefficient-only interactive evaluation. Python test
oracles are explicit and independent; fresh-process default/ownership tests run
outside their reference context and forbid Python financial callbacks.

- `store.py` is the existing repository-shaped facade and engine adapter.
  It contains no product-pricing mathematics. `persistence.py` implements
  durable operations; `repository.py` is portable SQLAlchemy Core targeting
  SQLite and PostgreSQL. Do not add PostgreSQL-only SQL to shared operations.
- One process serves one configured tenant/workspace. Never trust a request
  parameter to choose a tenant or object-storage prefix. End-user authentication
  and routing belong at the authenticated gateway until explicitly implemented.
- New mutable inputs must enter the typed snapshot, revision commit, and run
  manifest. Write artifacts before committing references; never publish done
  before storage succeeds. Never pickle user-controlled or persisted objects.
- Keep every query scoped. Concurrent edits compare the expected revision.
  Worker publication checks its lease, attempt token, cancellation status, and
  the current input revision for interactive state, in one transaction.
- API replicas enqueue durable jobs; only `app.worker` runs heavy calculations.
  Keep one worker process per workspace. Its native solver handles and pricing
  caches are process-local; accepted session commands and inputs are durable.
- Strategy evaluation uses prebuilt coefficients only. Recover/reprice outside
  request handlers. Never expose uncommitted worker candidates to evaluators.
- Preserve fixed-OAS, common-random-number, and frozen-prepayment behavior.
  Database durability does not change model coverage or validation status.
- Parquet local/S3 persistence and optional `app.iceberg` delivery are implemented.
  Iceberg rows and run markers commit together per table; the SQL receipt marks
  completion across tables. Preserve retry idempotency and original artifact roots.
  `app.maintenance` is offline-only when applying deletion; never present its
  operator offline assertion as an online concurrency lock.
- `uv run --extra dev python -m pytest tests -q` is the API gate. Set
  `TEST_POSTGRES_URL` to repeat storage and process-restart tests on PostgreSQL.
  Tests must use temporary files, unique workspaces, and loopback ports. Never
  modify a user's existing dev or production database to validate changes.
- Keep `docs/production-storage.md` and `.env.example` aligned with behavior.
- `/balance-stress/stream` is durable-worker-only. Preserve queued native identity,
  cancellation/lease fencing and partition descriptors; never hydrate its full
  journal in the API or concatenate all partitions for paging/downloads. Large-book
  admission requires both the deployment flag and an explicit request. Existing
  stress/saved-book routes remain independent of this opt-in result contract.

- `RiskSettings.compute_backend` selects the built-in product path and must be
  propagated into worker, incremental and decision `RunConfig` snapshots. Keep
  the prepared-discount request `backend` independent. Never silently fall back
  when native libraries or custom-model support are missing. Native saved-book
  replay materializes only the existing bounded route; large journals still use
  `/balance-stress/stream` descriptors.
- Durable model identity includes the selected `portfolio-workflow` executable.
  Preserve both saved-book preflight and executed-hash publication checks; a
  process-cached identity alone does not detect a binary replaced after startup.

## Architecture and interface maintenance

Current architecture: `../../docs/architecture.md`. Interface/transport and dependency rules are in
`../../docs/contracts/README.md`. Run `make check-architecture` from the root
for boundary/contract changes. Dated version sections document historical migration
stages; current production and shared-model contracts supersede their defaults.
Preserve independent reference tests and all applicable numerical/publication gates.

## Commercial follow-through (local 0.29.11)

This stage supersedes the unsupported-status notes in the local 0.29.10
commercial section. Native canonical cumulative coupon entitlement reconciles
legal balances without terminal plugs. Explicit performing draw tranches retain
actual funding, separate original cost/yield and complete source rights. Reviewed
provision, chargeoff, legal waiver and recovery actions retain separate modeled,
book and legal money. Before/after cash and reviewed rights-only exercise use
native phase ownership. Acquired units retain a separate original stopped
Physical cash proof; current scenarios cannot refit original effective yield.

The generated exercise lane fits native observable Gaussian market/credit
continuation on Pricing training only and evaluates fresh paths. An explicit
compact generated graph tier retains separately supplied Physical laws, paired
Pricing streams, locked original OAS and sealed original Physical acquisition
proofs. Native retained coefficients remain the sole interactive evaluation
inputs. Ordinary source/graph/journal budgets are unchanged. Component retained
capacity is not aggregate RSS or complete mixed-book scale acceptance.

`commercial-candidate-journal-1` reuses genuine finite native Physical owners in
the existing daily Sim at actual purchase dates, with exclusive commercial-loan
or issuer amortized-cost security classes. AFS/HTM marks, institution funding
liabilities, generic mixed backbooks and generated streaming candidates reject.
Supplied opening identity is provenance, not independently verified opening
accounting evidence. `commercial-treasury-1` independently replays reviewed
complete-tail journals for explicit closing capital and bounded expected FTP;
regulatory eligibility, requirements, units and policies remain supplied.

Durable analyses, acquired libraries and journals retain original deadlines,
raw input and executable identities, independent raw-source/persisted replay,
artifact rereads and final SQL cancellation/lease/token/revision/attempt fences.
Historical schemas/results remain readable; additive execution requires rebuilt
native artifacts and explicitly rebuilt selected saved libraries/sessions.

Full #15/#25 completion remains open: seasoned acquisition prefixes, nonintegral
legal fee ownership, general generated OAS calibration, path-specific generated
draw/review accounting and streaming mixed candidates, complete saved-book and
mixed adoption, observed histories/reviewed policies and full 60,000-position
joint graph/credit/solver/journal acceptance. Supplied synthetic laws and passing
local checks do not establish empirical accuracy, policy approval or deployment.
See `docs/models/commercial-generated-exercise.md`,
`docs/models/commercial-treasury-candidate.md` and the complete implementation
ledger. Production API/workers have not been restarted by this local work.

## Retained-Q commercial original OAS (local 0.29.12)

Explicit original generated quote calibration retains one native Pricing training
and distinct evaluation sample. Every trial refits continuation on training only;
final actual stopped native cash must meet the unchanged original quote tolerance.
Unbracketed, ambiguous, sampled nonmonotone and unresolved gap quotes reject.
Accepted policies and cash enter graph/acquired-unit preparation without original
sample regeneration. Current scenarios/bumps retain exactly the selected original
OAS; Physical acquisition cost/EIR remain separate supplied and verified anchors.
Interactive evaluation stays coefficient-only. Existing generated4B/128MiB and
ordinary component limits are unchanged; joined work can refuse expensive controls.

Calibrated process/C ABI requests select original-entry native execution controls
through decode, trial fitting, replay and bounded serialization. Durable jobs retain
original queue deadlines and source/executable/object/SQL authority fences. Historical
omitted/locked selections retain their routes. Additive fields require compatible
product, ledger and decision/workflow artifacts and selected-library/session rebuilds.
Local implementation does not restart deployed workers or close issues15/25.

Seasoned holdings, separate nonintegral fees, daily in-arrears indexed earning,
generated path-specific draws/reviews and mixed candidate streaming, verified opening
and complete saved/product adoption, empirical histories/reviewed policies, general
basis/path convergence and full60000-position mixed acceptance remain required.
See docs/models/commercial-generated-oas.md and the complete implementation ledger.

## Complete-origin commercial holding windows (local 0.29.13)

`commercial-holding-continuation-1` replays complete original acquisition histories
and derives a sealed opening plus subsequent native posting indices. Original cost,
EIR, gross coupon lots, draw anchors, undrawn commitment and reviewed default state
remain retained. Completed raw/economic prefixes and CRN identities cannot change.
Future interval/projected assumptions require explicit retained source conventions.
No caller monetary checkpoint, credit restart or second acquisition is accepted.
Independent raw and persisted replay, original deadlines and final source/executable/
object/SQL publication fences remain required. Rebuild compatible native artifacts
and explicitly rebuild selected saved sessions/libraries before additive execution.

This is a holding-window foundation; verified prior-publication opening, cut-started
mixed daily adoption, seasoned graph/cohort/strategy coefficients, stopped acquisition
and secondary clean/dirty rights allocation remain open. Fees, daily in-arrears
coupons, generated streaming, empirical/policy evidence and complete mixed60000
acceptance remain required. See docs/models/commercial-continuation.md and the full
implementation ledger. Local checks do not imply deployment or full issue closure.


## Complete-origin commercial holding journals (local 0.29.14)

`commercial-holding-journal-1` transfers sealed complete-origin continuation
owners into one existing native daily Sim, retaining original credit clocks,
yields and complete cash/rights/recovery tails. The prospective journal derives
a balanced cut opening from that full journal, rolls prior signed P&L into
opening equity and retains ordered future postings with original transaction
lineage. Closing non-P&L stock bits must remain identical. Python independently
recomputes full raw sources, the complete daily journal and persisted window.
Original execution/publication deadlines and source/executable/object/SQL fences
remain mandatory. Inner admission caps and final tolerances remain unchanged.

This is complete-origin daily replay with a derived journal window. Verified
prior-publication opening, a cut-started mixed Sim, seasoned graph/cohort/strategy
coefficients, secondary conveyed rights and empirical evidence remain separate.
Nonintegral fees, daily in-arrears indexed earning, generated draws/reviews and
streaming, complete saved/product adoption and mixed60000 acceptance remain
required under15/25. See docs/models/commercial-holding-journal.md. Rebuild all
matching native artifacts and explicitly rebuild selected libraries/sessions;
local validation does not deploy or close the full issues.


## Verified modeled commercial prior openings (local0.29.15)

`POST /models/holding-journals` resolves a completed native holding source in
the authorized repository scope, independently replays its raw source/full
persisted journal, and binds its exact published cut opening and every completed
posting to the new complete-origin native replay. No caller balances, source
object descriptors or verification flags grant authority. Separate execution
evidence records verified modeled publication; native producer flags remain
unchanged and observed bank reconciliation is not inferred.

Both holding schemas now use the shared commercial final SQL guard, including
original queue deadlines, source/output bytes, executable/bundle, workspace
revision, request/attempt/token/lease and cancellation. Prior source authority and
all consumed objects are rechecked under that lock. Historical source deadlines
remain provenance; new work obeys the new job's original budget. Integer native
clock reconstruction must preserve retained milliseconds without a float round
trip. Shared bounded source transport retains existing FTP capacity limits.

Cut-started mixed replay, seasoned coefficients and secondary conveyed rights,
fees/in-arrears earning/generated streaming, observed validation and full mixed
60000 acceptance remain required. See docs/models/commercial-prior-opening.md.
Matching deployment/restart and explicit selected-library/session rebuilding
remain required; local numerical/source gates do not close full15/25.


## Separate nonintegral fee ownership (local0.29.16)

`nonintegral-fee-accounting-1` replays reviewed physical fee earning, cash,
prepaid refunds, unpaid-right waivers, future-service cancellation and recovery
in Rust. Signed gross receivables/payables and deferred/prepaid balances retain
complete supplied tails separately from loan principal/G/EIR. Exact source,
service/review/clock and independent ordered/persisted replay gate the durable
analytical route and its original deadline/final SQL authority fences.

This additive component does not bind native credit events or enter the mixed
Sim, graph/units, acquisition fee allocation, capital/FTP or saved economics.
Those integrations, earned refunds/secondary rights, observed/reviewed evidence
and complete mixed60000 acceptance remain required under15/25. Historical
cash-income schemas and ABI/protocols remain readable unchanged. See
`docs/models/nonintegral-fees.md`; matching artifacts/restarts and explicit
selected-library/session rebuilds remain required before adoption.


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

## Commercial source and adoption follow-through (local 0.29.19)

Explicit fee graphs retain separate original loan OAS/EIR and fee spread/quote/
acquired cost, one joint exercise decision, complete signed fee rights and native
unit coefficients. Interactive evaluation uses retained separate fee-income
coefficients; external NII and the solver objective are unchanged. The additive
income-coefficients version1 exports do not alter numeric product ABI10.

Observable published-fixing prefixes and reviewed late coupon claims retain
actual information, recognition, settlement and recovery clocks. Generated
Physical path catalogs require complete ordered coverage, canonical draw sources,
path-paired original anchors and explicit reviewed actions; P outcomes never fit
Q continuation. Selected linear/cubic bases retain training-only fitting and the
existing work cap; historical quadratic ordering/output remains unchanged.

`commercial-generated-candidate-journal-1` uses compact native owners and bounded
partitioned publication. `commercial-candidate-fee-treasury-1` replays actual
candidate loan/fee journals and supplied separate capital/FTP policies.
`commercial-candidate-holding-journal-1` checks complete-origin source and monetary
prefixes before exposing a prospective window; it is not a cut-started mixed Sim.
The source-ID route additionally verifies an ordinary modeled predecessor at its
exact published cut, including gross unweighted loan/fee rights. Native prior
verification flags remain false; independently verified API publication evidence
retains source/object/current-job authority through the final SQL transaction.
Raw/persisted independent replay, original deadlines and final SQL/source/artifact
fences remain required. Historical omission/protocol paths remain separately
readable; component/phase bounds are not an aggregate RSS guarantee.

See docs/models/commercial-fee-adoption.md, commercial-fee-funding.md,
commercial-observable-coupons.md, commercial-generated-publication.md,
commercial-policy-refinement.md and the complete implementation plan. Matching
product/ledger/decision artifacts, service restarts and explicit selected library/
session rebuilding are required for adoption. Local implementation/tests do not
deploy services or close15/25. Secondary/conveyed rights, broader mixed/saved/
cohort adoption, reviewed/observed evidence and full60000 joint acceptance remain.

## Commercial transferred rights and generated fees (local 0.29.20)

Finite secondary acquisition replays the complete contractual origin and retains
buyer original OAS/EIR, separate clean/dirty cost and gross conveyed fee stocks.
Explicit dated principal can leave coupon entitlement of 100 with live principal of 80;
the buyer does not recognize the seller's prior income. Reviewed signed prepaid
refund acquisitions and acquired fee valuation offsets retain their own owners.
Closing capital and source-channels-4 FTP replay that same candidate journal;
regulatory eligibility and reviewed capital/FTP policies remain supplied inputs.

The explicit finite transfer source-work selection counts the actual origin and
both actual buyer builds, canonical scans and all stopped/proof owners. Historical
omission retains its original work floors and receipts. Admission does not prune
paths or complete recovery tails, infer monetary checkpoints, enlarge budgets or
provide an aggregate RSS guarantee. Broader joint exercise/holding and cut-route
admission remain separate engineering and evidence gates.

The selected complete-tail census keeps contractual cash rows separate from
coupon-claim lots and uses every supplied recovery policy across prepared
intervals. Before-growth guards cover all retained categories, nested bodies and
terminal traces. Explicit economic-only owners omit only the accounting book
they never compile; reviewed Physical owners retain that book. Original/buyer
source passes, signed rights and complete tails remain charged. Historical
omission is unchanged. See docs/models/commercial-finite-tail-work.md.

`commercial-generated-candidate-fee-journal-1` retains joint loan/fee exercise,
separate fee spread/quote/acquired cost, full signed fee cash and recovery tails,
and bounded compact native owners in protocol 7. Loan OAS/EIR/NII are separate;
Physical outcomes never fit Pricing continuation. Raw-source, ordered journal,
persisted partitions and final SQL/source/executable/deadline ownership checks
gate publication. Historical fee omission and generated protocol 6 remain readable.
Generated draw/review fee catalogs and generated secondary transfers reject.

The native protocol 8 cut component moves the actual reconciled Sim into a distinct
resumed Sim on its original calendar. It retains prior P&L in its original GL,
gross rights, cash, basis, source cursors and policy counters. Component controls
do not establish positive raw coordinator publication: the omitted raw specimen
still refuses joined admission before generation. Verified predecessor binding,
complete union tails and wider mixed adoption remain required.

Exactly constant exercise features are centered from their retained observation,
preventing roundoff from creating false varying regressors after defaults reduce
the training sample. Varying-column reductions and all tolerances are unchanged.
The unchanged 60-cell credit/call/put family now passes independent replay; paired
financial values, decisions, standard errors and admissions are identical before
and after the correction. This corrects regression metadata. The 24-cell credit
and 107-cell general benchmark retain their separate finite-family scope.
See docs/models/commercial-credit-exercise-convergence.md for the retained failed
predecessor, corrected evidence and analytical limitations.

Numerical evidence separates source replay, continuation approximation, sampling
error and observed accuracy. Finite supplied-law default-loss controls do not
prove empirical calibration. All original issue #15/#25 criteria remain required,
including saved/cohort/strategy/product-accounting adoption, reviewed policies,
observed holdouts and complete 60,000-position graph/credit/solver/journal acceptance.
The last sealed installed checkpoint is 0.29.19; matching combined 0.29.20 release
builds and full regression acceptance are pending. Local source/tests do not
deploy services. Rebuild matching product/ledger/decision/workflow artifacts,
restart deployed workers and explicitly rebuild selected saved libraries/sessions
before additive execution. Numeric product ABI 10 and historical protocols remain.

See docs/models/commercial-completion-plan.md, commercial-secondary-prepaid.md,
commercial-principal-entitlement.md, commercial-finite-source-work.md,
commercial-generated-fee-adoption.md, commercial-cut-mixed.md and
commercial-credit-convergence.md for exact supported policies and remaining gates.
