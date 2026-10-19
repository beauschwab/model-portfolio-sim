# Commercial and CRE credit: states, covariates, correlation and driver specification

Status: **proposal** for issues #107 (obligor covariate specification), #108
(macro-conditioned migration and correlated paths), #109 (CRE collateral and
maturity refinance), #110 (revolver utilisation EAD) and #111 (commercial lifetime
loss), under trackers #15 and #25. Nothing here changes saved economics or an
existing contract. It fixes the definitions an obligor/facility panel must be
mapped to so that `credit-covariate-1`, the interval-censored panel estimator,
`commercial-credit-ledger-1` and the joint driver machinery apply to C&I and CRE
books consistently.

The covariate vocabulary follows the FR Y-14Q C&I and CRE schedules so a bank's
existing stress-test extract maps without re-engineering. The estimators are this
repository's exact-likelihood continuous-time models and the CTMC event loop
already used by the commercial ledger; no supervisory coefficients are
reproduced. Portfolio-level loss depends on **correlated** obligor behaviour, so
section 4 specifies a systematic factor layer rather than independent hazards.

## 1. Shared state mapping

All facilities map to the fixed `credit-model-1` order
**performing, watch, delinquent, default, prepaid**. The mapping combines the
bank's internal risk grade and days past due, and is a declared, versioned policy.

| Product | performing | watch | delinquent | default (absorbing) | prepaid (absorbing) |
|---|---|---|---|---|---|
| C&I term and revolver | pass grades, current | special mention / criticized, or 1–29 dpd | substandard, or 30–89 dpd | nonaccrual, 90+ dpd, doubtful/loss, charge-off, bankruptcy | payoff, refinance away, syndication sell-down |
| CRE (income-producing) | pass, DSCR ≥ covenant | special mention, or DSCR below covenant but ≥ 1.0 | substandard, or 30–89 dpd, or DSCR < 1.0 | nonaccrual, 90+ dpd, maturity default, deed-in-lieu, foreclosure | payoff, refinance away, sale of property |
| Construction / land | pass, on budget | cost overrun or lease-up behind plan | interest reserve exhausted, or 30–89 dpd | nonaccrual, 90+ dpd, abandonment | conversion to permanent, sale |

Rules:

- Grade edges, dpd edges and DSCR covenants are bank policy inputs retained in
  the fit identity. Regulatory classifications (special mention, substandard,
  doubtful) are the usual anchors, supplied not assumed.
- Cure is any move to a better live state. A grade upgrade without a payment
  event is a transition; a payment that leaves the grade unchanged is not.
- Modifications, extensions and TDR/MLA-style restructures are **flags**, not
  cures. A maturity extension re-enters as a new segment with lineage (section 5).
- Existing defaults at valuation and their recovery inventory remain outside the
  transition fit, per `credit-model-1`. Reviewed allowance, chargeoff and legal
  waiver remain separate book actions, per `commercial-credit-ledger-1`.
- Issuer-credit securities keep their separate owner; this specification covers
  loans and committed facilities.

## 2. Observation contract

- Prefer exact event dates (grade change date, payment due/receipt, nonaccrual
  date, payoff date) with `credit-covariate-1`. Quarterly grade panels use the
  interval-censored estimator; snapshot endpoints are never relabelled as exact.
- Facility is the unit of exposure; obligor identity is retained so the factor
  layer can share draws across an obligor's facilities.
- Training window `(train_start, train_end]`, chronological holdout after
  `train_end`; split identities differ; holdout rows enter no objective.
- Minimum exposure per origin and edge is a declared policy. Commercial panels are
  thin at the default edge; a fixed row or a pooled industry row is a reviewed
  assumption, never an empirical zero.
- Left truncation at panel entry is allowed; initial-state probabilities are not
  fitted.

## 3. Covariates

Every column declares name, role (`obligor`, `facility` or `macro`), unit, raw
domain, frozen offset and scale, a scaling source and an availability lag.
Financial-statement covariates carry their statement date and a declared
reporting lag (typically 3–6 months for private obligors).

### 3.1 C&I obligor and facility covariates

