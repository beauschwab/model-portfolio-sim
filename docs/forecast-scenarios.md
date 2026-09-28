# Published forecasts and conditional balance-sheet income

The Market & Scenarios panel can fetch public forecast snapshots, preview their
monthly drivers, and run conditional monthly NII and runoff against the current
book. Source downloads never change the active curve or saved positions.

## Sources implemented

| Dataset | Download and selected fields | Executable selection |
|---|---|---|
| `fed_stress` | [Fed final domestic baseline/adverse CSVs and historical jump-off data](https://www.federalreserve.gov/supervisionreg/dfa-stress-tests-2026.htm). All published columns retained. | Baseline and severely adverse; 3m/5y/10y rates, mortgage rate and HPI drive the run. |
| `fed_sep` | [Fed SEP accessible tables](https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm). Medians and central-tendency/range endpoints for policy rate, GDP, unemployment, PCE and core PCE. | Published policy-rate median. Other macro variables and ranges are reference-only. |
| `philly_spf` | [Philadelphia Fed median-level workbook](https://www.philadelphiafed.org/surveys-and-data/real-time-data-research/median-forecasts). Five quarterly forecasts of Treasury bills, 10y Treasuries, Aaa/Baa yields and unemployment. | Median 3m and 10y rate paths. Corporate yields/unemployment are reference-only. |
| `nyfed_sme` | [New York Fed SME workbooks](https://www.newyorkfed.org/markets/market-intelligence/survey-of-market-expectations). Combined panel policy-rate median and 25th/75th percentiles. | Policy-rate median. Quartiles are disagreement statistics, not probabilities or joint scenarios. |

SEP reads the rounded median table, not exact individual dots. Longer-run values
are retained without assigning a future calendar year. No attempt is made to
connect anonymous dots into individual trajectories. SPF currently imports
rates/unemployment, not its GDP, inflation, individual-respondent or probability
workbooks. SME currently imports policy-rate paths, not every survey question.

Fannie housing forecasts, CBO, OECD, IMF, BoE/EBA and NGFS remain research
candidates; no adapter for those forecasts is claimed here.

## Workflow

1. Open **Market & Scenarios → Research market data**.
2. Choose a source and cutoff date, then **Fetch snapshot**. Existing snapshots
   can be selected directly.
3. Choose a published scenario, first source month and projection length.
4. **Preview forecast** shows driver values, source coverage, flat-tail months,
   unused variables and the modeling assumptions.
5. **Run conditional forecast** compares monthly NII with the base case and
   provides a result download containing monthly NII, base NII, differences,
   runoff, driver targets, source checksums and captured input revision.

The executable alignment is explicitly **relative replay**: the selected source
month maps to projection month one for the existing book. Contractual schedules
still start at the saved book valuation date. This allows a historical stress
narrative to be applied to today's research book without pretending to age the
portfolio or reconstruct the balance sheet at the source date. It is not a
point-in-time backtest. Input edits invalidate a preview; the server also rejects
stale revisions before queueing. Jobs use a frozen state on the existing worker.

## Publication and units

- SEP discovers dated published HTML releases and selects the latest release no
  later than the requested cutoff; future releases and comparison rows from the
  prior SEP are excluded. Archived pages may still have later corrections.
- SPF uses the current workbook update date and refuses a cutoff before it.
  Column `*1` is lagged actual; `*2` through `*6` are the current and next four
  quarters. Historical worksheet rows are not assumed to be unrevised vintages.
- SME's `survey_release_date` is the questionnaire date. It is **not** the public
  results date. Selection uses a conservative availability gate: the first day
  of the second month after the survey month. This intentionally delays access
  and does not prove exact historical publication/revision availability. Only
  workbook links present on the official page are eligible. SME workbook rate
  values are decimals even though questionnaire text describes percentage entry.
- Fed stress chooses an annual final package. Where the dated final report URL
  supplies its release date, requests before that date are rejected. Legacy
  packages lacking that date retain the explicit publication-availability warning.
  Historical CSV values can contain estimated jump-off observations; their
  `observed` classification denotes the source history, not audited realized data.
- Snapshot observations retain native percent/decimal/index units and period
  conventions. Forecasts are classified `assumed`. The engine derives decimal
  monthly driver targets; those targets are not newly observed market data.

## Engine contract and limitations

`analytics.forecast.compile_forecast` constructs driver targets;
`condition_paths` applies them to the existing Monte Carlo paths;
`run_forecast_nii` runs base and conditional cashflows with one shared CRN object.

- Quarterly average rates are repeated in all three monthly slots, preserving
  the published average. Year-end and dated rate endpoints are interpolated
  linearly by month; within-month meeting-day timing is not modeled. The first
  endpoint is carried backward if no earlier value exists.
- HPI is log-interpolated between quarter ends and rebased at replay month zero.
  A preceding historical/scenario anchor is mandatory. The Fed history provides
  the jump-off. Old snapshots without history must be refetched to replay their
  first quarter. Mortgage incentives retain the existing two-month lag.
- Source Treasury/policy rates proxy model rates with **zero basis**. Multiple
  source maturities are interpolated linearly in tenor, with flat extrapolation;
  a policy-only source preserves the initial model curve's tenor slope. This is
  not an exact bootstrapped SOFR curve. Missing mortgage/HPI paths retain the
  existing model outputs, so they do not acquire additional source-driven rate
  feedback in policy-only runs.
- Shifted rate deviations are rescaled to the monthly target means while keeping
  common random draws and the -2% rate floor. Mortgage means are shifted and HPI
  paths scaled. This conditional construction is **not an arbitrage-free pricing
  measure**, nor a fitted real-world distribution. It must not be used for EVE,
  OAS, derivative valuation or probabilistic loss estimates.
- After each driver's final source anchor, its level is held constant through
  the remaining engine horizon. Report-period flat-tail months are displayed.
  This terminal assumption also influences remaining-life cashflow accounting.
- Fixed-rate effective yields and opening carrying values are frozen from the
  base run. Floating instruments add their contractual coupon-income change to
  the base effective-yield roll; this is an accounting approximation, not a full
  effective-interest reset/catch-up implementation. Deposit opening rates are
  held fixed; the deposit rate/attrition recursion then runs on conditional rates.
- Existing prepayment, withdrawal and exercise behavior remains active. Money
  markets retain constant balances; swaps contribute settlements; swaption
  market-value changes are not income in this report. No new origination or
  reinvestment is inferred.
- GDP, unemployment, BBB yields, equity/CRE/VIX and other unused variables remain
  visible reference inputs. There is no default/loss, provision, funding-basis,
  stressed-volatility or regulatory-capital model linked to those series.
- The existing instantaneous scenario grid, risk pricing, fixed-OAS convention
  and interactive strategy evaluation are unchanged. No OAS is solved in this
  conditional income route.

## API

`POST /market-data/fetch` accepts the dataset IDs above. Standard snapshot list,
preview and export endpoints retain raw bytes and SHA256 provenance. Snapshot
previews expose `forecast` metadata independently of observation pagination.

`POST /forecasts/preview` and `POST /forecasts/run` share this body:

```json
{
  "snapshot_id": "<saved SHA256 identifier>",
  "scenario": "baseline",
  "start_period": "2026-01-01",
  "horizon_months": 27,
  "alignment": "relative_replay",
  "expected_revision": 7
}
```

Preview returns JSON. Run returns the standard asynchronous job; its result uses
the existing Arrow envelope. Neither route writes active inputs or caches a
strategy library. No external call occurs during scenario execution.

## Verification

Offline tests cover source selection/units/embargo, future period retention,
history versus forecast columns, HPI jump-off requirements, endpoint and quarterly
semantics, zero-conditioning identities, discount-factor consistency, base input
preservation, frozen accounting, floating income response, API revision guards
and UI preview/results/stale-selection behavior.

```powershell
uv run --project apps/api python scripts/check_forecasts.py --as-of 2026-09-28 --run
```

This downloads all four live sources and runs all five supported selections on
small disposable synthetic books (32 paths, 27 months). These passed on September
28, 2026. It does not modify a running API's state. Live source integration and
synthetic numerical tests do not establish production model suitability.

The isolated market-data/forecast branch passed 74 engine/API tests, all three
market-data/forecast browser tests, and the production web build. This excludes
the unrelated native-pricing, What-If and Decision Lab work in the shared
checkout. The live four-source/five-selection smoke also passed independently
on this isolated branch. Existing framework deprecations and the web bundle-size
warning remain.
