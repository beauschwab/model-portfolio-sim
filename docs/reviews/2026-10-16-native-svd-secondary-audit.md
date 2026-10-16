# Native SVD secondary caller audit

This read-only audit checks representative matrices from the three other native
SVD callers after the separate commercial stochastic-regression benchmark found
a genuine pricing defect. It changes no production source, financial tolerance,
dependency, official binary or execution policy. It does not clear that release
blocker or certify arbitrary matrices accepted by these callers.

## Caller controls

| Caller and accepted structure | Controls | Result |
|---|---|---|
| Empirical joint-driver selection: fixed 4-by-4 centered covariance, including zero/rank-one/rank-two and differently scaled drivers | 6 | Pass |
| History calibration: tall prior/intercept and ten-feature/intercept regressions, full rank, proportional/duplicate and zero columns | 5 | Pass |
| Nonlinear least squares: regularized augmented Jacobians with known least-squares solutions | 3 | Pass |
| Current-coupon calibration: permitted 2-by-11, 3-by-11 and 10-by-11 wide matrices, five full-row-rank/duplicate/constant/zero-column structures per size | 15 | Pass |

The first fourteen controls use the existing pinned nalgebra 0.34.2 library from
the primary checkout. The separate wide audit uses the same pinned version from
the existing tail-census build. Each standalone audit links its inventoried
library directly; neither creates another Cargo root or target directory.
Both use the actual callers' iteration settings and singular-value cutoffs.

The fixed audit threshold is 1e-12. Checks include reconstruction, the four
Moore-Penrose identities, thin-factor orthogonality, normal residuals, and known
minimum-norm coefficients where supplied. Comparisons divide Frobenius error by
the larger of one and the reference norm. The fourteen-control maximum is
8.162085824632541e-14; the fifteen-control wide maximum is
1.3384282446153818e-14. Genuine regularized least-squares residuals need not be
zero; their analytic coefficients and normal residuals are checked instead.

## Reproducible retained evidence

Ignored evidence is retained under `artifacts/svd-caller-audit/`. Receipts pin the
source commit, caller and dependency sources, exact library, standalone source,
compiler, executable and complete output. Earlier successful subsets remain
unchanged. An initial audit-only generic-type compile error is recorded; it
produced no numerical result.

| Evidence | SHA-256 |
|---|---|
| Fourteen-control receipt, `receipt.json` | `e8e669586449e8392a1a0787710bd31a0dea00a85b143f8c08b9fde3a7535b13` |
| Fourteen-control source, `audit.rs` | `db5172221514a917e2fd7e656685432d0f455e75d7414dbd4c1d96edd82068e4` |
| Fourteen-control output, `results.jsonl` | `d3902aa96029a864bcea9358c51587c4b2109f9056d87770f1e401a6cee0cb28` |
| Wide-control receipt, `wide-receipt.json` | `b9ef1f8ade82b2e8e94407137b7d96ddc4f6682681c7ce8b05a8f5407002db3f` |
| Wide-control source, `wide.rs` | `b013d130efb68578c7b452170bcdbc27a1146beb2e7c4a769a2d9bdafc07e79e` |
| Wide-control output, `wide-results.jsonl` | `32705f02aa92f160536e9d19283326eb8fa33cf995410031941bda265cb04364` |

The checks establish bounded evidence for these matrices, not full nonlinear
convergence, observed predictive accuracy, exact mathematical rank near a
floating cutoff, or general SVD correctness. Existing callers do not themselves
check reconstruction and Moore-Penrose identities. No additional affected caller
is demonstrated here. The independent commercial failure, its native repair,
unchanged benchmark replay and combined release acceptance remain separate gates;
see [the completion ledger](../models/commercial-completion-plan.md).
