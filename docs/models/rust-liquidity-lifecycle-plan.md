# Rust liquidity and trading book lifecycle implementation plan

Status: stages A to C delivered as an opt-in contract ([liquidity-lifecycle-contract.md](liquidity-lifecycle-contract.md));
stages D to I remain proposed implementation. This plan connects new instruments, hedges,
financing, margin and collateral to one chronological simulation, with particular
attention to full and partial trading-book unwinds. It specifies implementation
work and acceptance gates; the proposed schemas and services are not delivered
capabilities.

The feature must run without Python, from HTTP admission and durable job ownership
through native calculation, independent verification, artifact publication and
result reopening. All new backend code and backend tests are Rust. The existing
React/TypeScript interface remains the client. HiGHS retains its existing C++
implementation under the repository execution contract; Rust constructs its
problem, invokes it and validates the returned allocation.

## GitHub issue tracking

[Lifecycle tracker 42](https://github.com/beauschwab/model-portfolio-sim/issues/42)
contains this implementation design and is a native subissue of
[product tracker 25](https://github.com/beauschwab/model-portfolio-sim/issues/25).
The following are native subissues of the lifecycle tracker, with exact dependencies
and acceptance checklists in their bodies. The aligned
[ASC 815 plan 35](https://github.com/beauschwab/model-portfolio-sim/issues/35)
retains formal accounting ownership and shares the Rust verification/service path.

| Stage | GitHub subissue |
|---|---|
| A | [43 Relationships and ASC 815 handoff](https://github.com/beauschwab/model-portfolio-sim/issues/43) |
| B | [44 Acquisitions with funding and hedges](https://github.com/beauschwab/model-portfolio-sim/issues/44) |
| C | [45 Partial unwinds and settlement](https://github.com/beauschwab/model-portfolio-sim/issues/45) |
| D | [46 Independent Rust verification](https://github.com/beauschwab/model-portfolio-sim/issues/46) |
| E | [47 Rust API worker and storage](https://github.com/beauschwab/model-portfolio-sim/issues/47) |
| F | [48 Saved books and workbench](https://github.com/beauschwab/model-portfolio-sim/issues/48) |
| G | [49 Long term plans and policies](https://github.com/beauschwab/model-portfolio-sim/issues/49) |
| H | [50 Prepared previews and allocations](https://github.com/beauschwab/model-portfolio-sim/issues/50) |
| I | [51 Dynamic margin and mixed scale](https://github.com/beauschwab/model-portfolio-sim/issues/51) |

## Outcomes and invariants

A submitted acquisition or unwind must explain which obligations arise, when cash
and securities actually settle, which collateral is available, what financing is
needed before settlement, and what contracts and rights remain afterward. The
long-term simulation must use the same event sources as daily liquidity reporting.

The implementation must preserve these invariants:

1. Existing positions retain original base OAS, effective yield, purchase cost and
   contractual history. Surviving partial positions retain those anchors. A new
   hedge or replacement facility has its own identity and acquisition terms.
2. Risk scenarios share the applicable common random numbers. Scenario revaluation
   does not recalibrate an existing position's base OAS.
3. Physical obligations and Pricing valuations have explicit distinct provenance.
   A valuation or forecast projection is not settlement evidence.
4. Opening cash/equity are supplied and posted once per account. No balancing plug,
   implicit emergency loan or synthetic horizon-end payoff is introduced.
5. Every economic event has one exclusive financial owner. Generic legacy coupon,
   maturity, credit and collateral overlays cannot post that event again.
6. A reporting cutoff does not extinguish outstanding debt, pending settlement,
   collateral, accrued rights or recovery claims. Complete economic tails survive.
7. Interactive evaluation consumes published preparation only. Exact recalculation
   and recovery run in durable native jobs.
8. Existing final-output tolerances and stable financial reduction ordering remain
   unchanged. No fast-math or weakened independent publication checks are allowed.
9. Legal netting, economic hedge allocation, funding attribution, custody ownership
   and financial statement presentation remain separately declared policies.
10. Failed verification, cancellation, expired deadline, stale execution identity or
    lost job ownership publishes no completed manifest or interactive bundle.

## Existing components and required additions

The existing architecture assigns pricing to Risk, financial coordination and
HiGHS construction to Decision, and chronological journal execution to Ledger.
Preserve that dependency direction. Do not put another pricing model in Ledger or
another journal simulator in Decision.

| Component | Reuse | Required addition |
|---|---|---|
| `portfolio-model-core` | Shared identities, dated terms and strict input support | Bounded lifecycle relationship and event schemas without HTTP/storage dependencies |
| `portfolio-risk-native` | `funding_contract`, `funding_graph`, `derivative_contract`, `derivative_accounting`, `saved_derivative_source`, product graphs | Voluntary exits, residual contract state, post-action margin, collateral requirements and verified financial intents |
| `portfolio-decision-native` | `owned_workflow`, `saved_derivative_mixed`, `joint_deposit_derivative`, product libraries and solver | Relationship admission, acquisition/unwind coordination, settlement dependency resolution and candidate validation |
| `portfolio-ledger-native` | Existing daily `Sim`, dated owners, `derivative_intent_owner` and gross GL | Bounded event sequence, conditional settlement, shared inventory owner and funding/exit intent adoption |
| `portfolio-compute-control` | Native deadline/cancellation checkpoints | Carry one execution control through generation, replay, verification, streaming and publication |
| Proposed `portfolio-verification-native` | Source schemas and transport contracts only | Independent equations, source reconstruction, ordered journal replay and persisted report verification |
| Proposed `apps/api-rust` | Existing SQL schema, revisions, job and artifact contracts | Rust HTTP service, repository adapter, worker, artifact codec, migrations and publication |
| `apps/web` | `src/lib/api.ts`, generated transport types and partition paging | Lifecycle plan editing, settlement/collateral views and explicit validation status |

These proposed crate/module names are planning choices. Final interfaces must pass
dependency review before implementation. In particular the verification crate
must not depend on production Risk/Decision financial functions or Ledger replay.

Relevant current source boundaries:

- Standard `owned_workflow.rs` rejects nonempty saved hedge books. Keep that refusal
  until the selected route has complete trade, legal and journal ownership.
- `funding_contract::Exit` currently represents maturity/rollover and counterparty
  default. Voluntary repayment requires an additive, explicitly selected contract.
- The current derivative CSA retains static supplied opening IM and complete
  collateral return/default tails. Dynamic IM and voluntary termination require
  separate extensions.
- `derivative_accounting` supports reviewed undesignated policies. Economic hedge
  linking does not authorize formal hedge accounting.
- The current API/queue/persistence and many independent verifiers are Python.
  Their contracts are inputs to a Rust migration, not execution dependencies for
  this feature.

## Versioned inputs and relationships

Introduce an explicitly selected `liquidity-lifecycle-1` request. The identifier is
proposed and must be registered only when its implementation and refusal tests are
complete. Preserve historical product and ledger schema behavior.

The request contains immutable workspace/saved-book revisions, original source
references and hashes, complete opening accounts, relationships, actions,
scenarios, settlement/margin policies and reporting/full-tail calendars. Preserve
original raw bytes separately from any declared typed normalization. Reject
duplicate keys, unknown variants and unsupported domains before calculation.

| Record | Required fields and meaning |
|---|---|
| Position lot | Position/lot IDs, exact source revision, account/entity/currency/unit, quantity, accounting category, acquisition date and original cost/yield/OAS anchors |
| Hedge allocation | Exposure lot/pool, hedge position, allocation amount, risk basis, effective interval, review revision and selected unwind treatment |
| Funding allocation | Financing contract, attributed lots/pool, amount, effective interval and repayment/retention/replacement instruction; attribution alone does not pledge inventory |
| Pledge | Exact security lot, owner, agreement, holder, quantity, eligibility/haircut policy and independent outside encumbrance |
| Margin agreement | Reviewed legal scope, counterparty, settlement convention, VM/IM method, segregation/reuse rights, calendar and calculation/settlement cutoffs |
| Lifecycle action | Immutable action ID, selected lots/quantities, trade/effective/settlement dates, acquisition/unwind/offset/rebalance/refinance policy and conditional dependencies |
| Settlement obligation | Exclusive source event, kind, signed amount or native calculation source, asset quantity, due date, dependency group and actual/pending/failed/settled state |
| Facility | Provider, currency, commitment, drawn/available amount, access policy, expiry, draw/repayment terms and actual funding costs |

Hedge and funding relationships support many-to-many allocations. They must not
overallocate a position. Each legal trade remains a distinct owner even when rows
have identical economics. A package ID supplies lineage; it creates no legal
setoff, collateral reuse or cross-account cash permission.

Keep new records inside the existing revision/manifest model. Large immutable
relationship sources can be content-addressed objects referenced by a revision.
Do not establish another mutable book database or infer ownership from names.

Return structured refusals for incomplete mapping, unsupported policy, stale
preparation and insufficient inventory. Distinguish malformed input from a valid
but financially infeasible plan. Proposed error identifiers include
`LIFECYCLE_MAPPING_REQUIRED`, `MARGIN_REFRESH_REQUIRED`,
`SETTLEMENT_DEPENDENCY_CYCLE` and `INSUFFICIENT_SETTLEMENT_LIQUIDITY`.

## Native action coordination

Decision prepares a bounded staged action plan using the existing Risk owners.
Admission validates complete baseline/scenario coverage before source generation.
Expensive generation occurs once for exactly equal sources within the accepted
scope. Result intent types carry exclusive owner and immutable source identity.
Callers cannot supply fabricated financial intents in place of raw contracts.

Acquisition preparation must resolve actual purchase costs, settlement conditions,
financing availability, hedge entry terms, portfolio margin and eligible collateral.
Reservation does not mean settlement. New financing advances and acquisition cash
post on their actual supported settlement events, with original fees/accrual rights.

For each full or partial unwind, the instruction explicitly chooses:

1. Which lots and quantities are sold and when risk/title transfers under policy.
2. Which hedges terminate, partially terminate, offset, remain or rebalance.
3. Which financing is repaid, retained or replaced, including notice/consent/fees.
4. Which collateral must be substituted or released before delivery.
5. Which legal margin scopes must be recalculated and when returns become available.
6. Which bridge facility can fund obligations before expected receipts settle.

For voluntary derivative termination, add a versioned agreement with amount source,
valuation cutoff, effective date, settlement evidence/policy, fees and explicit
outstanding coupon/collateral disposition. Model-derived consideration and supplied
agreed execution quotes have different provenance; an execution quote is not
silently assumed equal to clean value. Preserve the resulting execution P&L.

Termination effective time can stop future contractual rights before consideration
settles. The remaining consideration is then an unpaid claim, not a live original
swap and not settled cash. Reject ambiguous coupon/termination ordering. Default
closeout keeps its own versioned waterfall and is not a voluntary exit shortcut.

For voluntary funding repayment, add partial/full principal exit, actual accrued
interest, break costs, consent/notice and pledge-release conditions. Support
remaining principal and contractual interest explicitly; do not shorten maturity
in place to disguise an early repayment. Replacement borrowing is a new contract.

An offset hedge creates a second trade. It does not extinguish the original trade,
its rights or its collateral. Compression/novation requires a separately supported
explicit action and complete ownership transfer; otherwise refuse it.

## Event ordering and settlement execution

Extend the existing daily `Sim` with a bounded event coordinate such as
`(ordinal, phase, sequence)`. Phase order is a selected immutable settlement policy;
sequence preserves deterministic ordering within a phase. Actual timestamps and
business-calendar provenance must be retained where supplied. A modeled sequence
must be disclosed as such and must not claim observed intraday payment accuracy.

Separate commercial execution, obligation creation, resource reservation and final
settlement. Enforce conservation and financing limits at each resource-changing
event. Keep date-end GL output and add minimum event-sequence usable cash.

Represent a plan as an acyclic dependency graph. A repo repayment may precede
collateral release, which precedes bond delivery/payment. If sale proceeds are
needed to make that repayment, require an explicit supported linked settlement or
available bridge facility. A circular dependency cannot be solved by netting the
whole day or inventing cash.

Link delivery/payment atomically only where the declared settlement mechanism
supports that exchange. A commercial package spanning different agreements does
not make repo repayment, hedge consideration and collateral returns atomic.
Different legs can fail or settle on different dates.

Failed obligations retain gross claims, pledged quantities and any supported
interest/cost accrual. Retry/disposition policies must be explicit. An unsettled
receipt never increases usable cash. Do not automatically reverse an already
settled financing or hedge because an unrelated sale leg fails.

Update restrictions and live trade state at the appropriate effective event, then
recalculate affected legal margin scopes. Exact dirty dependencies include trade
population, contract terms, valuation source, pending calls, IM method, inventory,
agreement reviews and settlement status.

## Shared collateral and margin state

Use one inventory owner in the shared simulation. Reuse existing funding ownership,
outside encumbrance, eligibility and haircut rules, adding declared derivative
pledges/custody and source-specific cash collateral ownership where supported.

Track owned, reserved, delivered, pending release and available quantities by exact
asset/owner/agreement. For exclusive inventory, available quantity is owned quantity
less outside encumbrance and all nonoverlapping reservations/deliveries. Validate
nonnegative quantities after every event; a pending release remains unavailable.
Any supported rehypothecation needs its own rights/chain and independent conservation
gate. It cannot be enabled by an economic allocation label.

Pledge, substitution and release of securities move inventory and collateral credit;
they do not create cash. Haircuts affect eligible collateral value, not actual sale
proceeds. Collateral liquidation and ordinary security sale have distinct ownership
and cash sources. Never repeat proceeds already owned by a funding recovery event.

VM calculation uses the affected live legal bucket, supported clean/exposure state,
thresholds, MTA, settled VM and unsettled calls. Handle pending calls when population
changes according to an explicit cancellation/replacement policy. Do not duplicate
a call already in flight. Keep reviewed collateral-to-market semantics distinct
from any future settled-to-market implementation.

At derivative termination, explicitly select whether existing VM is returned as a
separate obligation or applied against consideration. The verifier must prove that
the same collateral is not both credited against consideration and returned again.
Gross posted/received stocks survive until settlement or permitted disposition.

IM preparation has three explicitly versioned modes:

| Mode | Required source and acceptance |
|---|---|
| Supplied requirements | Exact live-portfolio identity, agreement, method/review, scenario/date and posted/received requirement; required for initial end-to-end support |
| Research methodology | Explicit equations, parameters and calibration status, independently implemented in Rust and bounded to its supported domain |
| Supported external or industry methodology | Versioned model/provider inputs, permissions, complete sensitivities/population and independent benchmark evidence |

Supplied post-action IM cannot be interpolated or reused for an altered allocation.
Missing exact requirements fail preparation. Never assume proportional IM release:
removing an offsetting trade can increase the remaining portfolio's requirement.
Segregated received IM remains custody memo, not usable account cash. Received VM
reuse follows its explicit agreement. Represent restrictions once; do not subtract
the same restricted cash again through both GL and a second availability overlay.

Margin calculation, call creation, delivery and return are distinct events. Model
settlement lag, availability/outages, remuneration and complete return tails. Static
opening IM remains valid only under its original separately selected legacy scope.

## Cash accounting and funding costs

The cash bridge uses actual settled signed movements:

```text
closing cash = opening cash
             + security purchase or sale settlements
             + contractual coupons and principal
             + financing advances and repayments
             + derivative contractual and termination settlements
             + cash collateral deliveries and returns
             + actual financing costs, fees and other supported cash events
```

Mark changes and NII accrual are separate GL events and do not enter this bridge as
cash. A security pledge release is not a cash collateral return. Facility capacity
is not cash until a permitted draw settles. Restricted cash/custody are reported
with exact ownership so usable cash is derived consistently with GL.

Risk prepares reviewed trading sale/termination intents that remove original
carrying and recognize only the difference between consideration and removed
carrying under the selected policy. Previously recognized marks cannot be counted
again as realized losses. Economic hedge linkage does not create formal hedge
designation, OCI or effectiveness accounting.

Actual funding contracts produce external interest/fees once. FVA/XVA diagnostics
remain separate and cannot become a second financing expense. Internal FTP can
later allocate those source-linked costs under explicit policies without creating
cash or changing external NII. Capital/FTP permission requires its own independently
verified source bridge; a lifecycle receipt does not grant it automatically.

## Long term scenarios and strategies

Expose three explicit native business-plan modes: contractual runoff, a supplied
schedule of actions, and bounded policy-driven decisions. Each uses the same action
contracts, settlement engine and original financial owners as liquidity stress.

Policy decisions may inspect only the admitted information prefix: settled cash,
current live positions, known pending obligations, declared available facilities
and observable market/model state. They cannot read future realized scenario
outcomes. A failed trade/funding leg changes the state from which subsequent policy
decisions are made. Do not execute the whole future action schedule as if every
intended receipt and facility draw succeeds.

Keep published forecast NII/runoff behavior under its existing identity. A new
broader scenario contract must separately identify Physical business/funding and
settlement assumptions, conditional market states and Pricing curves/volatility
used for marks. Forecast columns do not silently become derivative prices or
risk-neutral probabilities. Training/holdout and information cutoffs stay explicit.

Monthly reports aggregate the underlying dated sources into settled cash, recognized
income, actual funding expense, margin calls/stocks, inventory and residual risk.
Retain every event in the month and the full tail. Monthly averaging cannot hide
an event-level shortfall or an obligation just beyond the reporting cutoff.

Funding rollover across a long horizon is an explicit series of new facilities
and access decisions. The current component admits one replacement generation;
repeated renewal requires a versioned extension and bounded aggregate admission.
No perpetual constant-balance facility is inferred for this route.

## ASC 815 accounting plan alignment

[Issue 35](https://github.com/beauschwab/model-portfolio-sim/issues/35) owns formal
ASC 815 designation, eligibility, effectiveness, accounting risk attribution,
hedged-item basis, OCI and disclosure rules. This lifecycle owns economic actions,
funding, margin, settlement and inventory. Both consume exactly the same immutable
instrument sources and execute through one shared journal with one opening.

The shared contract retains separate economic allocation IDs, formal relationship
IDs, entity adoption/accounting-policy revision, original valuation anchors,
designation/assessment history, forecast transaction identity and event effective
times. An economic hedge label supplies none of those accounting permissions.

| Accounting issue stage | Lifecycle handoff and acceptance |
|---|---|
| 0 and 1: rule/evidence and relationship contracts | Agree exact IDs, source hashes, effective times, allocation limits, adoption and forecast evidence; preserve append-only history and explicit unsupported refusal |
| 2: valuation/effectiveness | Preserve economic PV/OAS/CRN; retain separate accounting inception/risk-attribution anchors and independent qualification; DV01 neutrality cannot qualify a relationship |
| 3: fair-value hedges | Partial/full sale, termination and discontinuation produce reviewed basis disposal/retained-basis amortization instructions; do not create a forced equal/opposite entry or repeat marks already in earnings |
| 4: cash-flow hedges | Deliver actual forecast occurrence, delays, cancellation, replacement funding and discontinuation facts; ASC 815 owner determines retained OCI/reclassification under reviewed rules, not swap termination cash alone |
| 5: asset portfolio layers | Update closed-portfolio actual/surviving quantities and supply breach facts; accounting owner determines supported dedesignation/basis treatment, never inferred liability-layer or proposed HTM eligibility |
| 6: additional issued lanes | Refuse unsupported products/legal/accounting lanes; amendments require the accounting issue's rule/adoption matrix before activation |
| 7 and 8: adoption/publication/scale | One exclusive event owner, independent Rust raw/persisted verification, shared Rust services, exact prepared coefficients and complete mixed-scale gates |

Use a native accounting handoff that binds lifecycle event, relationship revision,
source position/quantity, accounting-effective time, termination/sale/forecast
facts, before/after gross stocks and original basis/OCI origins. The accounting
owner returns typed financial intents and expected roll-forwards; clients cannot
inject substitute entries. Decide exclusive mark/coupon presentation in native
preparation so the undesignated derivative owner and formal owner cannot both
recognize the same change. Cash and collateral remain owned by their original
settlement events. Basis and OCI entries do not create additional cash.

The clocks for hedge termination, item sale, formal discontinuation, forecast
occurrence and settlement may differ. Do not erase a formal relationship when a
position row is removed. Redesignation creates a new relationship identity with
retained old basis/OCI history under the supported rule matrix. Do not assume
termination causes immediate OCI recycling; require the accounting owner's
reviewed forecast-status and timing treatment.

Initial economic acquisition/unwind can proceed under the existing reviewed
undesignated scope. A request selecting formal treatment requires the implemented
and reviewed accounting owner for that exact lane; missing/failed qualification
must not fall back to undesignated treatment. Freeze the shared contract with
accounting stages 0/1 before extending it, without making the entire economic lane
wait for completion of all accounting stages.

Integrated acceptance adds independently reviewed designated FVH/CFH partial/full
unwinds, instrument terminated while forecast remains expected, delayed/cancelled
forecast, replacement debt, retained basis, redesignation, portfolio-layer breach
and balanced duplicate mark/OCI/basis tamper cases. Financial rules remain supplied
by issue 35's refreshed paragraph-level matrix, not inferred from this checklist.
All new accounting integrations, independent verifiers and application services
for the shared feature are Rust; earlier Python target assignments do not govern
this selected route. Original issue 35 accounting breadth and acceptance gates
remain required.

## Interactive preparation and allocation

Preserve sub-ms coefficient-only evaluation for the current supported library.
Create separately versioned lifecycle preparation whose metadata states the exact
baseline, scenario, grid, legal scopes and supported allocation domain.

Linear contractual cash and risk coefficients can be prepared natively. Shared
netting, thresholds, portfolio IM and constrained inventory are not generally
additive. Interactive lifecycle results must distinguish an estimate within a
declared validity domain from an exactly replayed validated plan.

For initial optimization, choose discrete fully reviewed action packages or
independently established conservative margin/resource bounds. Construct permitted
linear rows in Rust and use HiGHS. Preserve the current worst-case external-NII
objective unless a new objective is explicitly selected under a separate contract.
Report trading P&L, unwind consideration and liquidity costs distinctly.

After solving, perform exact native reconstruction of affected live trade buckets,
margin, financing, inventory and ordered shared-journal settlement. Only then can
the candidate receive a validated flag. If it fails, return the violating date,
event, agreement and resource. Bounded refinement/re-solving can be added where
the refinement's mathematics is justified; do not invent linear cuts for a
nonconvex margin model. Exhaustion returns no validated plan.

Constraint-only changes reuse compatible preparation. Changed trades, allocation
domains, CSA/IM policies, funding contracts, collateral or scenarios require a
durable rebuild. Retained capital dependencies require explicit refreshed
coefficients under the existing `CAPITAL_REFRESH_REQUIRED` contract.

## Rust application services and durable publication

Build a Rust HTTP/repository/worker path for this selected feature. The browser
uses the existing typed client with same-origin routing to the Rust service. The
route must not proxy through FastAPI or launch a Python adapter. Read saved revision
manifests and bounded artifacts directly through Rust.

Implement repository compatibility with the existing tenant/workspace keys,
revision manifests, jobs, sessions, libraries and immutable object references.
SQLite/PostgreSQL portability is a required gate. Any necessary schema extension
is an explicit versioned Rust migration; it must not silently rewrite old results.
Raw authoritative source bytes live in checksummed artifacts because a database
JSON value alone cannot preserve duplicate-key refusal or all source distinctions.

Reuse the existing workspace lease as the single compute ownership authority.
Before enabling Rust job consumption, install an explicit operation-routing and
worker-ownership transition. The existing worker claims the oldest queued job
without a native-operation filter; two services polling the same queue cannot be
assumed safe merely because each has an allowlist. Define workspace cutover or a
reviewed shared routing migration, drain/fence old attempts, and test claims across
restart. One workspace must never have competing financial execution owners.

Rust service deliverables:

1. Configuration/authenticated gateway integration and fixed server-side scope.
2. Immutable revision reads/commits with expected-revision checks and idempotency.
3. Bounded job admission, operation routing, attempt/token claims and lease renewal.
4. Native execution controls, original accepted deadline and process watchdog.
5. Independent verification and typed bounded Arrow/Parquet output.
6. Checksummed local/S3 artifact writes, paged reads and reopen verification.
7. Final publication under the same scoped SQL transaction/ownership checks.
8. Rust migration/admin commands and native-only startup/build tooling.

Use accepted-deadline authority across retries and persistence; restart cannot
reset a job's budget. Reopening a source is provenance, while a new job has its own
accepted finite deadline. Retain actual producer completion and ordered partitions.
Do not synthesize completion from an empty stream.

Write artifacts before committing references. Independently reread/replay persisted
content and recheck raw input, source, executable, verification and artifact hashes
through the final publication fence. Cancellation, revision policy, status,
attempt/token and lease checks must remain effective during final I/O. Failed
publication removes only fresh owned unpublished output.

Proposed public routes are `/liquidity/lifecycle/validate`,
`/liquidity/lifecycle/runs` and `/liquidity/lifecycle/preparations`, with existing
job/table/manifest semantics implemented by the Rust service. Contract validation
checks ownership/admission; it is not financial feasibility validation. Generate
HTTP schemas/types from Rust definitions through Rust/TypeScript tooling. Preserve
existing partition/Arrow compatibility or introduce an explicit selected version.

## Independent verification in Rust

Create a verification implementation with no dependency on production financial
calculations. It can use transport/identity schemas and nonfinancial controls, but
must not call Risk prices, production margin equations, Decision preparation or
Ledger reduction to construct expected values. Enforce this dependency boundary
with Cargo dependency inspection and source checks.

Reconstruct contractual cash, voluntary exits, expected margin, collateral/quantity
conservation, ordered postings and gross account balances independently. Verify
exact source identities, dates, owners and declared typed normalization separately
from financial tolerances. Hashes establish identity, not economic correctness.

For a supported complex margin methodology, independent verification needs either
independent equations or authoritative benchmark receipts bound to the exact input
domain. Calling the same library twice is insufficient. Explicit supplied IM mode
validates scope, requirements, calls and custody; it does not claim independent
proof of the externally supplied methodology.

Rust tests combine rational/decimal hand fixtures where appropriate, independent
f64 equations, signed/zero cases, property-based conservation and persisted tamper
cases. Historical output controls are frozen data fixtures, not Python generators
executed during build/tests. If equivalent independent checks are missing, publish
no verified receipt for that domain.

## Worked unwind fixture

Use a reviewed supplied opening in currency units: cash 0, bond carrying 100,
repo liability 95, swap liability 4, posted VM asset 2 and posted IM asset 3.
Opening assets 105 less liabilities 99 reconcile to equity 6. Collateral inventory
separately identifies the bond pledged to the repo.

The selected agreement returns VM separately rather than applying it to termination
consideration. Ignore fees/interest only in this explicitly zero-cost hand fixture.

| Event | Cash movement | Cash after event | Required state |
|---|---:|---:|---|
| Opening | 0 | 0 | Bond, debt, swap and margin stocks reconcile |
| Day 2 bridge draw | +95 | 95 | Explicit permitted facility becomes debt |
| Day 2 repo repayment | -95 | 0 | Old repo debt extinguished; release still conditional |
| Day 2 bond release | 0 | 0 | Pledged inventory becomes available |
| Day 2 linked bond delivery/payment | +100 | 100 | Bond removed once; sale equals carrying |
| Day 2 bridge repayment | -95 | 5 | Bridge principal closes |
| Day 2 swap termination payment | -4 | 1 | Swap liability closes; VM/IM return claims remain |
| Day 3 VM return | +2 | 3 | Posted VM asset closes |
| Day 5 IM return | +3 | 6 | Posted IM asset closes |

The final cash and equity are both 6 with no period gain/loss. The gross sale
receipt is 100, but the financing bridge need before sale is 95. If the bridge is
unavailable, the dependency chain is infeasible despite a positive final net cash.
Do not simulate bond delivery from pledged inventory or count expected sale cash
before the supported settlement event.

A second fixture applies the 2 VM against the 4 termination consideration: cash
payment is 2 and the VM asset closes there, with no later VM return. It must reach
the same final cash 6. Fees, repo accrued interest and adverse execution prices
then have separate fixtures with independent cash/P&L bridges.

## Milestones and acceptance

| Stage | Deliverable | Required gate |
|---|---|---|
| A | Typed relationships, event/ownership schemas and independent hand fixtures | Exact mappings, duplicate/unknown refusal, bounded admission and historical compatibility |
| B | Native acquisition with explicit funding, swap, cash VM and supplied exact IM | One opening, complete financial source replay, inventory and shared GL conservation |
| C | Full/partial unwind, voluntary funding/derivative exits and settlement dependencies | Worked fixture, bridge refusal, retained rights and adverse settlement scenarios |
| D | Rust independent verifier and persisted artifact codec | Independent equations, ordered journal replay and balanced tamper refusal |
| E | Rust API/repository/worker, migrations and operation cutover | SQLite/PostgreSQL restart, original deadlines, lease/token/idempotency and publication fences |
| F | Saved-book inputs and React acquisition/unwind views | Actual Rust-only browser/worker/publication/reopen flow |
| G | Long-term schedules/policies and monthly aggregation | No lookahead, explicit repeated funding access and exact daily/monthly/full-tail reconciliation |
| H | Prepared lifecycle preview and allocation | No pricing in interactive evaluation; exact candidate replay before validation |
| I | Dynamic IM, broader products/legal scope and combined scale | Method-specific benchmarks, observed evidence where claimed and full issue acceptance |

Stages A through D establish a native end-to-end process on immutable fixtures.
Stages E and F establish the public feature with Python absent. Neither milestone
alone grants broad saved-book/product/legal adoption. Retain complete #12/#20/#21/
#23/#24/#25 acceptance; this first performing scope does not close them.

Implementation work packages and file ownership:

| Package | Proposed changes | Dependency |
|---|---|---|
| Relationships | Core lifecycle types/strict domains; Decision preflight | None beyond current shared contracts |
| Financial exits | Risk funding timeline, derivative lifecycle and accounting | Relationships and independent exit fixtures |
| Margin/inventory | Risk margin preparation and Ledger exclusive inventory/intents | Exact population, legal scope and settlement interfaces |
| Event runtime | Ledger bounded phases/dependencies and gross pending rights | Admitted native intents and source clocks |
| Coordinator | Decision lifecycle preparation and selected process dispatch | Exits, margin/inventory and event runtime |
| Verifier | New Rust crate for raw equations and persisted replay | Stable schemas; no production financial dependency |
| Service | Rust SQL/artifacts/API/worker/admin and ownership transition | Verifier, bounded dispatcher and storage compatibility |
| Interface | Typed client, plan editor and partitioned result views | Rust HTTP contract and public workflow |
| Forecast/solver | Native prefix policies, aggregation and prepared allocations | Exact replay and public source identity |

Each package needs a reviewable contract before implementation. Implementation in
an isolated checkout must capture required dirty-checkout prerequisites explicitly;
do not reset/revert unrelated concurrent changes or rely silently on a clean HEAD
that lacks the selected existing owners.

Mandatory financial and operational cases:

- Full unwind, partial lots, shared hedge allocation and retained financing.
- Offset without termination and equal/opposite trades in different legal scopes.
- Removal of a margin offset increasing the remaining requirement.
- Delayed VM/IM return, pending-call cancellation/replacement and settlement outage.
- Repo repay-before-release, release-before-sale, bridge failure and dependency cycle.
- Sale failure after debt or hedge settlement, partial settlement and residual rights.
- Outside encumbrance, collateral substitution, haircut shock and inventory shortage.
- Termination on a coupon date, signed rates/fees, gross receivables/payables and
  consideration differing from carrying.
- Report date before final debt repayment or collateral return, and no synthetic
  horizon payoff.
- Stale allocation/CSA/IM/funding preparation and refreshed capital dependencies.
- Native deadline/cancellation during generation, replay, verification, upload and
  final SQL publication; expired retries; executable/source replacement.
- Balanced but economically altered journal, forged source lineage, reordered
  transactions, changed artifact after replay and forged completion.
- Restart, lease loss, duplicate submit, old attempt returning late and cross-scope
  access refusal on both SQLite and PostgreSQL.
- Bounded raw input, array cardinality, dependency edges, pending obligations,
  source generation, graph/cache retention, serialized output and partition counts.

Performance acceptance measures bounded memory/work, generation counts, end-to-end
latency, event throughput, persisted verification and coefficient-only evaluation.
Component limits are not aggregate RSS guarantees. Define and measure a whole-process
budget separately, including HTTP/JSON staging, Risk caches, Ledger state, verifier
state, native solver and artifact buffers. Complete 60,000-position acceptance must
contain actual mixed positions, margin/credit/solver/journal activity under its
supported contract; an inactive-netting fixture is insufficient.

## Build and rollout gates

Use Cargo for new backend build, format, tests, lint and contract/admin tools.
TypeScript tools build the UI and exercise browser flows. Do not add Python code,
tests, fixture generators or build/runtime dependencies for the selected feature.
Python-based historical tooling is not an acceptance dependency for its isolated
native build; provide Rust/TypeScript equivalents for any checks it needs.

Update architecture, generated contract and production-policy documentation in the
implementation stage. The current `make check-architecture` contract remains a
repository boundary gate; introduce an equivalent native checker for the new route
so Python absence is verifiable, retaining coverage rather than dropping checks.

Prove the selected feature in an environment without a Python interpreter or
packages: admit a saved revision, acquire, unwind partially, verify persisted
output, restart, reopen and evaluate compatible preparation. Test process trees
and deployment dependencies as well as Cargo source boundaries.

Rebuild matching native artifacts, fence/drain prior execution ownership, restart
selected services and explicitly rebuild incompatible libraries/sessions. Source
validation does not deploy or authorize a live database migration. Rollback must
retain historical results and refuse unsupported new operations; it cannot route
new economics to a Python fallback. Explicit capabilities distinguish component
analysis, native verified process, durable publication, saved adoption, empirical
validation and complete-scale acceptance.

## Existing contracts and modeling references

- [Architecture](../architecture.md), [production execution](../rust-production-contract.md)
  and [interface contracts](../contracts/README.md).
- [Saved derivative integration](saved-derivative-integration-plan.md),
  [shared derivative journal](saved-derivative-mixed.md) and
  [derivative accounting](derivative-accounting.md).
- [Funding contracts](funding-contracts.md), [funding graph](funding-product-graph.md)
  and [dated funding coefficients](aged-funding-design.md).
- [Published forecast scope](../forecast-scenarios.md),
  [durable storage](../production-storage.md) and
  [full implementation acceptance](implementation-plan.md).
- [CPMI IOSCO settlement principles](https://www.bis.org/committees/cpmi/pfmi/overview)
  support conditional exchange-of-value settlement. They do not make independent
  commercial agreements one atomic settlement.
- [ISDA collateral operational practices](https://www.isda.org/collateral-management-sop/)
  support agreement-specific trade population, margin and segregation controls.
  They do not supply this feature's IM methodology or certify its implementation.
