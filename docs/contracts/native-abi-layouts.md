# Native product ABI 8 layouts

Generated from `native-abi-8.json`. All fields are borrowed, C-contiguous f64 buffers padded to three shape dimensions. Shapes and relationships are checked in quant::validate; this manifest names the positional contract, not a replacement for domain validation. Inputs must not overlap outputs, outputs are exclusively owned and disjoint, and no pointers survive the synchronous call. All output checks finish before any publication write.

Symbols: P paths, T captured months, S instruments, K CSR events, H reporting months, F forwards, C factors, Q quotes, J shocks. Empty optional buffers retain a live pointer. Integer/bool values are transported losslessly as f64 within the validated domain.

## 1: LMM

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | F0 | F | annual decimal rate/spread |
| 1 | sig_tab | T x F | model convention/parameter; packed fields documented in reference source |
| 2 | B | F x C | dimensionless/index/count |
| 3 | Z | P x T x C | dimensionless/index/count |
| 4 | shift | scalar | annual decimal rate/spread |
| 5 | dt | scalar | years |
| 6 | tenor | scalar | years |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | df | P x T | discount factor |
| 1 | swaps | P x 4 x T | annual decimal rate |
| 2 | short | P x T | annual decimal rate |

## 2: CORPORATE

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | short | P x T | annual decimal rate/spread |
| 1 | swap5 | P x T | annual decimal rate/spread |
| 2 | df | P x T | dimensionless/index/count |
| 3 | per_off | S+1 | dimensionless/index/count |
| 4 | pay_m | K | months/index as specified by reference signature |
| 5 | pay_frac | K | dimensionless/index/count |
| 6 | fix_m | K | months/index as specified by reference signature |
| 7 | fix_w | K | dimensionless/index/count |
| 8 | tau | K | years |
| 9 | prin | K | dimensionless/index/count |
| 10 | call_px | K | dimensionless/index/count |
| 11 | put_px | K | dimensionless/index/count |
| 12 | is_float | S | dimensionless/index/count |
| 13 | cpn | S | annual decimal rate/spread |
| 14 | cap | S | model convention/parameter; packed fields documented in reference source |
| 15 | floor | S | model convention/parameter; packed fields documented in reference source |
| 16 | call_thr | S | model convention/parameter; packed fields documented in reference source |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | discounted_cashflows | K | path sum per unit principal |
| 1 | interest | K | path sum per unit principal |
| 2 | principal | K | path sum per unit principal |

## 3: CD

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | short | P x T | annual decimal rate/spread |
| 1 | df | P x T | dimensionless/index/count |
| 2 | per_off | S+1 | dimensionless/index/count |
| 3 | pay_m | K | months/index as specified by reference signature |
| 4 | pay_frac | K | dimensionless/index/count |
| 5 | acc_m | K | months/index as specified by reference signature |
| 6 | tau | K | years |
| 7 | rem_y | K | years |
| 8 | call_px | K | dimensionless/index/count |
| 9 | rate | S | annual decimal rate/spread |
| 10 | pen_m | S | months/index as specified by reference signature |
| 11 | ew_mult | S | model convention/parameter; packed fields documented in reference source |
| 12 | call_thr | S | model convention/parameter; packed fields documented in reference source |
| 13 | ewp | 5 [base_annual,amplitude,incentive_slope,gap_threshold,cap_annual] | annual hazard fractions, dimensionless amplitude, inverse decimal-rate slope, decimal-rate threshold |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | discounted_cashflows | K | path sum per unit principal |
| 1 | interest | K | path sum per unit principal |
| 2 | principal | K | path sum per unit principal |

