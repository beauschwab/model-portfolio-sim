# Balance-sheet ledger completeness review

Reviewed: 2026-09-29, current engine 0.22.0 implementation.

**Historical review.** Subsequent fixes and remaining gaps are tracked in the
[0.23.0 implementation report](2026-09-29-ledger-implementation.md). The two
reproduced defects below are regression-tested and corrected there.

## Conclusion

The application has a useful, runnable, reconciled **cohort stress prototype**.
It does not yet have a complete bank general ledger, complete instrument
subledgers, or a single integrated pricing/accounting/strategy simulation.
Durable storage and successful tests do not establish financial-model completeness.
Earlier references to an end-to-end prototype describe the API/worker/UI flow,
not completion of the proposed G-SIB modeling scope.

This review inspected the new stress model and its tests, the existing accounting
and capital/optimizer implementations, and the API integration boundary. Two
targeted numerical probes reproduced additional correctness gaps. The existing
stress suite was rerun: **27 passed in 0.90 seconds**. This was not another full
repository security, performance or infrastructure audit.

## Priority findings

### P1: Management-action gains bypass the implemented tax calculation

In `analytics/balance_stress.py`, tax is computed at lines 481–485, before policies
execute at lines 496–550. Security-sale earnings are posted at line 548. The daily
aggregate is then cleared at line 558. These earnings never enter a subsequent
day's tax calculation.

Reproduction in consistent arbitrary amount units:

- Opening cash 20, security 100, funding 80, equity 40.
- AFS security, duration 2; rate shift -0.10 creates a market factor of 1.20.
- Account tax rate 20%; sell policy targets cash 80 with a 60-unit proceeds limit.
- On day 2 the policy sells 50 face for 60 and reclassifies 10 from AOCI to earnings.
- Actual output: sale earnings 10, **no tax rows**, final cash 80 and equity 60.
- Under the module's own positive-income tax convention, the realized gain should
  generate 2 units of tax; absent other changes cash/equity would be 78/58.

This is an ordering defect beyond the disclosed absence of deferred tax and tax
loss carryforwards. Accounting reconciliation remains exactly zero because both
cash and equity omit the same missing expense. Fix by calculating taxes after
all taxable daily events, with taxable-income classification separated from
book-equity movements and appropriate tests for action gains/losses.

### P1: Maturing pledged collateral becomes unrestricted cash

Secured funding increments an aggregate liability and a position encumbrance
fraction (lines 525–534). Asset maturity transfers the entire principal into the
single cash balance and removes the position (lines 437–445). There is no pledge
contract linking the asset, secured debt, maturity proceeds and restricted cash.

Reproduction:

- Opening cash 20, security 100, funding 80, equity 40.
- Security haircut 10%, fully eligible, maturity day 3.
- Policy borrows 90 against all 100 face on day 2, with zero interest for isolation.
- Day 2: cash 110, total liabilities 170, collateral fully encumbered.
- Day 3: cash **210**, liabilities still **170**, equity still 40.
- The 100-unit maturity proceeds are entirely spendable by subsequent policies.

There is no modeled repayment, collateral substitution, cash pledge or contractual
release authorizing that conversion. The actual treatment depends on the funding
contract, but the engine currently assumes unrestricted availability. It can
therefore overstate usable liquidity even while assets equal liabilities plus
equity. The opposite direction is also incomplete: repayment of existing repo
funding does not automatically release a linked pledge.

Fix with an explicit secured-funding/pledge subledger, funding maturities,
substitutions, margin maintenance and restricted cash. Test both debt maturity
and collateral maturity, including sales, haircuts and partial repayment.

### P1 architecture gap: The event table is not a complete double-entry journal

`post()` (lines 288–297) records cash, earnings and OCI deltas grouped by
day/entity-currency/event. Position balances, allowances, accruals, recovery
receivables, intercompany balances and borrowing balances are changed separately
in mutable simulation state.

Consequences:

- No chart of accounts or balanced debit/credit entry lines.
- No transaction IDs, posting/reversal relationships or instrument/lot IDs on
  ordinary ledger entries.
- No generic reconstruction of all closing subledger balances from journal
  entries alone; principal, allowance and funding states require model-specific
  transition logic and additional snapshots.
- `extra_assets` pools recovery and intercompany receivables, while `extra_debt`
  pools policy borrowing and intercompany liabilities. Counterparty, maturity,
  collateral and settlement detail are not represented as separate claims.
- The account identifier denotes an entity/currency bucket, not a general-ledger
  account. Equity is a single balance plus run-generated OCI, without a full
  opening equity/retained-earnings/OCI subledger.

The daily reconciliation and cash/equity bridges are valuable controls. They
cannot prove that all economic events were captured or that classifications and
contractual restrictions were correct.

### P1 integration gap: Three financial representations remain separate

1. Existing product engines and `analytics.accounting` generate priced and
   behavioral cashflows, effective-interest income and runoff.
2. Existing KPI/unit-library/optimizer paths use their established capital and
   liquidity approximations.
3. `analytics.balance_stress` receives a separate explicit cohort specification
   and calculates its own interest, marks, defaults and cashflows.

`apps/api/app/store.py:998` forwards the explicit specification and attaches the
workspace revision. It does **not** adapt saved book positions or engine-generated
cashflows into the stress model. The worker infrastructure is shared, but the
financial inputs and evolution are not unified. Existing detailed MBS prepayments,
deposit behavioral engines, effective-yield accounting and contract schedules do
not automatically drive the new stress ledger.

