# Consumer behaviour: states, covariates and driver specification

Status: **proposal** for issues #91 (product transition models), #92 (joint
consumer drivers), #93 (card behavioural EAD), #94 (auto collateral LGD) and
#95 (forecast-error-aware lifetime loss), under tracker #25. Nothing here changes
saved economics or an existing contract. It fixes the definitions a bank panel
must be mapped to so that `credit-covariate-1`, the interval-censored panel
estimator and `joint-market-credit-1` can be applied to card, auto and personal
books consistently.

The structure follows the competing-risk, multi-state approach used in the
[Federal Reserve's supervisory consumer models](https://www.federalreserve.gov/publications/files/2026-february-supervisory-stress-test-methodology.pdf):
delinquency-bucket transitions with cures and competing payoff, loss severity and
timing modelled separately, card exposure including additional borrowing. The
estimators are this repository's exact-likelihood continuous-time models, not the
Fed's quarterly logits, and no supervisory coefficients are reproduced. Lifetime
loss outputs must carry macro-sensitivity because, as the
[Philadelphia Fed (WP 26-09, Feb 2026)](https://fedinprint.org/item/fedpwp/102431/original)
shows on consumer data, lifetime estimates are highly sensitive to macro forecast
error and misspecification.

## 1. Shared state mapping

All products map to the fixed `credit-model-1` order
**performing, watch, delinquent, default, prepaid**. The mapping is a declared,
versioned policy per product; the engine never infers it from a label.

| Product | performing | watch | delinquent | default (absorbing) | prepaid (absorbing) |
|---|---|---|---|---|---|
| Auto | current, 1–29 dpd | 30–59 dpd | 60–119 dpd | charge-off at 120 dpd, repossession, bankruptcy | voluntary payoff, refinance, trade-in |
| Personal | current, 1–29 dpd | 30–59 dpd | 60–119 dpd | charge-off at 120 dpd, bankruptcy, settlement | voluntary payoff, consolidation |
| Card | current, 1–29 dpd | 30–59 dpd | 60–179 dpd | charge-off at 180 dpd, bankruptcy | account closed with zero balance |

Rules:

- Bucket edges are policy inputs and may differ by bank; the declared edges are
  retained in the fit identity. Regulatory charge-off conventions are the usual
  120/180 dpd defaults, supplied not assumed.
- Cure is any move to a better live state. Partial payments that leave the
  account in the same bucket are not transitions.
- Card `prepaid` is account closure with zero balance. A zero balance with an
  open line is **not** an exit; it is a transactor state in the behavioural layer
  (section 4).
- Modifications, forbearance and re-ages are **not** cures. They are separate
  flags that either (a) exclude the segment from the fit with a declared reason,
  or (b) enter as a covariate. Silent re-age treatment is rejected.
- Existing defaults at valuation and their recovery inventory remain outside the
  transition fit, per `credit-model-1`.

## 2. Observation contract

- Prefer exact event dates (payment due/receipt, charge-off date, payoff date)
  with `credit-covariate-1`. Monthly snapshots use the interval-censored panel
  estimator; snapshot endpoints are never relabelled as exact transition times.
- Training window `(train_start, train_end]`, chronological holdout after
  `train_end`; split identities differ; holdout rows enter no objective.
- Left truncation at panel entry is allowed; initial-state probabilities are not
  fitted.
- Minimum exposure per fitted origin and edge is a declared policy. An edge with
  no training events requires a reviewed fixed-row assumption or removal; it is
  never reported as an empirical zero.

## 3. Covariates by product

Every column declares name, role (`borrower` or `macro`), unit, raw domain,
frozen offset and scale (`z = (raw − offset) / scale`), a scaling source and an
availability lag. Values must be available at or before segment start. The
tables give the proposed set; a bank may drop a column it cannot supply, never
silently substitute one.

### 3.1 Borrower and account covariates

| Column | Auto | Personal | Card | Unit / domain | Notes |
|---|---|---|---|---|---|
| `orig_score_band` | ✓ | ✓ | ✓ | ordinal band 1–8 | Bureau score at origination; band edges declared |
| `curr_score_band` | ○ | ○ | ✓ | ordinal band 1–8 | Refreshed score when the bank supplies it; availability lag ≥ 1 month |
| `loan_age` | term level | term level | term level | months | Enters as term level, not a slope; grid in 3.3 |
| `orig_term` | ✓ | ✓ | – | months | 36–84 auto, 12–84 personal |
| `orig_ltv` | ✓ | – | – | decimal 0.3–1.6 | Amount financed / collateral value at origination |
| `new_used` | ✓ | – | – | {new, used} | Also selects the depreciation curve (section 5) |
| `equity_ratio` | ✓ | – | – | decimal −1–2 | Collateral value path / balance; time-varying via `covariate_boundary` |
| `dti` | ○ | ✓ | ○ | decimal 0–1 | At origination; payment-to-income preferred where supplied |
| `utilisation_band` | – | – | ✓ | ordinal 1–6 | Balance / limit at statement; time-varying |
| `behaviour_state` | – | – | ✓ | {transactor, revolver, minimum-payer} | From the behavioural layer, section 4 |
| `promo_flag` | – | – | ✓ | {0,1} | Promotional APR active |
| `apr_spread` | ○ | ○ | ✓ | decimal | Contract APR minus prime; card-specific signal |
| `channel` | ✓ | ✓ | ✓ | {direct, indirect/dealer, broker, digital} | Indirect auto and digital personal are known risk shifters |
| `vintage` | ✓ | ✓ | ✓ | year-quarter | Enters as a reviewed fixed effect or is excluded; never a free slope |
| `modified_flag` | ✓ | ✓ | ✓ | {0,1} | See section 1; use or exclude, declared |

✓ required where available · ○ optional · – not applicable

### 3.2 Macro drivers

Macro drivers come from the joint driver tape (#92) or supplied scenario paths,
with the same unit/lag discipline. Physical measure only; pricing-measure
consumers never receive driver paths.

| Driver | Auto | Personal | Card | Unit | Typical lag | Source class |
|---|---|---|---|---|---|---|
| `unemp_level` | ✓ | ✓ | ✓ | percent | 1 month | BLS national; state where supplied |
| `unemp_chg_12m` | ✓ | ✓ | ✓ | pp | 1 month | Derived from level |
| `used_vehicle_idx_chg_12m` | ✓ | – | – | percent | 1 month | Supplied index (Manheim-style); drives collateral path |
| `hpi_chg_12m` | ○ | ✓ | ✓ | percent | 2–3 months | Already in `model-market-1` regional HPI |
| `prime_minus_policy` | – | ○ | ✓ | pp | 0 | Consumer rate spread |
| `consumer_rate_chg_12m` | ○ | ✓ | ✓ | pp | 1 month | Card/personal APR environment |
| `real_dpi_growth` | ○ | ✓ | ✓ | percent | 2 months | BEA; optional |
| `short_rate` | ✓ | ✓ | ✓ | percent | 0 | From the LMM path; links credit to rate scenarios |

Drivers enter hazards log-linearly per `credit-covariate-1`:
`lambda_rj = exp(intercept_rj + beta_rj · z + term_effect_rj,k)`. Hazards above
the admitted domain reject; they are never clipped.

### 3.3 Term-level grids (loan age, months)

| Product | Grid | Rationale |
|---|---|---|
| Auto | 0–3, 4–6, 7–12, 13–18, 19–24, 25–36, 37–48, 49–60, 61+ | Early-payment default hump and trade-in cycle |
| Personal | 0–3, 4–6, 7–12, 13–18, 19–24, 25–36, 37+ | Short lives; front-loaded default |
| Card | 0–6, 7–12, 13–24, 25–36, 37–60, 61+ | Seasoning; later term levels mostly behavioural |

Reference level is the first term in each grid. Grids are declared in the fit
identity and may be coarsened for thin panels; a coarsened grid is still a
declared assumption.

## 4. Card behavioural layer (#93)

Layered on the credit states, three behavioural states per statement cycle:

| State | Definition | Typical signature |
|---|---|---|
| transactor | pays ≥ 99% of statement balance | utilisation low, no interest |
| revolver | pays between minimum and full | carries interest-bearing balance |
| minimum-payer | pays ≤ minimum + tolerance | high utilisation, elevated default hazard |

Monthly transition probabilities between behavioural states are fitted with the
interval-censored estimator on covariates: utilisation band, score band,
`apr_spread`, `promo_flag`, `unemp_chg_12m`. Given a behavioural state:

- **Payment rate** `p = payment / statement balance`, bounded in
  `[minimum due, 1]`, modelled by state with covariate adjustments.
- **Draw rate** `d = purchases / open-to-buy`, by state, with utilisation drift
  toward a state-specific target.
- **EAD at default** = outstanding + additional draws in the 3–6 months before
  default, expressed as a state-conditional credit conversion factor on undrawn
  line; this replaces the memo-only treatment of undrawn commitments.
- **Issuer actions** (limit increase/decrease by score band, inactivity closure,
  delinquency reprice) are supplied policies, never inferred.

All behavioural outputs enter the existing `consumer-card-1` mechanics as
supplied draws and payments; the mechanics stage is unchanged.

## 5. Auto collateral layer (#94)

- Collateral value path: `orig_value × depreciation(age, new_used, class) ×
  used_vehicle_index(path) × haircut`. Depreciation curves and haircuts are
  supplied assumptions with declared sources.
- `LGD = max(0, EAD − collateral_value × recovery_haircut + repossession_cost +
  remarketing_cost)`, recovery at a declared lag, deficiency-balance recovery as
  a separate tail.
- Prepayment hazard covariates: `equity_ratio`, `loan_age` term, score band, a
  weak declared rate-incentive term, channel. Default hazard covariates:
  `unemp_chg_12m`, `equity_ratio`, score band, `orig_ltv`, `orig_term`, DTI.

## 6. Lifetime loss reporting (#95)

Every lifetime expected-loss figure is reported with:

1. **Macro sensitivity:** recomputed under declared alternative driver paths
   (±1σ from the joint covariance; Fed-style baseline/adverse templates where
   supplied), on identical draws; the spread is reported next to the point value.
2. **Model-choice sensitivity:** constant-row vs covariate vs fixed-row
   assumptions, side by side.
3. **Challenger diagnostics** (analysis only): discrete-time hazard or
   gradient-boosted survival challengers on the same panel, compared on
   time-dependent calibration and Brier score. Python is permitted here as
   independent verification tooling, never as a production financial callback.
4. Labels distinguishing economic expected loss from reviewed allowance; CECL/
   IFRS 9 compliance is not claimed.

## 7. Eligibility and disclosure

- Synthetic fits are always `adoption_eligible=false`.
- `observed` data kind plus numerical eligibility admits projection; empirical
  accuracy still requires chronological holdout diagnostics and independent
  review by the bank's model-risk function under
  [SR 26-2](https://www.federalreserve.gov/supervisionreg/srletters/SR2602.htm).
- Fixed-row assumptions, coarsened grids, dropped covariates and excluded
  modified segments appear in the validation pack as supplied assumptions.

## 8. Research references

- Federal Reserve, [2026 supervisory stress test methodology](https://www.federalreserve.gov/publications/files/2026-february-supervisory-stress-test-methodology.pdf) and [January 2026 credit model documentation](https://www.federalreserve.gov/supervisionreg/files/credit-risk-models.pdf): consumer transition structure, card EAD with additional borrowing.
- Philadelphia Fed WP 26-09 (2026), [Model risk under CECL: a consumer finance perspective](https://fedinprint.org/item/fedpwp/102431/original): sensitivity of lifetime loss to macro forecast error.
- Heitfield and Sabarwal (2004), competing risks of default and prepayment on subprime auto loans; and the ABS-pool multistate study [arXiv 2211.09176](https://arxiv.org/pdf/2211.09176): prepayment follows used-vehicle values and equity more than rates; default is more macro-sensitive than prepayment.
- Survival-ML and evaluation references for challengers: [Springer 2025 review of ML credit scoring](https://link.springer.com/article/10.1007/s10462-025-11416-2); [arXiv 2506.02075](https://arxiv.org/pdf/2506.02075) on evaluating survival models beyond the C-index.
- Transformer utilisation forecasting (origination/account-management lane, not adopted here): [Taylor & Francis 2026](https://www.tandfonline.com/doi/full/10.1080/08839514.2026.2676392).