## 4: MORTGAGE

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | mtg | P x T | annual decimal rate/spread |
| 1 | hpi | P x T | dimensionless/index/count |
| 2 | yoy | P x T | dimensionless/index/count |
| 3 | df | P x T | dimensionless/index/count |
| 4 | moy | T | month of year (0..11) |
| 5 | season | 12 | dimensionless seasonal multiplier |
| 6 | pp | 9 [refi_max,refi_intercept,refi_slope,burnout_decay,turnover_cpr,cpr_cap,hpa_beta,lock_floor,lock_slope] | annual CPR fractions and dimensionless/decimal-rate prepayment-model coefficients |
| 7 | knots | L | loan-to-value ratio |
| 8 | coefs | 4 x (L-1) | LTV spline polynomial coefficients |
| 9 | smm_lut | lookup grid | monthly prepayment fraction |
| 10 | smm_scale | scalar | model convention/parameter; packed fields documented in reference source |
| 11 | burn_lut | lookup grid | dimensionless burnout multiplier |
| 12 | burn_scale | scalar | model convention/parameter; packed fields documented in reference source |
| 13 | wac | S | model convention/parameter; packed fields documented in reference source |
| 14 | net | S | model convention/parameter; packed fields documented in reference source |
| 15 | wam | S | months/index as specified by reference signature |
| 16 | age | S | months/index as specified by reference signature |
| 17 | oltv | S | dimensionless/index/count |
| 18 | fac | S | dimensionless/index/count |
| 19 | horig | S | model convention/parameter; packed fields documented in reference source |
| 20 | smult | S | dimensionless/index/count |
| 21 | oas | S | annual decimal rate/spread |
| 22 | horizons | H | months |
| 23 | want_fwd | scalar | dimensionless/index/count |
| 24 | rational | scalar | dimensionless/index/count |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | A | S x T | discounted path sum per unit principal |
| 1 | forward_value | S x H | path mean per unit principal |
| 2 | balance | S x H | remaining principal fraction, undiscounted path sum |
| 3 | checkpoint_balance | S x P x H | fraction |
| 4 | checkpoint_burnout | S x P x H | model state |
| 5 | interest | S x T | path sum per unit principal |
| 6 | principal_flow | S x T | path sum per unit principal |

## 5: MORTGAGE_STRESS

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | mtg | P x T | annual decimal rate/spread |
| 1 | hpi | P x T | dimensionless/index/count |
| 2 | yoy | P x T | dimensionless/index/count |
| 3 | df | P x T | dimensionless/index/count |
| 4 | moy | T | month of year (0..11) |
| 5 | season | 12 | dimensionless seasonal multiplier |
| 6 | pp | 9 [refi_max,refi_intercept,refi_slope,burnout_decay,turnover_cpr,cpr_cap,hpa_beta,lock_floor,lock_slope] | annual CPR fractions and dimensionless/decimal-rate prepayment-model coefficients |
| 7 | knots | L | loan-to-value ratio |
| 8 | coefs | 4 x (L-1) | LTV spline polynomial coefficients |
| 9 | smm_lut | lookup grid | monthly prepayment fraction |
| 10 | smm_scale | scalar | model convention/parameter; packed fields documented in reference source |
| 11 | burn_lut | lookup grid | dimensionless burnout multiplier |
| 12 | burn_scale | scalar | model convention/parameter; packed fields documented in reference source |
| 13 | wac | S | model convention/parameter; packed fields documented in reference source |
| 14 | net | S | model convention/parameter; packed fields documented in reference source |
| 15 | wam | S | months/index as specified by reference signature |
| 16 | age | S | months/index as specified by reference signature |
| 17 | oltv | S | dimensionless/index/count |
| 18 | fac | S | dimensionless/index/count |
| 19 | horig | S | model convention/parameter; packed fields documented in reference source |
| 20 | smult | S | dimensionless/index/count |
| 21 | oas | S | annual decimal rate/spread |
| 22 | h | scalar | months/index as specified by reference signature |
| 23 | h_idx | scalar | months/index as specified by reference signature |
| 24 | ck_bal | S x P | model convention/parameter; packed fields documented in reference source |
| 25 | ck_burn | S x P | model convention/parameter; packed fields documented in reference source |
| 26 | rational | scalar | dimensionless/index/count |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | value | S | per unit principal |

