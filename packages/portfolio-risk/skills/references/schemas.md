# portfolio_risk input/output schemas

## Published forecast replay (v0.20.1)

`POST /forecasts/preview` and `POST /forecasts/run` accept `snapshot_id`,
`scenario` (baseline/adverse/median), `start_period` (ISO first of month),
`horizon_months` (3..120), `alignment: "relative_replay"`, and
`expected_revision`. Source IDs: fed_stress, fed_sep, philly_spf, nyfed_sme.
Preview JSON exposes monthly driver targets, coverage, warnings and unused
variables. Run uses the existing asynchronous job/Arrow envelope and returns
monthly/base/delta NII, summaries, runoff, drivers and snapshot provenance.
Rates in driver targets are DECIMAL; HPI is a ratio rebased to 1. Snapshot
observations retain their native units. No probability, EVE or capital result
is implied. Active books/markets are never replaced by this route.

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
