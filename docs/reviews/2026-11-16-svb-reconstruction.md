# SVB reconstruction and historical comparison

**Conclusion: the aggregate ledger reproduces disclosed opening totals and demonstrates the liquidity failure mechanism. It does not reproduce the precise March cash position or predict the run.**

Engine 0.29.20; Rust financial runtime. Amounts below are USD billions unless stated. 20 aggregate runs plus native capital checks completed in 2.19 seconds, including local Parquet and independent journal verification.

## Scope and information boundary

Opening: December 31, 2022 consolidated SVB Financial Group, disclosed February 24. This differs from Silicon Valley Bank alone. Total equity includes $0.291bn noncontrolling interests. The opening balance sheet is frozen for the event-window proxy; there is no claim to have reconstructed the intervening two months.

The uncalibrated behavioral sweep uses no observed withdrawal volume. The separate event replay explicitly supplies the March 8 sale and March 9 withdrawals. The March 10 $100bn is a lower-bound requested/expected outflow counterfactual, not a settled historical payment.

## Opening reconciliation (input checks, not predictive validation)

| Measure | Native ledger | Disclosure | Difference |
|---|---:|---:|---:|
| total_assets | 211.793 | 211.793 | 0.000000000 |
| total_liabilities | 195.498 | 195.498 | 0.000000000 |
| total_equity | 16.295 | 16.295 | 0.000000000 |
| cash | 13.803 | 13.803 | 0.000000000 |

The 15 position cohorts cover AFS, HTM, net loans, nonmarketable equity, premises, goodwill, other intangibles, lease assets, other assets, two deposit cohorts, short-term borrowings, lease liabilities, other liabilities and long-term debt. Non-securities fixed-value assets use explicitly labeled ledger placeholders. Loans enter at net carrying value; the $0.636bn allowance is retained in the reference facts rather than re-estimated.

AFS: cost 28.602, fair value 26.069. HTM: net carrying value 91.321, disclosed fair value 76.169. Deposits: 173.109; parent equity: 16.004.

## Capital and hidden valuation risk

| Native metric | Recomputed | Reported | Difference (bp) |
|---|---:|---:|---:|
| cet1 | 12.0542% | 12.05% | 0.425 |
| tier1 | 15.4047% | 15.40% | 0.465 |
| total_capital | 16.1756% | 16.18% | -0.441 |
| tce_ta | 5.6219% | 5.62% | 0.194 |

Eligible capital and aggregate RWA are supplied from disclosures. These checks validate native arithmetic, not bottom-up capital eligibility, credit-RWA construction or a daily regulatory capital projection. Other generated treasury metrics are omitted when inputs or applicability are unavailable.

| Mark sensitivity | HTM market value | Unrecognized HTM loss | Parent equity less HTM gap, pre-tax |
|---|---:|---:|---:|
| baseline | 76.169 | 15.152 | 0.852 |
| rates_up_100bp | 71.447 | 19.874 | -4.783 |
| rates_up_200bp | 66.724 | 24.597 | -10.418 |

The $15.152bn opening HTM gap is 94.7% of parent equity and exceeds $11.880bn tangible common equity. Subtracting it leaves $0.852bn of parent equity or negative $3.272bn tangible common equity, before tax. **These are securities-only haircut indicators, not full economic equity, regulatory insolvency findings or resolution-loss estimates.** They omit liability franchise value, other fair-value changes, taxes and resolution recoveries. AFS marks are already in book equity; subtracting them again would double-count.

Incremental rate marks use disclosed 6.2-year HTM duration and assumed 3.5-year AFS duration in the existing linear-duration ledger model. They are sensitivities, not historical yield-curve/MBS repricing; convexity, extension, hedge offsets and term-structure changes are absent.

## Can the deposit behavior model predict the speed?

| Flight input | First-day model outflow | Share of observed $42bn | First cash breach (day, cash only) |
|---|---:|---:|---:|
| flight_005 | 0.752 | 1.8% | 20 |
| flight_010 | 1.614 | 3.8% | 9 |
| flight_020 | 3.841 | 9.1% | 4 |
| flight_030 | 7.493 | 17.8% | 2 |
| flight_040 | 21.121 | 50.3% | 1 |
| flight_050 | 63.885 | 152.1% | 1 |
| flight_100 | 63.885 | 152.1% | 1 |