## 6: DEPOSIT

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | dep | P x T | model convention/parameter; packed fields documented in reference source |
| 1 | short | P x T | annual decimal rate/spread |
| 2 | vel | P x T | model convention/parameter; packed fields documented in reference source |
| 3 | df | P x T | dimensionless/index/count |
| 4 | age_knots | L | months |
| 5 | age_coefs | 4 x (L-1) | age spline polynomial coefficients |
| 6 | off | S | annual decimal rate offset |
| 7 | base | S | baseline monthly attrition fraction |
| 8 | size_m | S | dimensionless size multiplier |
| 9 | fl_amp | S | dimensionless flight multiplier |
| 10 | fl_b | S | flight response inverse rate scale |
| 11 | fl_g0 | S | annual decimal rate gap |
| 12 | age0 | S | months |
| 13 | svc | S | annual servicing fraction |
| 14 | vel_coef | scalar | model convention/parameter; packed fields documented in reference source |
| 15 | attr_cap | scalar | model convention/parameter; packed fields documented in reference source |
| 16 | oas | S | annual decimal rate/spread |
| 17 | horizons | H | months |
| 18 | want_fwd | scalar | dimensionless/index/count |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | A | S x T | discounted path sum per unit principal |
| 1 | principal_runoff | S x T | undiscounted path sum per unit principal |
| 2 | forward_value | S x H | per unit principal |
| 3 | balance | S x H | fraction |
| 4 | checkpoint_balance | S x P x H | fraction |
| 5 | interest | S x T | path sum per unit principal |

## 7: DEPOSIT_STRESS

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | dep | P x T | model convention/parameter; packed fields documented in reference source |
| 1 | short | P x T | annual decimal rate/spread |
| 2 | vel | P x T | model convention/parameter; packed fields documented in reference source |
| 3 | df | P x T | dimensionless/index/count |
| 4 | age_knots | L | months |
| 5 | age_coefs | 4 x (L-1) | age spline polynomial coefficients |
| 6 | off | S | annual decimal rate offset |
| 7 | base | S | baseline monthly attrition fraction |
| 8 | size_m | S | dimensionless size multiplier |
| 9 | fl_amp | S | dimensionless flight multiplier |
| 10 | fl_b | S | flight response inverse rate scale |
| 11 | fl_g0 | S | annual decimal rate gap |
| 12 | age0 | S | months |
| 13 | svc | S | annual servicing fraction |
| 14 | vel_coef | scalar | model convention/parameter; packed fields documented in reference source |
| 15 | attr_cap | scalar | model convention/parameter; packed fields documented in reference source |
| 16 | oas | S | annual decimal rate/spread |
| 17 | h | scalar | months/index as specified by reference signature |
| 18 | h_idx | scalar | months/index as specified by reference signature |
| 19 | ck_bal | S x P | model convention/parameter; packed fields documented in reference source |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | value | S | per unit principal |

## 8: CSR_OAS

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | offsets | S+1 | dimensionless/index/count |
| 1 | times | K | years |
| 2 | sums | K | model convention/parameter; packed fields documented in reference source |
| 3 | target | S | supplied monetary/notional unit |
| 4 | paths | scalar | dimensionless/index/count |
| 5 | tolerance | scalar | model convention/parameter; packed fields documented in reference source |
| 6 | max_iter | scalar | dimensionless/index/count |
| 7 | lo | scalar | annual decimal rate/spread |
| 8 | hi | scalar | annual decimal rate/spread |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | oas | S | annual decimal spread |
| 1 | pv | S | supplied price unit |

## 9: CSR_PV

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | offsets | S+1 | dimensionless/index/count |
| 1 | times | K | years |
| 2 | sums | K | model convention/parameter; packed fields documented in reference source |
| 3 | spread | S | annual decimal rate/spread |
| 4 | paths | scalar | dimensionless/index/count |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | pv | S | supplied price unit |

