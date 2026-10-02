# Reusable native credit and earned-interest operator

`credit-operator-1` compiles an immutable pair of 7×7 matrices for continuous-time
credit transitions and interest earned before default/prepayment. It implements
the shared mathematical primitive needed by joint product pricing and daily
credit accounting. This component does not adopt the model into the daily ledger,
post journal entries, calibrate parameters or complete issue #11.

Implementation and independent tests are in
`packages/portfolio-model-core/src/credit_operator.rs` and
`packages/portfolio-model-core/tests/credit_operator.rs`.

## Public API and immutable identity

```text
compile(&CreditOperatorRequest) -> Result<CreditOperator, String>
operator.apply(&CreditOperatorState) -> Result<CreditOperatorState, String>
operator.apply_in_place(&mut CreditOperatorState) -> Result<(), String>
operator.transport_signed(&[f64;7]) -> Result<[f64;7], String>
```

The request requires `schema = credit-operator-1`, mandatory `measure = physical |
pricing`, `duration_years`, `transition_hazards_per_year` and `accrual_rates`.
Hazards are continuous annual intensities, not annual transition probabilities.
The 3×5 input uses the performing/watch/delinquent origins and
performing/watch/delinquent/default/prepaid destinations of `credit-model-1`.
Diagonal input intensities must be exactly zero.

The compiled operator retains schema, measure and duration and serializes
`transition` and `earned_interest`. Rust getters expose borrowed immutable
matrices. The object cannot be deserialized or constructed from arbitrary public
matrix fields; compilation is the only construction path. Cache keys must retain
the entire request, schema/model version, measure and numerical-policy identity.
A matrix's duration alone does not establish economic identity.

`CreditOperatorState` requires `schema = credit-operator-state-1`, the same
probability measure as the operator, an explicit `monetary_unit` label and five
independent vectors: `probability`, `principal`, `undrawn_commitment`,
`accrued_interest` and `book_adjustment`. No field is defaulted. All serde inputs
reject unknown fields. The monetary label is nonempty, at most 64 bytes and
contains no control characters; amounts are actual declared currency units,
without an implicit thousands/millions multiplier. The operator never converts
currency or a probability measure.

`apply` returns an owned copy. `apply_in_place` is the allocation-free repeated
application path, preserving the existing metadata strings. It calculates and
validates every new vector before committing any mutation. Failure leaves the
original state untouched. Neither method reconstructs a complete term credit
request, normalizes live probabilities, applies a CCF or multiplies monetary
balances by the probability vector again.

## State order, units and ownership

Every internal/output vector and matrix uses this order:

| Index | State |
|---|---|
| 0 | Performing |
| 1 | Watch |
| 2 | Delinquent |
| 3 | Default from performing |
| 4 | Default from watch |
| 5 | Default from delinquent |
| 6 | Prepaid |

Default buckets retain the live state immediately before default, including when
migrations/cures have occurred during the interval. All four exit buckets are
absorbing and earn no new interest. Existing exit balances transport unchanged,
so repeated application cannot default them again.

Probability is nonnegative absolute original-population mass and may sum to less
than one when contractual/ownership exits are tracked externally. It is never
renormalized to the surviving population. Principal and undrawn commitment are
nonnegative aggregate expected monetary amounts. Accrued interest and carrying
adjustments are explicitly signed: negative coupons, payables, discounts and fees
are retained rather than clipped.

**A monetary vector already contains its unconditional population weighting.**
This differs from `CreditRequest.exposure_at_default`, which is conditional
per-account EAD. Applying the probability vector to these amounts again would
count survival twice. The primitive intentionally treats probability and money
independently; the upstream population/position mapping must establish their
relationship. Passing unrelated vectors may be numerically valid without being
an economically valid book.

