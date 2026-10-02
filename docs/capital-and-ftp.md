# Capital and funds transfer pricing

Implemented in 0.29.2, with source bridges in 0.29.3. This framework calculates ratios and management profitability
from explicit bank inputs. It does **not** certify regulatory capital eligibility,
generate a GSIB regulatory filing, or replace the daily ledger acceptance gate.

## Coverage

| Measure | Numerator | Denominator | Status |
|---|---|---|---|
| CET1 | Common equity excluding retained earnings + retained earnings + eligible AOCI − deductions | Standardized credit + market + operational RWA | Native report and optional LP limit |
| Tier 1 | CET1 + eligible additional Tier 1 | Same RWA | Native report and optional LP limit |
| Total capital | Tier 1 + eligible Tier 2 | Same RWA | Native report and optional LP limit |
| Advanced CET1/Tier 1/total | Same eligible capital | Explicit advanced RWA | Three separate native ratios/limits |
| Tier 1 leverage | Tier 1 (not CET1) | Adjusted average assets | Native report and optional LP limit |
| SLR | Tier 1 | Total leverage exposure including supplied off-balance-sheet/derivative/repo adjustments | Native report and optional LP limit |
| TLAC, risk and leverage | Eligible **total** TLAC | Standardized RWA / total leverage exposure | Two ratios/limits; no automatic eligibility/haircuts |
| LTD, risk and leverage | Eligible long-term debt | Same two denominators | Two ratios/limits; no automatic maturity eligibility |
| Tangible common equity/assets | Explicit tangible common equity | Tangible assets | Management ratio and optional LP limit |
| Minimum/buffer/management headroom | Capital less applicable ratio × exposure | Amount and basis-point presentation | Calculated separately for every configured metric |
| LCR/NSFR | Existing research HQLA/ASF | Existing NCO/RSF | Existing KPI and solver paths; not capital ratios |
| Economic profit/RAROC | Pretax business profit after FTP and expected losses | Explicit allocated capital/time | Native management report |
| SCB, CCB, CCyB, GSIB surcharge/eSLR buffer | Bank/ruleset-specific inputs | Applicable capital/exposure base | Aggregate applicable buffer supplied per ratio; not generated |
| GSIB scores, instrument eligibility, deductions, SA-CCR/CVA, market/operational RWA models, TLAC/LTD maturity haircuts | External regulatory calculation required | External | Not implemented here |

Amounts are absolute currency units; rates and ratios are decimals, not percent
points. No cross-currency summation occurs. Entities and consolidation scopes must
be supplied as distinct snapshots. Do not add holding-company and bank capital:
intercompany eliminations and capital fungibility are not inferred.

`common_equity` excludes the separately provided retained earnings and eligible
AOCI. Every input is after relevant eligibility adjustments; `eligible_tlac`
already includes all eligible instruments and must not have CET1 added again.
The report does not infer applicability: absent requirements are `unconfigured`;
missing advanced RWA or zero denominator gives `unavailable`. Neither is a pass.
The example's requirements are synthetic, **not an effective regulatory policy**.

Each entity/currency/scenario/period snapshot is independent. Supply period-end
eligible capital and exposures from a reconciled forecast or ledger; no retained
NII multiplier generates these new snapshots. The older saved-book KPI/CET1
skeleton and daily CET1-based leverage proxy remain explicitly separate. The
0.29.3 closing bridge maps posted provisions, tax, distributions and OCI from the
reconciled ledger. Regulatory deductions and eligible capital issuance remain
explicit inputs; there is no full daily regulatory capital rollforward.
Conditional published income forecasts do not automatically become credit/capital
stress scenarios.

## FTP conventions

Curves carry an immutable ID, entity, currency and an `as_of` equal to the request.
An asset is charged and a liability credited for the interpolated reference rate
at its repricing tenor plus liquidity spread at its behavioral funding tenor.
Both tenors are supplied assumptions: contractual maturity is not a substitute
for loan prepayment life or stable deposit funding life. Linear rate interpolation
uses disclosed flat endpoint extrapolation.

```
signed base transfer = side × average balance × accrual fraction × (reference + liquidity)
FTP charge = signed base transfer + option charge + contingent liquidity charge
external profit = signed external interest + fees − operating cost − expected loss
business profit = external profit − FTP charge
economic profit = business profit − allocated capital × cost of capital × accrual fraction
RAROC = business profit / allocated capital / accrual fraction
```