## 10: MORTGAGE_BATCH

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | mtg | P x T | annual decimal rate/spread |
| 1 | hpi | P x T | dimensionless/index/count |
| 2 | yoy | P x T | dimensionless/index/count |
| 3 | df | P x T | dimensionless/index/count |
| 4 | scen | P | scenario index |
| 5 | n_scen | scalar | dimensionless/index/count |
| 6 | moy | T | month of year (0..11) |
| 7 | season | 12 | dimensionless seasonal multiplier |
| 8 | pp | 9 [refi_max,refi_intercept,refi_slope,burnout_decay,turnover_cpr,cpr_cap,hpa_beta,lock_floor,lock_slope] | annual CPR fractions and dimensionless/decimal-rate prepayment-model coefficients |
| 9 | knots | L | loan-to-value ratio |
| 10 | coefs | 4 x (L-1) | LTV spline polynomial coefficients |
| 11 | smm_lut | lookup grid | monthly prepayment fraction |
| 12 | smm_scale | scalar | model convention/parameter; packed fields documented in reference source |
| 13 | burn_lut | lookup grid | dimensionless burnout multiplier |
| 14 | burn_scale | scalar | model convention/parameter; packed fields documented in reference source |
| 15 | wac | S | model convention/parameter; packed fields documented in reference source |
| 16 | net | S | model convention/parameter; packed fields documented in reference source |
| 17 | wam | S | months/index as specified by reference signature |
| 18 | age | S | months/index as specified by reference signature |
| 19 | oltv | S | dimensionless/index/count |
| 20 | fac | S | dimensionless/index/count |
| 21 | horig | S | model convention/parameter; packed fields documented in reference source |
| 22 | smult | S | dimensionless/index/count |
| 23 | oas | S | annual decimal rate/spread |
| 24 | delay_y | S | years |
| 25 | rational | scalar | dimensionless/index/count |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | pv | J x S | per unit principal |

## 11: BEHAVIORAL

Variant 0 current coupon:

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | mode | scalar | dimensionless/index/count |
| 1 | swaps | P x 4 x T | annual decimal rate/spread |
| 2 | volatility_features | 6 x T | volatility feature conventions |
| 3 | beta | 11 | regression coefficients |
| 4 | cc_lambda | scalar | model convention/parameter; packed fields documented in reference source |

Variant 1 spread:

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | mode | scalar | dimensionless/index/count |
| 1 | eps | P x T | dimensionless/index/count |
| 2 | ou_parameters | 4 [kappa,theta,sigma,dt] | spread OU conventions; dt in years |
| 3 | initial_spread | scalar | annual decimal rate/spread |

Variant 2 HPI:

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | mode | scalar | dimensionless/index/count |
| 1 | s10 | P x T | dimensionless/index/count |
| 2 | eps | P x T | dimensionless/index/count |
| 3 | hpi_parameters | 4 [mu,beta,sigma,dt] | HPI model conventions; dt in years |

Variant 3 deposit rate:

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | mode | scalar | dimensionless/index/count |
| 1 | short | P x T | annual decimal rate/spread |
| 2 | rate_parameters | 7 [a,bmin,bmax,gamma,f0,lambda_up,lambda_down] | deposit rate model conventions |
| 3 | initial_rate | scalar | annual decimal rate/spread |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | paths | P x T | mode 0/1/3 decimal rate; mode 2 HPI ratio |

## 12: SWAPTION

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | swaps | P x 4 x T | annual decimal rate/spread |
| 1 | df | P x T | dimensionless/index/count |
| 2 | terms | S x 5 [tenor_index,expiry_month,semiannual_periods,sign,strike] | index in emitted [2,5,10,30] year swaps; month index; count; +/-1; annual decimal rate |
| 3 | n_paths | scalar | dimensionless/index/count |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | pv | S | supplied notional unit |