Inputs are stress severities, not probabilities. The daily model combines a monthly flight input with uninsured, concentration, digital and operational multipliers and converts it into a daily hazard. Uninsured share is approximately observed; the other three behavior parameters are assumptions, with no fitted SVB withdrawal history. The cap on monthly hazard creates saturation at extreme severities. Selecting a severity after seeing $42bn would be calibration to the answer. **The model can generate a large one-day outflow when forced hard enough; this experiment supplies no evidence it would anticipate the confidence shock or its date.**

## Conditional event replay and funding sensitivity

| Case | March 9 cash | Difference from reported -0.958 | March 10 counterfactual cash |
|---|---:|---:|---:|
| observed_day1_cash_only | -28.197 | -27.239 | -28.197 |
| observed_day1_sale21 | -7.197 | -6.239 | -7.197 |
| requested_day2_sale21 | -7.197 | -6.239 | -107.197 |
| observed_day1_all_free_afs | -2.611 | -1.653 | -2.611 |
| observed_day1_sale21_funding5 | -2.197 | -1.239 | -2.197 |
| observed_day1_sale21_funding10 | 2.803 | 3.761 | 2.803 |
| observed_day1_sale21_funding10_lag3 | -7.197 | -6.239 | 2.803 |
| requested_day2_sale21_funding20 | 12.803 | 13.761 | -87.197 |

Only cases prefixed `requested_day2` add the March 10 $100bn demand. Other March 10 values reflect the existing March 9 demand and any lagged funding execution, without another $100bn withdrawal. Negative cash is an unmet obligation.

**Unadjusted sale replay: 13.803 cash + 21.000 sale proceeds - 42.000 withdrawals = -7.197bn.** The observed balance was approximately -0.958bn, a $6.239bn difference. This is an unexplained reconciliation residual, not an inferred actual borrowing. Different dates/entities, intervening runoff, cash movements and funding explain why a December consolidated snapshot cannot tie a March bank cash figure exactly. No balancing plug was inserted.

Even selling all AFS available under the opening $0.530bn cost-based pledge approximation leaves a first-day shortfall. HTM borrowing cases assume capacity of $5bn, $10bn or $20bn, enough unencumbered eligible HTM, a 5% haircut, no funding interest and no outage. A higher cash-buffer target schedules borrowing on day 1: one-day lag settles March 8, three-day lag settles March 10. These are prearranged funding counterfactuals. They test capacity and settlement lag, not actual SVB facility access. The three-day delayed $10bn case cannot supply cash before the March 9 breach. Do not infer that generic HTM collateral value could have been monetized within actual payment-system deadlines.

## Sale-loss comparison

The sale uses year-end AFS cost/value allocation and treats the rounded $21bn sale size as proceeds. Native realized pre-tax loss is about $2.040bn. The actual sold subset and March 8 market marks were different.

| Assumed tax benefit | Proxy after-tax loss | Announced after-tax loss | Model minus announced |
|---|---:|---:|---:|
| 26% | 1.510 | 1.800 | -0.290 |
| 27% | 1.490 | 1.800 | -0.310 |
| 28% | 1.469 | 1.800 | -0.331 |

The 26-28% range comes from March 8 tax guidance and is only an external sensitivity, not an exact sale tax rate. No immediate tax refund is added to simulated cash. Native realization reclassifies existing AFS OCI into earnings without charging total equity twice. This table applies the tax rate outside the ledger to gross AFS OCI of -$2.533bn; the next section carries deferred tax inside the ledger and bridges it to reported AOCI of -$1.911bn.

## Tax, AOCI and CET1 bridge

The same sale replay, with the engine's `deferred-tax-aoci-1` policy at each guided tax rate. AFS OCI is carried net of a deferred tax asset, which is reclassified out of disclosed other assets so total assets are unchanged. SVBFG had elected to exclude AOCI from regulatory capital, so the CET1 proxy excludes it.

| Tax rate | Gross AFS OCI | Deferred tax | Net AFS OCI | Reported AOCI | Residual other components |
|---:|---:|---:|---:|---:|---:|
| 26% | -2.533 | 0.659 | -1.874 | -1.911 | -0.037 |
| 27% | -2.533 | 0.684 | -1.849 | -1.911 | -0.062 |
| 28% | -2.533 | 0.709 | -1.824 | -1.911 | -0.087 |

