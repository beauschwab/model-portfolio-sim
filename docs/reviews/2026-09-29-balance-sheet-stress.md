# Balance-sheet stress implementation and validation

Date: 2026-09-29. Engine: 0.22.0. Specification: `balance-stress-1`.

**Follow-up review:** [Ledger completeness review](2026-09-29-ledger-completeness-review.md)
identifies two reproduced defects (management-action tax ordering and pledged
collateral maturity), plus the remaining journal and saved-book integration gaps.
The passing tests below do not establish a complete bank ledger. Those defects
remain unresolved as of the follow-up review.

Implemented a separate daily cohort simulation, API worker operation and
**Balance-sheet Stress** workspace panel. It connects liquidity, credit,
accounting and capital through a reconciled ledger. The complete model contract,
equations, input example and coverage matrix are in
[balance-sheet-stress.md](../balance-sheet-stress.md). The
[HTML validation guide](../model-validation-guide.html#balance-stress) includes
the new workflow as chapter 25.

## Delivered behavior

- Explicit entity/currency accounts with supplied opening equity and daily
  reconciliation; separate interest accrual and cash settlement.
- Deposit sensitivity cohorts, commitments, default/migration/provision/recovery,
  wholesale maturities and rollover, current exposure/RWA/CET1/leverage paths.
- AFS/trading/HTM accounting, AFS sale reclassification, collateral eligibility,
  haircut/encumbrance limits, derivative netting-set losses and lagged posted VM.
- Non-anticipative policies for security sales, secured funding, transfers and
  dividend suspension, with execution lags/costs and explicit HTM permissions.
- Baseline, four joint scenarios, sampled reverse stress, local first breaches,
  cash/equity attribution, monthly cohort exposures and CSV/Parquet exports.
- Strict versioned input validation, immutable queued specifications, existing
  revision checks, durable SQLite/PostgreSQL worker execution and Parquet codec.

The existing pricing/CRN/fixed-OAS contracts, conditional forecast boundary and
coefficient-only strategy evaluation remain intact. There is no automatic mapping
from Book Editor positions or Strategy Lab allocations into the stress specification.
That mapping requires explicit entity, currency, accounting, credit and collateral
data rather than inferred defaults.

## Executed validation

| Check | Result |
|---|---|
| Full Python engine suite | **130 passed**, 94.08 seconds; includes 27 new stress cases |
| Full API/storage suite | **92 passed**, 81.20 seconds; SQLite and actual temporary PostgreSQL 17.6 |
| New browser workflow | **3 passed**; real scenario run, filters, incoming transfer visibility, CSV export, stale-result suppression, invalid specification, narrow panel |
| Web TypeScript/production build | Passed; existing bundle-size advisory remains |
| HTML guide structure/arithmetic | Passed; 25 chapters, 13 panels, no duplicate IDs or broken local links |
| HTML guide desktop/mobile/print | Passed; no script errors, remote dependencies or document overflow |
| Guide architecture diagrams | Passed; accessible labels, links, mobile scrolling and print layout |
| Operator skill archive | Repackaged and ZIP integrity checked |
| Git whitespace check | Passed |

The API suite includes a new separate-process test: queue while the worker is
absent, restart the API, start the worker, recover the queued job, verify the
original specification, query summary rows through DuckDB and download real
Parquet. This runs against both SQLite and PostgreSQL. S3 remains SDK-stub-tested
in the existing suite; no live S3 deployment or Iceberg catalog was exercised.

Review caught and fixed an unintended baseline wrong-way-risk multiplier and
corrected margin settlement to use historical targets rather than today's target
with a nominal delay. Tests independently reproduce AFS sale and maturity
reclassification, allowance/default/recovery identities, interest settlement,
transfer floors, collateral conservation, outage timing, zero severity and
pre-shock non-anticipation. Attribution totals reconcile to stressed-minus-base
cash and equity outcomes.

## Synthetic run evidence

The supplied example has two USD accounts, seven position cohorts, one netting
set, four management policies and a 360-day horizon. It evaluates baseline plus
four scenarios and twenty additional severity runs. One local in-process timing
was **0.258 seconds**, excluding interpreter imports, API queuing, serialization
and artifact publication. This is a small deterministic cohort example, not a
60,000-instrument pricing or production scaling benchmark.

Amounts below are **synthetic USD millions**. Shortfall is relative to the
configured cash buffer, not only to zero cash.

| Scenario | Account | First cash-buffer breach | Peak buffer shortfall | Final equity |
|---|---|---:|---:|---:|
| Baseline | Bank | None | 0.00 | 111.95 |
| Baseline | Dealer | None | 0.00 | 29.97 |
| Rates and competition | Bank | Day 123 | 167.39 | 93.44 |
| Rates and competition | Dealer | Day 7 | 2.08 | 26.40 |
| Recession credit | Bank | None | 0.00 | 72.26 |
| Recession credit | Dealer | None | 0.00 | 28.30 |
| Dealer margin | Bank | None | 0.00 | 92.42 |
| Dealer margin | Dealer | Day 7 | 61.08 | -3.91 |
| Confidence/outage | Bank | Day 8 | 717.33 | 123.55 |
| Confidence/outage | Dealer | Day 7 | 84.08 | 18.44 |

Maximum daily reconciliation residual was below `1.7e-11` in the example's amount
units. The confidence scenario illustrates why positive book equity does not imply
liquidity survival. Projections continue diagnostically after failure, so its final
equity is not a claim that a failing bank could execute the remaining cashflows.

## Limits and remaining integrations

This is a working **research prototype**, not completion of all production G-SIB
requirements. It uses stylized deterministic credit and deposit assumptions,
duration-based security marks, simplified netting-set/VM dynamics and user-weighted
regulatory proxies. It does not implement calibrated joint physical-measure
scenarios, full CECL/IFRS9, supervisory capital mappings, consolidated FX/capital,
intraday liquidity, full dealer/XVA/closeout, loan-level contractual schedules,
policy optimization or automatic replay of Strategy Lab allocations.

The next useful integration is a validated instrument/strategy-to-cohort adapter
that rejects missing risk metadata, followed by calibrated joint drivers and
contractual flow adapters. Do not silently promote conditional NII forecasts into
pricing or credit/capital probabilities. The new module does not justify a Rust
rewrite; profile representative calibrated workloads before choosing kernels to
move.

To use the feature, restart existing API/worker processes so they import the new
operation, open **Balance-sheet Stress**, inspect/edit the synthetic specification,
then select **Run balance-sheet stress**. Work is local and uncommitted; no
deployment, merge, live-bank calibration or production validation was performed.
