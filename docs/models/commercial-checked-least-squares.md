# Checked native continuation least squares

The private `stochastic_exercise/checked_least_squares.rs` wrapper corrects an
observed native SVD defect while retaining the commercial tall-domain contract.
Its implementation is shared with the separately bounded
[history least-squares owner](native-history-least-squares.md). Covariance and
nonlinear calibration callers retain their existing algorithms.
There is no Python execution path, new dependency, caller numerical allowance,
model selector, normal-equation solve, or approximate feature removal.

## Retained finding

The original Gaussian/credit benchmark completed all 48 declared cells: 36 passed
and 12 failed the unchanged native/independent coefficient comparison. The failed
cases include different native stopping decisions and monetary outcomes. Raw
requests, outputs, both original binaries and failures remain immutable in the
benchmark receipts. Three failures also reproduced on the optimized combined
release build. The initial release validation was therefore invalidated.

An audit captures the exact native matrix, targets, feature rows, centers and
scales immediately before decomposition. All six distinct failing matrices match
independent raw-source construction. Each normalized column has squared norm equal
to the retained live-row count. Nevertheless, the pinned nalgebra 0.34.2 factors
have relative reconstruction errors from approximately 0.19 to 0.38. A deterministic
16-by-3 example also fails. Explicit EPS/100000 convergence settings return the
same incorrect factors; this is not resolved by changing convergence epsilon.
The retained audit distinguishes correct feature construction from incorrect
decomposition and solve output.

## Numerical contract

For `m` retained live training rows and `k` actual basis columns, `m >= 2k`, the
helper first tries the same nalgebra arithmetic, ordering and `5*EPS` convergence
setting, bounded to 64 total iterations. The actual original matrix remains
available. A passing candidate returns its original coefficients and singular
values without recomputation or rounding changes. A candidate that exceeds the
iteration bound, fails its actual-matrix checks, or has nonfinite solve output is
replaced by a native direct one-sided Jacobi decomposition. This corrects existing
supported requests rather than leaving an optional known-incorrect execution lane.

Jacobi scales the original matrix by its maximum absolute element, rotates every
column pair in deterministic order, accumulates the right rotations, and caches
safe `hypot` column norms. Updated norms are computed in the actual rotation pass.
Angles use scaled norm ratios and normalized compensated inner products. A zero
norm is distinct from an unrepresentable nonzero rotation. The latter fails
explicitly. At most 64 sweeps, including a no-change convergence sweep, execute.
There is no silent acceptance of nonconvergence. Sorting is deterministic insertion
sort. Coefficient projection uses mantissa/exponent multiplication and division to
avoid premature intermediate overflow or underflow when the result is finite.

The singular cutoff remains exactly
`EPS * max(m,k) * largest_computed_singular_value`, with strict `s > cutoff`
retention. The solve is minimum norm in the computed retained right-singular
subspace. No final financial, coefficient, CRN, fixed-OAS or publication tolerance
changes. The independent financial oracle continues to use NumPy's SVD solve.

Checks cover finite ordered nonnegative singular values, reconstruction of the
actual original matrix, retained left-vector orthogonality, right-vector
orthogonality, retained-mode solve equations, normal-gradient residual and
coefficient projection into the retained right span. Derived metrics and limits
must themselves be finite. The normal-gradient limit explicitly allows the
residual implied by the unchanged truncated singular cutoff. Internal backward
checks use dimension-scaled machine precision; they do not replace financial
comparison tolerances.

This is a backward-checked computed decomposition, not a certificate of exact
mathematical singular rank. A spectrum arbitrarily close to the cutoff is
inherently sensitive to floating-point perturbations. Diagonal controls below,
at and above the cutoff and perturbed near-collinear matrices retain this boundary
explicitly; no threshold is shifted to force a desired classification.

Execution aborts have a separate internal error variant from numerical rejection.
A one-shot abort propagates once, with its original error, without entering a
repair. Native checks run before/after the bounded external candidate and between
owned pairs, matrix rows, projection modes and check columns. The external
64-iteration nalgebra call has no internal cooperative checkpoint; its finite
kernel bound and the existing process watchdog remain distinct from fine-grained
cancellation.

## Work and workspace before generation

