---
name: data-profiling
version: 0.1.0
description: Profile a bank's loan, deposit, security and GL extracts before they are mapped into the Rates Workbench. Use at the start of an onboarding, or when a bank sends a refreshed extract. Produces a data-quality report and gap list for gate G1. Read-only; it imports nothing into books.
---

# data-profiling

## Inputs

- Extract files or URIs listed under `data_sources` in `pod.yaml`
  (CSV/Parquet; server paths need `WORKBENCH_TAPE_ROOT`, S3 needs
  `WORKBENCH_TAPE_S3_PREFIX` — see `docs/tape-cohort-workflow.md`).
- MCP `ops` profile with `read,compute`.

## Procedure

1. **Inventory.** For each source record: product, as-of date, row count, column
   count, total balance, currency values, SHA-256 if available. Check intake limits
   up front: 32 MiB, 100,000 records, 128 columns per tape. Over-limit sources
   need splitting by product or entity; list them as gaps.
2. **Field coverage.** Compare columns with what the target product needs:
   - Cohort presets: `GET /cohorts/presets` lists the behavioral dimensions per
     product (mortgage, auto, personal, card, deposit).
   - Mortgage pool frame fields: `packages/portfolio-risk/skills/portfolio-risk-engine/references/schemas.md`
     ("Portfolio frame").
   - Mandatory separation keys for every product: product, currency, entity,
     accounting category, assumption set. There are no defaults for these.
   Mark each field: present, derivable (state the derivation), or missing.
3. **Quality checks** on present fields:
   - duplicate or non-string loan IDs (leading zeroes must survive)
   - zero or negative balances, closed accounts not excluded
   - rates/LTV/DTI not in decimals, prices not per 100
   - out-of-range values (FICO, terms, ages, rates)
   - missing values per behavioral dimension
   - balance by entity and accounting category, ready for GL tie-out
4. **Product support.** Flag products without a production pricer today
   (auto, personal and card cohorts can be grouped but need dedicated pricers;
   non-USD tapes cannot be published through the current pricing adapter).
5. **Write the report** to `evidence/data-profile.md`: inventory table, field
   coverage per product, quality exceptions with counts and examples (masked IDs
   only), unsupported scope, and questions for the bank.
6. **Draft gate G1** in `evidence/gates/G1-data-accepted.json` listing each
   exception with a proposed disposition (fix at source, exclude, explicit
   default for G2, out of scope).

## Do not

- Print or store full loan-level rows in the report; aggregate and mask.
- Treat a missing loan in a refreshed tape as paid off.
- Propose defaults for mandatory separation keys.