Side is +1 for assets and −1 for liabilities. Option spread is an explicit
nonnegative charge on both sides; contingent charge is a supplied dollar amount
for the period. Negative market rates are supported. Liquidity spreads are
supplied, not calibrated or added again to OAS. Credit/expected loss and capital
cost are separate from funding transfer pricing. RAROC is pretax and annualized;
zero allocated capital yields unavailable RAROC, not infinity. This is not a tax,
deposit replication, option valuation or contingent draw calibration model.

Treasury receives the exact opposite internal allocation. Business profit plus
treasury profit must equal external profit within each entity/currency/scenario/
period. Internal FTP never creates consolidated income, changes OAS, or posts a
new external cashflow. These are analytical management allocations, not journal
postings. Loan and cohort IDs remain in the output when supplied. Source-linked
reports retain the loan or cohort identity of the captured analytics; cohort
balances are not automatically expanded into actual loan-level analytics.

## Solver integration and stale-data protection

`strategy_units` optionally supplies the existing library grid (template, purchase
month `h`, side). `capital_deltas` specifies snapshot index, metric, scenario index,
month, and eligible numerator/exposure deltas **per dollar allocated** for each
unit. `include_management_buffer` selects minimum + regulatory buffer, optionally
plus the management margin. Rust produces `capital_limits`, accepted by
`optimize_balance_sheet(..., capital_limits=...)` and API optimization options.

For every supplied row Rust adds:

```
(required_ratio × exposure_delta − capital_delta) · allocation
    <= base_capital − required_ratio × base_exposure
```

Rows are intersected with existing funding, LCR, NSFR, EVE, CET1 and commercial
constraints. Identity/order must match the actual library; exposure increments
must be nonnegative and base exposure strictly positive. The current library is
USD only. Scenario and month are explicit indices, not inferred from report names.
Native allocation replay recomputes numerator/denominator/headroom independently
of LP matrix assembly; the reference Python optimizer performs its own replay.
No pricing is introduced into synchronous strategy evaluation.

These are supplied coefficients; neither exposure eligibility nor the sensitivity
of capital to all future actions is magically generated. Position/template edits
in a decision session with retained capital limits are rejected unless the caller
supplies changed/refreshed limits, or explicitly clears them. Constraints-only
updates remain pricing-free. Fresh limits must be generated from the revised bank
exposure model. The objective remains worst-case external NII; product FTP is not
inserted as an extra consolidated expense. A future economic-profit objective
must include treasury offsets and real operating/credit/capital costs consistently.

## Storage and operation

- Engine: `portfolio_risk.analytics.treasury.example()` and `evaluate(specification)`.
- Native protocol: `treasury-1`; no Python financial callbacks or fallback.
- API: `GET /treasury/example`; `POST /treasury/runs` with `expected_revision` and `specification`.
- UI: **Capital & FTP**. Inputs are explicit; the synthetic example is separate
  from saved-book KPIs. Results are paged and downloadable as Parquet.
- Durable worker snapshots preserve the full request, policy version, input hash
  and backend identity. Existing SQLite/PostgreSQL and local/S3 artifact adapters
  apply; optional Iceberg delivery includes the three analytical report tables.
- Bounds: 10,000 capital snapshots, 1,000 curves with 256 knots each, 60,000 FTP
  rows, 1,024 strategy units, 2,048 capital limits; native JSON limit 64 MiB and API
  treasury payload limit 32 MiB. This is bounded batch reporting, not streaming
  multi-million-position intake or a guarantee of sub-millisecond execution.

## Regulatory references checked October 1, 2026

