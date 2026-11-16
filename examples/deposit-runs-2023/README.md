# Large-bank deposit panel, 2018-2023

Public FDIC Call Report data for a held-out evaluation of deposit-run signals. It is
not a depositor tape and contains nothing below quarterly frequency.

- `fdic_large_bank_panel.csv`: quarterly balances (USD thousands) for the 162
  FDIC-insured banks with at least USD 10bn of assets on 2022-12-31, from 2018-12-31
  to 2023-06-30. The universe was fixed from the 2022-12-31 report before any 2023
  row was read.
- `source.json`: the queries, index names, field meanings, retrieval date and the
  FDIC failures list for 2023.

A bank with no row for a quarter filed no Call Report for it. Failures are taken
only from the FDIC failures list. Quarters before 2022-12-31 include a bank only
once it had USD 5bn of assets.

Run from the repository root:

```bash
uv run --project apps/api python scripts/evaluate_deposit_run_signals.py
```

The script reads only these files and writes
`docs/reviews/2026-11-16-deposit-run-signals.{md,json}`.
