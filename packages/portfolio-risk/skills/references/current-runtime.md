# Current runtime and workflow contract

Use this reference for incremental pricing, temporary assumptions, decision
sessions and durable analytical workflows. The repository root `AGENTS.md` and
`docs/rust-production-contract.md` govern production ownership and deployment.
Version logs are historical evidence only, in `docs/changelog/agents-history/`.

## Runtime, identity and numerical rules

Rust is the required built-in runtime, including calibration, shared draws,
graphs/coordinator, product cashflows/risk, accounting, units and allocation.
HiGHS is C++; native Rust owns its financial problem and allocation checks.
Python transports tables and independently verifies results. Missing native
capability requires a compatible rebuild, never Python or simplified fallback.
Historical results remain readable; switching saved execution economics and
rebuilding libraries/sessions are explicit actions. Restart workers after builds.

Keep fixed base OAS, shared CRN, original purchase cost/effective yield, financial
tolerances and deterministic posting/reduction order. A scenario changes current
marks without re-solving original OAS. Python reference tests must explicitly
select their independent backend; production ownership tests run outside that
context with Python financial callbacks forbidden.

Rates, spreads, coupons and vols are decimals; instrument spread overrides are
basis points. Input/output frame prices are percent of par, internal prices are
per-unit decimals, dollar risk scales by notional. Undefined ratios are null.
`RunConfig(n_paths=128, n_paths_base=512, horizon=27)` is scoped by `run_context`;
the MBS calibration uses `n_paths_base`, other products use `n_paths`. Validate
engine/API horizons against the active schemas before constructing requests.
Reference Numba paths freeze constants/LUTs and need a fresh process after edits;
native artifacts and selected saved libraries also need compatible rebuilding.

## Incremental pricing and what-if

- `analytics.incremental.price_books` takes raw books, market/history, `asof`,
  `RunConfig`, seed and cache; production passes no custom discount override.
- Native content-addressed nodes bind complete input/parent/model/backend
  identity. Cashflows do not depend on labels/notional/price targets/book yield;
  calibration adds targets and marks add scenarios/fixed OAS/spread overrides.
  Validate price/notional on hits; never mutate cached buffers or bypass parents.
- Eviction is safe recomputation. Retained identities count toward cache limits;
  in-flight paths, JSON copies and outputs are additional memory. Keep the active
  request's base/current chunks alive through risk/income even if nodes evict.
- `/run` kind `pricing` accepts selected books, an optional scenario and additive
  `spread_overrides_bp` for existing selected IDs (bounded to +/-2000 bp).
  Decimal global `spread_shift` excludes deposits; explicit BP overrides include
  them. Scope net value is selected assets minus liabilities, not complete EVE.
- Temporary `assumption_overrides` use `{book: {stable_id: {field: value}}}`.
  Read `analytics.whatif.FIELDS` / `GET /pricing/assumptions` for current domains.
  Reject unknown IDs/fields, nonfinite/out-of-domain values and fractional WAM.
  Targets, notionals, IDs, dates and frozen global constants are not temporary
  assumptions; null virtual attrition fields preserve segment defaults.
- Native baseline/revised comparisons retain base OAS and report calibrated
  identity, per-position changes, risk, monthly income/runoff and scope flags.
  Never describe partial scope as whole-book EVE or silently replace saved books.

## Unit libraries and decision sessions

Build units through the live native products once; retained coefficients and
base KPIs drive synchronous evaluation. Allocation rows are
`{template, purchase_m, notional}`: known template/grid, nonnegative integral
purchase month and finite nonnegative notional. Purchases outside the horizon
contribute zero. Month-zero KPI scalars are not purchase-date KPIs.
Additional cash budget is committed funding outside the base book, not HQLA
already counted. Worst-case NII includes base-book earnings when that lane has
a baseline; dated incremental libraries do not invent one.

- `/decision/sessions` creates a durable native session from `expected_revision`
  and optimizer options. Native handles are process-local; persisted API session
  IDs survive compatible worker recovery under their original IDs. Neither is
  tenant authentication; changed model/code identity requires explicit rebuilding.
- Updates require revision/version, `book:stable_id` edits and template changes;
  null removes an override. Constraints replace the full options, not a patch;
  scenario changes require rebuilding. Advance state only after independent
  allocation replay and publication/revision checks; infeasibility has no invented
  allocation outcomes. The native coordinator stages state transactionally.
- Constraint-only edits call no pricers. Dirty edits rebuild only affected
  sources/templates; exactly equal native sources may be retained. Shared grid,
  count or identity changes need full preparation; retained capital limits require
  refreshed source dependencies (`CAPITAL_REFRESH_REQUIRED`).
- `/strategy/eval` and session `/eval` use stored coefficients only, with no
  optimizer or engine calls. Reject absent/mismatched prepared state. Manual
  allocation evaluation is exploratory and does not certify constraint feasibility.
- Actual-purchase dated/product/commercial libraries retain original quote/OAS/EIR,
  full Physical age prefixes and cash/rights tails. Funding is scenario-specific;
  face-notional and purchase-cost caps are distinct. Read the selected native
  schema's exclusions before using credit, fees, draws or revised expectations.

## Forecasts, cohorts and durable publications

Joint market/credit analyses verify the original covariance and supplied/seeded
draws before hazard replay and again after persisted-table reread. A source
mismatch fails publication even when the returned tape and events agree.
See `docs/models/joint-market-credit.md` for spectral-certificate boundaries.
Market-driver calibration receipts require independently verified training-row
selection, count, domains and ranking; see `docs/models/market-driver-calibration.md`.

