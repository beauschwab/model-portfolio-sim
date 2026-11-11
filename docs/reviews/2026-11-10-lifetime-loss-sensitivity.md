# Lifetime-loss implementation checkpoint

This checkpoint implements the bounded analytical scope of #95. It does not close
observed accuracy, accounting-policy approval, deployment or the wider #15/#25
adoption and scale gates. See [the contract](../models/lifetime-loss-sensitivity.md).

## Focused validation

- 9 Rust tests cover shared draws/full recovery tails, unchanged zero coefficients,
  zero macro dispersion, hand loss, zero allocation, cohort scaling, completed
  outcomes, invalid domains, cancellation, duplicate bytes and original deadlines.
- 84 Python engine/API tests pass: 77 numerical/challenger/publication cases,
  6 final SQL authority tests and 1 existing durable model/restart case.
- Native all-target/all-feature clippy passes with warnings denied.
- The browser creates, persists and reopens the sensitivity and completed-outcome
  tables through the real durable API/worker (1 passing browser case).
- Root agent, architecture, native and HTTP contracts pass; the MCP catalog is
  regenerated for the additive example and documentation.

The release build installs four matching risk library/executable artifacts. The
lifecycle SHA256 is
`5280204e2c660d34c45faee884d7bb58d9f15ea49b335da3f9505df3a882ff3a`.
Unchanged ledger/decision/verifier artifacts are retained with checked hashes.
Full engine/API/native/browser matrix and hosted integration results are separate
acceptance receipts, not implied by this focused checkpoint.

## Reproducible synthetic analysis

`packages/portfolio-risk/tests/lifetime_loss_validation_pack.py OUTPUT.json`
retains one synthetic exact-event panel, native constant/covariate fits and an
independent discrete-time competing-risk challenger. All metrics use held-out
accounts. Native fitted hand rates are 3/8 overall and 1/4, 1/2 by group.
Two time horizons carry IPCW Brier and calibration comparisons for the challenger,
constant/covariate hazards and a fixed-row assumption. Zero macro dispersion
produces zero spread; doubling the declared macro deviation widens the two
cohort spreads from 3.75/6.25 to 11.25/17.5 in this monotone synthetic fixture.
These finite-path examples do not establish general monotonicity or accuracy.

Local evidence: `artifacts/lifetime-loss-2026-11-10/`. The synthetic pack hash is
`20a88a0eba8f1bfb9a60a510729c9be67ce53f7f0667b5992c63aac37cfa312f`;
its panel hash is
`c40a1ea25612521343fe7449daad86df9a33640c5a0d2da17371bafbc90b0a2c`.

An initial two-cohort test exceeded the unchanged aggregate admission budget;
the paired fixture was reduced to 16 paths per cohort. A duplicate-HTTP test
initially sent the wrong envelope key and was corrected to `schema_name`.
The failed focused log is retained alongside the passing rerun. A hand regression
also fixes reverse-KM event/censor ordering at tied days before acceptance.
The browser passes with the existing worktree font-serving warning; this check
establishes interaction/persistence, not typography acceptance.