The admission metric counts bounded kernel row/element iterations and bounded
scalar bookkeeping. It is neither a CPU-time guarantee nor a count of hardware
floating-point instructions. Let `I=64`, `J=64`, and `P=k(k-1)/2`. The private
typed summary reserves

```
C = 32*m*k² + 8*k³ + (I+1)*(2*k²+8*k)*(m+k) + 256*I*k + 8*m*k
Jb = J*P*(2*m+k+32) + 16*m*k + 12*k² + 16*m + 128*k
Q = 3*m*k² + 2*k³ + 12*m*k + 8*m + 16*k² + 64*k
numerical_work = C + Jb + 2*Q
workspace_bytes = 8*(5*m*k + 4*k² + 4*m + 32*k) + 1024
```

`C` covers bidiagonalization, thin left/right construction, the initial delimiter
plus every bounded QR/delimiter iteration, including nested cancellation sweeps,
sorting, solve and copies. A candidate invocation is charged its complete
structural ticket; it is not presented as an instrumented dependency loop count.

Each Jacobi pair charges the normalized dot pass `m`, actual rotated-column/norm
pass `m`, right-rotation pass `k`, and bounded scalar bookkeeping. Lower-order
terms cover the original maximum scan, scaled copy, initial norms, identity and
zero initialization, ordering/sorting, sorted left/right assembly, target scan,
coefficient projection and output copies. These terms also cover `k=1`, where
there are no pairs. `Q` covers finite scans, elementwise reconstruction without a
full reconstructed-matrix allocation, both orthogonality products, residual,
gradient, retained-mode equations and minimum-norm projection. Owned category
tickets are checked before the associated loops, not only asserted afterward.
All capacity arithmetic and shape checks fail without integer wrapping.

The workspace includes the caller's original row data/matrix, consumed candidate
bidiagonal body and thin left vectors; there is no `m²` left-vector allocation.
Invalid candidate factors are dropped before Jacobi allocates its scaled body,
right rotations and separately sorted left/right factors. Residual/target,
coefficient, norm, permutation and library work vectors have separate dimensional
terms. The fixed 1024-byte cell covers concrete matrix/vector headers and summary
objects; the dimension terms conservatively cover their simultaneously live
payloads. These are retained-component bounds, not an aggregate RSS promise.

Standalone admission retains its historical normalization/basis work ticket and
adds the numerical work for every actual training fit. The workspace is added
once because fits execute sequentially. Zero exercise dates add exactly zero
numerical work and workspace. OAS calibration adds a complete numerical ticket
for every reserved retained-sample refit; it does not regenerate source samples.
Actual basis columns must fit the admitted bound before matrix allocation.
The 4-billion-work and 128-MiB caps, sample minima and financial tolerances remain.
Additional honest reservation changes admission metadata and can expose a real
capacity refusal; it is never concealed behind an old receipt.

The first complete corrected run retained 48/48 Gaussian-credit, 60/60 constant
exercise and 24/24 option-free credit passes. Its unchanged 107-cell general
matrix retained four unexpected cubic-256 capacity refusals in addition to the
original declared larger refusal: the generic 128-column ticket at two exercise
dates added 2,763,997,184 work to 1,400,901,632 historical work. No expectation was
downgraded. That source, executable and full failure receipt remain sealed.

The subsequent structural refinement shares the existing OAS-trial raw-law bound
with ordinary numerical admission. Draw catalogs keep the full feature upper.
With no positive declared credit hazard and zero market covariance, every path's
feature history is deterministic, giving zero active features. With no credit
hazard and supplied principal payments, only current market drivers, optional
hazard-anchor drivers, every observation, net legal interest and each period's
legal amount may vary. Principal, live-origin flags and observed coupon nominal
are constant across samples at each contractual observation clock. This gives
`d + optional_d + observations + 1 + periods`. Other cases keep the full upper.
These are structural constants proved from raw law; no generated sample, fitted
coefficient, monetary netting, future outcome or observed feature correlation
affects admission. Every actual feature, normalization and polynomial basis stays
unchanged, and actual basis columns must fit before allocating matrix data.

The old normalization/basis ticket and all source passes remain charged. Only
the new numerical kernel/workspace dimension uses this proof. The four declared
cubic-256 cases have bounds 56/84 columns, while the larger declared refusal and
all unchanged numerical checks remain required. The complete family is replayed
under a new source/artifact receipt rather than replacing its failed predecessor.