- [Federal Reserve minimum capital requirements, 12 CFR 217.10](https://www.federalreserve.gov/frrs/regulations/section-21710-minimum-capital-requirements.htm): distinct capital numerators and leverage denominators.
- [Federal Reserve capital buffers, 12 CFR 217.11](https://www.federalreserve.gov/frrs/regulations/section-21711-capital-conservation-buffer-countercyclical-capital-buffer-amount-and-gsib-surcharge.htm): applicable buffers must reflect entity and rule scope.
- [Final eSLR modification, November 25, 2025](https://www.federalreserve.gov/newsevents/pressreleases/bcreg20251125b.htm): effective April 1, 2026, including conforming TLAC/LTD changes. Do not hard-code historical 5%/6% eSLR conventions as universal current requirements.
- [Annual large-bank capital requirements](https://www.federalreserve.gov/supervisionreg/large-bank-capital-requirements.htm): use the bank's effective capital requirements rather than the example.
- [OCC interagency FTP guidance](https://www.occ.treas.gov/news-issuances/bulletins/2016/bulletin-2016-7.html): FTP should allocate funding and contingent liquidity costs/benefits and complement broader risk management.

The source links inform the separation of concepts; they are not claims that the
synthetic inputs reproduce every provision of these rules.

## Source-linked reports (0.29.3)

In **Capital & FTP**, select Ledger closing or Captured cashflows, enter a completed
source job ID and load its template. Fill the unknown policy values, then run the
report. The template deliberately leaves regulatory amounts and eligibility
choices unfilled; it is not a regulatory policy. Downloadable audit tables explain
both the source balances and each mapping. Existing analytics jobs must be rerun
if they do not retain openings, accounting date and projection horizon.

API: `GET /treasury/sources/{job_id}/template?kind=ledger|cashflows` and
`POST /treasury/bridges` with `source_job`, `expected_revision`, `specification`.
Engine: `analytics.treasury.bridge`; native protocol: `treasury-bridge-1`.
Both run as durable jobs, with source result hash/revision receipts and immutable
policy/prepared-input artifacts. Conditional published forecasts are excluded.

### Ledger closing bridge

Compatible jobs are `balance_stress`, `saved_balance_stress` and
`streamed_balance_stress`. The bridge independently replays every persisted journal
partition before comparing its balances to the trial balance. Rust derives closing
capital and the API checks assets, liabilities and equity against source closing
statements. Corruption, missing mappings and inconsistent scopes prevent publication.

Supply one policy for every `(scenario, account)`. The policy contains a capital
snapshot, `include_aoci`, `equity_exclusions`, `intangible_assets`,
`leverage_addon`, `leverage_deductions`, `gl_risk_weights` and
`instrument_risk_weights`. Instrument weights override GL weights. Every nonzero
asset GL requires an explicit weight. `amount_multiplier` converts source ledger
amounts to absolute currency units; policy amounts already use those units.

Rust derives common equity from opening equity and distributions, retained earnings
from all P&L postings (including tax and provisions), eligible AOCI from OCI and the
policy switch, tangible equity/assets, leverage exposure, and credit RWA from signed
net carrying amounts times supplied weights. AT1, Tier 2, deductions, total eligible
TLAC, LTD, average assets, market/operational/advanced RWA and requirements remain
explicit **closing** inputs for each scenario. TLAC is not automatically refreshed
when CET1 changes. No account consolidation or intercompany elimination is inferred.
The bridge rejects negative total mapped RWA rather than clipping it into a ratio.

`capital_bridge` shows the book-to-capital reconciliation. `rwa_contributions`
shows each mapped asset GL. Net-carrying-value weights are a research exposure
mapping, not a regulatory credit-risk engine or SA-CCR/CVA implementation.
Zero-entry accounts currently require an explicit snapshot through the manual route.

The bridge tests also exposed and corrected a ledger defect: scheduled principal
repayments now release the repaid share of a loan's allowance on the same day in
Rust and Python. A final-day payoff no longer retains an allowance overnight.

Bounds: 250,000 trial-balance keys and 1,000 scenario/account policies. Journal
partitions are read sequentially; independent replay retains the closing key map.
These limits are not a whole-process memory cap.

### Captured cashflow bridge

Compatible sources are completed `cohort_analytics` jobs. Each `(book, id)` has an
explicit mapping to entity, currency, scenario and FTP curve. The source accounting
date and horizon must match. Individual repricing retains loan IDs; representative
repricing retains cohort IDs. The current tape analytics source is USD-only.

For each month, Rust calculates average principal as `(opening + closing) / 2`,
and funding tenor as the remaining principal-weighted payment life, measured from
that month's start. Payments occur at month-end. Effective-interest income uses
signed accrual interest plus book amortization. With zero supplemental fees/costs,
its external income reconciles to captured accounting NII.

If principal remains beyond the projection, `residual_tail_years` is required: all
remaining principal is assigned to that many years **after** the last captured
month. `ftp_preparation` exposes the residual amount and its share of remaining
principal. This is a disclosed tail approximation, not a fitted prepayment or
deposit-decay model. `repricing_years` is a separate reset assumption; null uses
funding life for fixed-rate assets. Supply an explicit reset for floating products;
non-maturity deposit sources reject a missing reset with `FTP_RESET_REQUIRED`.
Fees, operating/expected-loss rates, capital allocation,
capital cost, optionality and contingent charges remain policy assumptions.

All months must be present exactly once per position. Negative or excessive
principal, missing openings, incomplete mappings, date mismatches and unspecified
material tails fail closed. Maximum 120 months and 60,000 **position-month** rows;
the interactive source-template inventory is limited to 256 positions. Larger
admitted reports can submit mappings directly. No new solver objective or automatic
capital-limit coefficient generation is introduced by these bridges.
