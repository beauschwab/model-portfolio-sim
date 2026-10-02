# Exact-observation credit hazard calibration

`observed-credit-calibration-1` estimates homogeneous cause-specific transition
intensities for the shared `credit-model-1` state structure. The implementation is
`packages/portfolio-model-core/src/credit_calibration.rs`, exposed as
`credit_calibration::fit(&CreditCalibrationRequest)`. It returns fitted hazards,
training sufficient statistics and strictly chronological holdout diagnostics.
It never estimates a hazard from loan balances, a tape refresh, or monthly
snapshots whose intervening events are unknown.

This is a modular calibration stage for issue #11. No bank-history dataset was
supplied or calibrated during implementation. Tests use explicitly marked
synthetic histories. They establish the estimator and ownership contract; they
do not satisfy empirical accuracy, institutional validation, or calibration gates.

## Research and applicability

[Lange and Minin, *Fitting and Interpreting Continuous-Time Latent Markov Models for Panel Data*](https://pmc.ncbi.nlm.nih.gov/articles/PMC3795797/)
distinguish fully observed paths from periodically observed panel data, identify
the Markov/exponential-sojourn assumptions, and discuss latent trajectories when
intermediate transitions are unobserved. The current stage applies only to the
fully observed case; implementing an estimator for interval-censored snapshots
requires a separate likelihood and identifiability analysis.

[Bacci et al., *MM Algorithms to Estimate Parameters in Continuous-time Markov Chains*](https://arxiv.org/pdf/2302.08588)
similarly distinguish observed dwell times from observations without dwell-time
information. This implementation has directly observed states and elapsed times,
so its unrestricted row-rate MLE has an analytic solution rather than requiring
their iterative hidden-state machinery.

The [Federal Reserve's January 2026 credit model documentation](https://www.federalreserve.gov/supervisionreg/files/credit-risk-models.pdf)
is a credit-specific reference for competing default/payoff exits and fitted
payment-status behavior. This repository's homogeneous estimator does not reproduce
the Fed's regression models or supervisory results. An account’s originating
product, borrower attributes and macroeconomic regime remain important intended-use
conditions rather than inputs to this first estimator.

## Observation and provenance contract

Each `ObservedSpell` has an immutable caller-supplied `spell_id`, `account_id`,
live `origin`, exact `start` and `end`, and a tagged ending:

- `transition` identifies the observed destination at the exact end time.
- `right_censored` means the origin state persisted throughout the spell and
  observation ended without a known transition. It adds exposure and no event.

Times are `ObservationTime { ordinal, seconds_of_day }`. The ordinal is a Gregorian
date (`0001-01-01 = 1`); seconds are in `0..86399`. All times must use one consistent
elapsed-time reference supplied by the source. There is no timezone/DST conversion
or leap-second interpretation. Dated events known only to an uncertainty interval
cannot be declared exact by assigning its endpoint to this field. If a source
only records daily statuses, this calibration route is generally inappropriate.

The required `observation_mode` is exactly `exact_transition_times`. There is no
interval-censored option. The declaration asserts that **all transitions during
each spell are known**. Validation can catch contradictory paths but cannot detect
a transition absent from the source. Missing transitions or informative observation
termination can bias the estimator even when the request passes its schema.

All input structs reject unknown fields. Probability weights, balance weights,
implicit clocks and an option to refit on holdout data are deliberately absent.
Each spell is one observed account episode, not a fractional/probability-weighted
cohort observation.

Required request provenance is `source_id`, `source_revision`, `population_id`,
`model_policy_id`, distinct `training_split_id` and `holdout_split_id`, and
`data_kind = observed | synthetic`. These identities and the observation mode are
retained verbatim in the result. They are caller declarations: the component does
not authenticate the source, determine whether a dataset is actually observed,
hash source bytes or publish a durable artifact. The transport must retain raw
inputs and immutable hashes before using a result in a persisted policy.

Within an account, spells must be contiguous and state-consistent. The next origin
must equal the prior live transition destination; positive gaps/overlaps fail.
An account cannot resume after a right-censored spell or after default/prepayment.
Inputs may be unordered; sorting by account, start time and spell identity makes
aggregation deterministic. Global duplicate spell identities fail. A first spell
may start after the training window begins, conditional on its known initial
state. No unobserved exposure before entry is manufactured.

## Chronological split and annual units

The request supplies `train_start < train_end < holdout_end`. All observed spells
must lie inside the full declared window. Every window must contain positive
total observed live exposure. A spell crossing `train_end` contributes its
observed elapsed exposure separately to training and holdout. The training clock
never includes exposure after `train_end`.

Transition/censor endpoints use the convention

```text
training events: train_start < event_time <= train_end
holdout events:  train_end   < event_time <= holdout_end.
```

An event exactly at the training end contributes once to training, while any
subsequent exposure in its new state begins in holdout. Exposure is the positive
intersection length of a spell and its window, so point endpoints have zero
exposure measure. No event at the split is counted twice.

Exposure is summed as integer elapsed seconds before a single conversion to
annual time units:

```text
T_i = observed_seconds_in_state_i / (365 * 86400).
```

This is explicit ACT/365 fixed elapsed time, including leap-day seconds. It is
not ACT/ACT, thirty-day reporting months or an inferred reporting frequency.
Repeated borrowers may occur on both sides of this chronological split; it is
an out-of-time evaluation, not an out-of-borrower evaluation. A separate population
and source policy are needed to establish borrower-disjoint validation.

## Likelihood and estimator

The model is a continuous-time Markov chain with the live states performing,
watch and delinquent, and absorbing default and prepaid. Within the fitted
population/window all cause-specific annual intensities `h_ij` are constant.
There is no diagonal transition intensity. The complete observed-path log density,
conditional on known entry states and noninformative observation times, is

```text
log L(h) = sum(i,j != i) [N_ij * log(h_ij) - T_i * h_ij].
```

`N_ij` counts exact transitions from origin `i` to destination `j`; `T_i` is total
training live-state exposure. Default and prepayment share the same risk exposure
and are distinct competing causes. Cures contribute counts to a better live
destination and can subsequently generate further observed spells. An exact
right-censored spell contributes `-T_i * sum_j h_ij` without a transition factor.

For `T_i > 0`, maximization yields `h_ij = N_ij / T_i`. Zero observed events give
the boundary MLE zero. This is tagged `observed_no_events` for an entire zero-event
origin row; it is not evidence that future transitions are impossible. For an
individual zero-event cause in an otherwise observed row, its zero count remains
visible in the diagnostics. Rare-event uncertainty requires independent interval
or posterior analysis before economic use; no confidence intervals or safety
floors are implied by these point estimates.

For `T_i = 0`, rates are unidentified. Each origin therefore requires an explicit
`UnobservedRowPolicy`:

- `reject` fails instead of silently returning zeros.
- `supplied_row` supplies a complete five-destination hazard row, assumption ID
  and rationale. This row is tagged `supplied_assumption` in the result and remains
  distinguishable from estimated evidence.

A supplied row is used only when that origin lacks training exposure. It cannot
override an observed estimate, smooth an observed zero-event row or incorporate
holdout events. There are no default priors, shrinkage or empirically calibrated
fallbacks. Policy rows are still validated when unused.

## Holdout diagnostics and interpretation

Fitting uses only training exposure and events. After fitting, the same fixed
hazards evaluate holdout statistics. Changing only holdout transition destinations
cannot change fitted rates, estimate-source tags or training diagnostics.

Each window reports exposure by origin, observed transition counts, expected
event counts `h_ij * T_i`, complete-path log likelihood and censor counts.
Expected event counts are **compensators conditioned on the realized observed
origin exposure**. They are not unconditional forecast probabilities or a new
projection generated by the credit engine.

If any holdout event has zero fitted intensity, its path likelihood is zero and
its log likelihood would be negative infinity. The result represents this as
`log_likelihood = null` with an explicit `impossible_event_count`, preserving
strict finite JSON. It never drops that event, adds an undisclosed intensity
floor, or calls the model adequate. Training cannot have such an event after a
valid positive-exposure MLE. Log densities are measured in annual time units and
exclude the entry-state density; they are comparable only for consistent units,
observation conventions and evaluation populations.

## Admission, maintainability and validation

Limits are 200,000 spells, 100,000 accounts and a total window no longer than
100 ACT/365 elapsed years. Combined metadata is at most the shared 32 MiB limit;
identity strings are at most 256 bytes and fallback rationales 1,024 bytes, with
control characters rejected. Positive spell durations and bounded dates prevent
ambiguous/overflowing exposure. Work is one deterministic `O(N log N)` sort and
linear validation/aggregation, followed by the fixed 3×5 estimator and diagnostics.
These component limits do not claim an aggregate RSS cap.

Fitted and supplied rates must satisfy the `credit-model-1` admission domain:
nonnegative finite intensities, zero diagonal and total outgoing intensity at
most 100/year. A high-frequency fit outside this domain fails explicitly; it is
never clipped to an artificial maximum. Production integration must treat a
fit as a new versioned policy and rebuild/reprice with fixed-base-OAS/CRN and
independent accounting publication checks as appropriate.

`tests/credit_calibration.rs` independently calculates exposure sums, counts,
cure/default/prepay MLEs, split-endpoint counts, annual-unit log densities and
holdout compensators. It verifies one-second exposure precision, immutable
training fits under holdout-only edits, deterministic input ordering, explicit
unobserved/no-event distinctions, impossible holdout events and rejected domains,
path gaps/overlaps, absorbing resumes and unknown interval-censoring fields.
Run `cargo test --manifest-path packages/portfolio-model-core/Cargo.toml --test credit_calibration`.

Remaining issue #11 work includes calibrated covariates/term structures, source
data censoring audits, uncertainty and stability, product-specific PD/LGD/EAD,
recovery/liquidation fitting, correlated obligor losses and integration into
product cashflows and daily journals. Out-of-time discrimination, cumulative
incidence calibration, stress extrapolation and exposure-weighted error must be
assessed on appropriate real histories. A likelihood implementation and green
synthetic tests alone cannot prove those requirements.
