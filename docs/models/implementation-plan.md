# Product model enhancement implementation and evidence

Tracking scope: [GitHub issue 25](https://github.com/beauschwab/model-portfolio-sim/issues/25).
This document records implementation stages, not a reduced definition of completion.
Every child issue's acceptance criteria remain required. A numerical implementation
is not an empirical calibration, a regulatory methodology approval or a deployment.

## Formal ASC 815 hedge relationships (local 0.29.19; initial lane validated)

`hedge-accounting-1` implements issue #35 stages 0-4 and the durable part of stage 7
for the initial interest-rate lane: fair-value and cash-flow relationships on one
reviewed `derivative-accounting-1` Physical source, explicit hedged items/forecast
exposures, append-only lifecycle, cumulative dollar-offset assessment under reviewed
bounds at accrual-free dates, mark re-ownership into OCI, hedged-period
reclassification, frozen OCI release, basis amortization/derecognition and one
ordered journal. See [the contract](hedge-accounting.md). The rule matrix is
`asc815-asu2017-12`; ASU 2022-01 layers, the ASU 2025-09 dual-hedge election,
net investment, standalone options and defaulted paths refuse explicitly.
Forecast timing follows a reviewed `forecast_timing` policy: a hedged period
delayed within the documented window keeps its measured settlement and posts
on the new date, and a single period probable of not occurring releases its
pro-rata OCI share at once (native and engine hand-value cases with window,
late-recording, policy and repeat refusals).
Accrual-free dates are now any date strictly inside no period of the instrument
legs and hedged item/exposure, so forward-starting instruments and forecast
issuances are designated and assessed before their first period (native and
engine case: forecast debt issuance hedged by a forward swap terminated at issuance,
with OCI released over the issued coupons).
Jointly designated swaps (`additional_instrument_position_ids`, 815-20-25-45) form
one hedging instrument with per-swap proportion accounting and mark re-ownership
(native and engine hand-value cases with refusals).
Partial-term fair-value hedges (`hedged_term_end_ordinal`, 815-25-35-13B) measure
the benchmark component to an assumed maturity at a coupon period end and amortize
frozen basis to that date (native and engine hand-value cases with refusals).
Relationships designated before the derivative opening date carry a reviewed
`opening` block (hedged value, cumulative changes, basis/OCI balance) posted once
against opening equity, with measurement continuing from the derivative opening
clean values; evidence: two native tests (hand values for seasoned fair-value and
cash-flow relationships with a matured unpaid pre-opening coupon, nine refusals)
and 15 engine checks with independent replay and tamper refusals; the
`hedge-treasury-1` feed accepts seasoned and combined-option hedges with
independent row/capital replay. Openings also carry the remaining designated
scale and frozen basis/OCI tranches (two further native/engine hand-value cases
with eight refusals each). Verified prior-publication opening evidence remains open.

ASU 2025-09 (issued text, prospective under 815-20-65-7) is implemented for the
two interest-rate elections: similar-risk groups of forecast exposures with the
55-23A(a) pro-rata instrument method or the 55-23A(b) pairwise per-unit method,
similarity assessed at every assessment and 55-23D dedesignation as of the last
similar date (OCI through that date retained, later instrument changes returned
to earnings and later reclassifications reversed), and choose-your-rate liability
debt with documented index options, per-option hypotheticals with terms as of
inception, rate elections with the final retrospective assessment on the prior
rate and ineffective-period earnings recognition (35-37I through 35-37M).
Evidence: four native tests (hand values for both methods, dated dedesignation,
rate election and ineffective period, sixteen refusals, election-only
invariance) and 23 engine checks with independent replay and tamper refusals.
The `swap_written_option_exception` election (815-20-25-88) designates a swap
plus a written cash-settled swaption of matching notional as one instrument:
combined clean value and cumulative change, mark re-ownership of both positions
and proportion tracking on the option, with the hypothetical derivative method
required. Evidence: two native tests (hand values with the option's cash expiry
inside the hedge window, nine refusals) and 15 engine checks with independent
replay and tamper refusals. Tenor/timing changes, forecast-issuance
documentation updates, replacement-debt designation, qualitative similarity,
pre-inception-expiring written options and optioned hedged items/hypotheticals
remain open; physically delivered written options continue as the delivered
swap under the same mapped position (native and engine hand-value cases).

Evidence (worktree `claude/hedge-accounting-35`, Windows, rebuilt release binary):
six native hand/lifecycle/refusal/protocol tests plus the unchanged 45 derivative
contract/accounting tests and strict all-target Clippy; 26 engine checks (hand
values, partial dedesignation, sale, forecast outcomes, failed assessment, supplied
hypothetical, eleven tamper refusals, eight native refusals, fresh-process ownership,
cancellation, strict scalar and integer categories) with the derivative accounting suite
unchanged; ten durable API checks (original deadline, independent receipt, seven
persistence tampers, expired deadline, listing). The independent Python reference
replays the derivative source, item/hypothetical values, lifecycle, assessments,
journal and roll-forwards. Both oracles now treat rounding-level intents as zero
events; the previous derivative verifier dropped native floating-noise marks for
exactly offsetting legs, so at-market fixtures use a par rate with distinct legs.

`hedge-treasury-1` (same stage) feeds the verified complete-tail hedge closing to
the existing treasury bridge: per-instrument rows reconciled to the closing trial
balance, cash-flow-hedge AOCI under its own GL with the reviewed
`cash_flow_hedge_aoci_eligible` policy flag (default excluded), hedged-item basis
as a signed asset/liability adjustment, and the thirteen-ratio bridge with
supplied requirements and weights. Each policy carries the reviewed AOCI
treatment (deferred-tax rate disclosed as an unposted DTA/DTL, opt-out election,
phase-in fraction), the explicit #30/#72 inputs to eligible AOCI. Evidence: three
native tests, 21 engine checks, five durable API checks, independent row/capital
replay, unchanged treasury-bridge suites. An explicit `interim_report` selects
report-date capital at any hedge path node before the complete tail (rows through
that node reconciled to its trial balance; native and engine cases with refusals).
FTP, posted deferred tax/threshold deductions and approval remain open.

Not closed by this stage: accounting-standard approval, observed effectiveness,
the remaining ASU 2025-09 elections, portfolio layers, net investment, options, saved-book and shared
daily journal adoption, deferred-tax/AOCI opt-out and interim capital policy (#30/#72),
browser flow beyond the generic model-analysis page, PostgreSQL parity and the
60,000-position joint acceptance. Local validation does not deploy or restart workers.

### Issue #20 acceptance mapping

| #20 criterion | Current evidence | Open |
|---|---|---|
| Par/payer-receiver/schedule/negative-rate swap fixtures | `derivative_contracts.rs` hand cases (regular/irregular/unequal-frequency legs) | none |
| Swaption annuity/tenor and exercise convergence | European cash/physical hand cases and the native option study ([market-option-convergence.md](market-option-convergence.md)); legacy cash-settled annuity remains the historical approximation | Bermudan/American scope, empirical option validation |
| Netting vs gross, collateral, closeout conservation | CSA/MPOR/default native cases; independent raw replay before and after persistence | dynamic IM, own default/DVA |
| Missing mapping/CSA fail before publication | saved-derivative admission and `/derivatives/mixed/runs` publication fences | saved-book UI adoption |
| Margin transfers never counted twice as trading loss | derivative accounting intents; mixed shared-journal replay; hedge lane reuses the same owner once | treasury/capital feed |
| Dated settlements/exposure/margin to daily replay and treasury | mixed daily replay (`saved-derivative-mixed-1`); formal hedge accounting lane; `hedge-treasury-1` closing capital from the hedge journal | saved hedge graph/paired risk, strategy coefficients, treasury feed for the mixed saved-derivative journal, complete scale |

## Separate fee owner (local0.29.16; supported component validated)

The additive native supplied-history fee contract separates earning, cash,
prepayment, reviewed waiver/cancellation and recovery from loan G/EIR, with
signed gross rights and independent ordered/persisted replay. See
[the contract](nonintegral-fees.md). Native event binding, mixed journal and
pricing/unit/acquisition allocation/capital/FTP adoption remain required;
all original15/25 empirical, saved/cohort/strategy and scale gates remain.
Matching builds pass1232 native/strict Clippy,2632 full engine checks (two
existing reference skips),1174 full API checks (one optional MCP module skip)
and the existing dependency warning. Both dedicated MCP/API/worker journeys,
40 MCP checks, the web build, boundary/contracts and exact historical outputs
pass. The accepted validation freeze binds1083 files; a concurrent MCP test-only
follow-through is separately recorded from the identical financial build inputs.
See [the local review](../reviews/2026-10-12-nonintegral-fee-owner.md).

## Verified modeled commercial prior openings (local0.29.15; supported stage validated)

The new durable source-ID route independently replays an authorized prior holding
publication and binds the exact published cut opening/completed journal to a new
complete-origin native replay. Both holding schemas now retain final SQL guards.
Shared bounded completed-source transport and exact integer-clock reconstruction
keep storage/verification separate from Rust financial generation. See
[the contract](commercial-prior-opening.md). Observed bank reconciliation, a
cut-started mixed Sim, seasoned/secondary adoption and all original gates remain.
Matching official builds pass1,220 native tests and strict checks,210 commercial
engine and184 commercial API checks,2,598 full engine checks with two existing
reference skips and1,157 full API checks. The API environment skips the optional
MCP integration module; its dedicated run passes separately. MCP29, integration1,
and the web build pass. Native financial source/Cargo inputs remain exactly
unchanged across419 files, and four complete historical outputs are byte-identical.
The source freeze binds1,075 source/input files. See
[the local acceptance review](../reviews/2026-10-12-commercial-prior-opening.md).

## Commercial holding journals (local 0.29.14; supported stage validated)

The explicit `commercial-holding-journal-1` contract reuses sealed complete-origin
continuation sources in one existing native daily Sim. It derives a balanced cut
opening, rolls prior P&L into equity and retains ordered future postings with
full native transaction lineage. Independent raw-source, full-journal and persisted
window verification gate publication under the original deadlines. See
[the contract](commercial-holding-journal.md). Verified prior-publication opening,
cut-started mixed owners and seasoned/secondary adoption remain open. All original
issue15/25 criteria and unchanged admission/tolerance gates remain required.
Matching official builds pass1,220 native tests and strict checks,210 commercial
engine and135 commercial API checks,2,598 full engine checks with two existing
skips and1,100 full API checks with the existing dependency warning. Four complete
historical outputs are byte-identical. The immutable receipt binds1,054 exact
source/input bodies, eight binaries and26 logs. See
[the accepted local review](../reviews/2026-10-12-commercial-holding-journal.md).

## Historical commercial complete-origin holdings (local 0.29.13; supported stage validated)

The explicit native holding-window contract derives opening state from complete
acquisition replay and retains original yield, gross rights, draw anchors and
reviewed credit state. It rejects changed completed prefixes and preserves latent
credit waiting hazards through the cut. Independent raw/persisted verification
and original native/durable deadline fences are required. See
[the contract](commercial-continuation.md). This foundation does not establish
verified prior-publication opening, cut-started mixed journal or seasoned
saved/cohort/strategy and secondary-acquisition adoption. All original gates remain.
Matching official builds pass 1,216 native tests and strict checks, 191 commercial
engine and 126 commercial API checks, 2,579 full engine checks with two skips and
1,091 full API checks with the existing dependency warning. Historical whole-output
bytes remain unchanged; the immutable receipt binds 1,047 exact source/input bodies,
eight native artifacts and 25 logs. See
[the accepted local review](../reviews/2026-10-12-commercial-holding-continuation.md).


## Historical commercial original generated OAS (local 0.29.12; supported stage validated)

Explicit generated original quote calibration now retains one native Q training
and evaluation sample, refits continuation per spread and requires actual stopped
cash to meet the unchanged quote residual. Accepted policies/cash enter graph and
acquired-unit preparation directly; current scenarios retain the selected original
OAS. Original Physical acquisition cost/yield remain independently supplied.
Matching official rebuilds pass 1,211 all-target Rust tests and strict checks,
165 commercial engine and 117 commercial API checks, 2,553 complete engine
checks with two skips and 1,082 complete API checks with one dependency warning.
Four complete historical public outputs remain byte-identical. Fresh-process
default/C ABI checks forbid Python financial callbacks. The immutable receipt
binds 1,041 source/input files and all eight native artifacts. See
[the new contract](commercial-generated-oas.md) and
[current review](../reviews/2026-10-12-commercial-generated-oas.md).
General convergence, observed histories/policies, complete product/mixed adoption
and every original #15/#25 criterion remain open.

## Historical commercial #15 follow-through (local 0.29.11; supported stage validated)

Canonical legal closure, performing draw tranches, reviewed credit actions,
before/after-cash and rights-only exercise, original stopped Physical acquisition
proofs, supplied-law generated exercise and compact locked-OAS P/Q graphs are
implemented. Commercial cohort/acquired-library jobs, standalone finite candidate
journals with explicit AC issuer ownership and reviewed closing capital/daily FTP
have independent publication gates. Matching official releases pass 1,205
all-target Rust tests and strict checks, 148 commercial engine and 110 commercial
API checks, 2,536 complete engine checks with two skips and 1,075 complete API
checks with one dependency warning. The immutable receipt binds 1,035 frozen
source/input files and all eight native artifacts. See
[the current follow-through review](../reviews/2026-10-11-commercial-followthrough.md).

Full #15/#25 remains open: seasoned acquisition prefixes, nonintegral fee claims,
daily in-arrears indexed earning as fixings become available,
general generated OAS calibration and convergence, generated path-specific draws/
reviews, streaming mixed candidates and verified opening evidence, complete saved/
mixed/product adoption, observed histories/policies and full 60,000-position joint
acceptance. No original criterion is removed by this supported stage.

## Historical Commercial #15 installed integration (local 0.29.10; supported stage validated)

Original #15 scope remains unchanged and open. Installed native source/carry/coupon
components now join one existing daily Sim; complete source and recovery clocks,
actual day-zero acquisition and separate legal/recognized lots are retained.
New native commercial graph/coefficients/strategy and finite-distribution
conditional exercise are additive explicit selections. Matching official
artifacts pass 42 commercial engine and 42 durable API checks, plus 19 graph/unit
checks. All-target Rust suites pass 1,155 tests across the four crates and strict
all-target/all-feature Clippy passes. Final regressions pass 2,452 engine tests
with two skips and 1,009 API tests with one dependency deprecation warning.
These passing supported-stage gates do not establish full issue completion.
See [the commercial ledger contract](commercial-credit-ledger.md),
[native exercise](commercial-exercise.md), [graphs and strategy](commercial-graph-strategy.md)
and [the current validation review](../reviews/2026-10-11-commercial-native-integration.md).
The complete remaining adoption, policy, empirical and scale gates still apply.

## Public CD boundary and installed successor validation (local; 2026-10-04 UTC)

The copied public `cd-generated-graph-1` dispatcher now has a complete bounded
selected boundary. Its v5 receipt is
`f4f6f63abc7a881e2d5053622c5c083bfc58c86d38b8faf515fb7f377394da3b` and retains
1,050 source/compiler bodies after the v4 predecessor was archived. It passes 426
native all-target tests and strict checks. A fresh 35-process controller passes all
16 complete financial-bit parity cases, ten historical refusals, enum-object and
duplicate-key refusals, integer financial spelling, escaped-schema selection,
deadline/cancellation and Rayon/serialization controls; receipt
`8186e87dac604f49e4f2e576dc1ad9046fb0b3624eddc48b57b2113546a13b12`.

Selected output now serializes a borrowed flattened result directly through the
checkpointed bounded writer. The original raw request is retained separately from
the typed DTO, and an input-length transport reservation (`32 * envelope bytes +
4096`) joins graph capacity before financial source generation. The header probe
uses the original entry clock and a checked 32 KiB reader; its historical 64 MiB
outer peek and selected 2 MiB boundary are disclosed separately. The independent
peer repair review is sealed at
`87c706e855b40277f9824e5c7b3053d181e57ac7314ecd8f9aae4d2907f1405e`.

This validates a contained public native boundary only. It does not establish API
selection, durable worker publication, saved-book/library adoption, empirical CD
withdrawal or renewal calibration, capital/FTP eligibility, or complete issue #16.
The installed joint-funding successor is separately fenced; its prior worker
collection defect and each corrected harness prerequisite remain immutable.

The installed joint-funding fixture successor is now accepted locally under
`joint-funding-public-adoption-0298-fixture-successor-v5`, receipt
`5e5905ff198941d119b7119652902b0c61846a386babca46c3e6c689a26071d0`. It passes
85 worker tests (one warning), 2,373 engine tests with two skips, 847 API tests
with one warning, 52 browser tests and six FTP browser tests. The successor
records 1,025 current source files, binds the prior 1,043-file application and
freeze, and reports zero financial-source changes. The fixture repair only passes
the declared parametrized filename into the helper; the external native-process
pin is the prior composition-stage binary with its recorded SHA. This is fresh
contained installed-default evidence, not deployment or production restart.

## Source-owned CD graph and shared accounting reviews (local; 2026-10-04 UTC)

The copied `cd-exercise-graph-stage` now prepares complete native base/down/up
CD sources, calibrates one original OAS to an explicitly selected actual-asof
positive dirty holder cash quote, and retains that OAS in both paired curve bumps.
Its 516-file seal is
`d1dceceb0d558f0f2b87ff5430904208eaaf445682dd0f3919a50c879108434d`.
Fresh gates bind 175 compiler/reference inputs, 24 native tests, strict all-target
checks, 27 historical processes and 26 independent raw-source/OU/rollback/cash
graph processes. The complete nineteen historical accepted outputs and eight
refusals remain byte-identical without excluded cells; control receipt is
`22f5ae38d8b884137d6e44e5e15705d3bcee26ff606811a212b6b23dbe2ae0da`.
The actual native image is
`e87ab9d9241fd7b0b8e9e8c96126aabd6e466e2191a8c7bb9906b8c561258ddb`.

Calibration reuses one source/tree across 103 maximum base valuations; actual
source-generation counters gate exactly three generations for a complete graph
and zero for aggregate admission/cancellation refusals. All three integer
topologies are admitted before source generation. Both bumps retain identical
Gaussian states/children/probabilities and the immutable original base OAS.
The component reserves 128 million work units and 128 MiB retained/output
capacity; these are not process RSS or combined 60,000-position guarantees.
Separate curve/OAS clocks, signed liabilities, source coupon rights and complete
stopping tails are independently reconstructed. Quote fitting does not calibrate
the supplied constant volatility/mean reversion or physical retail behavior.

A native zero-net test exposed the historical event predicate dropping principal
100 and coupon -100, then failing principal conservation. The additive graph
explicitly selects `complete_gross_cash_components-1` and retains separate gross
components even at zero net cash. The historical owner selects its original
predicate; shared backward valuation arithmetic is retained. The actual original
9-pass/one-failure receipt and complete before-source bodies remain immutable.
Public/durable graph selection, broader contract terms, physical withdrawal/
renewal generation, cohort/library/saved-book/income/capital/FTP adoption,
empirical holdouts and full scale remain required under the original #16 scope.

Root reviewed the sealed funding default successor
`152194b56f37b357da06602a09f0da2c77eb05f7103e72b185900496408ddc75`
against all 414 source/reference/fixture/image bodies and retained evidence. Its
recorded 29 native/strict and 93 independent controls are separate from root's
four fresh native streams and four fresh independently verified Parquet replays.
All complete streams match their archived bytes. Root review receipt is
`9cea6703a2446cd98512a14047ac26885d5dc3cb4bfc75263da5ffdb636afa32`.
The open hand case retains book principal 40 and legal principal 40/interest 0.2
after collecting principal 60/interest 0.3; expected cash-tail completion implies
neither book closure nor legal waiver. Repo net recovery cash posts once and
disposed collateral cannot be reused. An initial root preflight counted a single
native result line instead of summing targets; it is retained and executed no
financial process. Public/saved/treasury/empirical/scale acceptance remains open.

Root also verifies all 735 retained source-factor files and their complete
compiled/parent/reference/image identities, and freshly passes 77 independent
controls with 68 complete native process receipts plus the broken-output-pipe
control. The sealed source factor stage is
`8e9e75d559c8846bcaf9cd4f81c42aa8bf07ead7532b2b39d5aa6a4278133bc1`;
root review is `9333c28fece3d7c355de4ca4b20f7a607ca3b8af2e34f54768944b501ada74d2`.
The final executed image is
`053a3577863d40deca66a7e5a5bfb1e93f2fe219ff7ba596d1145f82360cbb8d`.
It retains native source period/audit/cash identities and continuous subday
factor laws. It produces no principal, accrued money, carrying value, loss or
journal; Corporate owns the separate source entitlement/credit/accounting
compiler. All original #11/#15/#22 and combined adoption gates remain required.

## Generated CD exercise and reviewed funding milestones (local; 2026-10-03)

The next copied CD preparation component (`cd-exercise-prepared-stage`) separates
complete source/tree generation from borrowed repeated fixed-OAS valuation. Its
263-file seal is `216cd485789145057d8a15d9eb0366e33156d0af9ef819a22beec755d5c01a84`.
Fresh gates bind 172 compiler/reference inputs, fourteen native tests, strict
all-target checks and 27 independent processes. Nineteen complete accepted
outputs and eight empty refusal outputs are byte-identical to the observed
predecessor with no excluded fields; receipt
`3980c2992772c7dde951a99207200ca25234bb89824055eaa70f255ebd169ddf`.
The copied Bellman owner's financial body remains unchanged after exactly five
new computation checkpoints at entry and date/node loops. Original fixed OAS,
full contractual tails and current observed-state lineage remain retained.
Base-quote calibration, paired graph risk and public/saved/cohort/library adoption
are still required; this preparation component is not their acceptance. Blocked
I/O and final publication still require the public watchdog/fences. An initial
preflight proof failure is retained separately and executed no financial tests.

Root independently reconstructed the sealed commercial credit-carry component
from all 976 immutable bodies (971 inherited, five retained successor bodies),
verified all 34 retained files before/after and freshly executed its archived
nineteen native tests. Receipt is
`8bfe13e4b417a763b29344e92f9a0601de25a036eaa813280dcc20759cc004b1`.
This run does not recompile or rerun strict checks. Its accrued A is contractual
legal interest and its B is G-P-legalA; reviewed recognized interest needs a
separate subledger. Actual corporate coupon entitlement can retain the native
period opening notional after principal payments, so live credit P cannot be
universally substituted for source entitlement. Source-authoritative per-coupon
settlement/default disposition and one shared Sim remain required under #15/#22.

The collateral funding successor is separately sealed at
`d2a7e917dca4755922c585a959bbcd7708196072a877b70276de4f25ed6b25e0`.
Root verifies all 335 retained/source/image bodies and all 267 unchanged parent
bodies, recorded seventeen native and 47 independent checks, and one fresh raw
native process against the complete independent source/ordered-GL/security-memo
replay. Its fresh complete stream is byte-identical to the archived stream;
root receipt is `c2c4bbec096414f44ef91a8e09bd531c69a5695f2c4e55df4a59fd8cb1d1685e`.
Collateral quantities are explicit memo ownership, not additional cash, income
or inferred regulatory eligibility. Complete default/legal/book ownership and
public/library/saved/treasury/full-scale adoption remain required under #21.

Peer review of the generated CD predecessor identified a lineage defect:
different observed rate states shared one information-set label while making
different decisions. The original 352-file seal and peer review remain unchanged.
The separately versioned `cd-observed-exercise-1` successor in
`artifacts/cd-exercise-information-stage` binds each current actual-date/rate-state
observation within the separately retained tree. Collapsed/future-state labels
reject before valuation. Fresh gates bind 171 source/dependency files, eleven
native tests, strict checks and 27 actual independent process cases. All nineteen
accepted predecessor financial/source/date/OAS outputs and eight refusals are
exactly preserved after excluding only the new explicit envelope schema and
information labels. The 260-file successor seal is
`8cbbba3325043495e3c84bb86ea242553dcfd03e1e2933a9a584f73ca263d9e3`;
financial-output control is
`4793a216e5bb001e34adc1b2e4301510a1aa4f8544d1e197162ff85528a399ae`.
Successor peer review confirms the observed labels, admission and complete financial-output preservation; its receipt is `570badc1f4716124bea18c17e15b71ed97ca4db8b7ece2855313e04ba0513c5b`. No native rerun is attributed to that read-only review. Full public transport/blocked-I/O deadlines,
interior Bellman checkpoints and every original issue adoption/empirical/scale
gate remain required. This component remains uninstalled; no issue is complete.

The separate copied `cd-exercise-generation-stage` supplies a reusable native
curve-fitted Hull–White pricing tree and composes it with unchanged native CD
contractual cash and Bellman exercise owners. Exact permitted call dates, signed
accrued rights and fixed original OAS remain retained. Fresh final evidence binds
170 source/dependency files, nine native tests, strict all-target checks and 27
actual independent process cases. The analytical European bond-call convergence
and zero-mean-reversion limit are checked on four grids; 64-step errors are below
0.0033 currency units per face 100 in the two selected cases. This does not relax
existing final-output tolerances. The 352-file stage manifest hash is
`6540ee9fd7f266cfebfccb2f6cffd98dfbb7b0aa1b81444b759fd962247bdd1e`;
peer review `3c7c63af0d3de58811ee2a392077822a0d17214e04df34422e95c213b0067a2d`
identified the observation-lineage defect repaired by the separate successor
above. The original route remains uninstalled. Constant supplied model
parameters, one-factor Gaussian dynamics and simple fixed bullet/same-date
exercise terms remain explicit. Physical withdrawal/renewal, observed holdouts,
notice/settlement extensions and full graph/cohort/library/saved-book/accounting/
capital/FTP/scale adoption remain required under the complete original #16 gates.

Funding source/real daily-Sim milestone 1 remains separately sealed under
`repo-funding-adoption-stage`, manifest
`dd9801e68ce335567c55e55b1aa91218be044e2213c4cacbc04363f6d2bd1301`.
Root review rehashes all 267 source/dependency/reference/image files and retained
evidence, verifies the recorded 15 native and 38 independent checks, and freshly
replays one actual native complete stream against raw-source/ordered-GL controls.
Root review hash is
`20c7f9f95f00ae5134ce0decf9fefa164f2f6c9b0fbde1020593b7cdf0b1645f`.
Original yields/history, actual acquisition cash and old maturity before new
advance survive a current-only market-access outage. This performing component
does not establish collateral security-memo/default-rights/public-registry,
broader mixed owners, saved/library/treasury or full #21 adoption; those are the
next required integration seams. Its component budget is not aggregate RSS.

The main 108-file applied successor remains in fresh validation. Its official
product, ledger and Decision builds now pass against the unchanged 1,043-file source
freeze. Fresh Control (five tests), Core (335 tests), Risk (390 tests) and Ledger
(196 tests), including all four strict checks, also pass; remaining Decision and full installed-default
gates are running. No earlier parent test count is attributed to this financial
successor. All fifteen child issues remain open; no deployment occurred.

The exact reviewed CD/source2 continuation is now applied under
`cd-portfolio-public-adoption-0298`. Its 907-file installed freeze has aggregate
`43f6f460ce8b2859a55e7f517e4f55b158a7760c33abaf094e4c1c7d398e9bf4`.
The root independently verified all 86 current beforeimages, all 916 proposed
source files, retained contained receipts and native artifacts, and a complete
isolated Git patch roundtrip. The accepted 859-file FTP/storage/selector parent
is retained separately. Architecture and HTTP/native contracts pass after
application. The fresh official product, ledger and Decision builds pass; their
DLL/executable hashes are retained against this exact source freeze. The Decision
snapshot independently rechecks all 907 installed and 916 manifest source files.
Fresh all-target checks pass 333 Core, 390 product-runtime, 196 Ledger and 79
Decision tests (998 total), with strict checks for all four crates. Ten final
official DLL/executable images are archived independently under
`artifacts/cd-portfolio-final-native-archive-v2/`; its receipt hash is
`18ba2d8843aae05b95c5a8d7ddc0ef2343f21fb463a269a3a5b25f960cc756f8`.
The separate actual engine-default and source-worker controls pass 64 and 67
tests, respectively. The full installed-default engine suite passes 2,346 tests
with two skips in 385.77 seconds; the API suite passes 766 tests in 696.96 seconds.
The original browser gate terminates with 49 passes and one failure: the
repricing test incorrectly requires operation-name text inside a valid
base/revised graph request. Its original source, trace, failure context and
application/log receipts are preserved under
`artifacts/cd-browser-assertion-recovery-stage/evidence/failed-parent/`.
The assertion is replaced with an exact comparison of the entire loaded
example request. The new `cd-browser-exact-example-recovery-0298` application
checks all 906 other parent sources and ten official binary images unchanged
before/after every gate. Native/engine/API successes are explicitly reused;
the complete 50-case browser suite and separate four-case FTP suite are renewed.
Its expanded 979-file source/fixture/instruction/helper freeze has aggregate
`42d99dc58c7911847ecf5a1252437506c61ae9375be9f8ac209c496ba2f4566d`.
The first corrected renewal terminates with 48 passes and two failures. Both
new term graph/repricing cases pass. The two failed cases retain browser-console
`net::ERR_NO_BUFFER_SPACE` errors while fetching scenarios and the application
module; the underlying network-resource cause is unproven. Complete failed
application/log/source/trace evidence is preserved under
`artifacts/cd-browser-network-renewal-stage/evidence/failed-recovery/`.
The source-only repair attestation hash is
`dc558d596e4b9f4f2248080329c36316911d26e204e11a9234f122f5b9ffe4d7`.
The new `cd-browser-serialized-renewal-0298` controller runs the same full
50-case browser and four-case FTP gates after the parallel contained browser
processes terminate. It changes no source, timeout, financial tolerance or
error assertion. That serialized renewal now terminates successfully: all
50 installed browser cases and four installed FTP cases pass with the final
979-source and ten-image fences intact. Its terminal application hash is
`29af9dc7d11f985abcd1b7d7f492cc41cf648808c52226ba3ee451ad4d0ff2bb`.
The unchanged native/engine/API results above are reused successes, not new
executions. This CD/source2 local adoption is accepted; the next combined
derivative/funding successor still requires its own application and fresh gates.
The original failed application is never relabelled. No earlier
native test count is attributed to this new financial source set. The combined
contained proposal passed 158 distinct Python and 30 selected native checks;
those do not replace the installed-default gates. Source2 funding/FTP, derivative
joint ownership, saved/cohort adoption, empirical holdouts and combined scale
acceptance remain separate required work. No child issue is closed.

The merged derivative/joint/source2-FTP native component has a separate sealed
handoff (`c70a4c5153d039a3ecacc39a0a0c11a87b6b4e5a232107c3cd8ec4b1cd7dfc30`).
Root independent review rechecks all 1,072 frozen files, 986 complete compiled
source/setup files, eight matching archived native images, three original build
pins, 35 selected native tests and five strict all-target crate checks. Its
123 continuity checks retain a complete 1,030-file before/after source/reference
corpus and four complete historical output streams. Every one of its 36 native
beforeimages and the complete isolated patch roundtrip matches exactly; the
nine-file mortgage generator identity is unchanged. The review receipt is
`artifacts/merged-joint-ftp-native-root-review/review.json`, hash
`29f84abd0b33790ac3e45ba0bb53f5c5a5bc4b17c1a9fac47d91f039fa64cabb`.
This component review does not establish installed public adoption, full
empirical validation or complete issue 25 acceptance.

The complete 80-path public composition now has independent root final review,
hash `436e737b8dafe47f7ae4bf250e0bc2d90e33ca7491a4008f5401455ddc0229f2`.
It rechecks 1,697 retained files, 1,079 external dependencies, all actual
426 process/worker/DTO controls and six final browser cases, and the exact
108-path union with the native component. Disposable SQLite inspection retains
the original six completed happy-path jobs and four late-refusal source/FTP jobs
without mutating archived SQLite/Parquet bodies. Each historical before/after
corpus is checked against its exact archived helper/test/metadata versions;
the separately retained supplement is
`c2f286f4872a7f036c7a1789fdcf306f695e999a553d7f2f656d7e3b386fadb1`.
The test import correction changes only one `pathlib.Path` import and its
adjacent line ending; complete byte transformation and the remaining AST match.
Original failed review attempts and all earlier application/gate failures remain
distinct; no successful gate is relabelled as using later helper versions.

The exact union is now applied under `joint-funding-public-adoption-0298`.
Its 1,043-file source/fixture/instruction/helper freeze has aggregate
`5629b99333a0c8957db10b470748c1bf0dfa3cf217eb0aa880cd749552d51555`.
Architecture, generated HTTP and native contracts pass after application.
Fresh official builds, all-target tests/strict checks across all five native
crates and complete installed-default engine/API/browser acceptance are pending.
The application controller re-enumerates the complete source-file inventory and
checks every pinned byte through the gates; accepted-parent numbers are not
successor acceptance. Full empirical, saved/cohort/strategy and 60,000-position
gates remain required.

The separate source2 funding/FTP browser reproduction completed its source worker
but refused completed-source publication because nested preparation reported
different allocator-capacity counters. The retained complete source comparison
has five differences: four `native.retained_used` cells and aggregate
`usage.source_retained_used` (723,592 versus 790,632). Its diagnostic hash is
`c0f7b32e6a423b412dda7819d6f4bfca2966cc6b0564bbedec23bb28e273c7ad`;
complete outputs and failed trace are retained under
`artifacts/ftp-deposit-portfolio-stage/evidence/initial-browser-receipt-refusal/`.
Direct typed-byte parsing and nested `from_value`/cloning allocated different
vector capacities. The staged correction reuses the same selected typed-byte
decoder, explicitly reserves encoding/reparse work and capacity before projection,
and retains original raw/DTO identity, active deadline and financial mathematics.
Exact capacity/source comparison and all monetary tolerances remain required.
The corrected contained process now produces an empty complete retained-receipt
diff. Two actual source2 browser cases pass: the two-owner/two-run 40-day and
one-owner/two-run 90-day flows execute real source workers, explicit review,
native FTP, independent persisted replay and paged download/reopen without
full-result hydration. Two additional actual browser cases refuse inside the
late private SQL guard and publish no completed result. Their receipts pin native
executable `7b781e84e519f6b0503da55ec6424b8635f0bb3c2e1f2c4ba781a803ec78f057`
under `evidence/corrected-source2-browser/` and
`evidence/late-private-sql-refusal-browser/`. These are contained successor
checks; its final source freeze, application and installed-default acceptance
remain required. Original failed receipts and traces remain retained.

A retained source2 funding process/reference gate now passes 49 cases in 63.23s,
including complete original-source/FIFO/funding comparisons, duplicate economic
owner refusals, exact original-yield/type/signed-zero controls, cancellation and
two whole historical Card2/source1 output-byte controls. Its receipt at
`artifacts/ftp-deposit-portfolio-stage/evidence/final-process-reference-controls/`
pins the test file and executable before/after execution. The final complete
source/reference/fixture freeze and its matching rerun remain required; this
receipt does not attest hashes for every imported reference or financial source.

The separately sealed source2 successor now resolves that provenance gap. Its
28-path public handoff has SHA-256
`f56dd1b72fac1d78a5990a9571378516a82e191a595dfc6f9f10d54d65d75647`.
Fresh retained gates pass 49 process/reference cases in 67.30 seconds, 25 private
publication/reopen controls, two actual completed-source browser cases and two
actual late-private-SQL refusal cases. All gates retain the same complete
1,256-file source/reference/fixture/executable inventory before and after, with
digest `cf366b98b004a9faa305c21b1c370bd8070e6f42d55e87b231d3e4c91c1e87ff`.
The root independently rehashed all dependencies and 44 evidence/handoff files,
verified the executable, exact current beforeimages and a complete isolated
28-path patch roundtrip, and rechecked the three separately owned shared
Control/parser prerequisites. The independent receipt is
`artifacts/source2-ftp-root-review/review.json`, SHA-256
`603621b39515ec282c2f2fd7a7f793d1833019ac4753f200de95c57f7b774607`.
The unmatched late-browser runner selected zero tests; its failure is retained
separately from the corrected two-case passing execution. This package remains
unapplied. Contained runtime evidence and rebased contract/web checks do not
establish installed-default acceptance of its complete public composition.

The joint deposit/derivative and funding validation rationale is documented in
[`joint-funding-validation.md`](joint-funding-validation.md), with primary-source
research and required evidence for complete scope. It does not replace any
original child acceptance criterion or infer unprovided behavioral histories.

The joint native coordinator's pre-generation work reservation has a confirmed
review gap: the original `250000 + raw_bytes * 8` derivative bound does not
explicitly charge the compiled journal's complete-tail line scans. A
post-generation `Compiled.work` refusal cannot satisfy preflight. Review also
found the same gap in the proposed standalone saved-derivative route. The isolated
successor now shares a raw structural posting/cash-event work helper between
both routes, includes actual complete mixed days before financial generation
and charges dated work once. The corrected selected gates pass 18 standalone
and nine joint native tests, including an actual physical-delivery long-tail
standalone refusal before generation and a public joint Value work refusal with
both generator counters zero. Both observed-work guards reject compiled work
above the structural reservation. Full Decision strict checks pass. Final
matching artifacts, complete independent acceptance and installed-default gates
for both routes remain required. Existing supported
option/delivery/CSA/default policies remain required under genuine aggregate
bounds. Earlier passing joint process/worker receipts are retained predecessor
evidence and do not accept this correction. Zero-generator boundary refusal and
refreshed complete-domain and persisted-journal gates remain required.

An additional standalone public-boundary review found that decoding preceded a
fresh Control that discarded inherited cancellation/deadlines. The copied
successor now uses the shared checked reader, retains the dispatcher's original
entry clock, takes the tighter parent/request deadline and shares cancellation
authority. Selected tests cover expired/cancelled parents, elapsed original
budget, cancellation during flush and unchanged complete output with no parent,
an inherited parent or direct execution. The separate root source/receipt review
is `artifacts/derivative-public-control-root-review/review.json`, SHA-256
`1395ecd351000d836061b5be99b6e71b2a034bacdd2b6283a4560aab5f5e7bcb`.
This does not alter historical parsers or accept the complete final public
package. Older financial/process evidence remains separately retained.

The sealed standalone/joint public successor now has a 54-path application map
against the unchanged 907-file parent. Manifest SHA-256 is
`2c9cbd039b010a5b7196aebf0dc664dbefa808a8f002f8e11036b06921f014c1`.
Independent root review rehashes all 498 frozen proposal files, 334 immutable
parent files and 907 installed source files; every current beforeimage and the
complete isolated patch inventory/bytes/JSON match. Retained JUnit reports have
42 process cases, 86 worker/HTTP/reference cases and one operation-inventory case,
with zero failures/errors/skips and matching actual testcase counts. The
86-case gate comprises 41 actual copied worker/publication controls and 45
independent DTO/raw/structural checks. Separate standalone process evidence
passes 221 cases in 69.19 seconds. Those gates used dispatcher
`308b7274f9d6d72e8f149badf609eb5080f0258f78b877238d6d8adce39dcb4b`.
A subsequent native burst-test rebuild changed the target executable to
`3244216eed7afe8eac8528060729ed946686bf544e7b62a59fdd28217ca60b61`;
the older source/build/run receipts remain historical, and the old image was
not archived. Current artifact acceptance cannot follow from those receipts.
The root review at `artifacts/joint-public-root-review/review.json`, SHA-256
`ebc11375bd8a433b5a57b5d3ee25087fb692c87383f64fb4cb0f756742539eb8`,
records this distinction explicitly. A new public/native composition with the
source2 funding successor requires complete before/after imported-dependency
fences, matching final artifacts and renewed combined/default/browser gates.
The only overlapping public paths are the two generated HTTP contract files;
they must be regenerated from the composed routes and source selection.
Neither sealed package is modified or applied by this review.

The final narrow native predecessor is separately sealed with handoff SHA-256
`8246df28396dbf2b55a0ee4e5a44dbd9b07a2719b000b0d5081af879c6a6af67`.
Independent root review rehashes all 2,411 retained files, 975 complete source
files, 907 installed parent files, fourteen completed receipts/logs and eight
archived matching DLL/executable images. Its 28-path native patch roundtrips
with exact inventory, bytes and JSON against every current beforeimage.
The archived dispatcher has hash
`3af5065ea68c1a1a397472c1443e54b6ceb35229f7d3cd6d43c0c95245da7aff`.
Fresh retained controls pass five Control tests, two checked-parser tests,
nineteen standalone derivative and nine joint-native tests, five all-target
strict crate checks, 221 standalone process/worker checks and 119 CD/source2
checks. Four complete historical streams are independently inspected as
byte-identical, including their once-only completion markers. All native
receipts pin 366 compiled source files before/after. The two Python gates pin
images and a sixteen-file private test binding, but do not fence every imported
reference before/after. The subsequent combined package must renew those gates
with complete dependency inventories. Root receipt:
`artifacts/joint-native-root-review/review.json`, SHA-256
`d0b242b45b593c30eea01bea5311c8b8cdc06b0aad844ee259021d298ec525cd`.
This verifies the unapplied predecessor's stated scope; full official/default,
empirical, saved/cohort and scale acceptance remain required.

The next native acceptance runner explicitly includes the shared Control crate,
which the historical four-crate helper omitted. Financial crate test/strict
flags are unchanged. `artifacts/check_joint_funding_native.py` retains distinct
immutable stdout/stderr/receipt files and checks the supplied source freeze and
its own identity before and after each gate. An independent root copy of the
complete three-file Control crate passes all five tests and an all-target strict
check, with source `lib.rs` hash
`235cbda2d66387c4bc236c7740cac5ebbb524fabb6de98c893e8c9d2178b20c0`.
Receipts are `artifacts/root-joint-control-0298-native-control-test.json` and
`artifacts/root-joint-control-0298-native-control-clippy.json`. They cover the
standalone Control component only; final combined native/default acceptance
must execute all five crates against the complete final source freeze.

The following predecessor stages retain their original receipts and failures.

The reviewed 49-file modeled-product FTP public integration is now applied under
`modeled-product-ftp-public-adoption-0298`. The matching 851-file freeze has aggregate
`998d3fee404fde63dcfe3c27f801b8e2afa143d273c8774f6fb7b93a9d7874bd`.
All eleven official native build/check gates now pass: 309 Core, 373 Risk,
162 Ledger and 79 Decision tests, plus strict checks for all four crates.
The fresh installed-default engine gate passes 2,237 tests with two reference-only
skips. The installed-default API gate is terminal with 676 tests passing and
one initial API-health startup failure in the actual process restart test;
browser acceptance was not reached. The original temporary process log is no
longer available, so the failure cause remains unproven. A retained reproduction
exposed a separate real Win32 long-path failure in local object publication and
reads. The exact frozen preimage fails on the same long root that the new storage
helper successfully publishes and reads, with unchanged keys, hashes and scope.
The 852-file `modeled-product-ftp-public-adoption-0298-windows-storage-repair`
successor has aggregate `b82dd0a2458459ca3783faf9b48967ea8f5ea0b463d8acce395719e3a465ca2b`.
Only the local object-root helper, one checksum test's configured-root access and
two new long-path regressions differ from the 851-file source set. It passes
25 focused storage checks and the exact real process restart case with the
existing timeout. Its complete API gate passes 679 tests and the existing browser
scope passes 47 cases. The required FTP browser then failed because saving and
reopening a paged report still fetched the full result; this source set is not
accepted. The unchanged native
gate and engine results are explicitly reused evidence, not new executions.
Preserve the original failed receipt; this does not prove its missing-log cause.
This source set is separate
from the accepted 822-file deposit predecessor and cannot reuse that predecessor's
default acceptance. Completed-source Card2/deposit binders are a separate isolated
successor. See `modeled-product-ftp.md` for source/channel ownership, exact complete
tails, seven earnings lanes, acquisition phases and explicit Physical comparative
charges; these are not pricing-measure option fair values or full #24 acceptance.

The completed-source FTP successor is now separately frozen as a seven-file delta
with 105 contained native/source/storage/publication checks passing. Manifest
SHA-256 is `eb00338cffc9de01c22af7380afba083c33742b5eb97583da87814f97886bd11`.
It binds only explicitly reviewed completed Card2 mixed and performing-deposit
jobs, independently reopens their source and ordered journal evidence, and checks
source objects/SQL identity through final publication. A preserved predecessor
failure showed that a monetary comparator could accept a sub-tolerance mutation
to an original-yield input clone. The successor adds exact independently declared
DTO-clone identity checks, including original income anchors, without changing
financial tolerances or Rust calculations. The identity helper is itself included
in the queued/current code bundle. Four official-default source-worker tests are
authored but unexecuted; actual source-selection browser and installed acceptance
remain required. Its root read-only application review passes; application waits
for the current storage-successor acceptance run and a separately reviewed actual-source
identity/Codec repair successor. Actual source-worker/browser inputs exposed
context-insensitive normalization of opaque raw Value fields, missing declared
numeric DTO fields and unsupported real ObjectReference transport nodes. These
are preserved integration failures; the 105 operational-fixture checks do not
establish that missing actual worker/browser coverage. Financial tolerances and
Rust source remain unchanged by the pending transport/identity corrections.

The reviewed coherent 17-file correction is now applied under
`completed-product-ftp-ui-public-adoption-0298`, preserving all three root storage
repair paths. Its 859-file freeze has aggregate
`db900cf5b82d2371fa0ce9346b6049e37bb482ec0323e2d8849100a53d93ce9b`.
The correction binds independently declared DTO clones, real Codec object
references and bounded logical table expansion, supplies explicit completed-source
selection, and polls modeled report status without full-result hydration.
Actual official-default source-worker and metadata tests now pass all 22 cases,
including the four previously unexecuted completed-source tests. Architecture
and both contract checks pass. Fresh complete acceptance now passes 2,252 engine
tests with two reference-only skips, 701 API tests, 47 existing browser cases and
four actual modeled-product FTP browser cases. The unchanged 923-test native
build/check gate is explicitly reused. The application receipt is complete;
these counts belong to this 859-file parent, not the subsequent CD/source2 freeze.
The frozen proposal separately passes 106 operational, 33 focused, four happy
browser and two late-publication refusal cases. Those contained results remain
distinct from installed-default acceptance. Historical source1 and Card2 selection
do not establish source2 multi-owner deposit FTP or complete issue acceptance.

The separately frozen 50-file deposit portfolio public successor composes explicit
multiple owners through one shared dated Sim. Its source-bound contained evidence
passes 24 Core and 38 Ledger checks, strict Clippy, 49 independent/source/copied-worker
checks and one actual durable browser case with paging, download and reload.
Manifest SHA-256 is `6ff75e97357ce715e10961241720deb9702a7a6f20186ea4d18188d22e75a56c`.
It preserves original source1 and all historical optional-None streams. Exact
current patch check and isolated byte roundtrip pass. Full Decision/official builds
and installed-default engine/API/browser gates remain required for application.
This advances shared multi-owner replay; supplied histories, complete saved/cohort
adoption, broader product policies, FTP and combined scale gates remain open.
The separate exact FTP/deposit composition now builds the entire copied Decision
dispatcher and passes eight actual source2 process/persisted checks and strict
Clippy across Decision targets. Frozen component receipt SHA-256 is
`4a0fd1ac8355e818b5e5a496e370738b228c9c90f6b5a1666bf55287c03be442`.
Its selected source2 cases forbid Python production financial callbacks and use
an explicitly contained executable as a metadata-only validation-library anchor;
they do not replace official-default tests. Unchanged test/ABI setup dependencies
were copied after preserved missing-file failures. The composed old FTP binder is
not accepted by these eight deposit tests and remains held for the corrections above.

The frozen 38-file CD public successor passes 100 contained source/process/worker/
SQLite/Parquet checks, 35 native fixtures and strict checks. Manifest SHA-256 is
`6bb9412a380716e2c68252f64d52a56c4415b34aa65cc8073a9fef0f1f1409d7`.
Its separate actual full native lifecycle execution also passes all 100 checks.
The static CD/deposit merge preserves all nonshared proposal bytes and explicitly
merges only `model_jobs.py` and `worker.py`; complete combined native/default
acceptance remains required. Every original issue acceptance criterion is retained.

The separate `derivative-mixed-opening-stage` now extends independent shared-journal
verification to reviewed seasoned asset/funding basis, gross accrued opening rights,
source fees, day-zero/early cash and later recognition. All 87 contained process
checks pass (25 new plus 62 original); the frozen native executable and complete
predecessor evidence are unchanged. Gross zero-crossing residues remain in the
journal and use the original monetary tolerances, with arbitrary added GL ownership
rejected. Receipt SHA-256 is
`83db403eaa5abdbe2e443b133b06740f9444793533ddc978f079a54a7b179081`.
No new native build/unit/Clippy evidence is claimed for that verifier-only successor.
The next isolated `derivative-public-native` stage composes those native shared owners
against the exact 851-file FTP baseline and prepares selected original-byte public
stream dispatch. Its standalone exact-coordinator composition now passes ten native
checks, 58 fresh historical compatibility checks, two strict Clippy gates and 29
independent actual-process/source/ordered-journal checks. Receipt SHA-256 is
`28b220a66da784563d340c60a40a1a4a83bb0c613f5ad213b51977fbcb8bee57`.
The separate `derivative-public-dispatch` successor has now built the complete copied
Decision library and actual `portfolio-model-ledger` binary with HiGHS. It passes
29 independent actual-dispatch process checks, ten new coordinator/wrapper tests
inside the complete crate and strict Clippy across its targets. The unit run is
filtered to the new owner, not the complete Decision suite. Frozen validation
SHA-256 is `1e7ed7dd3159e2616e1b5420cff74dd2b37a8c5e6d2fdccb6f3c3e08bbdb2766`.
Its first C++ configure attempt failed on a generated Windows path; the unchanged
sources subsequently built under a shorter distinct owned target. The original
failure and byte-limit error-wording assertion repair remain preserved. Public durable,
graph/saved-book/strategy/capital/FTP, broader mixed owners, empirical and full-scale
issue gates remain required.

The isolated `saved-derivative-mixed-1` successor now executes native derivative
sources in the existing dated daily `Sim`, with actual opening cash/equity posted
once, source-bound gross derivative/collateral stocks and separate custody memo
lineage. Its actual validation records six new native checks, 58 unchanged
compatibility checks, two strict Clippy gates and 62 independent process/source/
ordered-journal/daily-gross checks. Full historical card, consumer and deposit
optional-owner streams remain byte-exact. The component receipt is
`artifacts/derivative-mixed/validation.json`; the preceding saved-source component
is unchanged. These are isolated debug-native checks, not installed/public/default,
empirical or complete-scale acceptance. See `saved-derivative-mixed.md` for the
fully dated independent backbook scope and remaining integration gates.

All eleven official native gates for the 822-file applied deposit successor
completed successfully. The first full engine gate failed one test setup case:
`unknown_prefix` indexed scalar `reference_rate` as an array; 2,230 passed and two
reference cases skipped. The original failed receipt is retained. The exact
one-file test repair passes four focused default cases and is independently
attested in `artifacts/deposit-scalar-test-repair/attestation.json`. Native source,
native tests and all recorded official artifacts remain unchanged; their evidence
is explicitly reused. The distinct repaired 822-file freeze has aggregate
`2c8aa218d632e09dafceb1d5cc674f185afa4659da550849b8f14c4b182abf5d`.
Fresh full acceptance now passes 2,231 engine tests with two reference-only skips,
640 API tests with one warning and 47 scoped browser tests under the separate
`deposit-performing-public-adoption-0298-scalar-test-repair` prefix. Its original
native evidence is explicitly reused unchanged; these are new engine/API/browser
executions. This completes that local application stage, not issue #14/#25,
observed validation, broader scale or a deployed worker restart.

The isolated saved derivative source now has eight native tests, strict all-target
Clippy and 59 actual process/independent checks recorded in
`artifacts/saved-derivative-source/validation.json` (SHA-256
`f708700aae668a1b55cd9e4b30340d3098f350fe350046e7394b4cc2c0213dd5`).
It preserves complete row/trade/legal/opening and binary64 anchor identity before
reusing the existing native derivative accounting owner. Shared Core JSON admission
rejects nested duplicate keys and bounds counted/staged serialization without a
second financial Value tree. The 822 installed freeze and 162 copied baseline
files were checked; this component changes no installed financial runtime. The
actual shared-Sim mixed successor is now being implemented separately under
`artifacts/derivative-mixed/`. Native graph/risk, saved/public/default adoption,
solver/capital/FTP, empirical and full-scale gates remain required. See
`saved-derivative-source.md` and `saved-derivative-integration-plan.md`; no issue
is completed by this component receipt.

## Earlier accepted 0.29.8 source application

This earlier accepted local source set contains 756 frozen files, aggregate
`fd6ebc5c78f46502d6c6143aba8d8858d9764f40c8f26b029ed185ebe9ac5b99`.
Fresh installed-default checks pass 2,227 engine tests (two reference-only skips),
604 API tests (one warning) and 46 scoped browser tests. The accepted receipt is
`70c5485b4ba296ffa36ac2875acf483ea1b38161887334c09b977e514e17f965`.
The indexed-book durable route, revised-deposit income and mortgage closing-capital
route are applied and accepted locally. Deposit source-owned continuation and
modeled-product FTP successors remain separate work in progress. The paragraphs
below retain the chronological repair evidence; earlier failures are not passes.

The independently reviewed 87-file `deposit-performing-income-ledger-1` successor
is now applied locally. Its 822-file installed source freeze has aggregate
`67fdae38dda2820f45e97e6b25f892459ce3c1312192403d96b79378cc63754f`
and receipt SHA-256
`7521b0d1b1562df08dde7d50025845b927db269a00a5e8a43fd484c969ab1cbf`.
The exact final patch, original before/after inventory and 311 root-pinned evidence
files were checked against the unchanged accepted predecessor before application.
Original queued deadlines, transaction-bound request/revision/attempt authority,
bounded object reads and separate encoded/logical Parquet admission resolve the
reviewed public wiring findings. The four-case default process test is in the
ordinary engine test folder. Owner evidence is six Core, 18 Ledger and 103
contained checks; reviewer evidence is 13 checks and one intentional predecessor
expected failure. Fresh official native rebuilds, all eleven serial native gates
and complete installed-default engine/API/browser acceptance are running separately
under `artifacts/deposit-performing-public-adoption-0298-*`. No pass is inferred
from the preceding accepted bundle, and no deployed worker was restarted.

The reviewed indexed registry/raw-book/month-360 and per-call runtime-control,
revised-deposit income, and public mortgage closing-capital successors are now
applied locally. The official product, ledger and decision/workflow rebuilds and
all eleven serial native gates completed against 750 frozen source/test/contract files, aggregate
SHA-256 `4891804a840bd37d262096e851621c7c5e737960f7699b5b2631c3ca257d1cd2`.
Architecture (107 documented cyclic edges), generated HTTP and numeric/additive
native capability contracts pass. Native tests pass 903 (Core 303, Risk 373,
Ledger 148 and Decision 79), with strict Clippy for all four components. The first
installed-default engine gate failed with 2,205 passed, two reference-only skips
and five setup errors from one mortgage capital test fixture. That fixture used
display strings instead of typed Polars schemas; only the fixture was corrected.
All eleven affected tests now pass. The immutable failed receipt is retained.
The `product-adoption-0298-schema-repair` full engine rerun passed 2,210 tests with
two reference-only skips. Its API run passed 586 and failed one stale operation
inventory assertion (29 instead of the reviewed mortgage capital addition's 30).
Only that API test was corrected to verify the exact thirty durable names and
the new handler identity; all three affected structure tests pass. The distinct
`product-adoption-0298-allowlist-repair` freeze retains 750 files, aggregate
SHA-256 `9af106fe61d629d9f99444951f0f001cfc907ad2d11ffe3a826efa8a8702f3a4`.
Its attestation explicitly reuses unchanged native and full engine evidence and
runs fresh complete API and durable browser gates. Its full API suite passed 587
tests; browser acceptance passed 42 and failed three. The failures exposed a
mortgage importer comparing original queued JSON to the declared native DTO and
integer-spelled deposit monetary zeros in the independent expected journal.
The original and both subsequent failed receipts remain immutable.

The mortgage importer now invokes the existing independent admission's exact
typed-request check while retaining the original specification Value and queued
raw bytes. The revised-deposit verifier emits only declared Float64 monetary
report cells as floats, including zeros. Raw types, signed zero, fields, dates,
metadata and financial tolerances remain unchanged. Focused default-native
regressions pass 29 engine tests and two actual durable worker/reopen tests with
browser number spelling. The distinct
`product-adoption-0298-browser-identity-repair` freeze retains 752 files, aggregate
SHA-256 `84712be406a81c6e456c08d14c720b8fbe5964627f2c3f84935e0c66048c176c`.
Unchanged Rust source/tests/manifests and all eight official native artifacts are
attested explicitly; the 903 native tests and four strict Clippy checks are reused.
Its fresh engine suite passed 2,220 tests with two reference-only skips, and
the API suite passed 589 tests with one warning. Browser acceptance passed 43
and failed two: a stale display-heading assertion and integer-spelled financial
policy fields in independent mortgage capital expectations. That failed receipt
and browser traces remain immutable.

The capital reference now normalizes only declared f64 money, risk weights and
requirement fields in a private report-policy copy, after exact original request
checks. Null advanced RWA remains unavailable; inclusion booleans, metadata,
raw JSON and signed zero retain their identities. No generic comparison or
financial tolerance changed. The display assertion now checks the actual
accessible column heading. Focused default-native validation passes 18 engine
and 11 durable API/source/capital/publication-fence checks, including browser
number spelling, altered weights, boolean-money refusal and process reopen.
The distinct `product-adoption-0298-capital-browser-repair` freeze retains 752
files, aggregate SHA-256
`ec2376d15b1986823d96b06c4ee204ed58a182014d4333fbcfa9fde20a022f0d`.
It changes exactly four existing Python/test files, retains all native bytes,
explicitly reuses 903 native tests/four strict Clippy checks, and freshly runs
identity, full engine, API and browser acceptance. Those full gates now pass:
2,227 engine tests with two reference-only skips, 590 API tests with one warning,
and 45 scoped browser tests. The completed acceptance receipt SHA-256 is
`ed83ad0ed7c353b6f303b3d4123113c6b2cb4f2163414959254585e56614f698`.
No failed predecessor is reclassified as a pass.

The separate actual-native option study now publishes 2,160 PV/DV01 comparisons
over 12/24/48 annual steps, nested 64/1,024/2,048 paths and 1/2/3/8/32 factors.
Four scalar controls and complete pathwise payoff/risk replay retain unchanged
tolerances. Same-grid three-factor ATM payer PV differs materially from the
full-rank reference in both supplied positive/negative curves; finer grids do
not remove that approximation. Paired risk uncertainty is separately reported.
The study preserves the legacy deflator phase and cash-annuity floor and grants
no new-model admission or observed validation. Details and remaining gates are
in `docs/models/market-option-convergence.md`; recorded source/artifact and
statistical evidence remain in `artifacts/option-product-convergence-study/`.
Earlier accepted 0.29.7 test counts below belong
to that preceding source/artifact set. References to the three now-applied routes
below record their historical review stage; their latest local acceptance is the
756-file checkpoint above. Other separately identified proposals remain unapplied.
No deployment or production worker restart occurred.

The source freeze and evolving serial gate are
`artifacts/product-adoption-0298-source-freeze.json` and
`artifacts/product-adoption-0298-native-gate.json`. Complete original issue gates
remain required; no child issue is closed by source application.

A separate contained dated FTP preparation proof passes three Rust and 28
independent checks with strict Clippy and formatting. Explicit funding/cash
policies, complete gross origins and distinct earned-income/loss lanes are in
`artifacts/modeled-product-ftp-stage/`. Product source reconstruction, ordered
persisted journal verification, funding-curve charging and public FTP adoption
remain false; this component does not complete #24.

A separate modular raw-product funding source stage now reconstructs six genuine
mortgage/installment/Card2/revised-deposit source journals before independent
funding comparisons. It passes ten native tests, strict all-target Clippy and
48 independent process/reference checks, with no skipped gates. The frozen
source map verifies exactly, aggregate
`03e00e0228738493087fe11951b5a47c03e7202cb1a4834f2bb052484c75d492`.
Separate source converters, admission and preparation preserve the complete ordered
GL, exact original raw JSON versus declared DTO fields and independently excluded
economic memos. The initial derived source reports have acquisition omissions
identified by the subsequent review below; their seven-lane/cash completeness is
not established by the earlier passing fixtures.
All unweighted paths, including zero allocations, admit before generation.
Source work is at most 48M and raw-bounded aggregate preparation at most 2M,
so joined admission is at most 50M; component capacity is not aggregate RSS.
The hand-controlled default/recovery case preserves principal loss and collection
cost once, without a second economic-loss charge. Native funding-curve charging,
Treasury offsets, compatible capital policy, persisted/publication/default gates
and complete issue #24 acceptance remain pending. This stage is unapplied and
does not authorize product FTP or a regulatory methodology.

A separate read-only review verifies all 39 frozen source-stage files and finds
two concrete gaps in its selected-owner reports. Product and mortgage acquisition
cash is absent from `opening_source_cash`; the retained authentic consumer journal
posts purchase cash -102 while the derived opening receipt reports zero. Reviewed
initial allowance is posted by the outer acquired product owner, but is absent
from the selected installment `opening_earnings.initial_allowance` lane because
both native and independent classifiers read primitive intents only. The full
underlying GL remains correct; this is not evidence of duplicate journal postings.
Four static/classifier checks pass and two required-behavior checks reproduce the
omissions as explicit expected failures. No financial runtime was rerun by that
review. Its manifest SHA-256 is
`51c78b6cd2fc1edb4c27a57d75a0fa9ba5571ce5af5977c9dc2938745fccb289`;
evidence is in `artifacts/modeled-product-ftp-source-review/`.
The separate charge successor must bind actual outer acquisition transactions to
raw owner/path/date/instrument identity, retain purchase cash independently from
funding movements, and recognize supplied initial allowance expense exactly once
with allocation/participation applied once. Acquisition losses must stay distinct
from original-EIR recurring income, later provisions and modeled economic loss.
Nonzero allowance, zero/scaled allocation and same-date/later-credit-expense
reconciliation cases are required before claiming the selected reports complete.
The frozen predecessor and failing witnesses remain unchanged.

The nonzero-contingency successor fixture also exposes a third source-authority
gap: the Card2 accounting primitive's live undrawn channel is zero, while its
separate native economic commitment points retain undrawn 100 followed by 200
on principal repayment before an explicit later limit cancellation. Accounting
money channels do not establish available commitment or a zero contingent charge.
The successor must retain the native economic commitment source and independently
reconstruct the raw card limit/draw/repayment sequence, including full dated tails.
No contingent-cost or full selected-source completeness claim follows from the
frozen predecessor's zero memo values.

The charge successor is separately frozen in
`artifacts/modeled-product-ftp-source-stage/charge-successor/`, with 59 files,
aggregate `437b183d155d9bcf6f361dfe2eef3fbc834bc6d6077f9a1dd89b891d81123567`
and manifest SHA-256
`6e32745c04a0567ac44c59cb85c75ec3029fda3e91bab5273346533425a030da`.
Eleven native tests, strict Clippy and 97 independent/operational/map checks pass.
Actual acquisition cash, reviewed initial allowance and native Card2 commitment
now have distinct raw-source/ordered-journal receipts. Seven earned lanes remain
separate; only original effective interest enters recurring Treasury NII. Reviewed
positive incremental funding/undrawn costs are supplied physical liquidity-cost
policies, not pricing-measure option values. Root separately verified all 59 frozen
files, the actual isolated executable and all 39 predecessor files without another
financial execution. New joined admission is 62M work and 384 MiB charged capacity,
including optional raw capital replay; these are component bounds, not aggregate
RSS. The self-contained Core/Risk/Decision integration map remains unapplied.
Actual public runtime, durable worker, bounded persisted replay and browser gates
must be prepared against the newly applied deposit baseline before FTP adoption.

The separately frozen performing-deposit continuation proof passes 21 focused
native and 54 independent/reference-process checks with strict Clippy. It retains
original anchors, supplied prefix/lag state and later revision/gross rights.
Its exact ordinary-partial-runoff repayment-guard failure is preserved in
`artifacts/deposit-performing-continuation-stage/evidence/partial-runoff-guard/`.
Representable witness cases do not establish arbitrary runoff support. Source
cash and existing guards/tolerances are unchanged. Shared daily replay, saved
source/cohort, FTP and empirical acceptance remain open under #14/#22/#25.

The subsequent source-owned principal proof in
`artifacts/deposit-source-principal-stage/` passes nine native and 51 independent
checks with strict Clippy. Its opt-in, independently reconstructed principal
allocation receipts resolve both recorded cash-subtraction signs through explicit
balanced principal/basis reclassification. Source cash, original EIR, the old
generic guard/refusal and tiny signed closing accounting residues are retained.
Root review additionally passes 24 seeded numerical cases with competing exits.
All 750 installed frozen files and the earlier continuation/FTP proofs remain
unchanged. This is a contained research component; shared daily replay and public
saved/cohort/capital/FTP adoption remain pending.

The subsequent `artifacts/deposit-source-principal-ledger-stage/` proof now uses
the existing daily Sim for actual source cash, native principal atoms, balanced
principal/basis reclassification and one later original-EIR expectation revision.
It passes 16 copied native checks and 43 independent raw-source/persisted checks;
four complete historical None NDJSON controls remain exact. A separate strict
captured-stream review v2 passes 33 checks over 500 ordered journal lines and 62
actual atom checkpoints across two supplied runs. Root preserved three accepted
invalid-type mutations in the predecessor verifier and confirmed v2 rejects them.
The original 19-test review is not strict identity acceptance.

The proof freeze aggregate is
`b54659aef251a5bdf76498e161a998d4aa6b44a94dbf492d1479005278e22b95`.
Canonical joined work is 3,476,280 and retained reservation 18,044,292 bytes, with
912,108 emitted bytes. The actual two-run 64-day and 10,000-principal witnesses
remain refused by joined capacity and opening account domains respectively.
Tiny signed closing basis remains visible. The staging patch is byte-roundtripped
but depends on artifact proof crates and is not a production architecture proposal.
A separately frozen modular Core/ledger successor removes those artifact
financial dependencies. Its 170 listed files verify exactly, aggregate
`cf03cd753e4a94ec7a825c820bad3085009f621ae0ae1ef8c84cad3b2788ca7c`.
The real Core deposit loop emits optional immutable principal atoms; historical
None paths preserve original arithmetic. Shared principal/continuation/income
contracts feed the existing Ledger Sim. Four Core tests include 32 complete
historical source byte controls; 16 Ledger checks, default/feature/Core strict
Clippy and 43 independent actual-process/persisted-stream checks pass. The
complete canonical 912,108-byte NDJSON remains byte-exact against its predecessor.
Root source review confirms the original-loop hook and no financial proof-crate
dependency; cheap capture admission/cancellation and formatting refinements belong
to the next separate public successor. The modular patch remains unapplied.
A separate modular-source audit verifies all 170 shared freeze files and 13
handoff files, with six static checks and exact canonical/v2 capture identity.
It finds no new blocking financial discrepancy in the selected contained CLI.
Its two actionable public-successor safeguards are private sealed prepared/path
financial receipts and selected-owner final write/flush/control probes. The
public successor owner has accepted both; static review is not a new native or
financial test execution. Evidence is in
`artifacts/deposit-source-principal-shared-core-review/`.
Actual model-ledger coordination, durable transport/publication and selected
installed-default gates remain required before public adoption.
The evolving public continuation proposal's contained Parquet and SQLite tests
do not yet prove its actual worker path. Independent review found inherited
retry deadline renewal and no selected-route completion guard inside the final
database transaction. The actual queued/first-attempt deadline must persist
without renewal; source, executable, raw/artifact identity, STOP, lease/token and
deadline checks must run through a private verified receipt at commit. Actual
worker/retry/reopen and late-IO/publication adversaries are required before
application. The predecessor's manually supplied SQLite guard is retained as
contained evidence only. Root's read-only application guard separately verifies
all 756 accepted installed files, eight native artifacts and predecessor receipt/
log identities; twelve nonfinancial adversarial guard tests pass. No public
continuation patch has been applied by these checks.
Wider migration/inflow/signed-rate/seasoned, cohort/saved-book,
FTP, empirical and complete mixed-book scale gates remain required.

The reviewed indexed-book durable successor is
`artifacts/indexed-book-publication-root-review/`. Five failing STOP-during-IO
regressions, including final SQLite publication, are retained; the correction and
all earlier contained cases pass 77 checks. Four follow-through refusal tests
pass, and the staged dependency graph retains exactly 107 existing cycle edges.
The twelve-file patch is byte-roundtripped. The first
`apply_after_acceptance.py` run refused source application when preceding default
engine acceptance failed; that failed receipt is retained. Adoption requires a
new matching successful predecessor and a distinct reviewed application/acceptance
receipt before fresh default engine/API/browser and contract gates. Unchanged
native source/artifact evidence must be explicitly reused rather than claimed as
a new native gate. The reviewed patch is now applied after the complete
`product-adoption-0298-capital-browser-repair` predecessor. The distinct
`indexed-book-public-adoption-0298-capital-browser-repair` freeze contains 756
files, aggregate
`fd6ebc5c78f46502d6c6143aba8d8858d9764f40c8f26b029ed185ebe9ac5b99`.
Architecture, HTTP/native contract checks and seven architecture fixtures pass.
Its fresh full engine gate passes 2,227 tests with two reference-only skips.
Its fresh full API suite passes 604 tests with one warning; all 46 scoped
browser tests pass, including actual indexed-book publication and reopen through
the ModelLab JSON stringify/parse route. Thirteen new default API cases and one
browser case are included. The complete source/application/acceptance receipt
SHA-256 is
`70c5485b4ba296ffa36ac2875acf483ea1b38161887334c09b977e514e17f965`.
No native source or artifact changed; 903 originating
native tests and four strict Clippy checks are explicitly reused. The same
durable model-analysis operation dispatches the new kind, so the exact thirty
durable operation names remain unchanged. Full original issue criteria remain
required; public source application alone does not close a child issue.

Verified checkpoint bodies for #10/#12/#13/#14/#15/#22/#24 and tracker #25 are
updated on GitHub with the accepted scope, measured default counts, preserved
repair chain, actual option study and remaining requirements. Every original
implementation/acceptance checklist is retained exactly, and all issues remain
open. The before/after bodies and verified publication hashes are retained in
`artifacts/indexed-book-public-adoption-0298-capital-browser-repair-issues/`.
Changes remain local and uncommitted; no deployment or production worker restart
occurred.

## Ownership and order

A separate `cd-election-sequence-1` owner now joins repeated withdrawals,
maturity renewal/switch sources and subsequent elected actions using the existing
native CD contractual step. It retains actual settlements and replaces only the
target position's superseded maturity forecast, preventing duplicated future
repayment. Eight native hand/admission/control cases, strict all-target Clippy
and four independent actual-process tests pass; the independent tests include
twelve signed rate/penalty comparisons and seven invalid raw-input mutations.
The contained source/dependency/executable validation receipt SHA-256 is
`ba7df7d1e97de6d9d70a1960fc0db1242551d1782052434d8f9e87bf2157fa28`.
Original raw JSON remains separate from the typed DTO. No installed source or
official native artifact changed. Detailed state, cash ownership, mathematics,
primary sources and required follow-through are in
`artifacts/cd-election-sequence-stage/README.md` and `docs/models/term-enhancements.md`.
The common no-election terminal projection, expanded renewal/switch/grace raw
replay, public graph/OAS/cache adoption, joint exercise generation, empirical
histories and all original #16/#12 acceptance gates remain required. This is an
implementation stage, not CD or full-issue completion.

The common `portfolio-model-core` crate holds reusable typed financial contracts.
Product runtimes depend on it; it must not depend on API, persistence or product
crates. Inputs are explicit, bounded and versioned. Calculation stays in Rust;
Python adapters transport raw inputs and explicit independent Python references
remain test support. New contracts do not implicitly overwrite old model economics.

1. Define and validate shared credit transitions and dated cashflow contracts.
2. Integrate whole loans, deposits and corporate contracts with these foundations.
3. Implement auto, unsecured personal and revolving-card models and full product
   adapters, views, cohort audits and applicable strategy templates.
4. Complete CDs, derivatives/netting and contractual markets/funding flows.
5. Integrate accounting, aged forward coefficients and capital/FTP, preserving
   independent allocation and persisted-journal publication gates.

Independent contract improvements may run alongside foundational work. Model
and calibration dependencies are explicit; no product silently receives a newly
estimated coefficient or a different amortization convention.

The fresh 60,000-position successor benchmark is additional scale evidence for
the native graph, solver, accounting and daily replay path. It is recorded
separately because its synthetic fixture does not satisfy the observed-data,
product-policy, saved-book, durable-publication or production-service gates in
the ledger below.

## Requirement ledger

| Issue | Scope | Current stage | Completion evidence still required |
|---|---|---|---|
| #11 | Credit states, term PD/LGD/EAD, competing exits and recoveries | Expected/realized credit journals, chronological fitting and frozen-driver replay; acquired supplied-path installment/card original-EIR/default/allowance/gross-recovery accounting in one independently verified mixed GL | Correlated stochastic adoption, broader product policies, observed data validation, joint market/borrower generation and complete issue gates |
| #12 | Actual-date scenario cashflows | Durable scheduled/expected/realized routes and acquired installment/card owners use one acquisition-aware daily Sim/GL with independently verified complete unweighted tails; the reviewed multi-owner source2 deposit continuation is now applied in the 907-file CD/source2 successor with fresh native/default gates pending | Full credit/behavioral product capture, saved-book adoption and complete 60,000-position mixed-book acceptance |
| #13 | Mortgage/MBS prepayment, credit, ARM and guarantee mechanics | Supplied-path borrower/investor, ARM/default/advance state and immutable P/Q full-tail pricing graph; acquired-source and bounded mixed daily GL retain exact combined gross-state admission and independent persisted replay; public closing-capital and browser financial-number handling pass the latest 2,227 engine /604 API /46 browser checkpoint, with originating 903 native tests explicitly reused | Joint market/behavioral calibration, cohort/library/saved-book adoption, full-scale ledger acceptance and empirical holdouts |
| #14 | Deposit retention, migration and unified stress | Daily asymmetric repricing/retention/migration state and explicit P/Q full-tail liability graph; additive exact-observation account-full-exit/closure/migration calibration reports chronological holdout diagnostics and rate uncertainty, with complete independently replayed diagnostics and bounded cooperative execution; original-EIR and revised-expectation source1 replay passed earlier installed gates; the complete multi-owner source2 continuation is now applied with exact original/source/FIFO ownership and fresh matching native/default gates running | Observed paid-rate/regime fitting, balance-weighted partial-withdrawal calibration, complete migration/inflow/signed-rate/seasoned ownership, cohort/saved-book/full-scale adoption and source2 funding/FTP |
| #15 | Corporate/commercial schedules, resets, exercise and credit | Explicit indexed terms, canonical rights, performing draws, reviewed credit actions, before/after/rights-only finite exercise, original stopped Physical acquisition proof, supplied-law generated exercise/compact P/Q graph with retained-Q original quote calibration, acquired coefficients/strategy, durable cohorts/libraries and standalone finite candidate/treasury journals and complete-origin holding windows implemented; local0.29.15 server-bound exact-cut modeled prior openings and final holding SQL/integer-clock fences also validated on matching official artifacts | Verified prior-publication openings, cut-started mixed holdings and seasoned coefficients/secondary rights, nonintegral fee claims, daily in-arrears indexed earning, broader generated basis/path convergence and policy uncertainty, generated draws/reviews and streaming mixed candidates, verified opening evidence, complete saved/mixed/product adoption, observed histories/policies and full original joint-scale gates |
| #17 | Auto lending | Native installment/repossession/recovery/P-Q graph plus acquired non-PCD supplied-path credit accounting and exclusive mixed daily owner; durable source/journal/closing-capital gates pass | Broader credit/nonaccrual/fee/modification policies, cohort/library/saved-book integration, joint scenarios and complete empirical gates |
| #18 | Personal lending | Native unsecured installment/collection/recovery/P-Q graph plus acquired non-PCD supplied-path credit accounting and exclusive mixed daily owner; durable source/journal/closing-capital gates pass | Broader credit/nonaccrual/fee/modification policies, cohort/library/saved-book integration, joint scenarios and complete empirical gates |
| #19 | Revolving credit cards | Native statement/grace/promotion/payment/utilization state and full-tail P/Q graph; public acquired source and shared tagged mixed GL; explicit Card2 gross unpaid interest, signed live/default basis and separate original-EIR/realization owner pass rebuilt-release source, durable and browser gates | Wider fees/grace/draw/CCF, default-before-principal-zero/nonaccrual/PCD/modification policies, cohort/library/saved-book adoption, joint scenarios and complete empirical gates |
| #16 | CDs | Explicit withdrawal/penalty/renewal/switch contracts; chronological graph2/reprice2 sequence retains locked original OAS, exclusive unsuperseded tails and post-principal-zero rights; the combined public CD/source2 successor is now applied after 158 distinct contained checks and exact 86-file root review, with fresh matching native/default acceptance pending | Complete installed/public sequence adoption, broader contractual policies, joint election/exercise generation, ledger/library/saved-book adoption and complete empirical gates |
| #20 | Derivatives and dealer counterparties | Raw dated swaps/European options, CSA/MPOR/closeout and reviewed physical accounting; frozen saved-source/shared-Sim components and seasoned opening replay; selected raw enum refusal passes 13 native/221 contained checks, and the durable 28-file proposal passes 221 rebased checks; additive CD/source2 and joint journal composition remains staged | Installed/durable saved hedge graph and mixed journal/solver/capital adoption and complete issue gates |
| #35 | Formal ASC 815 hedge accounting | Explicit `hedge-accounting-1` lane: FVH/CFH relationships, assessment, OCI/basis mechanics, single journal, independent Python replay and durable `/models/runs` publication; ASU 2025-09 similar-risk group, choose-your-rate and swap/written-option elections from the issued text; seasoned relationships with reviewed retained opening balances; `hedge-treasury-1` closing and explicit report-date capital with reviewed cash-flow-hedge AOCI, deferred-tax, opt-out and phase-in policy | Accounting-standard approval, observed effectiveness, ASU 2025-09 dual-hedge election, portfolio-layer lane, net investment/standalone options, saved-book and shared-journal adoption, interim capital/FTP and complete scale |
| #21 | Markets/repo/funding | Actual-date funding/rollover/collateral/default state plus whole-book P/Q pricing graph with retained replacement and collateral tails implemented | Ledger/library/treasury/saved-book integration and complete issue gates |
| #10 | Market drivers | Supplied multicurve/smile/PSD joint-driver contract; source-linked explicit-schedule par bootstrap with quote residuals; native incremental registry/raw-book/month-360/runtime controls and indexed-book durable route now pass installed-default acceptance; actual-native option study publishes 2,160 paired PV/DV01 comparisons; Treasury history and strike diagnostics remain isolated analysis | Joint stochastic market/borrower generation, quote-vintage/observed predictive calibration, smile/new-model option/risk admission, complete saved-workspace/cohort adoption and full issue gates |
| #22 | Income accounting | Dated yield/fee/review engine, credit/accounting/claims, deposit original-EIR/mixed acquisitions and public revised-expectation income route pass installed-default release/application gates; installment/Card2 original-EIR and realization share the verified GL; paired mortgage analytical source and bounded mixed owner remain independently replayed | Complete product yield/reset/modification/nonaccrual adoption, complete source-owned deposit portfolio adoption, product FTP and complete-scale issue gates |
| #23 | Forward strategies | Native actual-purchase fixed units and performing consumer/card/mortgage/deposit units retain original EIR/OAS/quote, actual prefixes and full-tail funding; native HiGHS, selectively retained staged preparation, verified durable coefficients/reports and one actual dated performing backbook/candidate GL | Full behavioral/credit state-conditioned products, saved-book integration and complete issue gates |
| #24 | Capital/FTP | Native closing-capital bridge retains verified credit/mixed journal lineage; the 859-file product FTP/source-selection parent now passes 2252 engine/701 API/47 existing+4 FTP browser cases with 923 native tests explicitly reused; reviewed Card2/source1 bindings, explicit policies and full-tail channels are accepted locally; subsequent CD/source2 gates are separate | Source2 multi-owner funding/FTP, jurisdiction policies, interim default regulatory policy, full product/report-date coverage and full issue gates |

## Research basis and design decisions

Research checked 2026-10-01, with FTP and model-validation sources refreshed
2026-10-03. Primary sources guide model structure; the project
does not copy supervisory parameters or claim supervisory suitability.

- The Federal Reserve's [2025 detailed model descriptions](https://www.federalreserve.gov/publications/2025-june-supervisory-stress-test-methodology-descriptions-supervisory-models.htm)
  motivate product-specific delinquency transitions and competing exits. The
  [2026 methodology](https://www.federalreserve.gov/publications/files/2026-february-supervisory-stress-test-methodology.pdf)
  documents the subsequent annual scope and adjustments. We use these as dated
  modeling references, not substitutes for a bank's data or applicable policy.
- The [Basel credit/ECL guidance](https://www.bis.org/publications/201512-guidelines-guidance-credit-risk-and-accounting-expected-credit-losses)
  supports keeping credit risk estimation, expected cash shortfalls and accounting
  policy explicit. A supplied economic hazard is not an accounting staging rule.
- Current [SR 26-2 guidance](https://www.federalreserve.gov/supervisionreg/srletters/SR2602.htm)
  supersedes SR 11-7. Validation records must use the current, risk-based framework
  while preserving independent financial reference tests and documented limits.
- The [interagency FTP guidance](https://www.occ.treas.gov/news-issuances/bulletins/2016/bulletin-2016-7a.pdf)
  distinguishes funding from contingent liquidity risk and calls for granular,
  transparent allocation. Its inception perspective motivates retaining the
  original funding policy/curve identity separately from a later closing source.
  The [BIS liquidity-transfer-pricing research](https://www.bis.org/publications/fsi-paper-10-liquidity-transfer-pricing-guide-better-practice)
  motivates matched maturities and marginal funding costs; it is research, not
  approval of this application's supplied policies. Product source reconstruction
  alone grants no permission to infer a liquidity tenor, set an optionality charge
  to zero, relabel basis/credit/revision P&L as recurring external interest or count
  modeled economic loss again in FTP. Contractual reset and liquidity tenors stay
  separate, consistent with the [OCC interest-rate-risk handbook](https://www.occ.ustreas.gov/publications-and-resources/publications/comptrollers-handbook/files/interest-rate-risk/pub-ch-interest-rate-risk.pdf).
- [New York Fed SOFR methodology](https://www.newyorkfed.org/markets/reference-rates/additional-information-about-reference-rates)
  distinguishes actual value-date day weights and overnight compounding. Contracts
  therefore require explicit observation intervals; no hidden conversion from
  three-month simulated forwards to contractual overnight fixings is permitted.

The shared deterministic credit stage uses explicit continuous-time hazards and
absorbing default/prepayment states, with cures among active states. This is a
project design choice: it prevents independently applied exit probabilities from
spending the same surviving exposure twice. Piecewise intervals, exposure inputs,
loss severity and recovery timing are supplied assumptions until calibrated.

## Validation levels

For each model retain controlled-path hand calculations, explicit independent
references, domain/failure admission, conservation and zero-change identities.
For stochastic models also retain seeded shared-draw, convergence and stress
comparisons. Fresh-process ownership tests must forbid Python financial callbacks.
Product-to-ledger adoption additionally requires independent replay of immutable
postings and failure/cancellation publication tests. Empirical outcomes use separate
chronological holdouts with source hashes and never borrow training observations.

## Open input requirements

Empirical prepayment, deposit retention, consumer transition/payment and recovery
fits require dated observed histories. The repository's synthetic fixtures cannot
prove their predictive accuracy. Regulatory eligibility/requirements and accounting
policy require explicit reviewed inputs. Code progress can continue without them;
those acceptance gates remain open and must not be marked complete.

## Validated acquired consumer-credit integration

The new [installment credit accounting contract](consumer-credit-accounting.md)
retains explicit purchase/original-yield anchors, reviewed allowance and default
policy, and separate gross recovery and collection costs. Its exclusive
[mixed daily owner](product-mixed-consumer-credit.md) executes with performing
backbook/candidates in the same native GL. The source and ordered journal gates
include zero participation, future acquisitions, reviewed initial allowance and
complete unweighted claim tails. Official product, ledger and decision/workflow
releases were rebuilt serially. The corrected bundle passes 1,450 engine tests
(two reference-only skips), 417 API tests and 34 browser cases, including durable
mixed publication and exact full-tail closing-capital use. Public component
coverage is 647 native tests; the separate card prototypes are not adoption
evidence. See the [validation report](../reviews/2026-10-02-product-model-stages.md).

This bounded integration advances #11, #12, #17, #18, #22 and #24 without completing
their wider criteria. Fees, nonaccrual, modifications, PCD, joint stochastic
generation, cohort/library/saved-book adoption, modeled FTP and empirical/combined
scale acceptance remain separate requirements. The subsequent public card
primitive and tagged installment/card mixed owner pass the release and
application gates below. The subsequent Card2 lane adopts the reviewed bounded
interest-basis policy through its separate source and mixed-journal gates.

## Validated public card and tagged mixed integration

The [card accounting contract](card-credit-accounting.md) has 15 public native
fixtures and 93 independent/public-process checks. The official lifecycle has
been rebuilt and passes ten durable primitive publication/failure-fence checks.
The [tagged product owner](product-event-owner.md) and
[mixed wrapper](product-mixed-product-credit.md) execute installment and card
paths in the existing daily Sim and GL. Full component suites pass 84 ledger and
71 decision tests with strict Clippy, and the installed
[independent verifier](product-event-reference.md) passes 84 checks against the
fresh DEBUG workflow. The old consumer wire result remains byte-identical.

The official workflow release is rebuilt; 300 focused source/journal checks and
28 new durable primitive/mixed publication, capital and failure-fence checks pass.
The complete bundle passes 1,627 engine tests (two reference-only skips), 445 API
tests and 37 browser cases. Public component coverage is 677 native tests.
The saved card/mixed source and exact full-tail closing-capital flows reopen with
their independently verified receipts. This stage does not adopt the separate
interest-basis proof, grace/fees/draws, nonaccrual, PCD, joint stochastic credit,
cohort/library/saved-book policies, empirical holdouts or combined scale gates.

## Validated Card2 gross-interest and basis ownership

The explicit [Card2 contract](card-credit-accounting-v2.md) retains whole-carrying
original EIR, separate live/default interest basis and complete actual reviewed
collections after principal zero. Its mixed atomic events post once through the
existing Sim/GL; unweighted gross rights, basis and commitments close separately
before any allocation/physical weight applies. Historical Card1/consumer bytes
remain unchanged. Component suites pass 733 native tests (316 Risk, 246 Core,
97 Ledger, 74 Decision) with strict Clippy. The focused group passes 215 source,
mixed journal and capital hand checks against the installed default release;
the 38 new durable mixed/closing-capital cases pass in the full API suite.

Separate matching `closing_capital_eligible:true` native/independent/persisted
permission requires ordered journal replay, exact full-tail closing date and
supplied capital/currency/unit/weight policies. The distinct FTP marker stays
false. All official native artifacts are rebuilt serially. The installed default
bundle passes 1,842 engine tests (two reference-only skips) in 444.72 seconds,
493 API tests in 410.61 seconds and all 40 browser cases in 2.5 minutes. No DEBUG
override is selected in these final gates. Identities and the local warmed
coefficient-only timing fixture are retained separately under
`artifacts/card-credit-v2-model-identities.json` and
`artifacts/card-credit-v2-interactive-benchmark.json`, with `deployed=false`.
These bounded synthetic/application checks do not complete wider #25 criteria.

## Delivered foundation stage

The subsequent [acquired mortgage source](mortgage-credit-accounting.md) is an
explicit analytical contract. Its existing-generator receipt seam retains exact
historical None-path bytes and captures actual boundary operators once. Original
and current Physical sources have separate full-tail hashes; changed prepayment,
credit or ARM observations cannot refit original purchase cost/yield/carry. Core
all-target tests pass 281 and Risk 316 with strict Clippy. Installed independent
hand/process checks pass 164 and durable publication/tamper/cancellation checks
pass 31 against the fresh explicitly selected DEBUG lifecycle. Strict raw parsing
at the new contract boundary rejects ignored fields, enum aliases and duplicates;
dependent releases/default-artifact/browser gates remain pending. No daily-journal,
capital, FTP, empirical or accounting-standard adoption is inferred. The separate
exact-day pending-exit prototype passes 16 native and 140 independent checks with
atomic compound actions and an explicit 10,000 money cap. It is not an installed
journal owner.

The later analytical acquisition and strict-instruction checkpoint rebuilt all
three official native runtimes. It passed 788 native component tests, 2,066 engine
tests (two explicit reference-only skips), 531 API tests and 41 browser cases
against default release artifacts. These results apply to that recorded source
checkpoint, not subsequent changes. See
[the validation ledger](../reviews/2026-10-02-product-model-stages.md).

Subsequent source changes add the explicit
[market-indexed adapter](market-indexed-adapter.md) and
[mortgage mixed expected-accounting route](mortgage-mixed-accounting.md) locally.
The mortgage route shares the ordered daily GL and adds exact combined gross-state
admission plus incremental independent persisted verification. Copied native and
independent source checks are recorded separately from installed acceptance.
Architecture, native/HTTP contracts and TypeScript/Vite checks pass. All three
matching official runtime builds are complete; Core, Risk and Ledger component
tests and strict Clippy pass: Core 288, Risk 342, Ledger 129 and Decision 79,
838 total. Current default engine/API/browser gates remain pending. The two
subsequent Core test-fixture repairs have
an exact production-code identity attestation. Applied mortgage source review
also retains every original process test alongside the added tamper checks.
No issue completion, empirical validation, accounting approval or mortgage
capital/FTP permission is inferred from applying these sources.

Detailed contracts: [credit](credit-model.md), [observed credit fitting](credit-calibration.md),
[dated events](dated-cashflows.md), [dated term production](dated-term-contracts.md),
[fixed level payments](contractual-amortization.md), [floating coupons](floating-rate.md)
and [native entrypoints](native-contracts.md). The new shared crate has no API,
persistence or product dependency. Model versions and complete supplied assumptions
remain explicit. No issue is closed by delivering these foundational stages.

[The daily integration design](daily-integration-plan.md) records the full native
state/event ownership boundary, accounting-policy separation and independent
publication gates. Version 0.29.6 adds the explicitly selected
[dated scheduled-recognition route](dated-ledger.md) and [bounded store](dated-store.md).
The [credit operator](credit-operator.md) and [credit-accounting policy](credit-accounting.md)
are separate tested primitives; their automatic native state adoption remains open.
Legacy daily specifications keep their historical behavior.

The current additive contracts include [consumer installment lending](consumer-installment.md),
[revolving cards](consumer-card.md), [indexed/optional corporate and CD contracts](term-enhancements.md),
[joint daily credit state](credit-state.md) and [daily credit replay](credit-replay.md).
`analytics.credit_ledger.run_credit_ledger` gates publication with an independent SciPy
matrix-exponential and policy/posting replay over persisted partitions. `/balance-stress/dated`
and `/balance-stress/credit` execute as durable worker jobs; the partitioned panel pages,
downloads and reopens their stored source evidence. The credit route is loan-only with
62,000 aggregate run-position contracts; it does not yet establish combined mixed-book
60,000-position graph/solver/journal acceptance. No empirical gate is completed by these
analytical fixtures or application checks.

The [dated replay and credit checkpoint](../reviews/2026-10-01-dated-ledger-and-credit-foundations.md)
records the 0.29.6 native/publication gates, rebuilt artifact identities and the
remaining integrated product/daily acceptance requirements.

The [local foundation validation report](../reviews/2026-10-01-model-foundations.md)
records 100 passing Rust tests, 20 native-boundary tests and the final full-engine
result of 755 passed with two intentional reference-only skips. This is a partial
implementation checkpoint; empirical and integrated daily acceptance remain open.
## Native workflow 60,000-position successor benchmark (local; 2026-10-04 UTC)

The fresh native workflow scale run at [artifacts/native-workflow-60000-successor/report.json](../../artifacts/native-workflow-60000-successor/report.json)
completed with 60,007 total positions (60,000 supplied positions plus the
generated candidate), eight paths, a 27-month horizon and four worker threads.
The Rust-owned graph/edit/unit/HiGHS/accounting/daily-ledger workflow completed
in 391.034 seconds, with 1,285.707 MiB peak process-tree RSS and 628.285 MiB
native RSS. It emitted 10,497,266 journal rows, 3,360,392 exposure rows,
364,834 trial-balance/GL keys and 1,620,189 cashflow rows. The persisted journal
replay passed (`journal_replayed=true`) and the report records the native binary
and source identities. The report SHA-256 is
`9ce8524d758a2bca6b633f402a37a3a73fb0a5a457dca0c93b997e2096083f4a`.

This is fresh synthetic scale evidence for the native workflow and its
independent replay. It does not close the complete issue-25 gate: the fixture
does not contain active credit-default, collateral, netting or management-action
states, uses research regulatory weights, has no durable publication, and does
not validate observed behavioral/credit calibration, saved-book/library/cohort
adoption, source-specific FTP, or production HTTP/object-storage behavior.

## Native shared-model and product-runtime validation (local; 2026-10-05 UTC)

The current checkout passes fresh all-target Rust tests for
`packages/portfolio-model-core` and `packages/portfolio-risk-native`, followed
by strict Clippy (`-D warnings`) for both crates. The shared core covers dated
cashflows, actual-date income, amortization, floating fixings, deposit state,
consumer/card/mortgage state, credit transitions, exact-observation fitting,
credit paths/accounting and bounded source capture. The product runtime covers
native market paths, mortgage/deposit/card/funding/derivative/term graphs,
cohort and product units, aged units, strategy coefficients, treasury bridges,
fixed-OAS reprice controls and cancellation/admission boundaries.

These fresh-process checks establish compilation, hand-case parity, strict input
domains and native failure behavior for the current source. They do not establish
observed predictive accuracy, saved-book/cohort/library adoption, accounting or
regulatory approval, durable production publication, or the complete mixed-book
credit/behavior/solver/ledger acceptance required by issue #25.

The matching independent Python reference slice also passes 543 tests in 93.55
seconds across credit, consumer/card, product-rights, candidate, mixed-product,
strategy and unit-contract references. That result is oracle coverage and does
not turn synthetic fixtures into observed calibration evidence.

The installed default CD sequence path now passes a fresh 56-test native process
gate, 34 durable worker/Parquet-reopen tests and 10 HTTP/admission tests (one
FastAPI deprecation warning). These checks exercise the release
`portfolio-lifecycle` binary without Python financial callbacks, preserve exact
raw/source/receipt verification and retain the explicit false capital/FTP
eligibility marker. They establish installed sequence transport and publication
behavior only; observed withdrawal/renewal holdouts, broader contractual policy
coverage, joint optimal exercise generation and saved-book/library/cohort/ledger
adoption remain open under issue #16.

The adjacent native term/product path also passes 66 independent term/native
owner tests and 13 durable indexed-book worker/publication tests. These checks
cover actual-date indexed resets, retained payments and balloons, supplied
market identity, fixed-anchor repricing, bounded product-library persistence and
reopen. Joint stochastic borrower/credit generation and complete corporate
ledger/library/saved-book adoption remain separate issue #15 gates.

The deposit continuation and income routes also pass 70 independent/native
reference tests and 45 durable worker-job tests. They cover asymmetric paid-rate
response, migration and runoff ownership, signed daily expense, original-EIR
basis, FIFO source retirement, complete tails, cancellation and persisted
reopen. Observed retention fitting, seasoned/inflow/negative-rate accounting and
full source-owned portfolio/FTP adoption remain open under issue #14.

The mortgage/whole-loan slice passes 256 independent/native tests and 90 durable
API worker tests. These cover ARM resets and recasts, borrower/investor P/Q
paths, advances, default buyout versus recovery, full recovery tails, mixed
rights ownership, capital-source fences and persisted replay. Joint empirical
prepayment/credit calibration, saved-book/cohort/library adoption and complete
production-scale ledger integration remain open under issue #13.

The auto/personal/card product slice passes 140 native product-process tests and
37 durable credit/path/treasury/library tests. These cover installment
amortization, prepayment, repossession and recovery, unsecured collection,
statement/grace/promotion/utilization state, default/recovery accounting,
original-EIR units, coefficient-only strategy preparation and credit-source
bridges. Broader fees, draws, CCF, nonaccrual, PCD/modification policy,
state-conditioned joint scenarios and observed holdouts remain open under
issues #11, #17, #18 and #19.

The derivatives/funding/strategy/capital slice passes 156 native/reference tests
and 158 durable API tests (one deprecation warning). It covers dated swaps and
options, CSA/MPOR/closeout accounting, collateral and rollover contracts, aged
unit coefficients, native strategy ownership, treasury bridges, source binding,
Parquet replay and publication-failure cleanup. Saved hedge/netting adoption,
joint credit/market generation, jurisdiction-specific regulatory policy and
complete mixed-book capital/FTP acceptance remain open under issues #20–#24.

The new `joint-state-1` shared-model contract now composes supplied market
discount factors with competing behavioral-exit, credit-default and prepayment
hazards under one caller-owned CRN stream. Rust validates actual-date
contiguity, identity, domains and bounded work; `evaluate_joint_state` remains
transport-only. Native dispatch and core tests cover exact competing-hazard
probabilities, default recovery/loss separation, identity preservation and
failure cases. This closes a reusable foundation gap, but it does not close
issue #10–#25: observed calibration, joint scenario generation, saved-book and
cohort adoption, daily journal/FTP integration, and the complete mixed-book
scale gate remain required.

The same contract is now admitted by the durable `/models/runs` route with an
independent Python publication oracle. Worker/reopen and failure-injection
checks cover native execution, source replay, immutable table persistence and
failed-publication cleanup for the joint-state example. This extends transport
and integrity coverage only; it does not convert the supplied driver fixture
into empirical evidence or mixed-book adoption.

The additive `market-driver-calibration-1` contract now exposes the existing
native four-factor moment fit through a versioned source/split identity. Its
chronological holdout diagnostics, deterministic scenario selection and
cooperative cancellation are independently replayed by the durable worker.
The synthetic example proves leakage control and arithmetic only; quote
bootstrapping, observed predictive calibration, uncertainty, product adoption
and the remaining #10 gates stay explicitly ineligible.

The durable model table adapter now materializes numeric vectors and matrices
as lossless Parquet tables and restores their original JSON shapes before the
independent verifier runs. This closes the transport gap exposed by the
market-driver diagnostics while preserving existing row-table layouts; the
focused worker publication case and 69 related table/publication tests pass.

The additive `joint-market-credit-1` contract now composes the PSD market
generator with the shared competing behavioral/default/prepayment state engine.
It binds market source and CRN identities, requires actual-date duration parity,
maps annualized innovations through bounded supplied hazard elasticities, and
supports cooperative cancellation without returning partial paths. Native,
independent adapter, durable source-replay and 41-model worker-matrix checks
pass. The synthetic route remains adoption-ineligible: observed joint
calibration, product policy, saved-book/cohort, journal/FTP and full-scale
acceptance remain open under #10, #11 and the product issues.

The market-driver calibration contract now reports an explicit Bartlett/HAC
mean standard error with a declared lag. Native and independent replay keep
the uncertainty fit on training rows only and preserve chronological holdout
leakage control. This is parameter-uncertainty and integrity evidence; it is
not observed predictive validation, quote bootstrapping, production adoption,
or closure of issue #10.

The exact-observation credit fitter now emits inverse-information rate
standard errors for positive observed transition counts. Zero-event and
supplied rows remain explicitly `null`, preserving the distinction between
missing information and zero future risk. This improves uncertainty disclosure
for #11 while leaving empirical censoring audits, product PD/LGD/EAD,
recovery fitting, stress validation and journal adoption open.

The additive `market-quote-bootstrap-1` contract now retains source-linked
par-quote schedules, solves explicit discount pillars without an implicit
calendar, and reports independent bootstrap and validation residuals. Native,
adapter and durable replay gates still mark the synthetic curve adoption
ineligible. Quote-vintage backtesting, arbitrage/smile calibration, collateral
policy binding and product graph adoption remain open under #10.

The additive `observed-deposit-calibration-1` contract fits exact account-full-exit
spells for withdrawal, closure and migration causes with shared exposure,
positive-count rate uncertainty and chronological holdout replay. Sampling units
are explicit; partial balance withdrawal rates are not inferred. Shared observation
clocks preserve the existing credit schema. Typed supplied assumptions, bounded
cooperative execution and independent replay of every diagnostic/identity field
gate the analytical output. It remains adoption-ineligible and does not infer
paid-rate elasticity, intraday behavior, inflows, maturity or FTP; those #14 gates
still require reviewed histories and product-owned journal integration.

2026-10-10 local follow-through clarified the explicit complete-account sampling
unit and added shared observation clocks, typed assumption provenance, bounded
cooperative fitting and complete independent deposit diagnostic replay. Release
validation passed 25 shared adapter cases, 18 replay/cancellation cases, 53 durable
worker/persisted-failure cases and eight final-publication fences. Core, Risk and
Ledger all-target regressions passed; dependent decision/workflow release
regressions also passed. Product, ledger and decision/workflow release
builds succeeded. An earlier Windows Python/Polars collection resource failure
and interrupted native test attempts are not passing evidence. No issue is
closed by these mechanics. Every original #10-#25 acceptance gate remains required,
including broader product adoption and observed histories/reviewed policies.

The full installed engine regression on this follow-through passed **2,378 tests
with two skips in 449.35 seconds**. This does not waive skipped, empirical,
complete product-adoption or combined-scale gates. Dependent decision/workflow
release tests passed after bounded two-job compilation; strict checks passed for
all five native crates. Full API results are recorded separately after completion.

The first full API pass reported **897 passing cases and one failure** in
completed Card2 source reopening for FTP. The selected-source decoder only
accepted record-table layouts, while analytical sources also retain scalar
vectors/matrices. Shared strict scalar-table restoration now serves both paths;
it rejects unknown/extra layout fields, boolean/zero/oversized widths, mismatched
columns and nested cells before source replay. Record JSON decoding and the
independent raw-source/ordered-journal gates remain in place. All **64 related
source, metadata, layout and deposit replay cases passed** after correction.
Further scalar transport checks found that heterogeneous numeric JSON lists
could fail Polars inference. Homogeneous primitive columns now retain explicit
types (including UInt64), while mixed or unrepresentable lists retain their
original JSON representation. Parquet roundtrips verify exact scalar types and
identifiers. All **66 related cases passed** with this final correction. The
intermediate API successor was interrupted before completion to test the final
source. The final-source full API suite passed **920 tests in 860.88 seconds**
with one existing Starlette/httpx deprecation warning. Neither interrupted nor
failed predecessors count as a pass. The full engine result, all five native
regression suites/strict checks, official release builds and architecture/native/
HTTP contract checks are separate local evidence. No production restart,
deployment, observed empirical validation or combined 60,000-position acceptance
is claimed; all fifteen child issues and tracker #25 remain open.
## Corporate exercise publication follow-through (local 0.29.9; 2026-10-10 UTC)

The next #15 stage closes the existing `conditional-exercise-1` durable
publication gap. The independent raw-tree oracle reconstructs every Bellman
value, stopping decision, live probability and complete pricing cash right.
The original OAS must match exactly; identities, exact dates, source provenance,
shape/type and finite domains are retained. Raw and reopened Parquet results
must both pass under the original deadline and ownership fences. The receipt
explicitly rejects physical-journal and empirical eligibility. Native caller
control is propagated across Rayon and checkpoints admission, both tree passes,
branches, conservation and final audits without returning a partial policy.

Final matching-source validation passes **409 native product all-target tests**,
strict product all-target/all-feature Clippy, **88 affected engine tests in
17.18 seconds**, and **285 affected API/worker/persistence tests in 113.80
seconds**. The three official release artifact builds succeed; official decision
release Clippy also passes. Architecture, seven boundary-script tests, native ABI
and HTTP contracts, synchronized 0.29.9 metadata/lock and operator packaging pass.
The wrong-root initial Python collection, DLL-locked predecessor build and
interrupted intermediate API run are not passing evidence. The full
2,378-engine/920-API checkpoint above remains prior-stage evidence.

See `docs/models/term-exercise-publication.md` for mathematical scope, controls,
independent stochastic call/put refinement and failure coverage. This is an
existing analytical route's verified-publication stage. Calibrated joint
market/credit production, commercial credit/accounting ownership, complete
ledger/library/cohort/saved-book adoption, observed validation and every original
#15/#25 acceptance criterion remain required. No issue is closed, deployment
performed, production worker restarted or combined-scale acceptance claimed.


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


## Reviewed earned-fee refunds (0.29.18)

The standalone nonintegral fee owner and native commercial loan/fee journal
support explicitly permitted earned AND collected service refunds. A reviewed
refund reverses fee cash/income, retaining separate cumulative refunded earning
and historical gross service. It cannot grant new earning/collection capacity,
use prepaid cash or waived recoveries, or change loan original cost/EIR/credit
paths. Native exit policies optionally select a bounded refund fraction and
reserve the additional group before generation; absent/zero selections retain
historical outputs and admission. Signed gross rights, full tails, independent
raw-source/persisted replay and original publication fences remain required.

See docs/models/nonintegral-fees.md and commercial-fee-journal.md for the supplied
policies, controls and exclusions. Fee Pricing/acquisition/units/capital/FTP,
exercise/candidate/holding and draw/secondary adoption, in-arrears earning,
generated streaming/convergence, full saved/cohort/mixed adoption and observed/
reviewed/full60000 acceptance remain open under15/25. Rebuild compatible product,
ledger and decision/workflow artifacts and explicitly rebuild saved sessions/
libraries before new execution; local validation does not deploy or close15.