## 13: PROGRAM

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | reference_paths | P x T | model convention/parameter; packed fields documented in reference source |
| 1 | amounts | H | supplied monetary unit |
| 2 | balance_factors | remaining contract months | principal fraction |
| 3 | spread | scalar | annual decimal rate/spread |
| 4 | floating | scalar | dimensionless/index/count |
| 5 | side | scalar | dimensionless/index/count |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | nii | H | supplied monetary unit |
| 1 | balance | H | supplied monetary unit |
| 2 | dv01 | H | supplied monetary unit per bp |

## 14: INCOME

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | cf | S x T | supplied monetary/notional unit |
| 1 | price | S | supplied monetary/notional unit |
| 2 | horizon | scalar | months/index as specified by reference signature |
| 3 | max_iter | scalar | dimensionless/index/count |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | nii | S x H | supplied monetary unit |
| 1 | balance | S x H | supplied monetary unit |
| 2 | yield | S | annual decimal book yield |

## 15: ACCRUAL

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | per_off | S+1 | dimensionless/index/count |
| 1 | acc_m | K | months/index as specified by reference signature |
| 2 | pay_m | K | months/index as specified by reference signature |
| 3 | vals | K | supplied monetary/notional unit |
| 4 | horizon | scalar | months/index as specified by reference signature |
| 5 | is_coupon | scalar | model convention/parameter; packed fields documented in reference source |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | accrual | S x H | supplied monetary unit |

## 16: MONEY_MARKET

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | short | P x T | annual decimal rate/spread |
| 1 | balance | S | supplied monetary/notional unit |
| 2 | spread | S | annual decimal rate/spread |
| 3 | sign | S | dimensionless/index/count |
| 4 | horizon | scalar | months/index as specified by reference signature |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | asset_nii | H | supplied monetary unit |
| 1 | liability_nii | H | supplied monetary unit |

## 17: VOLATILITY

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | queries | Q x 4 | model convention/parameter; packed fields documented in reference source |
| 1 | abcd_parameters | 4 [a,b,c,d] | abcd volatility model parameter conventions |
| 2 | F0 | F | annual decimal rate/spread |
| 3 | dfs | F+1 | dimensionless/index/count |
| 4 | B | F x C | dimensionless/index/count |
| 5 | tenor | scalar | years |
| 6 | shift | scalar | annual decimal rate/spread |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | value_jacobian | Q x 5 [value,da,db,dc,dd] | volatility value and analytic parameter derivatives |

## 18: CURVE

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | tenors | 10 standard pillars | years |
| 1 | rates | 10 standard pillars | annual decimal rate/spread |
| 2 | n_forwards | scalar | dimensionless/index/count |
| 3 | tenor | scalar | years |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | dfs | F+1 | discount factor |

## 19: RATE_PATHS

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | tenors | 10 standard pillars | years |
| 1 | swap_rates | 10 standard pillars | annual decimal rate/spread |
| 2 | abcd_parameters | 4 [a,b,c,d] | abcd volatility model parameter conventions |
| 3 | B | F x C | dimensionless/index/count |
| 4 | Z | P x T x C | dimensionless/index/count |
| 5 | rate_config | 3 [dt,tenor,shift] | years, years, decimal rate |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | df | P x T | discount factor |
| 1 | swaps | P x 4 x T | annual decimal rate |
| 2 | short | P x T | annual decimal rate |

## 20: MORTGAGE_PATHS

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | tenors | 10 standard pillars | years |
| 1 | swap_rates | 10 standard pillars | annual decimal rate/spread |
| 2 | abcd_parameters | 4 [a,b,c,d] | abcd volatility model parameter conventions |
| 3 | B | F x C | dimensionless/index/count |
| 4 | Z | P x T x C | dimensionless/index/count |
| 5 | rate_config | 3 [dt,tenor,shift] | years, years, decimal rate |
| 6 | cc_vol_points | 6 x 2 [expiry,tenor] | years |
| 7 | beta | 11 regression coefficients | regression coefficients |
| 8 | cc_lambda | scalar | model convention/parameter; packed fields documented in reference source |
| 9 | ps_parameters | 4 [kappa,theta,sigma,spot] | OU parameter conventions |
| 10 | eps_ps | P x T | dimensionless/index/count |
| 11 | eps_h | P x T | dimensionless/index/count |
| 12 | hpi_parameters | 3 [mu,beta,sigma] | HPI model parameter conventions |
| 13 | incentive_lag | scalar | months/index as specified by reference signature |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | df | P x T | discount factor |
| 1 | swaps | P x 4 x T | annual decimal rate |
| 2 | short | P x T | annual decimal rate |
| 3 | mtg | P x T | annual decimal rate |
| 4 | hpi | P x T | ratio |
| 5 | yoy | P x T | ratio |

