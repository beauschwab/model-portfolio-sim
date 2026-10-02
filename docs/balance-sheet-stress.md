# Balance-sheet stress research framework (0.22.0)

**Historical 0.22.0 specification.** The current implementation is documented in
[Balance-sheet ledger and replay, 0.23.0](balance-sheet-ledger.md). Version 2
supersedes the tax ordering, collateral lifecycle, ledger, mapping and validation
limitations described below; other research-model limitations remain explicit.

Open **Balance-sheet Stress** in the workspace rail and select **Run balance-sheet
stress**. The default is a balanced synthetic bank/dealer example in USD millions,
not the current pricing book or an actual G-SIB filing. Expand the JSON editor to
change the accounts, cohorts, netting sets, policies, scenario shocks or reverse
stress grid. Unsupported fields, nonfinite values and unreconciled opening
balance sheets fail before submission.

## Delivered scope

| Area | Implemented | Remaining production work |
|---|---|---|
| Ledger | Daily assets, liabilities, equity, separate interest accrual/settlement, provisions, recoveries, fees/costs, tax and dividends; reconciliation gate | Exact contractual schedules, business calendars, effective-yield/premium amortization, deferred tax and full accounting policy |
| Liquidity | Deposit cohorts with uninsured/concentration/operational/digital attributes; rate competition; draws, rollover, funding maturity, collateral haircuts/encumbrance, sales and outages | Calibrated deposit migration/relationship behavior, intraday payments, collateral substitution and repo-specific pledge release |
| Credit/capital | Two-state watch migration, expected defaults, LGD, delayed recovery, allowance roll, nonaccrual after default, dynamic exposure/RWA/CET1/leverage | Loan-level PD/LGD/EAD estimation, multi-rating migration, CECL/IFRS9, supervisory capital rules and buffers |
| Entities | Explicit legal entity/currency accounts, intercompany receivable/payable, transfer caps/delays, donor cash floor | Cross-currency transfers, FX/basis modeling, consolidated eliminations and entity-wide regulatory capital |
| Dealer | Explicit legal netting sets, counterparty IDs, stress losses, delayed posted VM, wrong-way credit loss, repo/reverse-repo balances | Derivative pricer integration, received VM dynamics, initial margin, closeout/default waterfalls, full XVA and securities lending |
| Management | Cash-triggered security sales, secured funding, transfers, dividend suspension; costs, limits, delays, HTM restrictions | Deposit repricing, origination/reinvestment and hedge policies; optimized policy search; validated mapping from Strategy Lab allocations |
| Explanation | Baseline plus four joint scenarios, first breach, cash/capital paths, event ledger, additive baseline attribution, cohort exposures, sampled reverse grid | Historical backtesting, uncertainty distributions and causal/Shapley risk attribution |

This is a deterministic **cohort research engine** in
`portfolio_risk.analytics.balance_stress`. It is not a replacement for the
Numba/Rust instrument pricing engines and has not been benchmarked as a
60,000-instrument G-SIB production simulation. The request has a bounded work
budget, rather than promising unbounded cohort or scenario scaling.

## Input and execution contract

`GET /balance-stress/example` returns the example, field types/defaults and current
workspace revision. `POST /balance-stress/run` accepts:

```json
{
  "expected_revision": 0,
  "specification": {
    "version": "balance-stress-1",
    "horizon_days": 30,
    "accounts": [{"id":"bank_usd","entity":"bank","currency":"USD","cash":20,"equity":40}],
    "positions": [
      {"id":"security","account":"bank_usd","kind":"security","balance":100,"classification":"afs","duration":2},
      {"id":"funding","account":"bank_usd","kind":"funding","balance":80}
    ],
    "scenarios": [{"name":"rates_up","rate_shift":0.01}],
    "reverse_severities": [0,0.5,1,2]
  }
}
```

Use the actual current revision. The API only validates and enqueues. The separate
worker runs against immutable request arguments and the queued workspace snapshot.
The explicit stress specification is included in the stored request and result;
it is not another mutable copy of the saved books. SQLite and PostgreSQL use the
same operation registration and durable queue. Output tables become immutable
Parquet artifacts through the existing local/S3 codec. Iceberg catalog publication
remains a future storage adapter.

