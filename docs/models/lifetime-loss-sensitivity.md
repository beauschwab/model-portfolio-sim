# Lifetime economic loss and challenger diagnostics

`lifetime-credit-loss-1` is an additive, bounded Rust analytical report for issue
#95. Select it through `POST /models/runs` or
`analytics.model_contracts.evaluate_lifetime_credit_loss`. The native owner is
`portfolio-risk-native/src/models/lifetime_loss.rs`; publication independently
reconstructs the raw source through `analytics.lifetime_loss_reference`.

## Supplied sources and model choices

A request declares source/version, synthetic or observed data kind, chronological
training/holdout identities, a training cut, assumptions, driver definitions,
1–8 Physical macro scenarios, 1–16 consumer cohorts and optional completed outcome
records. Products are auto, personal and credit card. All cohorts share the
original clock, interval grid and currency. Cohorts retain complete
`credit-path-1` inputs, original CRN keys/streams, signed accrual/basis, principal,
undrawn commitments, contractual boundaries and complete default recovery tails.

Each cohort selects exactly three distinct model kinds:

- `constant_row`: supplied constant annual origin/destination hazards.
- `supplied_covariate`: the source's interval hazards multiplied by
  `exp(beta * z)`, where `z = (supplied_value - frozen_offset) / frozen_scale`.
  Every coefficient row, source/version and training cut is explicit.
- `fixed_row_assumption`: one supplied hazard matrix per interval, bound to an
  identifier in the request's assumption list.

Driver metadata includes unit, raw domain, scaling source/training cut and
availability lag. Values are explicitly **supplied lag-aligned projections** at
interval starts; this report neither generates forecasts nor certifies the
upstream lag alignment. Forecast as-of dates cannot exceed the original clock;
coefficient/scaling cuts cannot exceed the declared training cut. No Physical to
Pricing conversion, holdout fit, clipping, zero-hazard fallback or future fixing
is inferred. A zero baseline edge requires zero coefficients. Unsupported or
nonfinite/overflowed hazards reject before any path generation.

The three models and all macro scenarios reuse identical random streams and path
keys. LGD, CCF, contractual amortization, coupon accrual and recovery assumptions
remain supplied and unchanged across these comparisons. This is a sensitivity
analysis of declared laws, not automatic adoption of the #91–#94 behavioral,
collateral or macro specifications.

## Economic loss and complete rights

Each cell retains every unweighted economic path, even at zero probability or
zero monetary multiplier. Active/truncated contractual tails reject. Default
claims and complete future recoveries remain visible beyond contractual maturity;
chargeoff or a principal report date never removes them.

Principal economic loss is the path's default principal loss. Signed interest
loss is default accrued interest less its full signed interest recovery tail.
Book adjustment, reviewed allowance, provision, chargeoff and legal rights stay
separate. Apply each supplied probability weight and monetary multiplier once.
Weights must sum to one within 1e-12 without exceeding one and are never
normalized. Default probability is not scaled by the monetary multiplier.

Results report cohort/model/scenario cells, product totals, macro ranges for each
model and model-choice ranges for each scenario. Ranges are min/max sensitivities
of the declared scenarios, **not confidence intervals**. Identical macro paths
or zero coefficients give zero macro spread; finite Monte Carlo paths can also
produce ties. Widening a general scenario envelope need not monotonically change
loss under arbitrary competing risks; the validation fixture checks a declared
monotone default-only example.

Optional outcome records bind exact cohort/vintage/currency and separate
source/version. Complete follow-up must reach contractual end plus the largest
supplied recovery lag. Supplied principal and signed interest losses are already
cohort-scaled. Base-scenario backtests report projected, reported and difference
for each model. Outcomes never alter coefficients, simulated cash or forecasts.
No conversion from chargeoffs/allowances to economic loss is inferred.

## Admission, cancellation and publication

