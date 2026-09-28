# Research market data

For published forecast adapters and conditional monthly NII/runoff execution,
see [forecast scenarios](forecast-scenarios.md). The four supported sources are
Fed supervisory stress, SEP, Philadelphia Fed SPF, and New York Fed SME.

Open **Market & Scenarios → Research market data**. Choose a source, observation
cutoff and history window, then **Fetch snapshot**. Downloads run in the existing
bounded job queue. Review the saved observations and warnings. Only Eris SOFR
discount-curve snapshots offer **Apply research curve**. Fetching other datasets
does not refit models or modify books, scenarios or assumptions.

## Available adapters

| ID | Feed | Normalization / access |
|---|---|---|
| `eris_sofr` | [Eris public files](https://files.erisfutures.com/ftp/) | Latest DF file no later than cutoff, at most seven calendar days old. Searches root and requested/prior archive month. Decimal DFs retained; engine-compatible par rates derived. |
| `eris_options` | Eris option settlement CSV | Settlement price, futures-price lognormal volatility and normal rate volatility retained with distinct units, expiry, strike and underlying. Not used as an OTC swaption surface. |
| `nyfed` | [NY Fed API](https://markets.newyorkfed.org/static/docs/markets-api.html) | SOFR, EFFR, OBFR, TGCR, BGCR; `SOFR-AVG` supplies 30/90/180-day averages and index. Rates converted percent → decimal. |
| `treasury` | [Treasury XML](https://home.treasury.gov/treasury-daily-interest-rate-xml-feed) | Nominal and real par yield histories, percent → decimal. Annual downloads, up to eleven calendar years per request. |
| `fed_zero` | [Fed GSW CSV](https://www.federalreserve.gov/data/nominal-yield-curve.htm) | Fitted `SVENY01`–`SVENY30` continuously compounded Treasury zero yields, percent → decimal. |
| `fred` | [FRED API](https://fred.stlouisfed.org/docs/api/fred/series_observations.html) | Selected series, native units and source/license notes. Requests the cutoff-date vintage. Missing observations are omitted, never replaced with zero. Requires `FRED_API_KEY`. |
| `pmms` | [Freddie Mac CSV](https://www.freddiemac.com/pmms/docs/PMMS_history.csv) | Primary 15/30-year mortgage rates, percent → decimal. Not MBS current coupons. |
| `fhfa` | [FHFA master CSV](https://www.fhfa.gov/hpi/download/monthly/hpi_master.csv) | Traditional purchase-only monthly/quarterly HPI, SA and NSA separately. `identifier` is place ID (`USA`, `CA`, etc.). Date is the first day of the observation period. |
| `fdic` | [BankFind API](https://api.fdic.gov/banks/docs) | `identifier` is certificate number (e.g. `3511`). Assets, deposits, equity, net income, interest income/expense in USD thousands; NIM in percent. Income fields retain the source reporting basis, not automatically standalone-quarter flows. |
| `fdic_rates` | [National rate tables](https://www.fdic.gov/national-rates-and-rate-caps) | The cutoff month's national deposit/CD rates (not rate caps), percent → decimal. Uses the table effective date; rejects dates before that month's publication. History-start is not used. |
| `sec` | [EDGAR companyfacts](https://www.sec.gov/search-filings/edgar-application-programming-interfaces) | `identifier` is CIK, `series` are us-gaap tags. Preserves units, period, accession and filing date; excludes filings after cutoff. Duplicate/revised period facts remain distinct. Requires `SEC_USER_AGENT`. |
| `fed_stress` | [Fed annual scenarios](https://www.federalreserve.gov/supervisionreg/dfa-stress-tests-2026.htm) | Final domestic baseline/adverse and historical jump-off CSVs. Source percent/index units retained. Explicit conditional income replay is available; never replaces instantaneous valuation scenarios. |
| `ffiec`, `pooltalk`, `freddie_loans` | Authorized extracts | Import normalized JSON using the panel or API. Registered downloads/account enrollment, raw loan-level aggregation and model fitting are not automated. |

Most feeds provide today's vintage of historical observations, not an archive
of what was known at a past date. Every affected snapshot warns about this.
FRED's explicit vintage and SEC filing dates improve point-in-time handling;
neither turns all the other sources into point-in-time history. Annual stress
packages are selected by year, not verified as published on a historical day.
SOFR fixings and compounded averages are not forward-looking CME Term SOFR.
ICE credit series on FRED have a three-year rolling history from April 2026.

## Curve interpretation

The engine still takes ten annual-unit-accrual par rates at 1, 2, 3, 4, 5, 7,
10, 15, 20 and 30 years. `core.curve.market_discount_factors_to_par` interpolates
provider DFs in log space using ACT/365F times from the file's valuation date,
then solves the annual par identity `(1-D(T))/sum(D(1)..D(T))`. This avoids
silently feeding vendor payment/day-count conventions into a different model.

Reconstructing the curve from ten pillars loses information, especially below
one year; it is **not exact import of the complete market DF grid**. Every
snapshot reports maximum absolute DF and continuously compounded zero-rate
errors over quarterly times through 30y; deviations above 5bp get an additional
warning. Beyond 30y, the existing flat-zero extrapolation remains. For the live
2026-09-25 sample, the maximum zero-rate error is about 42.71bp, primarily a
consequence of the absent sub-year pillars. Use this adapter for research with
that limitation; accurate short-end instrument valuation needs an engine curve
interface that accepts the complete discount grid.

Applying a curve retains volatility, prepayment/deposit histories, prices and
book valuation dates. The UI warns that this is research repricing of the
existing book rather than portfolio aging. Fixed-OAS scenario behavior, CRN and
prepay restart requirements are unchanged. Applying requires the expected
application revision; conflicts return 409. It invalidates strategy caches.
Queued jobs keep their captured market and include market provenance in status.
Manual market edits clear the source-snapshot association and label inputs
as assumed.

## Configuration and reproducibility

Optional API-server environment variables:

- `FRED_API_KEY`: a FRED API key; never returned to the browser or stored in
  snapshot URLs. No key is needed for the other public adapters.
- `SEC_USER_AGENT`: organization and contact identity required by SEC, e.g.
  `ResearchTeam research@example.org` (replace with your own contact).
- `MARKET_DATA_DIR`: snapshot repository directory. Default is
  `<repo>/.data/market-data`, ignored by Git.

Each download saves raw response bytes by SHA-256 and an immutable normalized
JSON snapshot with source URLs, nonsecret query parameters, request, retrieval
timestamp, units, classifications, warnings and derived curve diagnostics.
The snapshot ID hashes its serialized contents and is verified when read.
The export downloads normalized data and provenance; preserve the snapshot
repository as well if you need the original response files. Snapshots are never
silently refreshed or auto-applied. They persist across API restarts; active
in-memory market state still resets to demo on restart. There is no automatic
disk retention policy; archive or remove snapshots deliberately when desired.

Network reads have timeouts and a 40 MiB response cap, and normalized outputs
are limited to 100,000 observations. Requests choose a known source, not an
arbitrary URL. Schema changes, missing credentials, stale curves and invalid
values fail explicitly. No paid fallback or account/terms acceptance occurs.
Research access is still subject to each provider's terms.

## API examples

Fetch via `POST /market-data/fetch`:

```json
{"dataset":"eris_sofr","as_of":"2026-09-25"}
```

Poll the returned job at `GET /jobs/{id}`; its existing Arrow-envelope result
contains the snapshot summary. List snapshots at `GET /market-data/snapshots`.
Inspect `GET /market-data/snapshots/{id}?offset=0&limit=100` (max limit 1000),
or download `GET /market-data/snapshots/{id}/export`.

Apply with `PUT /market-data/active-curve`:

```json
{"snapshot_id":"<64-character snapshot hash>","expected_revision":0}
```

Use the actual revision from `GET /market`, not a fixed zero.

Registered-source import at `POST /market-data/import` (also accepted by the
file picker) takes this schema:

```json
{
  "dataset": "pooltalk",
  "as_of": "2026-09-25",
  "source_description": "Authorized PoolTalk export; pool factors aggregated by coupon",
  "observations": [
    {"date":"2026-09-25","series":"coupon-4.0:factor","value":0.92,
     "unit":"factor","classification":"observed"}
  ]
}
```

Classifications are `observed`, `derived`, or `assumed`. Imported extracts are
explicitly user-supplied; source provenance and aggregation are not independently
verified. Do not include account-level personal identifiers in cohort extracts.

## Verification

Offline source/API contracts: `bun run test:api`. Quant curve invariants:
`bun run test:engine`. UI tests: `bun run test:e2e`.

Live read-only smoke (downloads research snapshots, never applies a curve):

```powershell
uv run --project apps/api python scripts/check_market_data.py --as-of 2026-09-25
```

Use `--sources` to restrict checks. The script's one-year observation window
accommodates quarterly/lagged series. Live FRED/SEC checks require configured
credentials/identity; their transport and normalization are covered offline.

`--fdic-cert` and `--sec-cik` select the institution for those adapters.

Validated on September 28, 2026: all ten credential-free adapters returned
live observations; 69 engine tests, 47 API tests and eight browser tests passed,
and the production web build succeeded. A real Eris snapshot was applied in a
disposable TestClient process and completed a synthetic-book NII run, with
snapshot provenance attached to the job. This is integration evidence, not
validation of market-calibrated volatility, behavior or production suitability.
