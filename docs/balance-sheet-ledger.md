# Balance-sheet ledger and replay — 0.26.0

This release adds double-entry journals, independent reconstruction of closing
statements, saved-book cashflow adapters, dynamic candidate replay, calibration
diagnostics and backtesting. It remains a deterministic research simulator.
Accounting consistency does not establish empirical or regulatory validity.

## Execution and schema

The UI's synthetic example uses `balance-stress-2`. Version 1 submissions are
rejected: explicitly migrate the version, review the tax/collateral changes and
rerun. Historical persisted results remain historical artifacts.

- `GET /balance-stress/example`: example, typed contract and workspace revision.
- `POST /balance-stress/run`: explicit `specification` and `expected_revision`.
- `GET /balance-stress/inventory`: actual saved IDs and required mapping fields.
- `POST /balance-stress/saved-book`: `specification`, `position_mapping`, positive
  `amount_scale`, optional `allocation`, `template_mapping`, `observations`,
  `calibration`, and `expected_revision`.

Use **Load saved-book mapping** to generate the input skeleton. Complete every
null mapping and supply independently reconciled opening accounts. Mapping keys
are `book:id`; MBS uses its CUSIP as ID. Mandatory fields are account, kind,
classification, risk_weight, asf_weight, rsf_weight, lcr_outflow_weight and
hqla_weight. Additional credit/collateral assumptions use Position fields.
`amount_scale=0.000001` converts book dollars and candidate notionals into millions.
Never infer currency or legal/regulatory status from instrument names.

Saved-book requests must leave positions/cashflows empty; the worker creates
them from the queued immutable books. All heavy work stays in `app.worker`.
SQLite/PostgreSQL share the durable operation. Journals and other tables use the
existing immutable Parquet local/S3 codec and Arrow response format. No second
mutable book store or Iceberg catalog implementation was added.

## Journal, subledgers and collateral

Every event balances debit/credit entries within an entity/currency account.
`journal` identifies transaction, day, scenario, account, event, GL account,
instrument, debit and credit. `trial_balance` uses debit-positive signs. GL
accounts separate cash, restricted cash, principal, premium/discount basis,
accrued interest, allowances, recoveries, derivative value, margins, secured
funding, intercompany balances, opening equity, distributions, OCI and P&L.

Every daily materialized subledger is checked against journal balances. An
independent replay of journal lines reconstructs the closing balances and rejects
unbalanced transactions. `closing_statements` derives assets/liabilities/equity
from the GL. `consolidated` eliminates intercompany balances within each currency;
it is not FX-translated group capital. `ledger` remains the compact event bridge.
Arithmetic is floating point with scale-aware tolerances, not a cent-rounded
production general ledger with close/approval/reversal controls.

Taxes post **after** management actions, including realized security gains.
Dividends are distributions and are excluded from taxable-income deductions.
Tax remains immediate tax on positive daily earnings; deferred tax, tax loss
carryforwards and jurisdictional rules are not implemented.

Secured policies create explicit claims linking pledged face, debt, rate and
due date (`funding_tenor_days`, default 30). Funding maturity releases collateral.
Collateral principal proportionally repays linked funding; collateral maturity
extinguishes claims. Excess proceeds become free cash only after repayment.
Opening funding/repo links use `collateral_position` and `pledged_face`; total
links cannot exceed encumbered face. Unidentified opening pledges remain
restricted cash at maturity. No collateral substitution/default waterfall exists.

## Cashflow integration

`run_balance_sheet_nii(..., capture_cashflows=True)` emits opening principal and
basis, principal repayments, coupon cash, interest accrual and book amortization
per instrument. The saved-book adapter supports MBS, loans, debt, deposits, CDs
and MM through the existing product engines. Corporate/CD coupon cash uses the
payment month while accrual uses the existing monthly smearing convention.

The adapter maps months to day 30/60/etc. **This is not exact daily contractual
timing.** MM retains constant-balance rollover. Product cashflows use base
expected paths plus daily stress overlays; scenario-specific behavioral product
paths are not rebuilt. No published conditional forecast enters pricing or credit
calibration. Existing fixed-OAS/CRN contracts are unchanged.

