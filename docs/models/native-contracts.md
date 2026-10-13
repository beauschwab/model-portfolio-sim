# Native model contract entrypoints

Local0.29.15 adds [verified modeled prior openings](commercial-prior-opening.md) through a source-ID durable API route, with unchanged native financial schemas and final SQL/source/object fences. Cut-started mixed and observed bank adoption remain separate.

Local0.29.14 adds explicit [complete-origin commercial holding journals](commercial-holding-journal.md), retaining full native daily replay and independent prospective window verification. Verified prior opening and cut-started mixed adoption remain separate.

Local0.29.13 adds explicit [complete-origin commercial holding windows](commercial-continuation.md)
with raw prefix verification and original native execution controls. Matching native
rebuilds are required. Prior-publication opening and mixed/seasoned adoption remain separate.

Local0.29.12 adds explicitly selected [original generated commercial OAS](commercial-generated-oas.md)
through the existing commercial graph and acquired-unit schemas. An original
positive quote and bounded `generated_policy.oas_calibration` controls select
retained Pricing samples, per-trial continuation refitting and actual native
stopped-cash residual acceptance. Current scenarios retain the selected spread;
Physical acquisition remains separate. Calibrated adapters select original-clock
native execution controls. Historical omitted/locked controls retain their route.
Rebuild compatible artifacts and selected libraries before executing additive fields.

[Raw instruction validation](raw-instruction-validation.md) documents strict
additive request-field shapes and independent publication preflight checks.

Version 0.29.6 additionally supplies [dated daily replay](dated-ledger.md) through
the ledger executable, `portfolio_balance_validate` and
`analytics.dated_ledger.run_dated_ledger`, with stream protocol 3. The standalone
product analysis schemas below retain their existing envelope and ABI.

Version 0.29.5 adds explicit analysis contracts to the existing bounded native
product runtime. `portfolio-model-core` contains shared financial logic;
`portfolio-risk-native` exposes it through the same standalone/FFI protocol.
`portfolio_risk.analytics.model_contracts` is transport only. Missing or invalid
native artifacts fail; there is no Python calculation fallback.

## Contracts and their scope