Contractual maturity, asset sale, disposal and originated-account additions are
outside these seven states. The ledger must track those ownership changes and
the related rights explicitly. It must not label contractual maturity as
prepayment to make this vector fit. Prepayment cash settlement, default-contingent
CCF draws, LGD, recovery schedules, allowance policy and charge-off decisions
remain downstream economic/accounting operations. The component does not claim
CECL, IFRS 9 or regulatory capital compliance.

## Coupled equations and exact interval meaning

Build a seven-state generator `Q`: the three live rows contain supplied
migration intensities, origin-specific default intensities and prepayment
intensities; their diagonals are negative total exit intensity. Absorbing rows
are zero. Let `R` be diagonal with the three signed live-state accrual rates and
zero in the exit states. All inputs are constant within the compiled interval.

For row vectors, principal and accrued interest satisfy

```text
B'(t) = B(t) Q
A'(t) = A(t) Q + B(t) R.
```

The transition matrix and earned-interest matrix are

```text
P(dt) = exp(Q dt)
F(dt) = integral[0..dt] exp(Q s) R exp(Q (dt-s)) ds.
```

Apply them as

```text
probability_out = probability_in P
principal_out = principal_in P
undrawn_out = undrawn_in P
accrued_out = accrued_in P + principal_in F
book_adjustment_out = book_adjustment_in P.
```

Accrued interest therefore migrates and exits with the loan. New interest is
earned only while principal is in an accruing live state, and the new accrual
then follows subsequent state transitions. Zero accrual rate implements a
nonaccrual state for new earnings; it does not automatically reverse existing
receivables or establish a cure/reversal accounting policy.

The reward is not a second discounting operation. The physical measure supplies
physical accrual expectations; a separately supplied pricing measure can produce
pricing expectations for a product model. Relabeling inputs performs no P/Q
conversion. Simple annual principal accrual is not automatically effective-yield
income, market price, deferred-fee amortization, or legal cash payment. Those
models must supply compatible plans and avoid generating the same accrual again.

For a single live state with total exit intensity `h=d+s`, annual accrual `r`,
and interval `t`, independent closed forms are

```text
S = exp(-h t)
B_live = B0 S
A_live = (A0 + B0 r t) S
B_default = B0 d/h (1-S)
A_default = A0 d/h (1-S) + B0 r d/h^2 [1-S(1+h t)].
```

Replace `d` with `s` for prepayment. Total new accrual is
`B0 r (1-S)/h`, which has limit `B0 r t` as `h` tends to zero. Interest computed
only from closing live principal omits interest earned before exit; applying end
survival to all interval earnings misallocates that accrued interest.

Intervals compose with
`P12=P1P2` and `F12=P1F2+F1P2`. This permits daily reuse and subdivision at genuine
parameter changes or economic event times without an arbitrary first-jump rule.

## Numerical construction and primary sources

The block exponential identity

```text
exp([Q R; 0 Q] dt) = [P(dt) F(dt); 0 P(dt)]
```