| Column | Role | Unit / domain | Notes |
|---|---|---|---|
| `grade` | obligor | ordinal 1–10 | Internal risk rating; edges declared |
| `leverage` | obligor | decimal 0–12 | Total debt / EBITDA; winsorised at domain edge |
| `interest_coverage` | obligor | decimal −5–30 | EBITDA / interest expense |
| `current_ratio` | obligor | decimal 0–5 | Optional liquidity proxy |
| `obligor_size` | obligor | log USD revenue | Size effect on default |
| `industry` | obligor | NAICS 2-digit | Fixed effect or factor loading (section 4) |
| `facility_type` | facility | {term, revolver, letter_of_credit} | Selects utilisation layer |
| `secured_flag` | facility | {0,1} | Collateral present |
| `syndicated_flag` | facility | {0,1} | Shared national credit |
| `remaining_term` | term level | months | Grid in 3.4 |
| `utilisation_band` | facility | ordinal 1–5 | Drawn / commitment; time-varying (section 6) |
| `covenant_headroom` | facility | decimal | Distance to tightest financial covenant; optional |
| `vintage` | facility | year-quarter | Reviewed fixed effect or excluded |
| `modified_flag` | facility | {0,1} | See section 1 |

### 3.2 CRE covariates

| Column | Role | Unit / domain | Notes |
|---|---|---|---|
| `property_type` | facility | {office, multifamily, retail, industrial, hotel, other} | Selects price and cap-rate driver series |
| `dscr` | facility | decimal 0–4 | NOI / debt service; time-varying along the collateral path |
| `ltv` | facility | decimal 0.2–1.5 | Balance / value; time-varying |
| `noi_growth_12m` | facility | percent | From rent roll or scenario |
| `occupancy` | facility | decimal 0–1 | Physical occupancy |
| `market` | facility | MSA | Fixed effect or regional factor |
| `fixed_floating` | facility | {fixed, floating} | Floating CRE is rate-sensitive through DSCR |
| `months_to_maturity` | term level | months | Grid in 3.4; maturity outcome at zero |
| `io_flag` | facility | {0,1} | Interest-only period active |
| `recourse_flag` | facility | {0,1} | Guarantor support |
| `lease_rollover_24m` | facility | decimal 0–1 | Share of rent expiring within 24 months; optional |

### 3.3 Macro drivers

From the joint driver tape or supplied scenario paths, physical measure only.

| Driver | C&I | CRE | Unit | Typical lag | Source class |
|---|---|---|---|---|---|
| `unemp_level` / `unemp_chg_12m` | ✓ | ✓ | percent / pp | 1 month | BLS |
| `gdp_growth_4q` | ✓ | ○ | percent | 1 quarter | BEA |
| `bbb_oas` | ✓ | ✓ | bp | 0 | Corporate spread; credit-cycle proxy |
| `cre_price_idx_chg_12m_<type>` | – | ✓ | percent | 1–2 months | Supplied index by property type |
| `cap_rate_<type>` | – | ✓ | percent | 1 quarter | Supplied series by property type |
| `office_vacancy` | – | ○ | percent | 1 quarter | Optional, office only |
| `short_rate` | ✓ | ✓ | percent | 0 | LMM path; floating DSCR and refinance rate |
| `term_spread` | ○ | ✓ | pp | 0 | 10y − 3m from the LMM path; refinance pricing |
| `industry_output_<sector>` | ○ | – | percent | 1 quarter | Optional sector production index |

Drivers enter hazards log-linearly per `credit-covariate-1`. Hazards above the
admitted domain reject; they are never clipped.

### 3.4 Term-level grids

| Product | Grid | Rationale |
|---|---|---|
| C&I term (`remaining_term`) | 0–6, 7–12, 13–24, 25–36, 37–60, 61+ months | Refinance and covenant-reset cycle |
| C&I revolver | by `utilisation_band`, not time | Behaviour is utilisation-driven |
| CRE (`months_to_maturity`) | 61+, 37–60, 25–36, 13–24, 7–12, 0–6 | Maturity-wall effect dominates seasoning |

Reference level is the first term in each grid.

## 4. Systematic factor layer and migration (#108)

- Each obligor's hazards from section 3 are conditioned on a common factor path
  `F_t` composed of declared loadings on `bbb_oas`, `unemp_chg_12m`,
  `gdp_growth_4q`, and for CRE `cre_price_idx_chg_12m_<type>` and
  `cap_rate_<type>`, plus a residual industry/region factor with declared
  loading and variance. The factor path is shared across obligors within a
  scenario and across the shared random numbers; zero loading reproduces
  independent paths exactly.
