# Explicit actual-date consumer graph

`advanced-product-graph-1` is an additive native component. Its first product
variant is `consumer_installment`, dispatching the existing distinct auto/personal
models in `portfolio-model-core`. Original contracts and explicit scenario action
histories generate cash and states in Rust. There are no caller cashflow/metric
tensors, Python financial callbacks, custom discount backends or automatic changes
to saved monthly books. Source histories remain supplied assumptions rather than
fitted borrower behavior. The initial synthetic request is
`examples/models/advanced-product-graph-1.json`.

## Raw inputs, measures and native ownership

The request contains graph/model/input revision and supplied paired-history
source identity, actual as-of and reporting ordinals, currency, ordered scenarios
with explicit run ID/severity and raw collateral curves, a base scenario, bump
basis points and typed positions. Curves retain full source revision, discount
factor nodes, collateral role and explicit tail convention; native logDF lookup
uses the shared market implementation. This fixed-note slice consumes no index
curve. Indexed products must add a native typed index/reset model before adoption;
an index path or financial metric override cannot silently substitute here.

Each position has a native `consumer_installment` contract and complete history
coverage. The contract uses the existing `InstallmentRequest` field layout as a
**known template**, with `identity.measure="contractual"` and empty steps. This
template is never directly projected: the standalone core path correctly requires
physical or pricing measure. The graph explicitly builds four role-specific
requests by attaching the declared history, measure, run and assumption identity
to that same original/known opening state. No physical path is implicitly converted
to a pricing path. Loan/cohort/source identity, actual contractual dates, original
principal/payment, note rate/APR, initial state and recovery policy remain common.
Future action histories differ by role/scenario. Recovery policy currently stays
at its common supplied contract value; macro-linked/scenario recovery overrides
require further typed policy support, not an invented automatic stress.

Every position/scenario supplies `physical`, `pricing`, `pricing_down` and
`pricing_up` arrays. Paths include ordered ID, finite probability weight,
assumption ID and typed native actions. All pricing roles and scenarios require
the same ordered path IDs and exactly equal weights. Physical path probabilities
are separately declared. Each array sums to one within `1e-12`, without automatic
normalization. All paths project and retain complete native cash/state/recognition
and default records, including zero-weight paths; missing scenario coverage fails.
Outstanding terminal principal/interest/unrecognized income/fees or truncated
recovery tails fail rather than disappearing at the reporting horizon.

The supported randomness domain is `supplied_paired_histories-1`. It records a
stable supplied-history pairing, and **does not consume random numbers**. It is
not evidence of a calibrated native stochastic policy or an actual shared-normal
CRN generator. Such policies must later consume and validate native random inputs
and their complete rate/index/HPI/state dependency. The current module rejects
other randomness domains. Paired action histories are raw model inputs, not
caller-prepared financial coefficients.

## Frozen base calibration, signed valuation and scenario risk

`OasTarget` is either an explicit sourced locked spread or a positive-cash target.
The latter projects only the base pricing paths for calibration, requires
nonnegative **total** cash at every future settlement and positive probability-
weighted rights, then solves once within `[-5,5]` with 100 bounded bisections.
Log-sum-exp prices prevent endpoint overflow. A signed or empty/nonpositive
future-right set requires an explicit locked spread; the target is never silently
ignored or resolved under physical cash. Individual fee/cost components can be
negative when their complete event cash remains nonnegative and monotone in spread.

For a pricing path, closing-after-as-of-settlement value is:

`sum_{date > asof} (principal + interest + fees) * D((date-asof)/365) * exp(-base_oas*(date-asof)/365)`.

Signed event valuation uses a compensated native sum and monetary admission.
Path expectations use explicitly supplied weights. Day-zero cash is preserved
in the native source and excluded from this closing value. No survival haircut,
second credit loss or physical-schedule discount fallback is applied.

`BaseAnchor` retains complete canonical raw calibration content and per-position
spread. Collision-safe identity includes original/known contract, source/paired-
history/model identities, base pricing histories/probabilities, target and raw
base scenario/curve. `reprice` compares full canonical bytes and rejects changed
base dependencies with `BASE_ANCHOR_CHANGED`. It never recalibrates. `repriced`
returns a new immutable graph/current request/result retaining the original
anchor, so revised physical capture also uses that anchor. Caller publication
must retain the complete immutable raw request in addition to result identities;
nonbase and physical histories are not all duplicated into the calibration anchor.

Scenario `supplied_history_dv01` is `(PV_down - PV_up)/(2*bump_bp)`: native
projection of the supplied down/up **pricing** action histories, with native
negative/positive collateral zero-rate bumps, always at the same base OAS.
Identical down/up action histories isolate discount sensitivity. Different
supplied decisions add their explicitly declared cash sensitivity. This is not
a claim that the model estimated rate-dependent borrower decisions or full
effective DV01. Raw curves/histories and bumped measures stay independently
admitted; arbitrary saved monthly economics are not switched to this convention.