## 21: PARALLEL_SHOCK

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | mtg | P x T | annual decimal rate/spread |
| 1 | hpi | P x T | dimensionless/index/count |
| 2 | df | P x T | dimensionless/index/count |
| 3 | shock_config | 6 [horizon,shock_bp,dt,incentive_lag,cc_lambda,hpi_beta] | mixed months,bp,years,model parameters |
| 4 | beta | 11 regression coefficients | regression coefficients |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | mtg | P x T | annual decimal rate |
| 1 | hpi | P x T | ratio |
| 2 | yoy | P x T | ratio |
| 3 | df | P x T | discount factor |

## 22: FIT_CC

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | history | N x 11 [CC_FEATURES then cc] | explicit observed rates/features |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | beta | 11 | feature regression coefficients |
| 1 | fit_stats | 2 [lambda,r_squared] | dimensionless |

## 23: FIT_PS

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | history | N | observed decimal spread |
| 1 | dt | scalar | years |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | parameters | 3 [kappa,theta,sigma] | OU parameter conventions |

## 24: FACTOR_LOADINGS

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | n_forwards | scalar | dimensionless/index/count |
| 1 | n_factors | scalar | dimensionless/index/count |
| 2 | tenor | scalar | years |
| 3 | minimum_std | scalar | model convention/parameter; packed fields documented in reference source |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | loadings | F x C | dimensionless |

## 25: FIT_VOL

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | vol_pts | Q x 3 | model convention/parameter; packed fields documented in reference source |
| 1 | F0 | F | annual decimal rate/spread |
| 2 | dfs | F+1 | dimensionless/index/count |
| 3 | B | F x C | dimensionless/index/count |
| 4 | initial_parameters | 4 [a,b,c,d] | abcd volatility model parameter conventions |
| 5 | curve_config | 2 [tenor,shift] | years,decimal rate |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | parameters | 4 [a,b,c,d] | abcd volatility conventions |
| 1 | fit_stats | 3 [rmse,evaluations,status] | volatility,count,code |

## 26: FIT_DEPOSIT

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | history | N x 2 [ff,dep_rate] | annual decimal rates |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | parameters | 7 [a,bmin,bmax,gamma,f0,lambda_up,lambda_down] | deposit rate model conventions |
| 1 | fit_stats | 3 [rmse,evaluations,status] | decimal rate,count,code |

## 27: SHARED_DRAWS

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | seed_words | W | uint32 entropy transported as f64 |
| 1 | dimensions | 3 [paths,months,factors] | positive integer counts |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | rates | P x T x C | normal draw |
| 1 | spread | P x T | normal draw |
| 2 | hpi | P x T | normal draw |

