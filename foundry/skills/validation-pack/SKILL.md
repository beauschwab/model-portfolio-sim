---
name: validation-pack
version: 0.1.0
description: Assemble the model-risk validation pack a bank's validators need to approve the Rates Workbench for use. Use after gate G3, and whenever validators ask for evidence. Drafts documentation organized around SR 11-7 (conceptual soundness, outcomes analysis, ongoing monitoring), indexes repository and pod evidence, and prepares gate G4. Never claims the model is validated.
---

# validation-pack

The bank's validators decide. Your job is to make their review fast: every claim
points to evidence, and every limitation is stated before they find it.

## Inputs

- Approved G3 record and all pod evidence.
- Repository evidence for the pinned core release: `docs/model-validation-guide.html`,
  `docs/architecture.md`, `docs/contracts/README.md`, `docs/models/` (per-model
  methodology and limits), `docs/models/implementation-plan.md` (open acceptance
  gates), and the independent reference tests in `packages/portfolio-risk/tests/`.
- The bank's model inventory template, if supplied (use its structure instead of
  the outline below).

## Pack outline (`evidence/validation-pack/`)

1. **Model overview** — purpose and intended use at this bank (from `pod.yaml`
   `intended_use`), products in scope, users, outputs relied on.
2. **Conceptual soundness** — methodology per product model in scope, cited to
   `docs/models/`. Key assumptions and their sources. Fixed base OAS, shared
   random numbers, and deposit terminal treatment stated explicitly.
3. **Implementation and replication evidence** — Rust production engine with
   independent Python reference tests; which tests cover which outputs; the
   publication verification gates. Cite test files and the core release.
4. **Data and inputs** — summary of `data-profile.md`, `mapping-report.md` and
   `gl-reconciliation.md`, with assumptions from `adapters/assumptions.md`.
5. **Outcomes analysis** — bank-specific back-testing: model NII and runoff
   against the bank's actual history for the periods supplied, parallel-run
   differences against the incumbent system, cohort audit results. Mark any
   section without bank history as "not yet performed".
6. **Limitations** — copy, do not paraphrase away: research regulatory weights,
   unsupported products, open issue #25 acceptance gates, approximations
   (e.g. monthly capture, forward unit pricing). Include each with its impact on
   the bank's intended use.
7. **Ongoing monitoring** — proposed metrics, thresholds and frequency
   (reconciliation breaks, back-test error, assumption drift), and the upgrade
   and re-evidence process for new core releases.
8. **Evidence index** — table of every artifact: path or job ID, revision,
   date, what it supports.

## Procedure

1. Build the outline above, filling each section only from cited evidence.
2. Run the repository checks that back section 3 for the pinned release, or cite
   the release's recorded results; never cite results from a different release.
3. List gaps in a "Requested from bank" section (histories, incumbent outputs,
   inventory template, risk-tiering decision).
4. Draft gate G4 in `evidence/gates/G4-validation-handoff.json`.
5. After handoff, log validator questions and answers in `evidence/validator-log.md`
   and update the pack, versioning each change.

## Do not

- Write "validated", "regulatory-grade" or "approved" about the model.
- Present synthetic or demo data results as bank evidence.
- Drop a limitation because it looks unfavorable.