- Through-the-cycle to point-in-time bridge: a declared long-run quarterly grade
  migration matrix is shifted by the fitted factor (Belkin-style Z-shift) so a
  stress scenario moves every row consistently. The bridge is fitted from the
  quarterly grade panel with the interval-censored estimator using the factor as
  a macro covariate.
- Outputs per scenario: expected loss and loss-distribution quantiles with the
  factor realisation retained in lineage. Fixed base OAS and CRN are unchanged;
  the factor is physical measure only.

## 5. CRE collateral and maturity outcome (#109)

- Collateral value `V_t = NOI_t / caprate_t`; NOI follows `noi_growth` and the
  cap rate follows the scenario series by property type. LTV and DSCR are
  recomputed along the path and fed back as time-varying covariates.
- At `months_to_maturity = 0` a competing maturity outcome: `refinance` (away or
  with the bank), `extend` (declared policy; re-enters as a new segment), or
  `default`. Probabilities are driven by refinance proceeds at the scenario cap
  rate and DSCR at the scenario rate versus balance, under a declared lender
  underwriting box (max LTV, min DSCR).
- `LGD = max(0, EAD + foreclosure_cost − V_t × disposition_haircut)` with a
  declared time to disposition; deficiency recovery is a separate tail per
  `commercial-credit-ledger-1`.
- Prepayment: yield maintenance, defeasance and step-down penalties are
  contractual terms entering both the incentive and the cash tail.

## 6. Revolver utilisation layer (#110)

- Utilisation bands: 0–20, 21–40, 41–60, 61–80, 81–100 percent of commitment.
- A draw-rate hazard per credit state moves utilisation up as the obligor
  migrates toward default; fitted CCF by grade and facility type replaces a
  single supplied CCF where evidence exists.
- Covenant breach (DSCR, leverage) can freeze or reduce availability under a
  declared cure/termination rule; the undrawn commitment follows that rule.
- Draw cash, funding and original-yield treatment follow the existing performing
  draw tranche ownership; no new accounting owner.

## 7. Lifetime loss reporting (#111)

Every commercial lifetime expected-loss figure is reported with:

1. **Horizon and reversion:** a declared reasonable-and-supportable period during
   which scenario drivers apply, then a declared reversion rule (immediate,
   linear or exponential) to the long-run matrix; both in the identity.
2. **Macro sensitivity:** recomputed under declared alternative driver paths on
   identical draws, with the share of change attributable to each driver.
3. **Challenger diagnostics** (analysis only): through-the-cycle-only and
   no-reversion variants; Python permitted as independent verification tooling,
   never as a production financial callback.
4. Labels distinguishing economic expected loss from reviewed allowance; the
   `credit_ledger` capital bridge never deducts modelled loss a second time.

## 8. Eligibility and disclosure

- Synthetic fits and supplied factor loadings are always `adoption_eligible=false`.
- `observed` data kind plus numerical eligibility admits projection; empirical
  accuracy still requires chronological holdout diagnostics and independent
  review under [SR 26-2](https://www.federalreserve.gov/supervisionreg/srletters/SR2602.htm).
- Property valuation is a scenario mechanism, not an appraisal or accounting
  fair value. No CECL/IFRS 9 compliance or supervisory parity is claimed.

## 9. Research references

- Federal Reserve, FR Y-14Q instructions (Schedule H.1 C&I and H.2 CRE): covariate vocabulary and loss-model structure; [2026 supervisory stress test methodology](https://www.federalreserve.gov/publications/files/2026-february-supervisory-stress-test-methodology.pdf) for the wholesale and CRE model families.
- Belkin, Suchower and Forest (1998), *A one-parameter representation of credit risk and transition matrices*: the Z-shift used in section 4.
- Vasicek (2002), *The distribution of loan portfolio value*; and the multi-factor extensions in CreditMetrics and CreditPortfolioView: correlated migration conditioned on macro factors.
- Jiménez, Lopez and Saurina (2009), *Empirical analysis of corporate credit lines*: utilisation rises before default; basis for section 6.
- Federal Reserve and FDIC supervisory statements on CRE concentration and maturity/refinance risk (2023–2026): the maturity outcome framing in section 5.
- Search-result summaries only were available when this specification was drafted; page content could not be opened. Claims rest on those summaries and the primary documents already linked from `commercial-credit-ledger.md`.
