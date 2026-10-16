---
name: gl-reconciliation
version: 0.1.0
description: Reconcile Rates Workbench book balances to the bank's general-ledger trial balance. Use after books are published at gate G2, at each month-end refresh, or when a bank questions a balance. Produces a reconciliation with explained breaks for gate G3. Read-only.
---

# gl-reconciliation

## Inputs

- Approved G2 record and publication receipts from `evidence/mapping-report.md`.
- The bank's trial balance extract (listed in `pod.yaml` under `data_sources`).
- `adapters/gl-account-map.csv`: GL account → Workbench book, entity, currency,
  accounting category, sign convention. Draft it if missing; the bank controller
  confirms it at G3.
- `reconciliation.tolerance` from `pod.yaml` (absolute dollars and relative).
- MCP `ops` profile with `read,compute`.

## Procedure

1. **Pin the comparison.** Record the workspace revision, the as-of date of the
   books and of the trial balance. Different dates are a break in themselves;
   stop and ask for matching extracts.
2. **Book side.** Page each published book (`GET /books/{name}/rows`) and total
   balances by entity, currency and accounting category. Sum principal and
   carrying value separately; do not net them.
3. **GL side.** Apply `gl-account-map.csv` to the trial balance and total on the
   same keys. Apply the sign convention once.
4. **Compare.** For each key: book, GL, difference, relative difference, within
   tolerance (yes/no). Every GL account must map to a key or be listed as
   out of scope with a reason. Unmapped accounts are a break.
5. **Explain breaks** outside tolerance. Typical causes: excluded closed or
   charged-off accounts, accrued interest booked separately, premium/discount and
   fair-value marks, timing of the extract, products out of scope. Quantify each
   explanation; a break is explained only when the explained amounts sum to it.
6. **Opening-state check (when running ledgers).** Daily-ledger runs perform
   native opening reconciliation; include their result and job ID.
7. **Write `evidence/gl-reconciliation.md`** with the comparison table, break
   explanations, residual unexplained amounts and open questions.
8. **Draft gate G3** in `evidence/gates/G3-reconciled.json`.

## Do not

- Adjust book balances to force a tie. Fixes go back through `adapter-mapping`.
- Plug residuals into an "other" bucket.
- Infer allowance, chargeoff or accrued amounts the bank did not supply.
