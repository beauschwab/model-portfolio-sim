# mbs_risk input/output schemas

All frames are Polars. Rates, spreads, coupons, vols are DECIMALS
(0.0408 = 4.08%). Prices are % of par (98.5) in frames.

## Portfolio frame (required by run_risk / run_stress / setup)

| column | type | meaning |
|---|---|---|
| cusip | str | identifier |
| current_face | f64 | current balance, $ (drives all $ risk) |
| factor | f64 | pool factor (current/original balance) — CLTV back-out |
| wac | f64 | gross weighted-avg coupon, decimal |
| net_coupon | f64 | passthrough coupon paid to investor, decimal |
| wam | f64 | remaining term, months |
| age | f64 | loan age, months |
| oltv | f64 | original LTV, decimal (0.80) |
| fico | f64 | weighted-avg FICO |
| avg_loan_size | f64 | $ |
| state | str | dominant state code (unknown -> multiplier 1.0) |
| channel | str | "R"/"B"/"C" retail/broker/correspondent |
| price | f64 | market price, % of par — OAS solve target |
| hpi_orig_ratio | f64 | OPTIONAL: H_settle/H_orig; defaults to (1+HPI_MU)^(age/12) |

## Market inputs

- `swap_rates`: np.ndarray (10,) par swap rates at tenors
  [1,2,3,4,5,7,10,15,20,30]y, annual fixed leg, decimals.
- `vol_pts`: np.ndarray (9,3) rows = (expiry_y, tenor_y, ATM lognormal vol).
  Default grid: expiries {1,3,5} x tenors {2,5,10}.

## Model-fit histories (monthly, oldest first)

- `cc_hist` columns: `cc` (secondary current coupon), `s2 s5 s10 s30`
  (par swap rates), `v0..v5` (six ATM swaption vols matching
  config.CC_VOL_POINTS order: 1x10, 2x10, 5x10, 1x5, 3x7, 5x5).
- `ps_hist` columns: `ps` (primary minus secondary spread, decimal).

Fitters print diagnostics: CC lambda + fair-value R2, PS kappa/theta/sigma.
Sanity: lambda in (0.2, 0.6) and R2 > 0.9 typical; PS kappa O(1-5)/yr.

## Outputs

### run_risk -> portfolio frame plus:
| column | units |
|---|---|
| oas_bps | bp |
| model_price | % of par (reprices `price` to 1e-8) |
| dv01 | $ per 1bp parallel (sum of KRDs) |
| krd01_1y ... krd01_30y | $ per 1bp at that pillar (10 cols) |
| vega_1x2 ... vega_5x10 | $ per 1 vol point (9 cols) |

### run_stress -> (pos, agg, prof)
- `pos`: cusip, horizon_m (1..27), shock_bp, fwd_value_base ($),
  fwd_price_base (% of par on forward balance), fwd_value_shock ($),
  stress_pnl ($).
- `agg`: horizon_m, shock_bp, pnl_$, base_mv_$.
- `prof`: horizon_m, fwd_dv01_$ (present only if shocks include +/-100).

Sign conventions: positive KRD = long duration (gains when rates fall);
up-shocks produce negative stress_pnl for a long book.

## Deposit book (run_deposit_risk)
| column | type | meaning |
|---|---|---|
| id, balance | str, f64 | cohort id, $ balance |
| segment | str | DDA / NOW / SAV / MMDA (drives attrition params) |
| age_months | f64 | cohort account age |
| avg_account_size | f64 | $ (size multiplier; big balances fly) |
| rate_paid | f64 | current rate, decimal (anchors the path offset) |
| svc_cost | f64 | OPTIONAL annual servicing cost, decimal |
| price | f64 | liability price % (96.5 => 3.5% premium) -- OAS target |

dep_hist (monthly): `ff`, `dep_rate` (decimals). Outputs add: oas_bps,
model_price, premium_pct, wal_y, dv01, krd01_*, vega_*. Positive dv01 =
liability value rises when rates fall (sticky books). Heed the fitter's
plateau warning: equilibrium beyond the visited ff range is extrapolation.

## CD book (run_cd_risk; needs asof date, no fitted histories)
| column | type | meaning |
|---|---|---|
| id, balance | str, f64 | contract id, $ balance |
| rate | f64 | contractual fixed rate, decimal |
| maturity | date | contractual maturity |
| freq_months | int | coupon frequency; 0 = interest at maturity |
| daycount | str | "30/360" / "ACT/360" / "ACT/365F" / "ACT/ACT" |
| channel | str | "retail" (withdrawal on) / "brokered" (call honored) |
| penalty_months | f64 | early-withdrawal interest forfeiture |
| price | f64 | liability price % -- OAS target |
| call_schedule | list[(date,px)] | OPTIONAL issuer calls (brokered) |
| ew_mult, call_threshold | f64 | OPTIONAL behavioral overrides |

Outputs: oas_bps, model_price, dv01, krd01_*, vega_*. Callable CDs may
show locally NEGATIVE dv01 near the exercise boundary (rates down ->
called at par caps value) -- correct, do not "fix".