There is also no automatic validation of an optimizer allocation against this
daily stress framework. A feasible LP solution is feasible under its existing
coefficient model; it is not evidence of daily stressed liquidity or capital
feasibility in the new engine.

## Material simplifying assumptions

| Area | Current implementation | Missing for the intended complete model |
|---|---|---|
| Opening book | Cash/equity, six position kinds and separate netting sets | Full opening accrued interest, historical cost/fair-value bases, opening OCI, tax balances and other bank balance-sheet categories |
| Cashflows | ACT/365 daily interest, regular payment interval, principal maturity | Instrument calendars, amortization, prepayments, resets, options, settlement conventions, premium/discount and fee amortization in the new ledger |
| Valuation | Static duration-based security mark, 1% price floor; manually specified derivative stress loss | Reuse of product pricers, nonlinear option/convexity behavior, aging and full curve/volatility/spread dependence |
| Credit | Deterministic fractional defaults; one-way watch migration; annual PD times LGD allowance | Rating transitions including cures, default states, correlated losses, realistic recovery timing, loan-level calibration and lifetime allowance models |
| Deposits | Hand-set uninsured/concentration/digital/operational sensitivity formula and positive rate competition | Calibrated customer behavior, product migration, relationship effects, inflows and richer pricing response |
| Collateral/dealer | Fractions per security, aggregate borrowing, netting-set marks and posted VM targets | Linked pledge contracts, received VM evolution, initial margin, closeout, collateral substitution, counterparty contagion and full XVA |
| Entities | Separate entity/currency buckets and simple same-currency transfers | Consolidation/eliminations, FX translation and transfer pricing, legal-entity capital and detailed transfer restrictions |
| Capital | New model: supplied weights and deductions, 100% commitment CCF and watch uplift | Comprehensive capital components, deductions, buffers and exposure classifications |
| Existing KPI capital | Starting CET1 ratio times proxy RWA; NII times 0.43 times retention; static RWA | A complete PPNR, provision, tax, distribution and capital rollforward linked to accounting |
| LCR/NSFR | New model: uncapped weighted proxies; existing solver: separate stylized constraints | A shared regulatory rules layer, complete inputs, consistent scope and daily replay |
| HTM | Explicit sale permission and face cap per policy, tracked against position sales | Portfolio classification/limits, accounting consequences and integration into strategy optimization |
| Management | Cash-triggered sale, borrowing, transfer and dividend cut; specification order matters | Repricing, origination/reinvestment, hedging, capital issuance/buybacks and optimization of contingent policies |
| Scenarios | Deterministic step shocks, persistent credit/deposit modifiers, severity grid | Joint physical-measure stochastic drivers, calibrated dependencies, uncertainty and evolving/recovering paths |
| Failure handling | Negative cash followed by continued diagnostic simulation | Payment default/resolution states; capital and future action feasibility after failure |

In the new stress engine, only cash, CET1 and leverage trigger breach records
(lines 356–358). LCR/NSFR values are displayed as proxies; they do not have breach
floors in this specification. Its HTM sale policies do not extend the existing
strategy solver's constraint set.

The older accounting engine is more detailed in some areas: it already includes
effective-interest premium/discount amortization and scheduled product cashflow
aggregation. Those features should be integrated through adapters rather than
reimplemented independently. That older engine also explicitly uses static
time-zero effective yields and monthly accrual smearing, so integration alone
does not remove every approximation.

## What is genuinely implemented

- A real executable daily simulation with validated opening identities and
  per-day reconciliation of modeled assets, liabilities and equity.
- Cash/equity event bridges, classification-aware marks and tested AFS sale
  reclassification.
- Explicit local cash constraints, delayed policies, basic collateral usage,
  expected-credit-loss mechanics and margin settlement timing.
- Durable request/result storage, separate worker execution, Parquet artifacts,
  a usable scenario UI and reproducible synthetic examples.

The prior 130-engine/92-API/3-browser passing results are useful evidence for
their tested scope. They do not establish completeness, economic calibration or
production suitability. This review's two reproductions both satisfy the current
reconciliation gate; passing that gate alone does not detect the defects.

## Recommended completion sequence

1. **Correct and regression-test tax/action ordering and collateral maturity.**
2. **Introduce typed double-entry journals and instrument subledgers.** Separate
   cash/restricted cash, principal, accrual, allowance, recoveries, funding,
   collateral, intercompany, earnings, OCI and equity components. Require journal
   replay to reconstruct all closing balances independently of simulation state.
3. **Make the saved portfolio the common input.** Add explicit legal entity,
   currency, accounting classification and credit/collateral metadata. Adapt
   existing product cashflows and valuations; reject missing mandatory mappings.
4. **Unify reporting and capital/liquidity rules.** Derive projections and
   constraints from the same subledgers, with jurisdiction/versioned mappings.
5. **Replay solver candidates through the dynamic engine.** Preserve fast linear
   evaluation for search, then validate proposals against daily cash, accounting,
   collateral, capital and policy constraints before labeling them validated.
6. **Calibrate, backtest and then scale.** Add joint scenarios and richer product
   lifecycle rules before using large synthetic instrument counts as evidence of
   complete balance-sheet risk coverage.

No engine behavior was changed during this review. Reproduction output is saved
locally at `.data/balance-stress-review/reproductions.json`. The correctness issues
above remain open.