The structural refinement pays its own metadata work. A sealed local census first
debits the draw-header inventory before reading nested vector lengths. It then
checks a ticket before traversing hazards, covariance and period headers:
`S = 32 + 15*intervals + covariance_cells + 2*all_contract_periods + 8*draws`.
Observation **values**, strings, hashes and future realized states are not read;
only typed observation-vector lengths enter the structural inventory. The hazard
fact means literal `all(h == 0)`, rather than the absence of a positive value.
Owned scan counters must fit this header-derived ticket.

Each represented ordinary numerical role adds `8*S` work plus 128 bytes for its
structural summary, capacity and ledger headers. Standalone exercise traverses
the census three times; public graph adds an outer audit, and unit/candidate
preparation can add two further audits. Eight tickets conservatively cover this
concrete chain. Original and revised graph components remain separately charged.
Each reserved OAS trial also retains eight tickets, covering its one actual
retained-sample census and fixed admission overhead. Actual ordinary and
calibration call counts have targeted controls. No-fit routes skip this new
census and add zero. These are explicit additions, not deductions from the old
normalization/basis ticket or assumed slack in the numerical kernel allowance.

The combined fee owner retains all eleven observed coordinates per fee. Its
structural bound can exclude those coordinates from the *active upper* only
when the exact same raw role has literal zero hazards in every declared interval
and origin, Supplied principal, no draws, no reviewed credit actions and no loan
or fee transfer. Fixed supplied fee events and the unstopped deterministic loan
exit clock then produce identical fee observations across market paths at each
decision clock. Values may differ between decision dates. Positive market
covariance and changing coupon values do not enter this separate service state.
The paid typed census supplies the hazard fact; the fee proof reads only bounded
headers, within the explicit structural scan allowance. Every original/current
role recomputes it. Unsupported headers retain the full feature upper, including
all fee coordinates. No sampled equality supplies authority.

This proof applies only to the unstopped observations. Each genuine stopped
loan/fee trial, selected stop, source validation, full fee cash/rights tail and
the original normalization/basis work still executes and remains charged. Every
observed feature stays in its original order, with retained means/scales and
the existing exact-constant classification. Signed prepaid/earned-refund controls
use explicit complete exit policies and test both coincident cash timings and
multiple observation dates. The independent raw-law predicate separately checks
off-origin, late, smallest-positive and invalid hazards, ownership/policy mutations
and per-role recomputation; invalid raw inputs gain no financial authority.

## Compatibility and remaining gates

Valid historical candidate decompositions retain identical financial values and
coefficient/spectrum bits. A corrected defective decomposition can legitimately
change fitted coefficients, decisions, cash and sampling diagnostics. Saved
identities, native artifacts and libraries/sessions require the normal explicit
rebuild; prior faulty saved economics must not be silently reused.

Complete baseline/corrected JSON comparisons must isolate numerical admission
metadata from every other field. No-fit paths must remain byte-identical. All
declared 48 Gaussian/credit cells, prior 60 constant-state cells, the prior credit
and general stochastic benchmark families, native regression controls and strict
checks remain required after integration. The combined generated-fee family also
needs a matching combined artifact; this isolated base predates that feature.
Broader empirical validation, general convergence, cutoff-adjacent exact-rank
claims, arbitrary finite-matrix completeness and the original fraction-zero signed
gross-right valuation/adoption boundary remain open.

## Primary numerical references

[nalgebra's pinned SVD contract](https://docs.rs/nalgebra/0.34.2/nalgebra/linalg/struct.SVD.html)
documents its iteration limit and singular-cutoff solve. The implementation used
in the retained reproduction is the locked 0.34.2 source, not an unmerged proposed
dependency patch. A pinned pure-Rust faer SVD was considered; adopting it would
add dependency and internal iteration/cancellation audit scope, so it is not
included in this narrow correction. Rank-revealing QR alone would not preserve the
existing singular-value cutoff/minimum-norm contract without an additional SVD.

[LAPACK DGESVJ](https://www.netlib.org/lapack/explore-html/d9/deb/group__gesvj_ga7aec05d2a1523bbeee77ece21b12187c.html)
provides primary guidance for scaled column norms, finite Jacobi sweeps,
nonconvergence reporting and retaining meaningful left vectors. This private Rust
implementation is not LAPACK and does not claim its accuracy guarantees.
