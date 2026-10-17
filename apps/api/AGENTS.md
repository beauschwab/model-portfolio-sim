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

## Formal hedge relationships (local 0.29.19)

`POST /models/runs` with `hedge-accounting-1` queues the explicit ASC 815 lane on
the durable worker with the original queue deadline, strict raw-body duplicate
rejection and a 32 MiB bound. The worker runs the standalone native process,
independently replays the derivative source, relationships, assessments and the
single ordered journal (`hedge_accounting_reference.verify`), formats lossless
tables and re-verifies the persisted result before publication. The receipt keeps
`accounting_standard_approval`, `persisted_journal_verified`, `saved_book_adoption`
and `portfolio_layer_supported` false; nothing feeds capital or FTP. See
`docs/models/hedge-accounting.md`.
`hedge-treasury-1` follows the same durable route and receipt rules; its
independent reference (`hedge_treasury_reference.verify`) recomputes closing
rows, eligible AOCI and capital ratios, and the receipt reports the reviewed
cash-flow-hedge AOCI eligibility decision.
