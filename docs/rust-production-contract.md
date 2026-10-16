# Rust production contract and Python calculation deprecation

Effective engine version: **0.29.4**. This supersedes earlier instructions to keep
Python as the default or to select Rust only after workload-specific measurement.

## Execution policy

- Rust is the default built-in financial runtime for `RunConfig`, context-free
  public engine dispatch, new API settings and settings missing the backend field.
- The application accepts only Rust for new calculation execution. Python, NumPy
  and Numba execution choices are removed from production requests and controls.
  The engine's explicit Python path remains an independent validation oracle.
- Production pricing and what-if use the native dependency graph and coordinator.
  Adapters must pass no custom discount-backend object: such an override previously
  selected the Python graph even when product computation was Rust.
- Implement new built-in product, simulation, dependency/coordinator, capital/FTP
  and strategy financial logic in Rust. Keep HiGHS in C++; Rust constructs its
  financial problem, invokes it and validates allocations.
- Missing or incompatible native artifacts fail explicitly. Never silently fall
  back to Python, weaken parity tolerances, or substitute a simplified calculation.
- FastAPI, the worker service, persistence, artifact transport, cohort audit
  coordination and independent verification still contain Python. This contract
  deprecates the selectable Python calculation backend; it does not claim those
  application services have been rewritten. Further service migration is separate.

## Existing workspaces

Explicit historical Python settings and immutable results remain readable.
Execution rejects them with `PYTHON_BACKEND_DEPRECATED`; change the workspace
setting to Rust through Settings or `PUT /settings`, then rebuild unit libraries
and decision sessions. This is a revisioned user edit, not a silent mutation of
saved snapshots. New requests attempting to save Python receive HTTP 422; jobs
submitted with historical Python workspace settings receive HTTP 409. The worker
also rejects Python snapshots independently of API admission. Old-build queued
jobs may first fail the existing `MODEL_VERSION_CHANGED` identity check.

Do not reuse Python-prepared interactive libraries. Synchronous evaluation remains
coefficient-only and explicitly requires a Rust library. Do not rewrite previous
job manifests or report results to give them a different backend identity.

## Build and deploy

Build the product, ledger and decision/workflow artifacts on the target platform:

```text
uv run --project apps/api python scripts/build_native.py
uv run --project apps/api python scripts/build_ledger_native.py
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py
```

`make build-native` provides these steps; `make build` also builds the web app.
Deploy matching source and artifacts, restart API and workers, and rebuild cached
libraries/sessions. Do not replace native libraries inside running processes.
Database portability (SQLite/PostgreSQL) and immutable local/S3 Parquet storage
remain unchanged. No live database rewrite is part of the default change.

## Agent and test requirements

The root and nested AGENTS files, GitHub Copilot instructions, engine operator
skill and its packaged references all carry this policy. Historical reviews retain
their dated findings; they are not current execution instructions.

Keep independent Python expectations explicit. The engine test harness enters an
explicit Python reference context; native ownership/parity cases explicitly enter
Rust. Pre-existing reference `RunConfig` instances name Python, so a changed default
cannot make a parity test compare Rust against itself. Fresh-process production
tests run outside that reference context and check defaults, native execution and
failure when required artifacts are absent. API tests forbid Python graph/coordinator
callbacks and verify rejected options, worker fencing and persisted configuration.

Reference Python is test support, not a supported production deployment option.

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

## Checked numerical fitting and cut diagnostics (local 0.29.20 development)

Rust continuation and coupon/spread-history fitting share bounded checked native
least squares. Preserve each caller's original matrix shape, decomposition
epsilon and computed singular cutoff. Valid checked legacy candidates retain
their original coefficients; invalid decompositions require direct native repair
or explicit failure. Tall continuation and wide history admission remain separate.
No normal-equation replacement, feature deletion, tolerance relaxation or Python
financial fallback is allowed. History work/bytes are checked before design and
target allocation; continuation reserves each actual fit before generation.

Structural numerical-column bounds use the exact raw role and paid complete
hazard/covariance/header census. Every feature, path, normalization/basis pass,
source owner and stopped fee payoff remains. The separate fixed-fee observation
proof requires zero declared hazards, Supplied principal and no draws, reviews
or transfers. No sampled equality grants admission. See docs/models/commercial-checked-least-squares.md and
native-history-least-squares.md for limits and evidence.

The selected encoded cut-receipt route is diagnostic only: its complete source,
live output batch/flush and final-result census still refuses the genuine short
case. Publication explicitly rejects until independent raw-derived admission is
implemented. See docs/models/commercial-cut-receipt-admission.md. Protocol8
omission, all prior refusals and original issue15/25 gates remain unchanged.
The last fully accepted installed checkpoint remains0.29.19 until matching
combined artifacts, complete regressions and publication checks pass. Rebuild
matching native artifacts, restart deployed workers and explicitly rebuild saved
libraries/sessions before executing changed economics; no deployment is implied.
