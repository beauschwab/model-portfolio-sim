# Shared native credit model: `credit-model-1`

This model is a deterministic expected-credit foundation in
`packages/portfolio-model-core/src/credit.rs`. Its public function is
`credit::evaluate(&CreditRequest) -> Result<CreditResult, String>`.
It supplies multi-state migration and cures, competing default/prepayment,
state-conditional exposure at default, term LGD and explicit recovery timing.
It is a new versioned model; existing saved economics and legacy daily credit
rules are not silently replaced. The shared component alone does not complete
issue #11, reprice a product, or change a published journal.

## Modeling basis and source review

The [Federal Reserve's detailed 2025 methodology](https://www.federalreserve.gov/publications/files/2025-june-supervisory-stress-test-methodology.pdf)
(printed pages 34–35 and 40–41) provides a useful product modeling precedent:
mortgage transitions include cures and competing exits; loss severity and
liquidation timing are separately modeled; card EAD includes outstanding and
additional borrowing. This implementation shares those structural ideas, but
uses a continuous-time generator rather than the Fed's fitted quarterly logits.
No supervisory coefficients or results are reproduced here. The
[2026 methodology update](https://www.federalreserve.gov/publications/files/2026-february-supervisory-stress-test-methodology.pdf)
is the current-year companion and describes its relationship to the 2025 models.

The [BIS credit-risk modeling review](https://www.bis.org/publications/199904-consultation-credit-risk-modelling-current-practices-and-applications)
is historical research supporting the need to distinguish uncertain exposure,
credit quality migration and credit-related optionality. The
[BIS validation studies](https://www.bis.org/publications/studies-validation-internal-rating-systems-revised)
separate discriminatory performance, PD calibration, LGD and EAD assessment, and
benchmarking. They are research references, not a claim of regulatory approval.
Current U.S. model-risk guidance is
[SR 26-2, April 17, 2026](https://www.federalreserve.gov/supervisionreg/srletters/SR2602.htm),
which supersedes SR 11-7. Applicable institutional governance still requires
independent assessment of intended use, assumptions, inputs and limitations.

The equations and choices below specify this repository's model. They do not
claim CECL, IFRS 9, Basel capital, or supervisory stress-test compliance.

## State and input contract

All arrays use the fixed state order **performing, watch, delinquent, default,
prepaid**. The three live states are deliberately generic: the caller must attach
payment-status/rating definitions to persisted model policy. For example, watch
does not intrinsically mean a particular days-past-due bucket. Default and prepaid
are absorbing for the remainder of the request. Recoveries do not cause a
defaulted loan to return to performing. Cure means a migration from delinquent or
watch back to a better live state before default.

`CreditRequest` contains:

| Field | Required meaning |
|---|---|
| `schema` | Exactly `credit-model-1`; unsupported or omitted schema fails. |
| `monetary_unit` | Explicit 1–64 byte unit/currency label. Retained without conversion. |
| `initial_live_probability` | Three nonnegative probabilities summing to one, tolerance `1e-12`. Existing defaults and their recovery inventory are outside this contract. |
| `periods` | Ordered contiguous terms beginning at model year zero; each has explicit duration, hazards, EAD, LGD and recovery distribution. |

Each `CreditPeriod` specifies:

| Field | Units and convention |
|---|---|
| `duration_years` | Positive elapsed model years, at most one year. No implicit monthly/daily conversion. |
| `transition_hazards_per_year` | A 3×5 array of annual continuous-time intensities. Each diagonal must be exactly zero; each origin's total outgoing intensity is at most 100/year. |
| `exposure_at_default` | Three conditional EAD specifications: nonnegative drawn and undrawn amounts in the declared monetary unit, and CCF in `[0,1]`. |
| `loss_given_default` | Three fractions in `[0,1]`, interpreted as eventual nominal loss divided by EAD. These are not discounted economic LGDs. |
| `recovery_distribution` | At most 32 strictly increasing nonnegative year lags and nonnegative weights summing to one. Weights divide eventual recovery. The tail cannot be discarded. |

The input structure uses `deny_unknown_fields` throughout. An `annual_pd` or
`default_probability` field is rejected instead of being interpreted as a hazard.
Every numeric input must be finite. Conditional EAD and each drawn/undrawn amount
are bounded to `1e15` monetary units. Inputs are validated in full before any
transition computation or output allocation.

## Probability model and equations

For live state `i`, let `h_ij` be its supplied annual intensity to destination
`j`. The generator has `Q_ij = h_ij` for unequal states and
`Q_ii = -sum(j != i, h_ij)`. Absorbing rows are zero. Within each supplied term,
intensities are constant, and a row-vector probability distribution propagates as

```text
p(t + dt) = p(t) exp(Q dt).
```

This allows multiple live-state migrations within a term, including cure cycles.
Default and prepayment compete for the same surviving population. They are not
two independent deductions from the beginning balance.

For the single live-state special case with default intensity `d` and prepayment
intensity `s`, the closed forms are

```text
survival = exp(-(d+s) dt)
default_probability = d/(d+s) * (1 - survival)
prepay_probability  = s/(d+s) * (1 - survival).
```

The zero-intensity case returns survival one and both exits zero. The helper
`competing_exit_probabilities` computes the small exit probability using
`-expm1(-(d+s) dt)` to avoid subtraction cancellation.

Annual conditional single-cause PD is distinct from intensity. The explicit helper
`annual_pd_to_hazard(q)` computes `-ln(1-q)` using `ln_1p`. It rejects `q=1`
because the corresponding intensity is infinite. This conversion assumes one
constant default cause and no competing exit or migration. It must not be used
to convert an observed cumulative default frequency that already incorporates
prepayment or migration without a separately fitted statistical model.

Each period reports **marginal PD**, the new default probability relative to the
original population, and **conditional PD**, marginal PD divided by the live mass
at that period's beginning. Cumulative PD is the default state mass. For example,
single-cause conditional PDs of 10% and 20% produce marginal PDs 10% and 18%, and
cumulative PD 28%. Summing the two conditional PDs would incorrectly produce 30%.
Conditional PD is absent (`None`/JSON null) if no live mass remains.

## Numerical propagation, admission and error control

Propagation uses nonnegative uniformization. Choose `nu` as the maximum total
outgoing intensity. With `P = I + Q/nu`,

```text
exp(Q dt) = exp(-nu dt) * sum(k >= 0, (nu dt)^k/k! * P^k).
```

Terms with `nu dt > 8` are split into equal chunks so every Poisson mean is at
most eight. The tail after the current order is bounded by a geometric series
once the order is above the mode. Summation stops only when that bound is at most
`2e-16 * min(Poisson mean, 1)`; this extra relative scaling preserves small exit
probabilities. At most 128 terms are evaluated per chunk. Nonconvergence fails explicitly.
No fast math, renormalization or probability clipping is applied. Every chunk and
full-period output must be finite, nonnegative and conserve probability; the
full-horizon mass tolerance is `1e-11`.

Default absorption internally uses three separate origin buckets. This retains
the live state immediately before default, even after multiple migrations. The
published probability vector aggregates the three default buckets into one state.
At the beginning of the next period only the live mass is propagated; previously
absorbed default/prepaid masses remain unchanged.

Admission bounds are 4,096 terms, at most 100 total model years, at most 100/year
outgoing intensity per origin, and 1,000,000 worst-case uniformization iterations
across the whole request. At most `4096 * 32 * 3 = 393216` recovery rows can be
emitted. These are component limits, not an aggregate runtime memory cap.

## Exposure, expected loss and recoveries

For each live origin `i` and period `k`,

```text
EAD_ki = drawn_ki + CCF_ki * undrawn_ki
defaulted_exposure_ki = marginal_default_probability_ki * EAD_ki
expected_loss_ki = defaulted_exposure_ki * LGD_ki
eventual_recovery_ki = defaulted_exposure_ki * (1 - LGD_ki)
recovery_payment_kil = eventual_recovery_ki * weight_kl.
```

EAD is conditional on being in the origin state immediately before default;
the probability supplies the unconditional weighting. Do not pass an already
survival-weighted balance as EAD. Multiplying that balance by default probability
again would double-count survival.

Each recovery is emitted at `period_end_year + lag_years`, carrying the default
period and live-origin lineage. Recoveries are sorted by payment time, with stable
input period/origin order for ties. The entire tail remains in the result, and
`recovery_after_horizon` identifies nominal payments after the final credit term.
An empty recovery distribution is allowed only when every state LGD is exactly
one. LGD zero produces complete eventual nominal recovery, which may still arrive
after the horizon.

The money reconciliation is
`defaulted_exposure = expected_loss + eventual_nominal_recovery`, subject to f64
roundoff. The component does not discount recoveries, infer accrued interest,
book allowances, post provisions or determine whether a loss is a charge-off.
Those policies belong to explicit product/accounting integration.

## Explicit simplifications and next integrations

Hazards, EAD, CCF, LGD and recovery lags are supplied inputs. There is no inferred
relationship to FICO, collateral, unemployment, housing prices, loan vintage,
borrower income or interest rates. A calibrated feature/scenario layer must
produce versioned terms before this component can be empirically meaningful.
The Markov model has no memory of time spent in delinquency beyond the supplied
current state and term; duration-dependent cure/default behavior requires an
expanded state model or additional fitted term conditioning.

Hazards are integrated continuously, but monetary exposure and loss severity are
constant within each supplied period. All defaults in that period are recognized
at its end for recovery timing. Shorter periods reduce that timing discretization;
they do not change probability propagation when hazards remain constant. Exact
within-period recovery convolution, calendar-dated product cashflows and daily
journal linkage remain future integration work.

This is an expectation model. It supplies no correlated obligor default draws,
portfolio credit-loss distribution, rating-dependent spread repricing, feedback
from commitment draws to borrower state, defaulted-loan cure/redefault, or new
origination. Performing amortization/prepayment principal settlement and interest
cashflows must be generated by product models that share these probabilities.
Using this component alongside a separate independent prepayment/default haircut
would reintroduce double counting and is prohibited for a joint product model.

## Calibration and validation separation

Future calibration should preserve immutable loan/account transitions, censoring
and payoff observations; fit state/cause hazards on training data only; record
the fitted parameter/policy version; and evaluate out-of-time and out-of-segment
cohorts without refitting to holdout outcomes. No production coefficients should
be invented from the hand-built examples in the tests.

Independent empirical validation should assess cause-specific cumulative incidence
and calibration over multiple horizons, migration/cure rates, discrimination,
exposure-weighted default EAD, LGD conditional on default, recovery timing/tails,
and stress extrapolation. Competing payoff exits and loss-data censoring must be
included. Pure default calibration does not establish EAD/LGD accuracy. Portfolio
expectation accuracy does not establish tail-risk accuracy.

`tests/credit.rs` gates implementation against independent closed-form competing
risks, two-state cure dynamics, Erlang migration-to-default, piecewise conditional
versus marginal PD, hand-computed EAD/CCF and recovery cash amounts. It also gates
tiny and large intensities, zero/all loss, interval refinement, long-grid mass
conservation, rejected monetary/probability/time domains, omitted recovery tails,
and strict serde schemas. These prove numerical and contract behavior, not
empirical predictive accuracy or end-to-end ledger publication.

Run the standalone crate gate with
`cargo test --manifest-path packages/portfolio-model-core/Cargo.toml --test credit`.
Product integration will require independent product cashflow, fixed-OAS/CRN,
accounting and persisted-journal publication gates in addition to this command.