Explicit dated flows override built-in interest/bullet maturity. Supply `all`
or complete baseline-plus-scenario schedules, never overlapping schedules.
Defaults, withdrawals and sales condition future flows proportionally on
surviving principal. This is an explicit approximation, not a joint stochastic
default/prepayment model. Future draws require separate scheduled positions.
Defaulted accrued interest is written off; sales separately settle accrued interest.

AC/HTM and AFS/trading premium/discount amortization are supported. For securities,
`opening_market_price` is the clean quote per unit principal (1.05 means 105% of
par; default 1). Cost is principal plus `book_adjustment`; marked carrying value
is principal times the quote. Opening AFS OCI is fair value minus cost and is
already part of supplied opening equity. It is not new period income. Trading
opening value is included in opening equity without AFS OCI. Amortization changes
cost; the offsetting fair-value adjustment runs through OCI for AFS and earnings
for trading. Sales allocate basis and reclassify only the sold AFS OCI. Redemption
removes principal, basis and fair-value adjustment without duplicating losses.
Forward purchases pay cost before marking to their supplied quote.

Saved-book capture supplies both basis and the engine's target clean quote;
mapping overrides cannot replace that quote. AFS/trading saved-book mappings
require it. Stress marks remain a duration approximation relative to the opening
quote, not a scenario-specific full product repricing. Saved hedge trades are rejected
pending a trade-to-netting-set valuation/settlement adapter; they are never
silently omitted. Explicit netting sets retain supplied FV/stress-loss and
posted-VM behavior, not full dealer/XVA pricing.

## Shared limits and candidate replay

Ruleset `research-weights-v2` centralizes daily headroom and acceptance checks.
Cash, CET1, leverage, LCR, NSFR and HTM limits affect breaches, reverse stress and
candidate validation, including day zero. LCR/NSFR floors default to zero
(disabled), HTM asset limit to one; set internal limits explicitly.

Daily LCR and legacy KPIs share the 40% Level 2A composition and 75% inflow caps.
Set `hqla_level` to `level1` or `level2a`; supply weights and inflow/outflow
assumptions. This unifies arithmetic, not all jurisdictional classifications.
NSFR weights, capital deductions, 100% commitment CCF and watch uplift remain
research proxies. The older optimizer retains its static capital approximation.

Paste `{template,purchase_m,notional}` allocations into a saved-book request and
supply explicit `template_mapping`. Exact requested grid units originate against
cash at `purchase_m*30+1`; actual unit-engine monthly cash/accrual outputs enter
the journal. Existing deterministic forward coupon/time-shifting assumptions
remain. This is candidate replay, not nonlinear re-optimization or trade execution.

`/strategy/eval` remains coefficient-only. Legacy optimizer `validated=true`
means coefficient replay; new fields state that scope and
`dynamic_validation_status=not_run`. A separate replay returns
`validation.dynamic_validated`, ruleset, breach count, queued revision, allocation
and specification SHA-256. It validates the submitted candidate, without
authenticating its claimed solver provenance. A failure does not automatically
generate optimization cuts.

## Calibration, backtesting and bounds

`observations` rows require scenario/account/day/metric/actual/source. Supported
metrics: cash, equity, assets, liabilities, CET1 and RWA. `backtest` reports actual,
predicted, error and absolute error. Duplicate/nonfinite/unmatched observations
are rejected; comparison is limited to retained reporting dates.

Optional `calibration` supplies history, ISO training_end and source. History
columns are date, rate_shift, spread_shift, deposit_flight and market_shock.
At least 30 training and 10 chronological holdout observations are required.
Joint means/covariances use training rows only. Outputs include source hash,
holdout bias/RMSE/exceedance and suggested observed joint scenarios. Holdout rows
cannot affect fitting or scenario selection. Suggestions require a subsequent
explicit specification edit. This does not fit PD/LGD, deposit responses or tail
probabilities and never sets production validation true. No real-bank dataset
has been supplied or calibrated in this implementation.

Bounds remain 50 accounts, 2,000 positions, 500 netting sets, 100 policies,
8 scenarios, 12 reverse severities, 30–1,080 days and a 3,000,000 work-unit budget;
dated cashflows are capped at 250,000. Daily checks cover every date; reporting
paths retain daily values through day 30, then monthly values. Journal retention
can dominate memory. Cohort aggregation must be explicit and preserve mappings.