Use the existing `/jobs/{id}`, `/result`, `/manifest`, `/table` and `/parquet`
endpoints. The UI provides CSV exports of selected tables. Example specifications
can also be run directly:

```python
from portfolio_risk.analytics.balance_stress import example_specification, run_balance_stress
result = run_balance_stress(example_specification())
result["summary"].write_parquet("balance_stress_summary.parquet")
```

Amounts must use one consistent unit within each currency. Rates and weights are
decimals. Time is integer simulation days; `maturity_day=0` means beyond horizon.
Interest accrues daily using ACT/365 and settles every `payment_interval_days`
(default 30), or at maturity. There is no valuation-date calendar in this module.
Output samples every day through day 30 and every 30 days afterward, plus the final
day; accounting checks and breach detection still run **every day**. Cohort
exposures are sampled at day zero and 30-day intervals.

Limits: 50 entity/currency accounts, 2,000 positions, 500 netting sets, 100 policies,
8 scenarios, 12 reverse severities, 30–1,080 days. Additionally,
`days * (positions + netting_sets + accounts + policies) * runs <= 3,000,000`.
These are admission bounds, not throughput guarantees. Reverse runs retain
summaries only. No new paths × instruments × dates tensor is allocated.

## Model equations and ordering

Opening equity must satisfy `cash + net assets − liabilities = supplied equity`;
it is never manufactured as a plug. Reconciliation is checked each day with a
relative tolerance of `1e-8` against gross assets/liabilities. Ledger earnings and
AOCI bridge opening to closing equity; ledger cash bridges opening to closing cash.

Each day: recoveries settle; securities re-mark; commitments draw; credit
migrates/defaults and allowance resets; interest accrues/settles; deposits run off;
assets/funding mature; derivative marks and credit losses post; historical margin
targets settle; operating cashflows, taxes and dividends post; policies trigger
and due actions execute; the balance sheet reconciles and limits are observed.
This is a daily close model: within-day payment ordering and temporary intraday
breaches are not evaluated.

Credit migration grows watch exposure by an annual hazard transformed to a daily
probability. Effective annual PD is `base_PD * scenario_multiplier *
(1 + watch_fraction * (watch_PD_multiplier - 1))`, clipped below one. Daily default
is `performing_balance * (1 - (1 - PD) ** (1/365))`. Defaulted principal stops
earning interest. Recoverable principal becomes a receivable until its settlement
day; LGD is removed from the allowance. The new allowance target is remaining
balance × annual PD × LGD. Provision expense equals target minus allowance after
chargeoff. Thus chargeoff and provision are **not separate duplicate losses**.
This is a one-year expected-loss approximation, not lifetime CECL or IFRS9 staging.
Accrued interest is not included in default EAD in this prototype.

Deposit monthly outflow probability is base runoff plus positive rate competition
plus scenario flight times cohort sensitivity. Sensitivity is
`(.25+.75*uninsured)*(1+concentration)*(1+.5*digital)*(1-.5*operational)`.
Daily runoff uses `1-(1-monthly_probability)**(1/30)`. These coefficients are
illustrative assumptions, not fitted estimates. Commitment draws occur over the
first 30 stress days and cannot exceed the original undrawn amount. Wholesale
rollover is the supplied fraction less the scenario reduction; residual funding
rolls beyond the horizon with its supplied rate plus the stressed funding spread.

Securities use `max(.01, 1-duration*(rate_shift+spread_shift))` market factors.
AFS changes enter AOCI, trading changes enter earnings, HTM/amortized cost retain
book value until sale/maturity. AFS sales reclassify accumulated OCI to earnings
without another total-equity loss. HTM sale policies require explicit permission
and a cumulative face limit; this is an internal modeling restriction, not a
conclusion about permitted sales or classification consequences under accounting
standards. Mark factors are static stress proxies, not aging, full-repricing or
credit-adjusted bond valuations. Securities repay face at maturity.

