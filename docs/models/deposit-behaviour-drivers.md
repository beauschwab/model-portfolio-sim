# Deposit behaviour: states, covariates, rate response and driver specification

Status: **proposal** for issues #102 (covariate hazards and partial withdrawals),
#103 (fitted paid-rate response), #104 (off-balance-sheet destinations and surge
decay), #105 (migration in the accounting tier) and #106 (deposit market drivers
on the joint tape), under tracker #25. Nothing here changes saved economics or an
existing contract. It fixes the state and covariate definitions a bank's account
panel and repricing history must be mapped to so that `deposit-state-1`,
`observed-deposit-calibration-1` and the joint driver machinery can be applied to
non-maturity deposits consistently.

The structure follows the supervisory separation of **repricing**, **runoff** and
**migration** as distinct, separately evidenced assumptions
([OCC Interest Rate Risk handbook](https://www.occ.gov/publications-and-resources/publications/comptrollers-handbook/files/interest-rate-risk/pub-ch-interest-rate-risk.pdf);
[Basel SRP98](https://www.bis.org/committees/bcbs/basel-framework/standard/srp/98/inforce/2026-01-01/published/2024-07-16)).
The estimators are this repository's exact-likelihood competing-hazard models.
No vendor or supervisory beta table is reproduced; every sensitivity is either
fitted on the bank's own history with a chronological holdout or declared as a
supplied assumption.

## 1. Shared state mapping

A deposit cohort is a set of **balance states**. Internal states are the bank's
own products; external states are destinations outside the bank. Each state is a
declared, versioned segment in the request identity; the engine never infers a
segment from a product label.

| State class | State | Definition | Interest expense |
|---|---|---|---|
| internal | `nib_dda` | non-interest-bearing transaction | none |
| internal | `ib_dda` | interest-bearing transaction / NOW | administered |
| internal | `savings` | statement savings | administered |
| internal | `mmda_tier_<k>` | money-market deposit account by balance tier | administered, tiered |
| internal | `cd_<tenor>` | time deposit by tenor bucket | contractual; migration destination only at maturity |
| internal | `brokered` | brokered/listing-service deposits | contractual |
| external | `off_balance_mmf` | money-market mutual fund / sweep | none (bank) |
| external | `off_balance_brokerage` | brokerage, Treasuries, other securities | none |
| external | `other_bank` | competitor bank account | none |
| absorbing | `closed` | account closure, zero balance | none |

Rules:

- **Withdrawal** is a balance reduction with no observed destination. **Migration**
  is a balance movement with an observed destination state. A bank that cannot
  observe destinations declares `destination_observability=none` and fits
  withdrawals only; it never labels unknown outflow as `other_bank`.
- **Closure** is the account's exit; a zero balance with an open account is a
  live state, not closure.
- **Surge balance** is a declared split of opening balance into `trend` and
  `surge` components per segment (section 5). The split is supplied with its
  source; the kernel never infers it.
- **Recapture** is a flow from an external state back to a named internal state.
  It is an inflow with lineage, not a reversal of the original migration.
- CD states are destinations only at the origin's contractual maturity or on a
  declared early-withdrawal policy; `deposit-state-1` already rejects migration
  past a true maturity.
- Insured and uninsured balances are **not** separate states; insured status is a
  covariate (section 3) because it drives behaviour without changing the product.

## 2. Observation contract

Two sampling units are admitted and must be declared:

| Sampling unit | Event | Exposure | Estimator |
|---|---|---|---|
| `account_full_exit` (existing) | closure, full withdrawal, migration of the whole account | account-years | `observed-deposit-calibration-1` or the covariate fit (#102) |
| `balance_weighted` (#102) | partial withdrawal with withdrawn fraction, migration of part of the balance, closure | balance-years | covariate fit with a separate withdrawn-fraction distribution |

- Spells carry account, segment, exact start/end timestamps, balance at start and
  one ending event. Partial withdrawals end a spell and start a contiguous one in
  the same segment at the lower balance.
- Paid-rate, reference-rate and competitor-rate histories are daily rows with the
  observation lag declared per segment, exactly as `deposit-state-1` requires.
  Missing days reject; nothing is forward-filled.
- Training window `(train_start, train_end]`, chronological holdout after
  `train_end`; split identities differ; holdout rows enter no objective.
- Minimum balance-years per fitted origin and cause is a declared policy. A cause
  with no training events requires a reviewed fixed row or removal; it is never an
  empirical zero.
- The 2020–2021 surge and the 2022–2024 tightening are **distinct regimes**. A fit
  whose training window excludes an up-cycle must report `regime_support=partial`
  and the plateau flag false (section 4).

## 3. Covariates

Every column declares name, role (`account` or `macro`), unit, raw domain, frozen
offset and scale (`z = (raw − offset) / scale`), a scaling source and an
availability lag. Values must be available at or before spell start, except the
rate gaps, which are recomputed at each `covariate_boundary` as the paid rate moves.

### 3.1 Account covariates

| Column | Role | Unit / domain | Notes |
|---|---|---|---|
| `paid_gap_market` | account | pp, −2–8 | `mmf_yield − paid_rate`; the primary driver of external migration |
| `paid_gap_competitor` | account | pp, −2–8 | `competitor_rate_segment − paid_rate`; drives `other_bank` and internal migration |
| `balance_tier` | account | ordinal 1–6 | Declared tier edges (e.g. <10k, 10–50k, 50–100k, 100–250k, 250k–1M, >1M); above-insurance tiers behave differently |
| `insured_flag` / `uninsured_fraction` | account | {0,1} / decimal 0–1 | Per account or per cohort; the 2023 run driver |
| `tenure_months` | term level | months | Enters as term level, not a slope; grid in 3.3 |
| `relationship_depth` | account | ordinal 0–4 | Count of payroll, bill-pay, loan, card, brokerage links; declared mapping |
| `operational_flag` | account | {0,1} | Commercial operational account under the bank's own definition |
| `digital_flag` | account | {0,1} | Digital-only or digital-primary channel; raises velocity sensitivity |
| `rate_sensitivity_history` | account | {0,1} | Account has moved balance for rate before; optional, time-varying |
| `vintage` | account | year-quarter | Reviewed fixed effect or excluded; never a free slope |
| `segment` | origin | declared | The origin state; one hazard row per origin |

### 3.2 Macro and market drivers

From the joint driver tape (#106) or supplied scenario paths, physical measure
only, with the same unit and lag discipline.

| Driver | Unit | Typical lag | Source class |
|---|---|---|---|
| `short_rate` | percent | 0 | LMM path; the reference rate for repricing |
| `mmf_yield` | percent | 0–1 month | Supplied index (government MMF 7-day yield); near-unit beta to policy |
| `competitor_rate_<segment>` | percent | 0–1 month | Supplied national or local rate survey by product |
| `brokered_cd_rate_<tenor>` | percent | 0 | Supplied listing-service rate |
| `reference_velocity` | pp/day | 0 | Already in `deposit-state-1`; absolute change of the reference rate |
| `reserve_balances_chg_12m` | percent | 1 month | H.4.1; system-wide liquidity proxy, optional |
| `unemp_chg_12m` | pp | 1 month | Household liquidity proxy for savings drawdown, optional |
| `confidence_stress` | index | 0 | Scenario input, as today; not fitted from history |

### 3.3 Tenure grid (months)

0–3, 4–12, 13–24, 25–60, 61–120, 121+. Reference level is 0–3. New accounts
churn; accounts past five years are the stable core. Grids may be coarsened for
thin panels; a coarsened grid is still a declared assumption.

### 3.4 Hazard form

Per origin segment `r` and competing cause `c` (withdrawal, closure, each
migration destination including external states), with term level `k`:

`lambda_rc = exp(intercept_rc + beta_rc · z + term_effect_rc,k)` per balance-year
(or account-year under `account_full_exit`).

All causes share the origin's time at risk; the combined row intensity respects
the existing 100/year admission limit and is never clipped. For
`balance_weighted` runoff the withdrawn fraction is a separate bounded
distribution per origin (declared Beta or empirical quantiles); expected daily
runoff is intensity × expected fraction. Projection writes the fitted
`gap_sensitivity`, `velocity_sensitivity`, `uninsured_coefficient`,
`digital_coefficient`, `relationship_protection`, `operational_protection` and
migration gap sensitivities into `deposit-state-1` with
`parameter_status=fitted` beside any `supplied_assumption` rows.

## 4. Paid-rate response (#103)

The offered-rate target already supports linear and logistic beta with
competitor weight, spread stress, floors, ceilings and asymmetric half-lives.
This specification fixes how those parameters are **fitted**:

- Observation: segment paid rate, reference rate and competitor rate per day or
  month over the training window.
- Forms: `linear`, `logistic` (as in `deposit-state-1`) and `threshold`
  (`beta_low` below a fitted reference-rate threshold, `beta_high` above).
  Threshold is the parsimonious description of the 2022–2024 experience and is
  the default challenger to logistic.
- Lag structure: upward and downward half-lives estimated from the partial
  adjustment `paid_t = paid_{t-1} + a·(target − paid_{t-1})`, by direction.
- Identification: the logistic extremes are declared **not identified** unless
  the observed reference range crosses the fitted midpoint by at least the
  fitted scale; the result sets the `ObservedRange` plateau flag from this test.
- Mandatory sensitivity grid attached to every fit: beta ±25%, half-lives ×0.5
  and ×2, threshold ±100 bp. Scenario reports show the grid beside the point run.
- The output is a versioned `deposit-rate-response-1` identity; selecting it
  requires an explicit library/session rebuild. Saved books are unchanged.

## 5. External destinations and surge decay (#104)

- External states carry the gap driver `paid_gap_market` (MMF) or
  `paid_gap_competitor` (other bank) and a **recapture** intensity to a named
  internal state when the gap closes. While external, no interest expense and no
  liability.
- Surge decomposition: `balance = trend + surge`; `surge` decays at a fitted or
  supplied daily rate toward zero independently of gaps; `trend` follows the
  ordinary hazards. Opening split and its source are supplied.
- Rights identity: opening + inflows + recaptures = withdrawals + external
  migrations + closures + maturities + closing. Migration between internal
  states does not appear in the identity.

## 6. Accounting adoption (#105)

- Internal migration posts a balanced liability reclassification between segment
  sub-accounts plus the migrated accrued interest. Never cash, never P&L.
- External migration posts liability and cash reduction once; recapture posts a
  new inflow with origin lineage.
- Original-yield routes declare `migrated_balance_yield_policy ∈ {retain_origin_eir,
  reanchor_destination}`; absent policy rejects.
- FTP: behavioural funding life uses the migration-adjusted expected balance path
  from the same kernel; the migration source identity is retained in
  `treasury-bridge-1` lineage.

## 7. Eligibility and disclosure

- Synthetic fits are always `adoption_eligible=false`.
- A fit with `regime_support=partial` or an unidentified plateau may be published
  only with its sensitivity grid and a reviewed statement of the unsupported
  range.
- Fixed rows, coarsened grids, dropped covariates and declared surge splits
  appear in the validation pack as supplied assumptions, alongside the
  chronological holdout diagnostics required by
  [SR 26-2](https://www.federalreserve.gov/supervisionreg/srletters/SR2602.htm).
- Nothing here classifies deposits for LCR/NSFR outflow rates or Basel core
  deposit slotting; research weights remain research weights.

## 8. Research references

- OCC, [Interest Rate Risk, Comptroller's Handbook](https://www.occ.gov/publications-and-resources/publications/comptrollers-handbook/files/interest-rate-risk/pub-ch-interest-rate-risk.pdf): repricing and runoff as separately documented assumptions.
- BCBS, [SRP98 application guidance on IRRBB](https://www.bis.org/committees/bcbs/basel-framework/standard/srp/98/inforce/2026-01-01/published/2024-07-16): behavioural stability vs contractual liquidity; runoff, constant and dynamic balance views.
- Drechsler, Savov and Schnabl (2021), *Banking on Deposits: Maturity Transformation without Interest Rate Risk*, Journal of Finance 76(3) ([NBER WP 24582](https://www.nber.org/papers/w24582)): deposit betas, franchise value and the 2023 reassessment.
- Jiang, Matvos, Piskorski and Seru (2023), [Monetary Tightening and U.S. Bank Fragility in 2023](https://www.nber.org/papers/w31048), NBER WP 31048: uninsured deposit concentration and run incentives.
- Koont, Santos and Zingales (2024), [Destabilizing Digital "Bank Walks"](https://www.nber.org/papers/w32601), NBER WP 32601: digital channel and deposit beta/velocity.
- New York Fed, [Deposit Betas: Up, Up, and Away?](https://libertystreeteconomics.newyorkfed.org/2023/04/deposit-betas-up-up-and-away/) (2023) and the Federal Reserve [Senior Financial Officer Surveys](https://www.federalreserve.gov/data/sfos/september-2023-senior-financial-officer-survey.htm) (2023–2024): observed cumulative retail and wholesale betas; Dallas Fed WP 2315 on [deposit convexity](https://fedinprint.org/item/feddwp/97131/original).
- Search-result summaries only were available when this specification was drafted; page content could not be opened. Claims rest on those summaries and the cited primary documents already linked from `deposit-state.md`.