connects this coupled system with the Fréchet derivative of the exponential.
[Al-Mohy and Higham (2009), section 2](https://eprints.maths.manchester.ac.uk/1218/1/alhi09.pdf)
provides the general block-function identity and motivates retaining the block
structure instead of treating a doubled matrix as an unrelated dense problem.
This implementation uses uniformization for its small CTMC block, rather than
claiming to reproduce their Padé algorithm.

[Van Dijk, van Brummelen and Boucherie (2018)](https://ris.utwente.nl/ws/files/24801192/2018_XDijkBrummelenBoucherie_S0166531617301451_main.pdf)
discuss exact uniformization for CTMC transitions and rewards. The
[Fox–Glynn Poisson probability work](https://web.stanford.edu/~glynn/papers/1988/FG88.html)
emphasizes bounded truncation and numerical range; here the mean is bounded to
eight and a direct Poisson recurrence is sufficient. We do not claim to implement
the full Fox–Glynn algorithm.

Choose `lambda = max(maximum total exit intensity, maximum absolute accrual rate)`.
Let `U = I + Q/lambda` and `C = R/lambda`. `U` is nonnegative and stochastic, and
`||C||_infinity <= 1`. The uniformized 14×14 block matrix is represented by two
7×7 blocks. Its powers obey

```text
P0=I, F0=0
P(n+1)=P(n)U
F(n+1)=P(n)C + F(n)U.
```

The Poisson-weighted sums produce the chunk's transition and reward operators.
The reward powers satisfy `||F(n)||_infinity <= n ||C||_infinity`, even for signed
rates. A probability-only tail bound would therefore be insufficient for accrued
interest.

After retaining order `m`, let `a` be the first omitted Poisson weight and let
`rho = mean/(m+2) < 1`. Explicit upper bounds are

```text
transition_tail <= a/(1-rho)
reward_tail <= ||C|| * a * [(m+1)/(1-rho) + rho/(1-rho)^2].
```

The sum must be at most
`1e-16 * min(mean,1) / [chunks * (1 + max_abs_rate * duration)]`.
The chunk-count/reward factor conservatively accommodates accumulation through
composition; relative mean scaling preserves tiny exit/accrual coefficients.
This controls series truncation separately from floating-point roundoff. It is
not a claim that every small signed result has a universal relative-error bound:
cancellation can make a net signed result ill-conditioned.

At most 128 series terms are evaluated for one chunk with Poisson mean at most
eight. The identical chunks compose by binary powers, with at most 1,250 chunks
and 11 composition levels under the admitted domain. Probabilities are checked
nonnegative and finite, and every compiled row and applied probability vector
must conserve mass within `1e-11`. No probability clipping, renormalization,
fast math or model-output tolerance relaxation is performed.

When all hazards are zero, `P=I` and `F=R*dt` are constructed directly, avoiding
unnecessary Poisson work. Zero duration returns an exact identity, retaining
signed zeros and state metadata on application.

## Bounds and failure behavior

Duration is finite in `[0,100]` model years. Outgoing hazard sums reuse
`credit::MAX_ROW_HAZARD = 100/year`. Signed annual accrual magnitudes are at most
100/year. All probabilities and monetary inputs/outputs are finite. Each monetary
component vector's absolute sum must not exceed the shared
`limits::MAX_MONETARY_AMOUNT = 1e15`; nonnegative vectors cannot contain negatives.
Output earnings exceeding admission fail before state mutation, even when all
inputs individually passed. No result is silently capped.

Compilation and application use fixed-size arrays. This component has no global
cache, callback, mutable registry or internal thread scheduling. The surrounding
runtime owns caching, deadlines, journal publication and total resource admission.
These coefficient/component limits do not establish a total process RSS cap or
large-book throughput claim.

## Validation and remaining integration

Eleven independent tests cover closed-form default/prepay principal and accrued
flux, two-state cures and nonaccrual, semigroup subdivision across 24 repeated
intervals, signed coupons and book vectors, tiny/maximum hazards, long durations,
reward-dominant uniformization, existing absorbed balances, partial probability
mass, deterministic reuse/thread evaluation, strict domains/serde and atomic
allocation-free failure. The closed-form tests do not call `credit::evaluate`
or reuse the uniformization implementation as their oracle.

Run `cargo test --manifest-path packages/portfolio-model-core/Cargo.toml --test credit_operator`
and the crate's all-targets Clippy gate. Native transport tests should also compare
the full 14×14 block exponential against an independent explicit Python/SciPy
reference in test contexts. That independent calculation must not become the
production financial path.

Remaining daily adoption requires actual dated term coverage; explicit maturity,
disposal and origination; state-aware draws, cash settlements and recovery tails;
reviewed allowance/nonaccrual/write-off policies; independent persisted-event and
journal verification; and complete mixed-book scale/deadline acceptance. Product
calibration and empirical validation remain separate requirements. Correct
coefficients alone do not establish a complete product or accounting lifecycle.
