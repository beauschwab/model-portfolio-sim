# Consumer panel specification and native fit projection

`consumer-behaviour-spec-1` is an explicit standalone analytical entrypoint.
`analytics.model_contracts.fit_consumer_behaviour` transports an immutable request
to the native lifecycle process. `portfolio-model-core::consumer_behavior` owns
panel mapping, calls the existing Rust likelihood estimators, and projects the
fitted Physical hazards into the complete shared credit-path operator. No Python
financial callback, saved-model replacement or durable ModelLab publication is
added by this entrypoint.

This implements part of #91. Daily installment/card action adoption, independent
publication verification and full product-workflow validation remain required.
In particular, a fractional-time credit event cannot be silently rounded into a
daily product action, and a card zero-balance closure needs its own complete
interest/fee/commitment ownership. The analytical credit-path result does not
authorize either conversion.

## Product policy and panel records

A specification selects `auto`, `personal` or `credit_card`, an explicit policy
identity/revision, ordered DPD thresholds, permitted default causes, driver
definitions, term levels and minimum training support for each live origin.
No threshold is supplied implicitly. The example uses 30/60/120 DPD for auto;
a card fixture explicitly selects 30/60/180. These are synthetic policies, not
regulatory or accounting approvals.

DPD thresholds map to performing/watch/delinquent/default. Declared bankruptcy,
chargeoff, repossession or settlement can select default; repossession is auto
only. Installment voluntary payoff and card `closed_zero_balance` are distinct
terminal tags. A payoff/closure requires zero balance, zero DPD and a closed line.
A zero-balance open card remains live. Existing absorbing default origins require
their separate recovery owner. Modifications/re-ages reject; they never become
cures or get excluded using future holdout information.

Records retain account/record/source identities, start/end clocks, raw start/end
statuses, supplied loan age, observation kind and named raw features. Observation
kinds are exact transition, exact covariate boundary, exact right censor or
snapshot. Exact boundary/censor records cannot change state; exact transitions
must change state. Existing native estimators enforce account continuity,
absorbing exits, observation windows, edge domains and source provenance.

`fit_template` selects either an empty `credit-covariate-1` request or an empty
`credit-snapshot-calibration-1` request. Its policy identity must match the product
specification. The mapper fills its columns, term levels and observations; callers
cannot supply a second competing segment/snapshot inventory. Snapshot inputs never
become exact events. Default severity outcomes remain a separate supplied contract.

## Drivers, terms and training separation

The product allowlist includes shared origination/current score, channel, vintage,
APR spread and DTI/payment-to-income; auto LTV/new-used/equity/trade-in features;
and card utilization/behavior/promotion features. Macro names cover unemployment,
short/consumer rates and HPI, auto used-vehicle changes, and personal/card income
and prime-policy spreads. Each selected column declares borrower/macro role, raw
unit/domain, frozen offset/scale, scaling source/training cut and availability lag.
Unknown product columns and inconsistent roles reject.

Numeric values and explicitly coded categories are distinct input types. A
category map is a supplied numeric coding, not an inferred unordered effect or
automatic one-hot model. Selected values must match the declared coding and raw
domain. Scaling cuts cannot follow the training cut. An observation is usable no
earlier than both its publication time and observation time plus the declared lag;
it must be usable at interval start. No missing value, future publication,
unemployment series, categorical coding or transform is imputed.

Term grids start at month zero and increase strictly. The explicit convention is
`hold_start_level_within_interval`: supplied start age selects a level held over
that observed interval. Neither continuous seasoning within an interval nor
intermediate snapshot states are inferred. The first level is the reference.
The assumption list discloses omitted drivers and coarsened grids.

Exact training support counts known origin exposure clipped at the chronological
cut. Snapshot support counts duration of intervals *starting* in that state and
ending within training; this is labeled `observed_start_state_interval_duration`,
not latent state occupation. `minimum_training_support_years` applies to that
declared support convention. Thin fitted rows reject and require an explicit
review-tagged fixed row. Fixed rows retain their supplied assumption status.
Cross-split snapshot intervals follow the template's explicit rejection/exclusion
policy; no unknown split-boundary state is fabricated.

Fitting, scaling and model selection use training data only. Holdout records feed
the existing likelihood/exposure/endpoint diagnostics. The prepared specification
has its own typed SHA-256 identity, separate from the estimator's coefficient
identity, so a policy change remains visible even when fitted hazards are equal.
Hash identity does not authenticate data or approve a policy.

## Projection, rights and controls

Optional projections retain the fitted specification, model/review/scenario
identities and supplied Physical driver intervals. Raw values use the retained
column order/domain and frozen transforms. Intervals partition the exact source
clock/grid; their source must be available at projection origin. Forecast values
are supplied projections rather than observed fixings. Upstream forecast and lag
alignment remain supplied assumptions; they are not inferred by the projector.

The complete `credit-path-1` source supplies monetary laws, original path keys,
draw streams, contractual boundaries, signed accrual/basis, LGD/CCF and full
recovery tails. Hazard slots must be explicitly zero placeholders. Projection
fills only those slots. It neither replaces other monetary inputs nor regenerates
draws. There is no Physical-to-Pricing conversion, OAS solve or pricing adoption.
Complete credit-path outputs retain recoveries beyond contractual end. The
separately selected [daily product adapter](consumer-behaviour-product-paths.md)
uses zero-monetary shared state paths and one daily product cashflow owner.
Neither component establishes journal acceptance.

The fit input is bounded to 4 MiB, 20,000 records, eight columns and 32 term
levels; existing estimator work/parameter limits still apply. Projection has a
4 MiB input bound, at most 64 intervals, 128 paths/boundaries/jumps and 4,096
prospective path-event slots. Its serialized response is bounded to 16 MiB. The
coordinator admits an 8 MiB input and reserves a response budget before fitting:
12 times raw serialized bytes, plus 4 MiB and 4,096 bytes per prospective event
slot. Requests exceeding the 32 MiB total budget reject before fitting. Actual
serialized output must also fit that reservation. This is a conservative budget
policy, not an exact output-size estimate. Individual maxima do not promise
simultaneous admission or an aggregate RSS cap.

The original native entry deadline covers strict duplicate-key decoding and final
serialization. Mapping/projection checkpoints retain caller cancellation and the
original scalar budget; bounded estimator/path calls are checked before and
after. The process transport additionally has a watchdog. These are separate
controls; a cooperative callback does not interrupt a blocked external process.

## Evidence and remaining adoption

The small exact-event example checks two-group hazards 0.25 and 0.50/year. The
snapshot fixture instead checks `-log(0.75)` from observed endpoints; it cannot be
replaced by 0.25 exact-time event/exposure MLE. A separate synthetic monthly
example has 528 account intervals, real calendar dates, borrower bands and
chronological training/holdout periods. It has no observed bank provenance.

Component tests cover all three products, explicit state/closure rules, held-out
coefficient invariance, raw/availability refusal, minimum support, category/term
definitions, complete recovery tails, identical draws and cancellation. Matching
installed artifacts, independent raw/persisted publication verification, the
separately selected daily adapter, full regression and actual integration are separate acceptance
steps. `adoption_eligible` and empirical-accuracy flags remain false.