| Envelope schema | Python adapter | Detailed contract | Current scope |
|---|---|---|---|
| `commercial-product-graph-1` | `analyze_commercial_graph(request)` | [Commercial graph and strategy](commercial-graph-strategy.md) | Complete separate Physical/Pricing commercial and issuer-credit sources, fixed original OAS, paired risk and explicit finite or compact generated stopping; complete saved/mixed adoption and general convergence remain separate |
| `commercial-units-1` | `prepare_commercial_units(request)` | [Commercial graph and strategy](commercial-graph-strategy.md) | Native acquired commercial coefficients, original stopped Physical acquisition proof, effective income, gross credit exposure and complete funding tails; seasoned prefixes remain separate |
| `strategy-commercial-1` | Native `optimize_commercial` / `create_commercial` | [Commercial graph and strategy](commercial-graph-strategy.md) | Native coefficient preparation, HiGHS, source/allocation replay and retained coefficient-only actor updates; explicit capital dependencies |
| `commercial-holding-continuation-1` | `continue_commercial_holding(request)` | [Complete-origin holdings](commercial-continuation.md) | Sealed original acquisition replay, unchanged completed prefixes, retained credit clocks/yields and prospective group indices; verified prior-publication and mixed/secondary adoption remain separate |
| `commercial-holding-journal-1` | `journal_commercial_holding(request)` | [Holding journals](commercial-holding-journal.md) | One existing complete-origin native daily Sim, derived balanced cut opening, prior P&L rolled into equity and ordered prospective postings; independent full/persisted replay; cut-started mixed/seasoned/secondary and prior-publication adoption remain separate |
| `commercial-credit-ledger-1` | `analytics.model_ledger.run_model_ledger` | [Commercial ledger](commercial-credit-ledger.md) | Explicit commercial owners and cash accounts share one daily Sim; protocol 6, complete recovery clock and independently replayed durable publication gates |
| `commercial-generated-exercise-1` | `core.model_native.run(schema, request)` | [Generated commercial exercise](commercial-generated-exercise.md) | Supplied joint Gaussian/credit laws, training-only quadratic continuation, fresh evaluation, exact native stopped cash replay and antithetic cluster diagnostics; no empirical or general convergence claim |
| `commercial-generated-policy-1` | Explicit `generated_policy` within commercial graph | [Generated graph](commercial-graph-strategy.md#generated-policy-graphs) | Separately supplied Pricing/Physical laws, paired streams, locked original OAS and compact original acquisition/accounting recipes; generated candidate streaming rejects |
| `commercial-candidate-journal-1` | `core.commercial_candidate_native.run` | [Purchased journals](commercial-treasury-candidate.md) | Standalone finite native source acquisition in one daily Sim; commercial loans or AC issuer securities, complete tails; supplied opening provenance is not verified evidence |
| `commercial-treasury-1` | `core.commercial_treasury_native.run` | [Commercial treasury](commercial-treasury-candidate.md) | Exact complete-tail closing capital and bounded daily FTP from independently replayed reviewed journals; regulatory policies remain supplied |
| `product-candidate-journal-1` | `analyze_product_candidates(request)` | [Candidate acquisition](product-candidate-journal.md) | Standalone performing candidate acquisition, original-EIR recognition, ordered physical journals and complete tails; independently checked durable analysis, mixed backbook adoption remains open |
| `product-mixed-dated-1` | `analyze_product_mixed_dated(request)` | [Performing mixed replay](product-mixed-dated.md) | Supplied dated backbook and performing candidate acquisition in one chronological expected journal; independent raw/persisted replay and explicit full-tail closing capital, broader credit/joint/saved-book adoption remains open |
| `product-mixed-consumer-credit-1` | `analyze_mixed_consumer_credit(request)` | [Mixed installment credit](product-mixed-consumer-credit.md) | Exclusive acquired physical installment credit owners in the same dated daily GL; independently verified durable publication and complete-tail closing capital, regulatory/joint/empirical gates remain separate |
| `product-mixed-product-credit-1` | `analyze_mixed_product_credit(request)` | [Tagged mixed credit](product-mixed-product-credit.md) | Installment/card owners share the existing daily Sim and one GL; official source/ordered-journal, durable publication, full-tail closing-capital and browser reopening gates pass, wider product adoption remains separate |
| `credit-covariate-1` | `fit_credit_covariates(request)` | [Covariates](credit-covariate.md) | Exact physical spells, reviewed feature transforms, bounded fit and isolated holdouts |
| `credit-covariate-projection-1` | `project_credit_covariates(request)` | [Covariates](credit-covariate.md) | Frozen physical model and supplied driver intervals; synthetic adoption rejected |
| `advanced-product-graph-1` | `analyze_advanced_graph(request)` | [Consumer graph](advanced-product-graph.md) | Native supplied paired P/Q consumer sources and fixed original OAS |
| `advanced-product-reprice-1` | `reprice_advanced_graph(base,current)` | [Consumer graph](advanced-product-graph.md) | Changed scenario inputs against complete original calibration identity |
| `card-product-graph-1` | `analyze_card_graph(request)` | [Card graph](card-product-graph.md) | Explicit P/Q statement histories, full rights/recovery tails and original OAS |
| `card-product-reprice-1` | `reprice_card_graph(base,current)` | [Card graph](card-product-graph.md) | Repricing against the retained original base dependencies |
| `mortgage-product-graph-1` | `analyze_mortgage_graph(request)` | [Mortgage graph](mortgage-product-graph.md) | Explicit borrower/investor histories with complete remittance/recovery tails |
| `mortgage-product-reprice-1` | `reprice_mortgage_graph(base,current)` | [Mortgage graph](mortgage-product-graph.md) | Original fixed base OAS and complete original dependency identity |
| `deposit-product-graph-1` | `analyze_deposit_graph(request)` | [Deposit graph](deposit-product-graph.md) | Supplied P/Q retention, asymmetric repricing and full liability cash tails |
| `deposit-product-reprice-1` | `reprice_deposit_graph(base,current)` | [Deposit graph](deposit-product-graph.md) | Original liability calibration retained through scenario edits |
| `funding-product-graph-1` | `analyze_funding_graph(request)` | [Funding graph](funding-product-graph.md) | Whole-book cash valuation with exclusive collateral and replacement tails |
| `funding-product-reprice-1` | `reprice_funding_graph(base,current)` | [Funding graph](funding-product-graph.md) | Original whole-book funding spread and immutable base dependencies |
| `graph-cohort-audit-1` | `audit_graph_cohorts(request)` | [Product graph audit](graph-cohort-audit.md) | Supplied representative/member contracts, original spreads, exact context and per-metric tolerances; diagnostic only |
| `term-product-graph-1` | `analyze_term_product_graph(request)` | [Term/CD graph](term-product-graph.md) | Original amortization/fixing/election sources with complete signed tails |
| `term-product-reprice-1` | `reprice_term_product_graph(base,current)` | [Term/CD graph](term-product-graph.md) | Exact original base anchor and separately supplied current histories |
| `product-units-1` | `prepare_product_units(request)` | [Product units](product-units.md) | Supplied actual purchase and age histories, reviewed EIR, original OAS/quote and scenario-specific funding |
| `strategy-product-1` | `strategy.product.optimize` / `ProductSession` | [Product allocation](product-strategy.md) | Raw native preparation, reviewed category/capital limits, HiGHS and retained coefficient-only evaluation |
| `advanced-scheduled-capture-1` | `capture_advanced_graph(graph,owners,policy)` | [Consumer graph](advanced-product-graph.md) | Complete explicit par-note scheduled physical sources; unsupported credit/fees reject |
| `derivative-accounting-1` | `account_derivative_contract(request)` | [Derivative accounting](derivative-accounting.md) | Raw physical CSA path, reviewed recognition, complete rights and report-only XVA |
| `aged-units-1` | `prepare_aged_units(request)` | [Aged coefficients](aged-units.md) | Actual-purchase fixed corporate/CD schedules, original-yield replay and fixed purchase OAS risk |
| `credit-path-1` | `evaluate_credit_paths(request)` | [Stochastic credit](credit-path.md) | Exact event-time supplied or counter-based draws and realized exposure/recovery paths |
| `joint-state-1` | `evaluate_joint_state(request)` | [Joint market/behavior/credit state](joint-state.md) | Supplied market marks and competing behavioral/credit/prepayment hazards with one shared random stream; no fitting or measure conversion |
| `joint-market-credit-1` | `evaluate_joint_market_credit(request)` | [Joint stochastic market/credit generation](joint-market-credit.md) | One versioned PSD market tape drives explicitly supplied hazard elasticities and the shared competing-state engine; synthetic adoption remains ineligible |
| `credit-path-accounting-1` | `account_credit_paths(request)` | [Path accounting](credit-path-accounting.md) | Physical event-time credit, exact accrual and reviewed daily accounting with complete principal/interest recovery tails |
| `credit-snapshot-calibration-1` | `fit_credit_snapshots(request)` | [Panel calibration](credit-snapshot-calibration.md) | Interval-censored likelihood, declared covariate/term effects and isolated holdouts |
| `derivative-contract-1` | `evaluate_derivative_contract(request)` | [Derivatives](derivative-contracts.md) | Raw dated swaps, European exercise, explicit mapped CSA margin and counterparty closeout |
| `model-market-1` | `analyze_model_market(request)` | [Market models](market-models.md) | Supplied collateral/index curves, explicit strike smile and named PSD joint Gaussian driver paths |
| `market-driver-calibration-1` | `fit_market_drivers(request)` | [Observed market-driver calibration](market-driver-calibration.md) | Training moments and chronological holdout diagnostics with source/split identity; synthetic fits remain ineligible for adoption |
| `market-quote-bootstrap-1` | `bootstrap_market_quotes(request)` | [Quote bootstrap](market-quote-bootstrap.md) | Explicit source-linked par schedules, closed-form discount pillars, and bootstrap/validation residuals |
| `dated-income-1` | `evaluate_dated_income(request)` | [Dated income](dated-income.md) | Explicit actual-day original/current yields, cash/earned rights, integral fees and reviewed prospective/catchup changes |
| `deposit-state-1` | `simulate_deposit_state(request)` | [Deposit state](deposit-state.md) | Supplied physical daily repricing, competing withdrawal/closure/migration and terminal policy |
| `observed-deposit-calibration-1` | `fit_observed_deposit_behavior(request)` | [Deposit calibration](deposit-calibration.md) | Exact account-full-exit/closure/migration spells, typed assumptions, uncertainty and fully replayed chronological diagnostics; no partial-balance runoff inference |
| `mortgage-state-1` | `project_mortgage_state(request)` | [Mortgage state](mortgage-state.md) | Explicit ARM/prepay/credit and borrower/investor/agency ownership on supplied paths |
| `mortgage-credit-accounting-1` | `account_mortgage_credit(request)` | [Acquired mortgage source](mortgage-credit-accounting.md) | Complete paired Physical original/current whole-loan sources, retained original cost/EIR and exact sequential carrying; strict new-owner raw parsing, 164 independent/process and 31 durable API DEBUG gates pass; official release and daily-journal adoption remain separate gates |
| `funding-contract-1` | `evaluate_funding_contract(request)` | [Funding contracts](funding-contracts.md) | Actual-date funding/rollover and supplied collateral/default policies |
| `consumer-installment-1` | `project_consumer_installment(request)` | [Installment lending](consumer-installment.md) | Explicit auto/personal terms, supplied path actions and separate recognition/recovery rights |
| `consumer-credit-accounting-1` | `account_consumer_credit(request)` | [Acquired installment accounting](consumer-credit-accounting.md) | One physical installment source, frozen supplied original yield and reviewed allowance/default/gross-recovery intents; independent raw/persisted verification, whole-book adoption has separate gates |
| `card-credit-accounting-1` | `account_card_credit(request)` | [Acquired card accounting](card-credit-accounting.md) | Supplied non-grace/no-fee/no-draw runoff, original yield and reviewed credit/gross-recovery intents; public release source and durable primitive gates pass, wider adoption remains separate |
| `card-credit-accounting-2` | `account_card_credit_v2(request)` | [Interest-basis ownership](card-credit-accounting-v2.md) | Explicit whole-carrying original EIR, separate interest basis and reviewed disposal; rebuilt default source/shared-GL, 1,842 engine, 493 API and 40 browser gates pass; wider policy/product adoption remains bounded |
| `consumer-card-1` | `simulate_consumer_card(request)` | [Cards](consumer-card.md) | Class balances, actual statement/grace/payment/promotion and supplied credit/collection actions |
| `indexed-contract-1` | `generate_indexed_contract(request)` | [Term enhancements](term-enhancements.md) | Supplied observed/projected indices with exact availability; retained original payment and balloon |
| `conditional-exercise-1` | `value_conditional_exercise(request)` | [Exercise publication](term-exercise-publication.md) | Bellman call/put valuation on a supplied conditional pricing tree with fixed OAS; cooperative controls and independent complete raw/persisted policy/cash replay |
| `cd-election-1` | `elect_cd(request)` | [Term enhancements](term-enhancements.md) | Supplied withdrawal/penalty/grace/maturity/renew/switch election |
| `credit-model-1` | `evaluate_credit(request)` | [Credit transitions](credit-model.md) | Deterministic state probabilities, competing exits, EAD/LGD and complete recovery tails |
| `observed-credit-calibration-1` | `fit_observed_credit(request)` | [Observed calibration](credit-calibration.md) | Exact transition-time homogeneous hazard MLE, explicit positive-count rate standard errors, and chronological holdout diagnostics |
| `dated-cashflow-1` | `resolve_dated_cashflows(schedule, scenario)` | [Dated events](dated-cashflows.md) | Validation and identity-preserving scenario selection of supplied dated events |
| `dated-term-1` | `generate_dated_term_flows(request)` | [Fixed term producer](dated-term-contracts.md) | Fixed, nonoptional corporate/CD contractual cashflow generation |
| `floating-rate-1` | `evaluate_floating_coupons(request)` | [Floating coupons](floating-rate.md) | Arithmetic from available supplied fixings and explicit observation/principal intervals |

The request's inner schema is required where its typed model contains a schema.
The dated-cashflow envelope contains `{schedule, scenario}`; the schedule carries
its own `dated-cashflow-1` schema. Unknown fields fail, including unknown fields
inside tagged enums. Dates use Gregorian ordinals (`0001-01-01 = 1`), rates are
annual decimals and monetary units are explicit. Currency identifiers are
structural labels; no FX conversion or legal currency eligibility is inferred.

## Native boundary and adapters

The standalone `portfolio-lifecycle` reads one JSON object from stdin:

```json
{"schema":"credit-model-1","threads":1,"request":{"schema":"credit-model-1","monetary_unit":"USD","measure":"physical","initial_live_probability":[1,0,0],"periods":[{"duration_years":0.5,"transition_hazards_per_year":[[0,0,0,0.2,0.3],[0,0,0,0,0],[0,0,0,0,0]],"exposure_at_default":[{"drawn":100,"undrawn":20,"credit_conversion_factor":0.5},{"drawn":100,"undrawn":20,"credit_conversion_factor":0.5},{"drawn":100,"undrawn":20,"credit_conversion_factor":0.5}],"loss_given_default":[0.4,0.4,0.4],"recovery_distribution":[{"lag_years":1,"weight":1}]}]}}
```

This is a synthetic hand-calculation example, not a fitted risk policy. Its
performing survival is `exp(-0.25)` and its default probability is
`(1-exp(-0.25))*0.2/0.5`. Default EAD is 110; nominal recovery is 60% of default
EAD, paid one year after the interval-end default recognition. The recovery
remains in the output even though it falls after the six-month model horizon.

Success returns `{ok:true,result:...}`; failure returns `{ok:false,error:...}`.
The adapter unwraps the result and raises a `ValueError` for native failures.
All financial calculation stays behind that boundary:

```python
from portfolio_risk.analytics.model_contracts import evaluate_credit
result = evaluate_credit(request)  # request is the inner typed input above
```

The FFI and standalone input/output limits are 64/128 MiB. Threads must be
1..256. Typed models add independent event, period, work, date, monetary and
metadata bounds documented on their respective pages. These are component
admission limits, not a total process-memory cap. The transport serializes
dates/JSON and manages result ownership; it contains no quant formulas.

## Model adoption and compatibility

These functions do not automatically replace saved product economics, assign
accounting policies, publish a journal or populate a Strategy Lab library.
Fixed corporate `level_payment` is also available through existing native term
pricing; its [amortization contract](contractual-amortization.md) preserves
historical `annuity` economics. The new dated producer rejects floating/options,
heuristic sinking schedules and active CD withdrawal behavior. It supplies full
contractual coupon amounts; daily income recognition requires an accounting
policy and journal integration. Conditional income forecasts remain separate
from pricing and physical credit measures.

Keep the immutable complete request with the result when an artifact is stored.
Source IDs/revisions identify supplied assumptions; they do not certify source
quality, calendar generation or empirical accuracy. New calibrated policies
require explicit adoption, a fresh library/session build and new result identity.
Scenarios retain the original base OAS and shared random numbers. No prepared
interactive evaluator reprices instruments.

Rebuild the product and decision/workflow binaries after upgrading, then restart
API/workers. These additions retain product ABI 8 and existing ledger protocol;
an older ABI-8 binary still cannot serve the new additive schemas. Rebuilding
locally is separate from restarting an executing deployment.

## Current source execution (0.29.7)

The durable `/balance-stress/dated` and `/balance-stress/credit` routes transport
the complete immutable request to their native runners. Credit execution explicitly
selects stream protocol 4. Independent persisted credit/source/GL verification
gates publication and retains full recovery tails. Neither scheduled dated capture
nor the separate loan-only credit route automatically upgrades mixed saved books.

`treasury-bridge-1` additionally admits `kind="credit_ledger"` over independently
replayed signed credit journals and their closing trial balances. Its policies
must cover live and default principal/accrual/basis and allowance GLs explicitly.
Severity-one baseline/scenario runs are selected; all retained reverse runs are
verified. Economic modeled loss is not deducted a second time from posted capital.
The Capital & FTP panel exposes this durable source bridge; regulatory eligibility,
commitment CCF/leverage additions and requirements remain explicit policy inputs.

Engine 0.29.6 raises the native product ABI to 9. The mortgage kernels (ops 4, 5
and 10) take a ninth per-pool vector, the prepay speed multiplier, directly after
the static multiplier; ops 28 and 29 take it as their last input, empty for
"1 everywhere". `mortgage-risk-1`, owned mortgage JSON and accounting requests
accept an optional `prepay_multiplier` array of the same length as the book.
An ABI-8 binary is refused by the ABI check; rebuild and restart.

Engine 0.29.7 raises the native product ABI to 10. The deposit kernels (ops 6
and 7) take the terminal-treatment code (0 payout, 1 truncate, 2 runoff_tail) as
their last input, and op 6 returns a seventh output: per-cohort path sums of the
residual balance before terminal treatment. Every deposit JSON request accepts
an optional `assumptions.terminal` (`payout` when omitted, never serialized, so
default request hashes and cache identities are unchanged; unknown names are
rejected). `deposit-risk-1` adds `residual` ($). `deposit-horizon-1` takes the
`deposit-risk-1` request with `horizons` as report horizons in months (1..8,
increasing, first >= 2, at most 482 on the 161-forward grid, no `shocks`) and
returns `policy`, `base_mode`, `modes`, fixed `oas`, `residual[h][i]`,
`pv[h][m][i]`, `terminal_pv[h][m][i]` and `implied_oas[h][m][i]`.

## Validation and remaining work

Independent Rust fixtures and `test_native_model_contracts.py` cover analytical
expectations, domain failures and both native entrypoints. Fresh-process tests
run outside the explicit Python reference context. Level-payment pricing checks
use independent calendar dates, annuity payments, a scalar OAS root and fixed
spread marks. Existing convention, term ownership and full engine gates remain
required. Numerical, parity and synthetic tests do not replace real-data model
validation or persisted-journal replay.

[The implementation ledger](implementation-plan.md) preserves every issue's
full acceptance scope. Joint product credit/behavioral cashflows, correlated
credit draws, interval-censored fitting, future index projection, daily ledger
adoption and empirical validation remain open.

## Explicit dated replay route (0.29.6)

`analytics.dated_ledger.run_dated_ledger` selects `dated-ledger-1`, embeds the
strict `dated-store-1` source/ownership/recognition contract and executes the
native ledger protocol 3. Its separately versioned immutable manifest retains
raw input, event lineage and complete closing rights. Independent persisted
source/journal verification gates publication. Existing `balance-stress-2`
requests and protocol 2 remain separately readable.

This route supports supplied dated scheduled recognition and settlement, with
explicit physical/generated or contractual ownership. It does not automatically
wire the shared `credit_operator` or `credit_accounting` Rust APIs into daily
execution. Native graph capture, joint behavioral/credit state, saved-book and
candidate mapping, and durable API/UI exposure remain required.

See [dated replay](dated-ledger.md), [bounded source store](dated-store.md) and
[checkpoint evidence](../reviews/2026-10-01-dated-ledger-and-credit-foundations.md).
Rebuild the ledger and dependent decision/workflow deployment binaries and
restart workers before using the additive schema.


`credit-path-ledger-1` selects ledger stream protocol 5 through
`analytics.credit_path_ledger.run_credit_path_ledger` and durable
`/balance-stress/credit-path`. Every persisted raw path, reviewed intent, source,
ordered posting and closing is independently recomputed before immutable
publication. Canonical JSON cells retain variant sources and exact integer keys.
Verified realized sources can feed explicit native closing-capital policies;
realized paths are not expected-flow FTP sources. See [realized ledger](credit-path-ledger.md).

`graph-dated-ledger-1` and `driven-credit-ledger-1` execute native model/source
preparation and daily replay in one bounded `portfolio-model-ledger` process.
`analytics.model_ledger.run_model_ledger` retains and independently recomputes
prepared sources before persisted journal verification and atomic publication.
Durable `/balance-stress/modeled` jobs expose both routes with compatible explicit
closing-capital policies. See [consumer capture](graph-dated-ledger.md) and
[frozen credit drivers](driven-credit-ledger.md). They retain their admitted
par-note and loan-only boundaries; neither establishes complete mixed-book adoption.

`deposit-graph-dated-ledger-1` adds reviewed par/simple-interest deposit capture
and daily replay through that same durable modeled route. Complete source cash,
daily expense recognition and retained rights are independently recomputed before
persisted journal publication. Legacy deposit flight never applies to dated-owned
positions. Migrations, new inflows, negative-rate accounting and nonpar EIR require
separate explicit owners. See [deposit replay](deposit-graph-dated-ledger.md).

`deposit-graph-income-ledger-1` explicitly supplies a purchase-time quote,
original carrying value and reviewed original effective yield. Full original
physical expected cash and unchanged scenario cash must reconcile; daily
contractual expense and liability basis amortization retain separate origins.
Changed expectations, seasoned openings, migration/inflow and negative-rate
accounting reject. Independent source/anchor/income recomputation and persisted
journal verification gate durable publication. See [deposit original income](deposit-original-income.md).

`market-indexed-adapter-1` adds supplied named-index/collateral curve preparation
and actual-date indexed term/swap valuation to `portfolio-lifecycle`. The original
reviewed OAS and source parents remain immutable; conditional projections have
explicit pricing-only scope. Independent source recomputation gates durable
`/models/runs` publication and persisted reload. See
[market indexed sources](market-indexed-adapter.md) for bounds and conventions.

`joint-state-1` is also admitted by the durable `/models/runs` worker. Its
publication receipt independently replays competing hazards, event ordering,
recovery/loss amounts, retained market drivers and the caller-owned CRN identity
before Parquet persistence. This is a source/journal integrity gate; it does
not make supplied hazards empirically calibrated or adopt them into saved
product economics.

`mortgage-mixed-accounting-1` explicitly selects registered acquired Physical
mortgage sources and a reviewed dated backbook in `portfolio-model-ledger`.
The selected typed envelope reparses original bytes after bounded schema dispatch.
Stream protocol 3 retains the actual producer completion, complete unweighted
source tails, separate monetary and borrower/economic claim state, and one ordered
daily GL. Exact combined gross-state checks, independent source/journal/partition
replay and immutable publication fences are mandatory. The durable modeled route
and partitioned panel transport the same schema. See
[mortgage mixed accounting](mortgage-mixed-accounting.md) for 64-day, byte, work
and monetary bounds. Accounting approval, capital/FTP and complete product adoption
remain false. Rebuild matching official artifacts before executing either schema;
source application and contained validation do not deploy production workers.

The locally applied 0.29.8 source adds explicitly selected successor contracts.
Their installed-default acceptance awaits matching official artifacts and fresh
validation. Historical contracts and numeric product ABI 8 IDs/layouts remain
unchanged; source application does not grant empirical or regulatory approval.

- `deposit-revised-income-ledger-1` transports original source, quote, cost and
  effective yield with reviewed day-zero expectation revision. One native daily
  owner retains separate gross FIFO earning/cash rights and posts the reviewed
  catch-up once. Complete raw/preparation/completion identity, independent source
  and persisted journal replay remain publication gates. See
  [deposit revisions](deposit-revised-income.md).
- Indexed `graph-1` registry and raw-book owners retain immutable source parents
  and the original reviewed OAS. The explicit opt-in book route has a 360-month
  component grid ceiling with unchanged joined work and retained-byte ceilings;
  some path/grid combinations still refuse. It does not add saved-book,
  income-accounting, capital or general product support. See
  [indexed graph](market-indexed-graph.md) and [indexed books](market-indexed-books.md).
- `graph-call-control-1` is additive capability version 1 for per-call indexed
  execution. Its five exports and memory/caller obligations are generated from
  `docs/contracts/native-graph-runtime-1.json` in the ABI layouts documentation.
  Native tokens are one-shot and library-scoped; explicit requests fail if the
  capability is missing. Financial JSON, original OAS/CRN and cache parents remain
  unchanged. Bounded callback reads occur during execution, after native return
  and after decoding. Whole-job deadlines, durable lease/atomic publication
  fences and the watchdog remain separate. See [graph control](graph-runtime-control.md).
- `mortgage-closing-capital-1` is a separate reviewed complete-tail permission,
  reached through `portfolio-strategy` and durable
  `POST /treasury/mortgage-closing-capital`. A completed source remains a research
  journal; a new job retains its own original deadline, private source/job/code
  binding and independently encoded typed normalization digest. Raw native
  source/GL rebuild, independent source/capital replay, checksummed output rereads
  and final SQL ownership/source guards are mandatory. FTP, interim capital and
  accounting/regulatory approval remain false. See
  [mortgage closing capital](mortgage-closing-capital.md).


## Separate fee lifecycle (local0.29.16)

`nonintegral-fee-accounting-1` exposes the supplied Physical service/settlement/
review tape, signed gross fee states and complete-tail ordered balanced intents.
It retains independent raw/persisted verification and commercial final SQL guards.
Loan G/EIR is untouched; native-credit/mixed/graph/unit/FTP adoption remains false.
See [the detailed contract](nonintegral-fees.md).


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
