# SVB reconstruction: remaining work

The [historical comparison](2026-10-01-svb-reconstruction.md) and its reference
fixture are complete as an explicitly bounded aggregate experiment. The following
issues track the gaps; merging that experiment does not close or validate them.

| Gap | Issue |
|---|---|
| Bank-only historical balance sheet and intervening cash bridge | [#26](https://github.com/beauschwab/model-portfolio-sim/issues/26) |
| Intraday withdrawal requests, collateral and facility settlement | [#27](https://github.com/beauschwab/model-portfolio-sim/issues/27) |
| Calibrated confidence-driven correlated deposit runs | [#28](https://github.com/beauschwab/model-portfolio-sim/issues/28) |
| Historical security-level repricing and sale-loss attribution | [#29](https://github.com/beauschwab/model-portfolio-sim/issues/29) |
| Deferred-tax, AOCI and CET1 realization bridge | [#30](https://github.com/beauschwab/model-portfolio-sim/issues/30) |
| HTM sales and accounting reclassification consequences | [#31](https://github.com/beauschwab/model-portfolio-sim/issues/31) |
| Legal-entity liquidity restrictions and transfer timing | [#32](https://github.com/beauschwab/model-portfolio-sim/issues/32) |
| Source-backed LCR/NSFR and correct regulatory applicability | [#33](https://github.com/beauschwab/model-portfolio-sim/issues/33) |

Each issue includes evidence, implementation scope, acceptance criteria and links
to the existing product/model backlog. New financial behavior belongs in Rust;
HiGHS remains C++. Mechanical reconciliation, empirical predictive accuracy and
regulatory validation remain separate acceptance gates.
