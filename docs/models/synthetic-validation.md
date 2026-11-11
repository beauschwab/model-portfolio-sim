# Reproducible synthetic behavioral and credit validation

The user authorized invented historical data on 2026-10-19. These are hypothetical
observations with known generating laws, explicitly marked `data_kind="synthetic"`.
The accompanying policies are example assumptions with `reviewed=false` and
`approved_for_production=false`. No institution, reviewer or approval is invented.

## Generate and validate

From the repository root, with the engine dependencies and a matching native
product build available:

```text
python scripts/make_synthetic_model_validation.py --output artifacts/synthetic-model-pack
python scripts/validate_synthetic_model_pack.py artifacts/synthetic-model-pack --report artifacts/synthetic-model-validation.json
```

The generator refuses a nonempty output directory. The verifier refuses to
overwrite a report. The manifest records every input/truth/policy file's SHA-256.
Verification checks that identity before fitting and records the actual native
library hash, source commit and both verification-script hashes. No fitting is
performed by the generator or by a production Python financial implementation.
The verifier calls the existing Rust calibration contracts through their adapters.

Default seed: `20261019`; 600 initial accounts per live origin/segment in each
window. Exact-spell training is 2021-01-01 through 2023-01-01; snapshot training
uses 2022 only to respect the native one-year maximum per observed interval.
Holdout is 2023-01-01 through 2024-01-01. The windows use independent cohorts and never random row splitting.
All intensities use the model's fixed 365-day year. Simulated continuous waiting
times are rounded up to the next second, an error below one second per sojourn.

| File family | Data and model | Known-truth verification |
| --- | --- | --- |
| Auto, personal, card, mortgage, commercial credit | Exact complete state spells, three live origins, default and prepayment absorbing; explicit different hypothetical hazards | Independent raw exposure/count MLE, annual-time path likelihood and sampling comparison with the generating hazards |
| Commercial credit shift | Same training spells; four times the default intensity only in holdout | Bitwise unchanged training fits and diagnostics; deteriorated holdout default residual |
| Deposits | Retail/commercial migration, full-account withdrawal, closure and censoring | Independent cause-specific counts/exposure and complete path likelihood |
| Deposit shift | Same training spells; four times full-withdrawal intensity only in holdout | Unchanged training fit and deteriorated holdout withdrawal residual |
| Snapshot credit | Performing/default endpoints, without invented event times | Binomial closed-form MLE and independent endpoint likelihood |
| Snapshot credit shift | Same training endpoints; increased holdout default intensity | Unchanged training parameters/information/severity; changed holdout diagnostics |
| Complete default outcomes | Separate exact default records, two later recoveries, explicitly complete resolution | EAD-weighted undiscounted realized LGD 0.5 and undrawn-weighted realized CCF 0.6 in each window |

The five product labels exercise a shared credit fitter; they do not claim five
distinct borrower models have been calibrated. Initial-state sampling intentionally
covers every live origin, rather than estimating a portfolio's state distribution.

The numerical MLE/likelihood checks are separate from a broad, predeclared sampling
check: fitted hazards must lie within six count-based standard errors plus
two events per unit exposure of the generating law. Snapshot hazard recovery uses
the existing one-parameter binomial oracle tolerance of 1e-6. Numerical parity
does not depend on a statistical test passing. The deliberate exact-history
regime shifts must produce an exit residual above six under the frozen training
fit. These diagnostics are checks of this finite synthetic family, not general
confidence guarantees or proof that a real stress forecast is calibrated.

## Recorded local result (2026-10-19)

The corrected default pack passes all ten actual Rust calibration cases and
three paired holdout-separation checks. All 15 regression controls pass, including
deterministic generation, complete chronology, native fitting, tampered-fit
refusal and source-hash refusal before fitting. The ten histories contain 45,102
spells/snapshot intervals; the twelve input/truth/policy files total 10,157,283 bytes.

The largest training-hazard standardized error is 2.792. Under the deliberately
changed holdout law, the commercial default residual is 41.572 and the deposit
withdrawal residual is 38.122; the corresponding training fits are bitwise
unchanged. Both snapshot fits retain the identical training hazard
0.3832143249778019 while their holdout diagnostics change. Complete realized
outcome checks recover LGD 0.5 and CCF 0.6 independently in both windows.

Evidence is under `artifacts/issue25-synthetic-validation-2026-10-19/`:
`data-v2/manifest.json`, `validation-report.json`, `synthetic-tests-v2.log` and
`validation.log`. Manifest SHA-256 is
`fe36200032920dd689cd261462dac1aac1b4ac816329787ac8bc27bfa79e2072`.
The actual tested product library SHA-256 is
`e8469449a848b109aef757760f9d911f45e948e66d8dca3c6e042556473169f9`.
The first pack and failed run remain retained: the initial snapshot fixture
exceeded the native one-year interval bound. Its successor uses one training
year without changing that bound or any verification threshold.

Main was merged at `100ab9c` from `8f48fa0`; the commercial and hedge routes are
both retained. These calibration results do not replace full installed-release
acceptance after that merge. The separately recorded 2026-10-18 scale runs use
three processes and do not establish one combined daily owner.

## Completion boundary

This pack supplies repeatable implementation evidence and explicit sample policies.
It does not establish observed portfolio accuracy, institution-specific policy
review, regulatory/accounting approval, product adoption or combined scale
acceptance. Deposit rate beta and partial withdrawals, mortgage incentive/seasoning
prepayment, card utilization/payment allocation, borrower/macro covariates and
product-specific LGD/EAD estimation require separate models and evidence. Recovery
examples are complete realized diagnostics, not a discounted LGD forecasting model.

No saved economics, pricing measure, original OAS/EIR, production runtime selection
or publication requirement is changed. The complete acceptance criteria in
[the implementation plan](implementation-plan.md) and issues #15/#25 remain.
