---
name: adapter-mapping
version: 0.1.0
description: Map a bank's extracts onto Rates Workbench inputs and import them. Use after gate G1, or when a bank's extract layout changes. Writes the pod's column maps and assumptions, imports tapes, builds and audits cohorts, and prepares gate G2. Publishing into books requires the publish grant and an approved G2.
---

# adapter-mapping

## Inputs

- Approved G1 record and `evidence/data-profile.md`.
- MCP `ops` profile: `read,compute` for import, build and audit; `edit` to adopt a
  tape into the workspace; `publish` to publish cohorts into books (after G2 only).

## Procedure

1. **Reuse first.** Look in `foundry/adapters-library/` (when present) and the
   pod's `adapters/` for a mapping from the same core banking or GL system.
   Start from it and record which one you used.
2. **Write the column map** to `adapters/<product>.cohort-config.json`, following
   `docs/tape-cohort-workflow.md`:
   - `column_map`: canonical name → source column, for every field the product
     needs.
   - `defaults`: only the explicit missing-value assumptions dispositioned at G1,
     each with a comment entry in `adapters/assumptions.md` naming who approved it.
   - Dimensions and buckets: start from `GET /cohorts/presets` for the product.
   - Deposit `assumptions`: attrition overrides only if the bank supplied them;
     these are engine-domain inputs, not fitted values.
3. **Import** each tape with `POST /cohorts/imports` and record the job ID and
   source SHA-256. Adopt into the workspace with `PUT /cohorts/tapes/{name}` using
   the observed `expected_revision` (needs `edit`).
4. **Build cohorts** with `POST /cohorts/builds`. Check from
   `GET /cohorts/builds/{job}/summary`:
   - record count and total balance equal the source inventory exactly
   - no rejected records left unexplained
5. **Audit accuracy** with `POST /cohorts/builds/{job}/audit` on all selected
   mortgage/deposit records. Where checks fail tolerance, try the suggested splits,
   rebuild and re-audit. Passing the default tolerance is not model approval.
6. **Write `evidence/mapping-report.md`:** mapping table per product, assumptions
   used, import/build/audit job IDs, conservation checks, audit results and
   residual exceptions.
7. **Draft gate G2** in `evidence/gates/G2-mapping-approved.json`.
8. **After G2 is approved and `publish` is granted:** publish with
   `PUT /cohorts/builds/{job}/publish` (`mode=replace_tape` when other sources
   share the book). Record the publication receipt in the mapping report.

## Do not

- Invent historical book yields; selective mortgage publication into a
  historical-yield book needs `book_yield` supplied explicitly.
- Interpret an empty selection as a deletion of a product.
- Retry a write whose outcome is uncertain; check job and revision state first.
