# Exact-observation deposit behavior calibration

`observed-deposit-calibration-1` fits cause-specific physical intensities for
withdrawal, closure and migration between explicitly named deposit segments.
It is implemented in `portfolio_model_core::deposit_calibration` and exposed
through `fit_observed_deposit_behavior`. It is an evidence contract only: it
does not alter `deposit-state-1`, infer a paid-rate response, or authorize a
saved liability graph.

The required `sampling_unit="account_full_exit"` explicitly makes withdrawal
a complete account exit. Partial balance withdrawals cannot be fitted by this
contract, and account intensities cannot be substituted for balance-weighted
daily runoff hazards. Each spell supplies an account, segment, exact start/end
timestamps and one ending event. Migrations before observation end require a
contiguous destination spell; migrations exactly at observation end need no
unobserved continuation. Withdrawals, closures and right-censoring terminate the
observed account path. Exposure is measured in elapsed ACT/365 seconds. The
chronological split uses `(train_start, train_end]` for training events and
`(train_end, holdout_end]` for holdout events. A spell crossing the split
contributes exposure to both windows without inventing an event at the split.

For segment `i`, migration destination `j`, exposure `T_i` years and event
count `N`, the homogeneous competing-cause estimate is

```text
lambda_withdrawal_i = N_withdrawal_i / T_i
lambda_closure_i    = N_closure_i    / T_i
lambda_migration_ij = N_ij           / T_i
```

The row shares one account exposure clock across its competing causes.
Positive observed counts also report the local
Poisson information standard error `sqrt(N)/T`. Zero-event causes and supplied
rows are `null`, preserving missing information instead of implying zero
future runoff risk. Rows with no training exposure require an explicit
reviewed supplied row or fail closed. `estimate_origin` retains a typed tag;
supplied rows also retain the assumption ID and rationale as separate fields.
Standard errors are local asymptotic information estimates, not rare-event
confidence intervals or evidence of held-out predictive accuracy.

The result retains segment/source/policy identities, training and holdout
counts, compensator expectations, path likelihoods, impossible holdout events,
and the adoption boundary. Synthetic data are never marked adoption-eligible.
The fit assumes exact transition times, noninformative observation endings,
homogeneous hazards within the selected population, and no unobserved
balance-weighted cohort inference. Paid-rate response, competitor-rate
elasticity, confidence/regime effects, intraday behavior, inflows, maturity,
seasoned original-EIR ownership, cohort representativeness and FTP remain
separate supplied policies and acceptance gates.

Admission is bounded to 200,000 spells, 100,000 accounts, 32 segments and a
100-year combined window, with the shared metadata budget. Cooperative native
checks run through validation, ordering, exposure accumulation and fitting;
independent replay checks cancellation/deadline/ownership during both source
verification and persisted replay. Sorting is bounded and checked immediately
before and after. The independent verifier recomputes every result field and
rejects truncated vectors/matrices, altered expected migration counts, counts,
provenance, timestamps and unexpected output fields. It never fits production
requests or calls the native fitter to construct its expectations.

The exact-history likelihood is `sum(N log(lambda)) - sum(T lambda)`, conditional
on initial states and observation windows. This follows the distinction between
exactly observed paths and intermittently observed snapshots described in
[Jackson's multi-state model paper](https://www.jstatsoft.org/article/view/v038i08)
and [the author's observation-type documentation](https://chjackson.github.io/msm/reference/msm.html).
This implementation deliberately supports the homogeneous exact-history slice;
the references do not validate deposit-specific assumptions or this code.

Controlled native and independent tests cover migration continuity, competing
withdrawal/closure counts, exact split/second exposure, chronological holdout
invariance, supplied unobserved rows, standard errors, unknown destinations,
strict JSON, reordered histories and cancellation. The fixture is synthetic mechanics
evidence only; observed retention histories, held-out regime performance,
daily graph adoption and immutable journal replay are still required for #14.

Local validation on 2026-10-10 passed seven core deposit cases, nine unchanged
exact-credit calibration cases, four native deposit dispatcher cases, 25 shared
model adapter cases, 18 independent replay/cancellation cases, 53 durable worker
and persisted-failure cases and eight final-publication fence cases. The 18
replay cases were first run against the debug executable and repeated against
the release executable; the 53/8 and adapter cases used the installed release
artifacts. Product, ledger and decision/workflow release builds succeeded.
One earlier release-test attempt failed during Python/Polars collection with a
Windows resource error while compilation was running; its result is not counted
as a pass. The subsequent release checks passed without bypassing CPU checks or
changing financial tolerances. These are local synthetic/contractual evidence,
not observed-history, complete scale, deployment or empirical acceptance.

Final local broad regressions passed 2,378 engine tests (two skips) and 920 API
tests. All five native crate regression suites and strict checks passed. The
full API run first exposed a completed Card2-to-FTP source-layout incompatibility;
shared strict scalar restoration and lossless type selection corrected it.
Sixty-six focused source/replay/layout cases passed before the final broad run.
Earlier failed/interrupted attempts remain separate from these passing results.
