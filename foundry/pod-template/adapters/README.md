# Adapters

Bank-specific mapping only. No financial logic and no core changes.

| File | Written by | Purpose |
|---|---|---|
| `<product>.cohort-config.json` | adapter-mapping | Tape `column_map`, explicit `defaults`, dimensions, assumptions (format: `docs/tape-cohort-workflow.md`) |
| `assumptions.md` | adapter-mapping | Every explicit default, its reason and the approver at G2 |
| `gl-account-map.csv` | gl-reconciliation | GL account → book, entity, currency, accounting category, sign |

`gl-account-map.csv` columns:

```
gl_account,description,book,entity,currency,accounting_category,sign,in_scope,reason
```

`sign` is `1` or `-1`, applied once to the trial-balance amount. Rows with
`in_scope=false` must give a `reason`.