## 28: MORTGAGE_RISK

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | tenors | 10 standard pillars | years |
| 1 | swap_rates | 10 standard pillars | annual decimal rate/spread |
| 2 | vol_quotes | Q x 3 | model convention/parameter; packed fields documented in reference source |
| 3 | cc_history | N x 11 [CC_FEATURES then cc] | decimal rates/vol features in CC_FEATURES order |
| 4 | ps_history | N | annual decimal spread |
| 5 | book | S x 13 [wac,net_coupon,wam,age,oltv,factor,fico,avg_loan_size,state_code,channel_code,price,current_face,pay_delay_days] | source book coupon percent/months/LTV percent/fraction/score/money/codes/price percent/money/days |
| 6 | original_hpi | empty or S | HPI ratio |
| 7 | seed_words | W (uint32 entropy) | uint32 entropy transported as f64 |
| 8 | risk_config | 17 [base_paths,sensitivity_paths,months,forwards,factors,dt,tenor,shift,float32_paths,curve_bump,vol_bump,hpi_mu,hpi_beta,hpi_sigma,incentive_lag,ps_spot,rational] | counts/years/decimal rates/model parameters; see RiskConfig |
| 9 | fixed_oas | empty or S | annual decimal spread |
| 10 | month_of_year | T | month of year (0..11) |
| 11 | seasonality | 12 | dimensionless seasonal multiplier |
| 12 | prepay_parameters | 9 [refi_max,refi_intercept,refi_slope,burnout_decay,turnover_cpr,cpr_cap,hpa_beta,lock_floor,lock_slope] | annual CPR fractions and dimensionless/decimal-rate prepayment-model coefficients |
| 13 | ltv_knots | L | loan-to-value ratio |
| 14 | ltv_coefficients | 4 x (L-1) | LTV spline polynomial coefficients |
| 15 | smm_table | lookup grid | monthly prepayment fraction |
| 16 | table_scales | 2 [smm_scale,burnout_scale] | lookup index scale |
| 17 | burnout_table | lookup grid | dimensionless burnout multiplier |
| 18 | cc_vol_points | 6 x 2 [expiry,tenor] | years |
| 19 | fico_x | spline grid | FICO score |
| 20 | fico_y | spline grid | dimensionless multiplier |
| 21 | size_x | spline grid | loan size in source monetary units |
| 22 | size_y | spline grid | dimensionless multiplier |
| 23 | state_multipliers | code lookup vector | dimensionless multiplier |
| 24 | channel_multipliers | code lookup vector | dimensionless multiplier |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | oas | S | annual decimal spread |
| 1 | price | S | price percent of principal |
| 2 | dv01 | S | per unit principal per bp |
| 3 | sensitivities | (10+Q) x S | risk per unit principal |

## 29: MORTGAGE_FORWARD_STRESS

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | tenors | 10 standard pillars | years |
| 1 | swap_rates | 10 standard pillars | annual decimal rate/spread |
| 2 | vol_quotes | Q x 3 | model convention/parameter; packed fields documented in reference source |
| 3 | cc_history | N x 11 [CC_FEATURES then cc] | decimal rates/vol features in CC_FEATURES order |
| 4 | ps_history | N | annual decimal spread |
| 5 | book | S x 13 [wac,net_coupon,wam,age,oltv,factor,fico,avg_loan_size,state_code,channel_code,price,current_face,pay_delay_days] | source book coupon percent/months/LTV percent/fraction/score/money/codes/price percent/money/days |
| 6 | original_hpi | empty or S | HPI ratio |
| 7 | seed_words | W (uint32 entropy) | uint32 entropy transported as f64 |
| 8 | risk_config | 17 [base_paths,sensitivity_paths,months,forwards,factors,dt,tenor,shift,float32_paths,curve_bump,vol_bump,hpi_mu,hpi_beta,hpi_sigma,incentive_lag,ps_spot,rational] | counts/years/decimal rates/model parameters; see RiskConfig |
| 9 | fixed_oas | empty or S | annual decimal spread |
| 10 | month_of_year | T | month of year (0..11) |
| 11 | seasonality | 12 | dimensionless seasonal multiplier |
| 12 | prepay_parameters | 9 [refi_max,refi_intercept,refi_slope,burnout_decay,turnover_cpr,cpr_cap,hpa_beta,lock_floor,lock_slope] | annual CPR fractions and dimensionless/decimal-rate prepayment-model coefficients |
| 13 | ltv_knots | L | loan-to-value ratio |
| 14 | ltv_coefficients | 4 x (L-1) | LTV spline polynomial coefficients |
| 15 | smm_table | lookup grid | monthly prepayment fraction |
| 16 | table_scales | 2 [smm_scale,burnout_scale] | lookup index scale |
| 17 | burnout_table | lookup grid | dimensionless burnout multiplier |
| 18 | cc_vol_points | 6 x 2 [expiry,tenor] | years |
| 19 | fico_x | spline grid | FICO score |
| 20 | fico_y | spline grid | dimensionless multiplier |
| 21 | size_x | spline grid | loan size in source monetary units |
| 22 | size_y | spline grid | dimensionless multiplier |
| 23 | state_multipliers | code lookup vector | dimensionless multiplier |
| 24 | channel_multipliers | code lookup vector | dimensionless multiplier |
| 25 | horizons | H | months |
| 26 | shocks | J | basis points |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | base_value | H x S | supplied monetary unit |
| 1 | base_price | H x S | price percent |
| 2 | shocked_value | J x H x S | supplied monetary unit |
| 3 | pnl | J x H x S | supplied monetary unit |
| 4 | aggregate_base | J x H | supplied monetary unit |
| 5 | aggregate_pnl | J x H | supplied monetary unit |
| 6 | forward_dv01 | H or empty | supplied monetary unit per bp |
| 7 | oas | S | annual decimal spread |