Tax-effecting the AFS mark explains most of the gap between gross AFS OCI and reported AOCI: the unexplained remainder is 0.037 to 0.087bn instead of 0.622bn. That remainder is other AOCI components and the true effective rate, which need the disclosed AOCI note; it is carried as an explicit residual, not fitted.

| Tax rate | Realized pre-tax loss | Tax benefit (receivable) | After-tax loss | Announced | Total equity after sale | CET1 change on sale |
|---:|---:|---:|---:|---:|---:|---:|
| 26% | 2.040 | 0.531 | 1.510 | 1.800 | 16.295 | -1.510 |
| 27% | 2.040 | 0.551 | 1.490 | 1.800 | 16.295 | -1.490 |
| 28% | 2.040 | 0.571 | 1.469 | 1.800 | 16.295 | -1.469 |

Selling securities whose loss was already in AOCI leaves total equity unchanged and creates no cash from the tax benefit, which is a receivable. Because AOCI was excluded from regulatory capital, the sale moves the after-tax loss into CET1: that is the capital cost of realizing it. The after-tax loss is the same proxy as the sale-loss comparison above, now produced by the ledger; it remains about 0.3bn below the announced figure because the actual sold lots are unknown.

CET1 from identifiable components: total equity 16.295 less preferred 3.646, noncontrolling interests 0.291, goodwill 0.375 and other intangibles 0.136, with reported AOCI of -1.911 excluded, gives 13.758 against reported 13.697: a residual of 0.061bn (12.11% against 12.05% of RWA). **The residual is unexplained regulatory adjustments; it is reported, not plugged.** The AOCI election is taken from the Federal Reserve review and was not re-read in the environment that produced this record.

## HTM sale accounting counterfactual

SVB did not sell HTM securities. This asks what the books would have shown had it sold some for liquidity: all free AFS is sold first, then $5bn of HTM proceeds at the disclosed year-end fair value. A liquidity sale is not one of the ASC 320 exceptions, so under the research `asc320-htm-v1` rule with zero materiality the rest of the portfolio is reclassified to AFS that day and its unrealized loss enters AOCI.

| Treatment | Qualification | Realized HTM loss | Reclassified to AOCI | Total equity after | Tangible common equity after |
|---|---|---:|---:|---:|---:|
| asc320-htm-v1 | non_exception | 0.995 | -14.157 | 1.143 | -3.272 |
| none | not_assessed | 0.995 | 0.000 | 15.300 | 10.885 |

With the rule selected, the realized loss and the reclassified remainder add to the whole $15.152bn opening HTM gap, recognized once: total equity falls from 16.295 to 1.143. Without a selected policy the same sale is reported as not assessed and only the $0.995bn realized loss reaches equity. Cash is identical in both (2.389 on March 9): the accounting changes equity, not liquidity. **This is a mechanical consequence of an assumed sale, pre-tax. It is not a finding about what SVB's auditors would have required**; management intent, auditor judgment, the tainting period and regulatory capital treatment of the resulting AOCI are not modelled.

## Legal-entity cash counterfactual

The opening balance sheet is consolidated. These runs assume part of the consolidated cash sits at the parent, outside the bank, and replay the March 8 sale and March 9 withdrawals at the bank. **The parent cash amounts are an assumed sweep, not the disclosed parent-only balance**, and every position and borrowing stays in one bank-and-subsidiaries account.

| Case | Assumed parent cash | Bank March 9 cash | Parent March 9 cash | Consolidated March 9 cash | Bank March 10 cash | Transfer day |
|---|---:|---:|---:|---:|---:|---|
| entity_1000_trapped | 1.000 | -8.197 | 1.000 | -7.197 | -8.197 | none |
| entity_2500_trapped | 2.500 | -9.697 | 2.500 | -7.197 | -9.697 | none |
| entity_5000_trapped | 5.000 | -12.197 | 5.000 | -7.197 | -12.197 | none |
| entity_2500_prompt_transfer | 2.500 | -7.197 | 0.000 | -7.197 | -7.197 | 2 |
| entity_2500_late_approval | 2.500 | -9.697 | 2.500 | -7.197 | -7.197 | 4 |

Consolidated cash is -7.197bn in every case; the split only decides where the shortfall lands. Cash left at the parent deepens the bank shortfall one for one. A transfer scheduled before the event covers it; one that waits for approval until March 10 (day 4) arrives after the March 9 breach. No transfer is inferred from common ownership. This moves the model further from the observed bank cash of about -0.958bn, not closer: the consolidated snapshot already overstates the shortfall, so trapped parent cash cannot explain the residual. The bank-only balance sheet and roll-forward remain the missing inputs.

