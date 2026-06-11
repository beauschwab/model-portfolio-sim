# AGENTS.md — tests

What each test GATES. If your change touches the left column, the right
column must pass — and "loosen the tolerance" is never the first answer
(one tolerance was tightened this project, none loosened; one assertion
was corrected because the ENGINE was right and the test was wrong —
callable negative dv01 — and that correction is documented inline).

| change | gating tests |
|---|---|
| kernels.py either MODEL-BLOCK | test_zero_shock_invariant (engine vs stress_engine must agree exactly at zero shock) + test_stress_pnl_signs |
| deposits DEPOSIT-MODEL-BLOCK | test_deposit_zero_shock_invariant |
| Padé / LUT / sigmoid numerics | test_rational_sigmoid_oas_accuracy (<0.05bp asserted; measured 0.007bp) |
| pricing / solver | test_oas_roundtrip (+ every product's end-to-end reprice check) |
| conventions | test_day_counts (hand-computed values), test_us_holidays_and_adjust (rule-generated 2026 dates incl. Good Friday/observed July 4), test_schedule_taus_sum |
| corp engine / deck | test_cd_matches_corp_bullet (cross-engine), test_callable_below_bullet / puttable ordering, test_sink_schedule_shortens_duration, test_exact_time_discounting (12-day maturity shift must move price ~13bp), test_exact_fixing_interpolation (1e-12 vs hand-computed on controlled paths) |
| deposit rate model / fitter | test_rate_model_fit_recovery — asserts the IDENTIFIABLE objects (lambdas + equilibrium function pointwise ≤35bp), not the raw (b_max,k,pivot) triple |
| attrition / deposit engine | test_balance_conservation_and_wal (runoff sums to 1 at 1e-9), test_segment_flight_ordering |
| CD options | test_cd_withdrawal_option_raises_liability_value, test_cd_issuer_call_lowers_liability_value |
| ModelSuite / registry | test_custom_prepay_model_swaps_in, test_stress_rejects_custom_prepay |

## Patterns to copy when adding tests

- **Zero-magnitude invariant**: any new stress/checkpoint path gets a
  test where the zero shock must reproduce base values (rtol ≤1e-6).
- **Controlled-path hand replication**: build a 1-path market with a
  deterministic ramp and match the engine to a hand-computed price at
  ~1e-12 (see test_exact_fixing_interpolation) — the strongest pattern
  here for cashflow-logic changes.
- **Cross-engine degenerate contract**: new engine with all options off
  must reprice an existing engine on identical terms and paths.
- **Fit recovery on a known generator**: assert identifiable quantities;
  if parameters are weakly identified, test the FUNCTION they define and
  say so in the docstring.
- Keep fixtures module-scoped and small (≤200 positions, ≤128 paths);
  JIT dominates wall time, not the math.

## Gates added v0.6-v0.14
| change | gating tests |
|---|---|
| deposit engines / fitter | test_rate_model_fit_recovery (joint NLS, identifiable objects), test_balance_conservation_and_wal, test_deposit_zero_shock_invariant |
| CD engine / options | test_cd_matches_corp_bullet (cross-engine), withdrawal/call ordering tests |
| accruals / accounting | test_book_yield_par_bond, test_balance_sheet_nii_end_to_end, test_nim_reconciles_to_reported_with_basis_and_mm (the 2.94%-vs-2.47% ladder) |
| kpis | test_kpis_end_to_end (EVE == equity plug, density 59.6%, CET1 path) |
| hedges | test_swap_pricing_and_parity (par MtM~0, payer/receiver mirror, swaption parity at 1e-12), test_hedge_book_cuts_irrbb_outlier |
| strategies / unitlib | test_strategy_at_market_carry_and_reinvestment, test_unitlib_interactive_kpis (linearity exact, KPI directions) |