Forecast preview/run retains snapshot/source/date/unit/interpolation identity,
coverage/warnings and relative alignment. Targets use decimal rates and HPI
rebased to one; raw observations keep source units. Conditioned paths are
income/runoff only, never arbitrage-free OAS/EVE/derivative inputs. Fit/select on
training observations only; holdouts are diagnostics, ties retain observation
order and driver columns sort by name. Read `docs/forecast-scenarios.md`.

Cohort intake is bounded to 100,000 records/32 MiB. Audit each selected mortgage
or deposit through Rust in <=256-record batches with shared CRN/fixed OAS, explicit
absolute-dollar/relative tolerances, base/+/-200bp PV/DV01/NII and monthly cashflows.
Suggested splits require new builds/audits. Refresh reconciles additions/removals/
term/balance changes without inferring payments or journals; selective tape
publication preserves other sources, namespaces, cohort IDs and historical yields.

All durable analyses/ledgers/libraries retain immutable raw/source/executable
identity. Independently reconstruct raw sources and ordered journal postings,
then reread persisted artifacts before publishing. Preserve original deadlines,
cancellation, revision/attempt/token/lease and source/object authority through final
SQL publication, including after progress/rename. Failures publish no manifest;
remove only newly owned unpublished output. Managed input references survive GC.
No producer flag, caller checkpoint or net-zero closing balance substitutes for
independent gross-rights verification. Complete unweighted tails remain required
even at zero allocation; apply allocation and physical weights exactly once.

## Reading the selected product lane

Research discount-curve projection takes ascending ACT/365F times from zero
through at least 30y, finite positive discount factors and D(0)=1. It derives
annual-payment par rates on the ten model pillars, validates the shifted forward
domain and reports reconstruction error. It does not supply an exact sub-year
curve, replace volatility/behavioral fits, or change book date/quotes/histories.

Security accounting keeps cost basis separate from clean opening market price
(per-unit principal, default one). Opening AFS OCI is FV minus cost and already
belongs in supplied equity, not new income. Amortization, partial sales, forward
purchase and redemption must reconcile basis/FV/OCI without duplicate losses;
saved capture supplies the quote and metadata cannot override it. Monthly capture
and duration-based stress marks retain their disclosed approximation scope.

Read `docs/models/native-contracts.md` for explicit analysis/ledger protocols and
`docs/models/implementation-plan.md` for outstanding empirical/adoption gates.
Use the root contract's product-specific links before changing dated recognition,
consumer/card/mortgage credit, indexed/floating sources, deposit revisions,
commercial exercise/OAS/holding/fees, or formal hedge accounting.
Ordinary fixed and fee `commercial-cut-mixed-journal-1` publications retain
complete original/current source proofs, gross rights and tails under unchanged
joined/component bounds. The source-bound `/models/cut-mixed-journals` route
independently verifies up to eight predecessors and permits later cuts before the
complete daily tail, including between sparse path reports. Fee predecessors use
version 3 evidence; no-fee versions 1 and 2 remain readable. Original OAS/EIR,
zero-allocation evidence and final object/executable/deadline/SQL checks remain.
Read `docs/models/commercial-fixed-fee-cut.md` and
`docs/models/commercial-cut-prior-opening.md` for supported inventory and limits;
advanced owners and broader adoption/empirical/scale gates remain separate.

Read `docs/capital-and-ftp.md` for supplied eligibility/RWA/requirements, exact
verified closing dates, expected-flow FTP and independent capital dependencies.
Economic loss is not an implicit allowance or a second capital deduction;
research liquidity weights and supplied policies do not prove compliance.
Component bounds are not total RSS or mixed 60,000-position acceptance. Check the
exact selected lane's current admission and immutable receipts; historical scale
evidence, synthetic fixtures and passing local tests do not prove deployment,
empirical prediction, reviewed bank policies or accounting/regulatory approval.

## Consumer lifetime-loss analysis

`lifetime-credit-loss-1` retains complete paired Physical credit sources and supplied macro/model choices. Rust calculates economic loss, full recovery tails, product ranges and completed-outcome comparisons. Independent raw/persisted replay, original deadlines and final SQL/object/executable/source checks gate durable publication. The offline competing-risk challenger scores holdout Brier/calibration on the same declared panel; it is not a production backend. See repository `docs/models/lifetime-loss-sensitivity.md` for admission, assumptions and the reproducible synthetic pack. Allowance approval, observed accuracy, broader product adoption and deployment remain separate.

## Consumer product panel analysis

`consumer-behaviour-spec-1` is standalone native analytical fitting and shared-credit-path projection. It requires explicit product DPD/exit policies, exact versus snapshot observations, frozen driver transforms/availability lags, term conventions and training support. Monthly snapshots retain the interval-censored likelihood. See repository `docs/models/consumer-behaviour-spec.md`. Daily installment/card action ownership, independently verified durable publication, observed predictive validation and adoption remain separate; this entrypoint is not exposed as a saved ModelLab model.
`consumer-behaviour-product-paths-1` explicitly projects fitted Physical states into daily installment/card cashflows. Shared CTMC monetary stocks are zero; the product owns all cash and rights. Retain original fractional clocks, selected ceiling-to-day mapping, rejection of same-day collisions, complete recovery tails and exact-zero card closure. See repository `docs/models/consumer-behaviour-product-paths.md`. Saved books, independently verified publication, accounting, capital/FTP, observed validation and deployment remain separate.

The Rust core consumer macro components bind supplied Physical scenario paths and actual shared credit draws to bounded daily installment/card owners. Training-only innovation moments and holdout diagnostics are separate. Controlled native process entrypoints are available through `analyze_consumer_macro` and `project_consumer_macro_products`; independently verified ModelLab publication and joined Pricing/frozen-OAS and frozen-EIR income integration remain open under #92. See repository `docs/models/consumer-macro-scenarios.md`.