Inputs are at most 2 MiB (the HTTP route reserves envelope overhead); each credit
source is at most 1 MiB. A path source has at most 128 intervals, boundaries and
jumps. Aggregate preflight checks the complete cohort/model/scenario/path product
against 32 million work units and 32 MiB output allowance before generation.
Declared limits multiply; the individual maxima cannot all be used together.
These are work/serialization allowances, not aggregate process RSS guarantees.

Original entry deadlines cover strict duplicate-key parsing, bounded individual
path calls, serialization and response completion. Python transport adds its
process watchdog. Durable jobs preserve the original queued deadline and raw
request. Independent replay recomputes hazards, every path, complete recoveries,
losses, ranges, product totals and outcome comparisons with 3e-12 relative/absolute
numerical comparison and exact typed raw identities. It calls no native model.
Fresh persisted object rereads, source/executable hashes and scoped SQL
lease/attempt/token/cancellation fences precede publication. Missing capability
requires matching rebuilt artifacts and restarted deployment processes.

## Independent challenger and reproducible validation pack

`analytics.consumer_challenger_reference` is offline independent analysis only;
it is absent from production dispatch. An explicit exact-event panel identifies
account/product/stratum/vintage/cohort, origin, inclusive observation end, default
or payoff, training cut, evaluation date and time bins. Account overlap rejects.
Training labels are truncated at the training cut. Holdout observations never fit
transition or censoring probabilities.

The challenger estimates competing default/payoff transitions using empirical
product-limit risk sets within each time bin (Aalen–Johansen cumulative incidence).
Partial censored exposures remain in risk sets until their observed end. There
is no terminal-bin completion or substitute estimate for a thin/unobserved bin.
Training-only reverse Kaplan–Meier estimates censor survival within each stratum;
observed exits precede censoring at tied days. Explicit independent-censoring and
stable-censoring-across-splits assumptions are mandatory. Zero censor survival at
an evaluation horizon rejects; weights are never clipped or renormalized.

Holdout-only predictions carry time-dependent IPCW Brier score and calibration
error by product, stratum, vintage and cohort. Payoff is a competing observed
nondefault outcome, not censoring. `compare_predictions` requires exactly the same
held-out account/horizon coverage for each declared comparison model. Model
lineage is retained; the scorer does not certify a supplied prediction's fit.

Run `packages/portfolio-risk/tests/lifetime_loss_validation_pack.py OUTPUT.json`
with matching native artifacts and the engine on `PYTHONPATH`. It fits native
constant and covariate hazards on the same synthetic exact-event panel, checks
hand MLE rates (3/8 overall, 1/4 and 1/2 by group), trains the independent
challenger on those same accounts, scores all three native model assumptions
and the challenger on held-out accounts, then independently verifies complete
native lifetime-loss reports and zero/positive macro spreads. The pack retains
raw panels, fits, source hash, split identities, assumptions and complete report.
Its analytical CTMC comparison probabilities are independent validation formulas,
not production financial callbacks. Synthetic recovery outcomes are separately
identified; no generated file is observed bank evidence.

Every report keeps observed-accuracy, accounting approval and adoption flags
false. Economic expected loss is not a CECL/IFRS 9 allowance. Broader product
adoption, production deployment, observed accuracy and reviewed policy acceptance
remain distinct requirements under #11/#15/#25.

## Method references

- [Aalen and Johansen, empirical transition matrix (1978)](https://www.jstor.org/stable/4615704): product-limit transitions with censoring.
- [scikit-survival reverse Kaplan–Meier convention](https://scikit-survival.readthedocs.io/en/v0.22.0/api/generated/sksurv.nonparametric.kaplan_meier_estimator.html): events precede censoring at ties.
- [The Brier Score under Administrative Censoring, JMLR](https://www.jmlr.org/papers/volume24/19-1030/19-1030.pdf): censoring assumptions and limits of IPCW evaluation.
- [Philadelphia Fed WP 26-09](https://fedinprint.org/item/fedpwp/102431/original): motivation for macro/model sensitivity; no coefficients are imported.