Netting-set fair value decreases by supplied stress loss × market shock.
Counterparty expected loss applies to positive FV net of received collateral,
with supplied wrong-way multiplier under market stress. Margin targets reflect
negative FV less the threshold and settle after the configured lag (zero allowed).
Posted collateral is an asset; posting cash is not another P&L loss. Separate
netting sets never offset one another. Received margin is an opening balance,
not a dynamic receipt engine. Counterparty loss does not model collateral recovery
on posted margin, closeout conventions or a stochastic default event.

Policies are declared before a run and inspect current cash only. Trigger today,
execute after at least one day, then recheck need and available resources. They
cannot read future scenario results. Multiple policies execute in specification
order and consume the same collateral and cash state. Secured borrowing uses
eligible unpledged face after haircuts; sold collateral cannot also be pledged.
Already encumbered collateral remains encumbered unless sold portions were free;
funding maturity does not automatically release an unidentified pledge. A
transfer creates matching intercompany receivable/payable and leaves donor cash
above its floor. No transfer is inferred from common ownership or consolidated
surplus. Outages delay policies, not scheduled contractual outflows. Dividend cuts
affect future distributions only. Policy funding remains outstanding through the
horizon and pays daily interest.

## Capital, liquidity ratios, and failure interpretation

CET1 proxy = equity less explicit deductions, optionally excluding AOCI. Credit
RWA uses current net carrying balances × supplied weights × `(1+watch_fraction)`,
plus undrawn commitments at a disclosed 100% CCF and receivables at 100% weight.
Leverage exposure is assets plus undrawn commitments. Per-account CET1 and leverage
floors are user assumptions, not jurisdiction-specific requirements. Applying
capital separately to currency accounts is a diagnostic allocation; true
legal-entity capital consolidation requires FX translation and eliminations.

LCR proxy uses positive cash plus unencumbered market value × user HQLA weight,
divided by user-weighted outflows. NSFR proxy uses equity and user-weighted funding
over user-weighted assets. These exclude regulatory eligibility/caps, inflow caps,
maturity buckets, netting and many adjustments. They are **labeled proxies**, and
the stress engine does not assert regulatory LCR/NSFR compliance. The existing
Strategy Lab/Decision Lab constraints and their established approximations remain
unchanged; `/strategy/eval` still performs coefficient-only evaluation.

Negative cash records unfunded obligations. The model continues to quantify the
shortfall without inventing emergency financing or claiming the institution can
execute a feasible post-failure plan. First cash/capital/leverage breaches include
day zero. Peak cash shortfall is measured against the configured floor, not only
zero. A bank can therefore breach its buffer before exhausting cash.

Reverse stress evaluates every requested severity and records breaches. It does
not assume monotonicity or report an unobserved exact failure boundary. Severity
zero is a baseline-equivalence gate; multipliers interpolate from one, additive
shocks scale from zero, and outage duration scales with severity. Baseline deficits
can cause a breach even at severity zero. Event attribution is additive stressed
minus baseline cash, earnings and OCI; it does not allocate nonlinear interactions
to causal macro factors.

## Validation and next integrations

`test_balance_stress.py` gates independent hand calculations, cash/equity bridges,
zero shock, pre-shock identity, accrual/payment separation, classification,
AFS sale reclassification, HTM policy limits, default/provision/recovery accounting,
commitment caps, exact funding maturity, no collateral reuse, trapped local cash,
transfer limits, derivative margin lag and malformed-input rejection.
API tests gate revision conflicts, real jobs, typed Parquet roundtrip, separate
API/worker execution, queued-run survival across restart and result exports.
Browser tests exercise real runs, scenario/account/detail selection, CSV export,
invalid input and narrow-panel rendering.

The next production integration is an **explicit, validated mapping** from saved
instrument books and optimized allocations to these cohorts, carrying source IDs,
accounting classes, legal entities, currencies, credit inputs and collateral
ownership. Missing fields should block production use, not be guessed from IDs.
Add calibrated physical-measure joint drivers and contractual flow adapters before
claiming realistic G-SIB risk. Conditional published forecasts remain NII/runoff
inputs only; they are never promoted into OAS/EVE or derivative pricing paths by
this module. No fixed-OAS or common-random-number pricing contract changes here.