## Physical interest and credit-state outputs

Physical reporting uses exact `(left,right]` boundaries. Core recognition amounts
are allocated by overlap with their native actual-day simple-interest interval;
instant cash-basis recognition belongs to its explicit recognition date.
`gross_physical_contract_interest` is **gross supplied-policy note-interest**,
not frozen original effective-yield income, provision-adjusted NII, net income
or pricing-measure expected coupons. Fees, principal cash, economic default
principal and principal-only loss remain separate. Principal cash includes
recoveries and must not be mistaken for performing-principal retirement.

Native source defaults retain recognized/unrecognized accrued interest, fees,
principal retirement, gross recovery, cost, net recovery, principal loss and
recovery date. Default removal is not cash. Gross interest already recognized
can require reversal/retention/charge-off under a separately reviewed accounting
policy. Neither the graph nor its gross-interest metric invents that policy.
The original installment formulas and primary product references are documented
in [consumer-installment.md](consumer-installment.md).

## Strict scheduled capture boundary

`AdvancedGraph::capture_scheduled(owners, policy)` prepares typed sources only;
it does not execute a ledger or publish journals. Every position must have an
explicit unique account/source owner, source revision equal to the immutable
graph input revision, receipt direction and `fully_scenario_generated` ownership.
The explicit policy is `consumer_simple_interest_par-1`, book basis
`par_note_simple_interest`, fees `reject`. Opening par carrying value is native
known principal. This is a narrow accounting basis, not discount/premium EIR.

Every physical run must contain exactly one path with weight one. Default,
nonaccrual, delinquency, fees, day-zero settlement, unsupported opening accrued/
unrecognized/fee balances, incomplete rights or multiple expected physical paths
reject the **entire capture**. Pricing paths never become GL sources. Analysis
still retains these cases and all their source events; capture does not prune
economic events. Scheduled payoff without a fee/penalty remains supported.

Native recognition origins are FIFO assigned to actual interest settlements.
Splitting an origin preserves its original exact interval; amounts across all
assignments sum to the original recognition, retaining receivables after partial
interest cash. The explicit simple-interest segment amounts distribute uniformly
inside that original interval, and payment cash is posted once. Output includes
`DatedSchedule`, per-event origin/recognition segments, par opening values,
owner/run identity and explicit full scenario coverage, including `NoEvents`.
Capture schedule selectors/coverage use execution **run IDs**, as required by
the dated store; graph metrics retain their separate economic scenario IDs.
No reverse-severity or missing-source fallback is generated.

Root's later native mapper must construct complete dated-store owner/run/plan
inputs, verify ledger opening reconciliation and exclude duplicate monthly
capture, legacy stress/prepay/credit overlays, sales/pledges and incompatible
income policies. Default adoption must additionally map principal retirement,
accrued impairment, recovery receivable recognition and delayed cash through
the credit coordinator. A recovery cash receipt alone leaves defaulted principal
alive in the scheduled ledger and is therefore explicitly rejected here.
Independent immutable journal replay remains the publication gate.

## APIs, bounds and evidence

`AdvancedGraph::{new,request,snapshot,base_anchor,reprice,repriced,capture_scheduled}`
uses immutable references/`Arc` snapshots; `analyze` is the raw convenience route.
Admission allows 256 positions, 13 scenarios, 128 paths per role, 4,096 aggregate
native runs, 250,000 projected cash/state/recognition rows and eight million
conservative pricing/root/physical-reporting work units. Bounds are checked
before path creation, then actual projected rows are checked too. There are at
most 120 actual reporting periods. Core consumer money/date/action bounds and
native curve bounds remain additional. Canonical input is bounded at 32 MiB;
logical retained input/result/base identity uses conservative four-times
serialized charging within 128 MiB. Bounded counting writers precede identity
serialization. Cooperative checkpoints run across paths, scenarios and root
iterations. These are component limits, not aggregate process/RSS or complete
graph/solver/ledger scale guarantees.

Focused fixtures cover independent leap/month-end payment/interest/PV/risk,
separate P/Q and explicit bumped histories, base-anchor identity and frozen OAS,
auto collateral depreciation through repossession, signed unsecured recovery
cost, loss/retirement versus cash, full native capture/partial-interest origins,
revised immutable capture, expected-path rejection, fees/nonaccrual, zero-event
coverage, terminal/domain/work rejection and the persisted synthetic example.
The focused native suite passed all 16 tests, and the targeted Clippy check
passed with warnings denied. This evidence applies to this component's source
and fixtures; the release boundary must be rebuilt and validated separately.
Kernel fixtures do not establish public/API/cohort integration or empirical fit.
Full #12/#17/#18 acceptance retains native graph/cache/controller adoption,
consumer state-aware credit/daily journals, original effective-yield accounting,
stochastic policy/driver ownership with CRN, source-linked histories and held-out
product/cohort errors. Card, deposit, mortgage, indexed and funding dispatches
require their own native measure/state contracts.