## Regulatory comparison

SVB was not subject to LCR/NSFR at failure. The Fed retrospectively estimated reduced LCR of 103.1% at December 30 and 102.5% at February 28. Those ratios did not establish resilience to an hours-long run. The reduced/full LCR distinction and legal applicability matter. This reconstruction does not claim to reproduce LCR or NSFR without the relevant flow, encumbrance and funding-detail data. SVB was not a GSIB; GSIB TLAC/SLR requirements must not be applied indiscriminately.

Every account in all 20 runs declares LCR and NSFR `not_applicable`, and every scenario reports that status. No proxy ratio is computed, so none can be read as a pass. The engine refuses a `regulatory` label for its research proxies, and reproducing the Fed estimates would need its HQLA, outflow and inflow components, which are not transcribed here.

## Validation and remaining gaps

All 37 explicit reconstruction checks passed. Each run persisted Parquet, checked its hashes and schema, independently replayed the journal and reconciled the native closing ledger. Cash-breach scenarios intentionally have `dynamic_validated=false`; that is a stress result, not failed journal validation. Native binary and engine source identities, inputs, timings and artifact paths are retained in the JSON companion.

The first [reconstruction](2026-10-01-svb-reconstruction.md) and its record are kept unchanged. The original ten runs reproduce it exactly; declaring LCR/NSFR applicability changes only their status labels. The HTM and legal-entity runs are new counterfactuals with labelled assumptions, checked against values computed in the script from the opening facts.

Reconciliation is strong; sale-loss magnitude is approximate; bank cash tie is materially incomplete; run probability and timing are unvalidated. The HTM and legal-entity counterfactuals show mechanisms and do not narrow any residual. No solver was invoked because this is a historical reconstruction with specified management actions, not an optimized hindsight strategy. No API database, saved user portfolio or production state was changed.

Prioritized next requirements:

1. Bank-only December and February/March opening trial balances, actual deposit and security cohorts, and a full intervening cash bridge.
2. Intraday withdrawal requests, settlement queues, payment cutoffs, collateral location/prepositioning and facility-specific execution capacity.
3. Separate ordinary deposit decay from confidence-driven correlated jumps; estimate parameters on pre-event data and test across held-out bank events.
4. Historical security-level repricing with observed curves, spreads, prepayment/extension and hedge positions; preserve fixed-OAS scenario contracts.
5. Deferred-tax and AOCI-to-CET1 bridges; the disclosed parent-only balances and dividend restrictions; the Fed LCR components.

## Reproduce

```powershell
uv run --project apps/api python scripts/reconstruct_svb.py
```

Reference facts and assumptions: `examples/svb-2022/reference.json`. Each run gets fresh immutable artifact directories. The default script runs offline from the reviewed facts; it does not silently refetch or revise disclosures.

## Primary sources

- [10k](https://www.sec.gov/Archives/edgar/data/719739/000071973923000021/sivb-20221231.htm) — Consolidated Balance Sheets; Note 22 Regulatory Matters; fair value disclosures. opening observations, available before event.
- [march8](https://s201.q4cdn.com/589201576/files/doc_downloads/2023/03/Q1-2023-Mid-Quarter-Update-vFINAL3-030823.pdf) — slides 6 and 18. outcome comparison and explicitly conditioned sale replay only.
- [fed_evolution](https://www.federalreserve.gov/publications/2023-April-SVB-Evolution-of-Silicon-Valley-Bank.htm) — SVBFG balance-sheet growth, uninsured funding, HTM duration and SVB failure. retrospective source; 94% uninsured and 6.2-year duration describe year-end exposures, not independently calibrated behavioral parameters.
- [dfpi](https://dfpi.ca.gov/wp-content/uploads/sites/337/2023/03/DFPI-Orders-Silicon-Valley-Bank-03102023.pdf) — possession order: March 9 withdrawals and closing cash. held-out cash outcome; withdrawals also used in separately labeled conditional replay.
- [fed_regulation](https://www.federalreserve.gov/publications/2023-April-SVB-Federal-Reserve-Regulation.htm) — Table 11 and capital/liquidity applicability. retrospective regulatory context, not simulation calibration.
