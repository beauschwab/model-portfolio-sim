# Native SVD secondary caller audit

This read-only audit checks representative matrices from the three other native
SVD callers after the separate commercial stochastic-regression benchmark found
a genuine pricing defect. It changes no production source, financial tolerance,
dependency, official binary or execution policy. It does not clear that release
blocker or certify arbitrary matrices accepted by these callers.

**Subsequent finding:** a separate 28-case current-coupon-shaped collinearity
family finds six failures, including four incorrect solves. The earlier 29
passing controls remain valid for their declared matrices; they did not expose
this additional affected shape. The new evidence and remaining real-caller gate
are recorded below. A commercial-only correction does not fix historical
current-coupon calibration.

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
floating cutoff, or general SVD correctness. Existing callers did not themselves
check reconstruction and Moore-Penrose identities. The subsequent family below
demonstrates an additional affected caller. The independent commercial failure, its native repair,
unchanged benchmark replay and combined release acceptance remain separate gates;
see [the completion ledger](../models/commercial-completion-plan.md).

## Subsequent current-coupon-shaped collinearity family

All 28 controls cross row counts 16, 22, 32, 64, 128, 256 and 512 with four
deterministic rank-two, eleven-column designs. Every design has the real
caller's intercept in the last column. The first two designs contain normalized
`x,x` or `x,7x` and eight zero features. The other two use positive affine rate
and volatility features, with differing or repeated loadings. The target is
`0.03 + 0.001*x`. These are mathematical controls, not observed histories or a
calibrated mortgage model.

The standalone audit uses the actual history caller's pinned SVD, epsilon,
100,000-iteration setting and unchanged floating rank cutoff. It retains every
outcome against the fixed 1e-12 reconstruction/solve/normal-residual limits.

| Rows and design | Reconstruction error | Solve residual | Finding |
|---|---|---|---|
| 22, duplicate normalized features | 1.07156 | 0.00296750 | Incorrect fit |
| 64, duplicate normalized features | 1.56045 | 0.00628050 | Incorrect fit |
| 256, proportional normalized features | 0.419050 | 0.00243050 | Incorrect fit |
| 512, proportional normalized features | 0.343871 | 0.00243541 | Incorrect fit |
| 128, positive affine features | 4.70846e-7 | 7.59703e-14 | Invalid reconstruction; small solve residual |
| 512, positive affine features | 4.11110e-7 | 1.15846e-13 | Invalid reconstruction; small solve residual |

Errors use the normalization defined in the source, with reference norms floored
at one. Twenty-two controls pass. The exact source, all 28 outputs, executable
and status are retained separately, without replacing the earlier evidence:

- `coupon-collinearity.rs`: SHA-256
  `a297adb5029c3186b17f45ddf2cf26f7e90afec44d9af2db853616b1f7d28629`.
- `coupon-collinearity-results.jsonl`: SHA-256
  `60387023aeccda43f2ae94773b7343ef4e3200ba0b21fbfab41b2d67c4a92d7d`.
- `coupon-collinearity.exe`: SHA-256
  `00292ef0e52c62502dc15a9655b322eae1b7a482c81b0cdcf05d35e37089a239`.

The same 28 controls subsequently ran through the actual public
`fit_current_coupon` function against independent NumPy 2.4.6. All native calls
returned success, but four returned incorrect financial fits: the first four
rows above. The largest fitted current-coupon error was 0.001338685850604
(13.3869 basis points); its fitted adjustment speed was 0.02 instead of 1 and
R-squared was 0.383676257 instead of 1. The other two reconstruction failures
retained financial outputs within the existing comparison tolerances; they are
not counted as additional financial failures.

`coupon-actual-fit-receipt.json` pins all 28 original public-call results and the
independent comparison, SHA-256
`2a0d6d4d2a0c052ab049cb31498577e709f29f9b5733beb7bcd7636a71aa48bc`.
The shared correction must preserve each caller's conventions and pass matching
release validation. Tall commercial training and wide historical regressions
have different admitted shapes; adopting the commercial-only wrapper blindly
would change the history caller's contract. The original failure evidence is
retained separately from subsequent repair controls.