Reproduce the measured full-journal benchmark:

```powershell
uv run --project apps/api --with psutil python scripts/benchmark_balance_ledger.py
```

See [measurements](reviews/2026-09-29-ledger-benchmark.json) and
[validation report](reviews/2026-09-29-ledger-implementation.md). This is distinct
from the earlier 60,000-instrument pricing benchmark.

## Remaining production scope

Exact daily calendars and scenario-dependent product cashflows;
hedge/netting adapters; received/initial margin, closeout and XVA; FX translation
and consolidated regulatory capital; tax/hedge accounting policy; empirically
fitted behaviors and reviewed jurisdictional mappings; nonlinear policy search;
distributed journal output and 60,000-instrument full-ledger validation remain
unfinished. These gaps are not covered by passing accounting or storage tests.

See the [Rust decision and 0.24 validation report](reviews/2026-09-29-rust-ledger-decision.md)
for measured hotspots, current Python optimizations and gates for a native ledger pilot.

## Optional native journal pilot

Engine callers can use `run_balance_stress(spec, journal_backend='rust')` after
running `scripts/build_ledger_native.py`. The explicit reference `python` journal retains the original
journal; `columnar` is a Python control sharing Rust's compact data layout. The
API/UI still use the default. This is a batched journal reduction/checkpoint
kernel, not a Rust product engine or Rust policy/state machine. Python creates
financial events and derives risk limits; Rust validates ordered daily postings,
updates GL balances and replays the journal at close. No per-instrument native
calls or retained native pointers are used.

`execution` adds backend identity and the binary SHA-256 to engine results.
Requested native backends never silently fall back. Failed checkpoints abort
before result return and do not publish partial GL state. Independent Python
replay validates native-produced journals in tests; a native run itself uses
native closing replay. Both pilots retain the complete journal in memory and
preserve current request limits. See the [measured pilot report](reviews/2026-09-29-native-ledger-pilot.md).

## Full native daily state with partitioned output

`analytics.balance_stream.run_streamed_balance_stress(spec, directory,
backend='rust')` runs the full daily research event/state loop in a Rust child
process. `backend='python'` runs the independent reference into the same sink.
The Rust loop covers the existing position, credit, collateral, netting, margin,
policy, tax and daily limit semantics; it does not regenerate product Monte Carlo
paths or expand the financial-model coverage described above.

The result is a path to an immutable local manifest, not a fully materialized
dictionary of tables. Journal and reporting partitions carry ordered row counts,
schemas and SHA-256 hashes. Independent Python replay reads the saved journal
partitions, checks transaction sequence/balance across partition boundaries and
reconciles every closing GL key. Closing statements and attribution are derived
before publication. The manifest records source/binary identities, input snapshot,
capacity tier and validation scope. A fresh hidden attempt directory is renamed
only after verification; failed attempts are removed on handled exceptions.
Process termination may leave an unreferenced hidden attempt, never a returned
published manifest. There is no checkpoint-resume implementation.

Use `load_streamed_result(manifest)` only for small-run analysis. Large consumers
should scan manifest-listed Parquet partitions rather than materializing the
complete journal. The runner writes at most 65,536 rows per Parquet partition;
native transport batches at most 16,384 rows. State and reporting rows still scale
with portfolio/event counts, so partitioning is not a constant-memory guarantee.

`large_book=True` explicitly admits up to 60,000 positions / 90 million work units
in this engine-only runner. API requests and existing workers retain their earlier
limits; the historical Python default is superseded by the 0.29.4 Rust production contract. Local manifest publication does not substitute
for worker lease/revision/attempt fencing. S3/Iceberg integration and durable job
admission for this backend remain separate work. See the
[state-engine and streaming validation report](reviews/2026-09-29-native-state-streaming.md).

## Scheduled loan repayment allowance (0.29.3)

Daily credit provisioning occurs before contractual cashflows. When scheduled loan
principal is repaid, both native and reference runners now release the corresponding
share of that day's allowance immediately through `repayment_allowance_release`.
The entry debits the allowance and credits earnings; it does not generate cash.
This also handles full payoff on the closing day, so no stale allowance survives
until the next day's credit step. Independent journal replay and Python/Rust
closing-balance parity gate the change.