## 30: MORTGAGE_CACHE

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | clear | scalar | dimensionless/index/count |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | statistics | 5 [entries,retained_bytes,hits,misses,evictions] | counts/bytes |

## 31: INTERACTIVE

| Input index | Name | Shape | Unit |
|---|---|---|---|
| 0 | vectors | A x 10 x H (unitlib.METRICS order) | prepared per-unit coefficients |
| 1 | amounts | A | supplied monetary unit |
| 2 | base_kpis | empty or 13 [eve,net_dv01,mv_assets,hqla_l1,hqla_l2,outflow,asf,rsf,cet1,rwa,l2_cap,ni_to_nii,payout] | money/money per bp/research fractions |

| Output index | Name | Shape | Unit |
|---|---|---|---|
| 0 | values | 11 x H (METRICS then funding gap) | supplied monetary units; dv01 row per bp |
| 1 | kpi_path | 3 x H [eve_shock,lcr,nsfr] | percent |
| 2 | scalars | 7 [nii,dv01,eve_shock,duration_gap,lcr,nsfr,cet1] | money,money/bp,percent,years,percent |

## Additive graph-call-control-1 capability

Generated separately from `native-graph-runtime-1.json`; numeric ABI 8 IDs/layouts above remain unchanged.

| Export signature | Result |
|---|---|
| `uint32_t portfolio_graph_runtime_version(void)` | 1 identifies this additive capability |
| `uint64_t portfolio_graph_cancel_new(void)` | Nonzero inert one-shot token; zero is refusal |
| `int32_t portfolio_graph_cancel_signal(uint64_t token)` | 1 succeeds for Fresh or Running; 0 refuses |
| `int32_t portfolio_graph_cancel_release(uint64_t token)` | 1 removes Fresh or Finished; Running and unknown refuse |
| `char* portfolio_term_graph_controlled(const uint8_t* input, size_t length, uint64_t token, uint64_t timeout_ms)` | Owned JSON status/result allocation freed exactly once with portfolio_term_free |

The input range stays live during the synchronous call. All token, graph, invocation and free operations use the originating captured library. Bare u64 IDs may coincide across libraries; native cross-library collision rejection is not claimed.

Only explicit indexed graph/book creation, matching indexed pricing, info and clear are controlled. Each call receives a fresh deadline and one-shot flag; a graph retains neither. Runtime fields are not financial input or cache/source identity.

Bounded thread-safe IO callback, whole-job deadline, durable lease/cancellation and atomic publication fences remain caller-owned. A cold book operation may make two calls. This capability provides cooperative checkpoints, not a hard interrupt or aggregate RSS/scale acceptance.
