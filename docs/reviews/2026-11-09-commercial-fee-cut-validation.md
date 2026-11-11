# Ordinary commercial fee cuts and cross-platform regression

The implementation in [PR #123](https://github.com/beauschwab/model-portfolio-sim/pull/123)
includes main `21b6189`, complete ordinary fixed/fee source inventories, and bounded
verified predecessor chains. The [fee contract](../models/commercial-fixed-fee-cut.md)
records ownership, admission and publication behavior. Issues
[#15](https://github.com/beauschwab/model-portfolio-sim/issues/15) and
[#25](https://github.com/beauschwab/model-portfolio-sim/issues/25) retain the broader
product, adoption, scale and evidence requirements.

## Numerical and regression corrections

The Jacobi solve can encounter a finite rotation angle that underflows to zero.
That step now retains both columns unchanged and continues; it does not delete a
mode, relax the original rank cutoff, or bypass reconstruction/minimum-norm
certification. The failing native regression was reproduced before the correction.
Windows native validation passes 719 tests and strict all-feature Clippy.

Windows exact floating-point fixtures remain unchanged. A separately captured
GNU x86_64 Linux fixture records complete IEEE bits from the pre-correction native
source. All eight Linux cases were independently checked against the existing
NumPy tolerances before accepting that platform fixture. Platform-specific exact
comparisons remain exact; these fixtures do not establish observed accuracy.

Two regression assertions also depended on checkout/platform representation:

- The source fixture identity now pins LF-normalized fixture bytes. Production
  artifact hashes and raw-source verification still use their existing identities.
- The cut refusal census pins structural work independently of native decimal
  spelling. Every embedded receipt byte contributes two preparation scan visits,
  then one encoded byte and one child-cell bound in each of three final-result
  scans: eight visits in total. The test independently pins all three final shape
  dimensions and the remaining work; the exact independent verifier and all work
  and memory ceilings remain unchanged.

The first receipt-length correction counted only two of those eight visits. Its
Linux run retained one failed assertion, 4,165 passing tests and three skips. The
complete correction passes all 30 focused controls against the rebuilt artifacts.
The failed run remains evidence of that earlier test source, not a passing run.

## Evidence and completion

Final test results, exact head and integration status are recorded in PR #123 and
the issue status sections. Local immutable logs, source inventories and installed
artifact hashes are in `artifacts/full-suite-2026-11-09-main-sync/`; earlier
interrupted runs remain in their separate dated directories. Release artifacts
were rebuilt after merging main. The full matrix checks source and artifact
identities before and after each gate and covers engine/API/native/MCP/browser,
build and generated-contract checks.

A later change confined to the census regression test is validated separately
while the broader local matrix retains its original frozen source. Its Git delta
must identify that single test; runtime validation is applicable only where the
source and executable identities match. Hosted Windows/Linux full engine checks
cover the corrected assertion. A pending or failed run is never a full-suite pass.

Local validation and synthetic histories do not establish deployment, observed
predictive accuracy, reviewed accounting/regulatory approval, advanced-owner cut
adoption or combined 60,000-position graph/credit/solver/journal acceptance.
