# Checked native history least squares

The coupon-history and spread-history fitters use the shared private native
`math/checked_least_squares.rs` owner. Stochastic continuation retains its existing
explicit tall-domain wrapper, EPS5 candidate and unchanged admission receipts.
Covariance pseudoinversion and nonlinear trust-region steps are separate callers
and are not adopted by this change.

## Financial and numerical contract

History design columns, intercept position, reduction order, partial-adjustment
clipping, spread clipping, residual population variance and final-output tolerances
are unchanged. Histories first try nalgebra 0.34.2 on the **original matrix shape**,
with the history caller's original EPS1 decomposition parameter. Continuation uses
EPS5. The finite candidate iteration cap is 64. A backward-checked valid candidate
returns its original coefficients without another reduction or projection. Invalid
or nonconverged candidates use direct one-sided Jacobi on the original matrix for
tall/square shapes, or on its transpose for wide shapes. Transposed factors are
swapped back and certified against the actual original matrix and target.

Both paths retain the existing computed cutoff
`EPSILON * max(rows, columns) * largest_computed_singular_value`, with strict
`singular_value > cutoff` retention. The certificate checks reconstruction,
retained-mode orthogonality, the truncation-aware normal residual, retained solve
relations and coefficient projection onto retained right modes. Wide repair need
not manufacture null right vectors. This is a backward check of a computed SVD,
not a claim to determine exact mathematical rank near a floating-point cutoff.
No normal-equation matrix, feature deletion, ridge penalty or Python callback is
introduced. A finite but unrepresentable solve, failed certificate, unsafe rotation
or nonconvergence fails explicitly.

## Before-allocation admission

Let `M=max(rows,columns)`, `K=min(rows,columns)`, `P=K(K-1)/2`.
All arithmetic below is checked before allocating the history design or target.
The units are conservative kernel-row visits, not FLOPs or elapsed-time promises.

```
C = 32 M K^2 + 8 K^3 + 65 (2 K^2 + 8 K)(M+K) + 256*64*K + 8 M K
J = 64 P (2M+K+32) + 16 M K + 12 K^2 + 16 M + 128 K
Q = 3 M K^2 + 2 K^3 + 12 M K + 8 M + 16 K^2 + 64 K
B = 8 (5 M K + 4 K^2 + 4 M + 32 K) + 1024
```

For wide original matrices only, `Qw=Q+4MK+8M+8K`,
`T=4MK+2K^2+16K`, and `Bw=B+8(2MK+2K^2+4M+16K)`.
For tall/square matrices use `Qw=Q`, `T=0`, `Bw=B`.
History preprocessing/report work is `H=8*rows*columns+32*rows+64*columns`.
The complete history ticket is `C+J+2Qw+T+H`, with fixed component limits
4,000,000,000 row visits and 128 MiB. No caller can supply a larger allowance.
The stochastic wrapper's prior `C+J+2Q` and `B` remain byte-for-byte identical.

The five matrix bodies in the history candidate peak conservatively cover the
borrowed complete history, row-major design, column-major actual A, consumed
candidate/bidiagonal and thin U. Additional K^2 and vector allowances cover thin
right factors, library permutation/workspace, target, certificate residual and
projection, coefficient/result copies and headers. Invalid candidate factors and
solve buffers drop before transpose/Jacobi allocation. Wide additional bytes cover
A-transpose, factor-transpose coexistence and longer coefficient/projection vectors.
The original A and borrowed history remain live throughout the repair/certificate.
Transpose and owned numerical-loop tickets debit before growth or iteration.

Coupon histories retain all valid original shapes beginning at two rows and eleven
columns, including wide 2/3/10-row, square 11-row and tall 12-21-row histories.
With eleven columns, 118,437 rows admit (3,999,972,966 visits; 55,909,976 bytes),
whereas 118,438 rows exceed work (4,000,006,734). Spread regressions retain two
columns and admit 1,198,357 regression rows (1,198,358 history observations),
while the next row exceeds 128 MiB. These are shape-budget boundaries, not
statistical adequacy or empirical validation claims.

Owned row/column/mode loops have cooperative checkpoints. The nalgebra candidate
has checkpoints immediately before and after its bounded call; its inner kernels
are not cooperatively interrupted. A direct synchronous C ABI call has no process
watchdog supplied by this helper and no inner-candidate cancellation guarantee.
Existing watchdogs apply only when an actual hosting process supplies one.
The C ABI validates all output descriptors before computation and writes no fit
outputs when admission or numerical verification fails.

## Evidence and remaining boundaries

The retained pre-change actual-public-fitter/NumPy audit contained all 28 outcomes:
24 passed and four had material fitted-coupon/adjustment errors. Two additional
bad-decomposition cases still met the old final-output tolerances and remain
separate evidence. The matching repaired public fitter passes all 28 exact-input
NumPy comparisons at the original tolerances. Twelve shared-helper native controls,
four native history controls, 59 new public ABI controls and 23 retained calibration
controls pass, including wide minimum-norm solves, 4096-row history acceptance,
shape-budget boundaries and unchanged output buffers on failure. Strict risk
all-target Clippy passes. The broader intermediate native run passed 186/188;
two generated-policy tests explicitly refused the new stochastic numerical work
reservation and await their separate raw-law column-bound correction. The initial
architecture run also exposed the sealed stochastic reference import cycle, owned
by that separate correction. These are retained integration gates, not relaxed
limits or new history numerical failures. The prior 29 small-matrix audit passes
do not certify the entire caller domain. Original failure artifacts are preserved.

The repair follows direct one-sided Jacobi principles; it is not represented as a
LAPACK implementation or its accuracy guarantee. Primary algorithm references:
[DGESVJ documentation](https://www.netlib.org/lapack/explore-html/d9/deb/group__gesvj_ga7aec05d2a1523bbeee77ece21b12187c.html)
and [DGESVJ source](https://www.netlib.org/lapack/explore-html/d1/d5e/dgesvj_8f_source.html).
