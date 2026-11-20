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

Update 2026-11-16: #31, #32 and #33 are closed in the engine and the
[extended reconstruction](2026-11-16-svb-reconstruction.md) now exercises them as
labelled counterfactuals. For #30 the ledger now has a deferred-tax and AOCI opt-out
policy, and the reconstruction bridges reported AOCI and CET1 with explicit residuals
(about $0.04-0.09bn and $0.06bn); itemizing them needs the disclosed AOCI and capital
notes. For #27 the engine has a standalone minute-resolution settlement replay
([`intraday-settlement-1`](../models/intraday-settlement.md)); it is not applied to SVB
because no timestamped request, collateral-location or facility data is transcribed.
For #26 the reconstruction now opens Silicon Valley Bank alone from its year-end FDIC
Call Report and reports bank-only and consolidated results separately; the March cash
residual is larger on the bank's own opening (about $7.5bn), which locates it in the
unobserved January-March roll-forward. For #28 a
[held-out evaluation](2026-11-16-deposit-run-signals.md) tests year-end 2022 signals on
160 large banks against 2023 outcomes; it shows the signals rank exposure with many
false alarms and that pre-2023 data cannot support estimating a run component. No
deposit model is calibrated. #29 remains open.

Each issue includes evidence, implementation scope, acceptance criteria and links
to the existing product/model backlog. New financial behavior belongs in Rust;
HiGHS remains C++. Mechanical reconciliation, empirical predictive accuracy and
regulatory validation remain separate acceptance gates.
