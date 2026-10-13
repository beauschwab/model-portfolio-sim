# SVB year-end 2022 reconstruction

This fixture is a consolidated SVB Financial Group reference case, not a
security/depositor tape or a calibrated prediction of the March 2023 bank run.
All source amounts in `reference.json` are **USD millions**. Each observation has
a source category; assumptions and post-event comparison targets are separate.

Run from the repository root:

```powershell
uv run --project apps/api python scripts/reconstruct_svb.py
```

The runner uses the Rust daily ledger and capital engine, persists immutable
local Parquet runs, and independently replays their journals. It writes the
comparison report and machine-readable evidence under `docs/reviews/`. Existing
application portfolios and databases are untouched. Native runtimes must already
be built; there is no automatic Python fallback.

The 15 aggregate positions reconcile the published December balance sheet.
Opening valuations and eligible capital are observations, not model predictions.
Loans are frozen at net carrying value and other noncash assets use fixed-value
ledger placeholders. There is no Jan-March operating roll-forward, security-level
Monte Carlo pricing, or calibrated depositor-confidence model. The event replay
conditions on the sale and withdrawal amounts and explicitly exposes the residual
against the historical bank cash outcome. The March 10 request is not treated as
a settled historical withdrawal.

Independent assertions cover opening totals, zero-event conservation, capital
rounding, cash accounting, AFS realization without duplicate equity loss,
non-anticipation and funding settlement timing. Passing these checks validates
the reconstruction mechanics; it does not establish historical predictive fit.
