# Model research references

Consolidated bibliography of every external paper, standard, supervisory
document and market convention cited by the model documentation in
`docs/models/` and the top-level `docs/` pages, as of PR #112. Each entry gives
the source, what the repository uses it for, and which documents cite it. Links
are reproduced as cited; **none could be re-verified from the drafting
environment** (outbound HTTPS was blocked), so a reader should open each one
before relying on a quoted paragraph number. Sources first cited by name only
were resolved to publisher or working-paper links through web search; the two
FASB codification items are subscription-only and link to the FASB index.

Nothing in this list is reproduced in the engine. Supervisory model
descriptions inform structure only; no supervisory coefficient is copied, and
citing a standard is not an accounting or regulatory opinion.

## 1. Supervisory and regulatory guidance (United States)

| Source | Used for | Cited in |
|---|---|---|
| Federal Reserve, [2026 Supervisory Stress Test Methodology](https://www.federalreserve.gov/publications/files/2026-february-supervisory-stress-test-methodology.pdf) (Feb 2026) | Structure of wholesale, CRE and consumer loss models; delinquency-bucket transitions with competing payoff | implementation-plan, credit-model, consumer-behaviour-drivers, commercial-credit-drivers |
| Federal Reserve, [2025 Supervisory Stress Test Methodology](https://www.federalreserve.gov/publications/files/2025-june-supervisory-stress-test-methodology.pdf) and [detailed model descriptions](https://www.federalreserve.gov/publications/2025-june-supervisory-stress-test-methodology-descriptions-supervisory-models.htm) (Jun 2025) | Historical example of loan/borrower and macro conditioning | credit-model, credit-state, credit-covariate, implementation-plan |
| Federal Reserve, [Credit risk models documentation](https://www.federalreserve.gov/supervisionreg/files/credit-risk-models.pdf) (Jan 2026) | Consumer and mortgage transition structure; card EAD with additional borrowing | credit-calibration, credit-state, mortgage-state, consumer-behaviour-drivers |
| Federal Reserve, [SR 26-2](https://www.federalreserve.gov/supervisionreg/srletters/SR2602.htm) and [attachment](https://www.federalreserve.gov/supervisionreg/srletters/SR2602a1.pdf) (2026; supersedes SR 11-7) | Model risk management: independent validation, holdout evidence, documentation of assumptions | credit-model, implementation-plan, market-quote-bootstrap, joint-state, market-driver-calibration, all three driver specs |
| Federal Reserve, [Dodd-Frank Act stress test scenarios 2026](https://www.federalreserve.gov/supervisionreg/dfa-stress-tests-2026.htm) | Baseline/adverse scenario templates | market-data, forecast-scenarios |
| Interagency, [Advisory on Interest Rate Risk Management](https://www.federalreserve.gov/frrs/guidance/interest-rate-risk-interagency-advisory.htm) and [FAQs](https://www.federalreserve.gov/frrs/guidance/interest-rate-risk-frequently-asked-questions-regarding-interagency-advisory.htm) (2010/2012) | Deposit assumption documentation and stress expectations | deposit-graph-capture |
| OCC, [Interest Rate Risk, Comptroller's Handbook](https://www.occ.gov/publications-and-resources/publications/comptrollers-handbook/files/interest-rate-risk/pub-ch-interest-rate-risk.pdf) | Repricing sensitivity and runoff as separately documented assumptions | deposit-state, deposit-product-graph, deposit-behaviour-drivers, implementation-plan |
| Interagency, [Guidance on Funds Transfer Pricing related to funding and contingent liquidity risks](https://www.federalreserve.gov/frrs/guidance/interagency-guidance-on-funds-transfer-pricing-related-to-funding-and-contingent-liquidity-risks.htm); OCC [Bulletin 2016-7](https://www.occ.treas.gov/news-issuances/bulletins/2016/bulletin-2016-7.html) ([PDF](https://www.occ.treas.gov/news-issuances/bulletins/2016/bulletin-2016-7a.pdf)) (2016) | FTP separation of funding, liquidity and contingent charges | joint-funding-validation, capital-and-ftp, implementation-plan |
| Interagency, [Policy Statement on Allowances for Credit Losses](https://www.federalreserve.gov/frrs/guidance/interagency-policy-statement-on-allowances-for-credit-losses.htm) (2020) | Reviewed allowance as a separate book action from modelled loss | dated-ledger |
| OCC, [Bank Accounting Advisory Series](https://www.occ.treas.gov/publications-and-resources/publications/banker-education/files/pub-bank-accounting-advisory-series.pdf) (Aug 2026) and [allowance resources](https://www.occ.treas.gov/topics/supervision-and-examination/bank-operations/accounting/allowance-for-credit-losses/index-allowances-for-credit-losses.html) | Nonaccrual, charge-off and allowance conventions | credit-accounting |
| OCC, [Credit Card Lending, Comptroller's Handbook](https://www.occ.treas.gov/publications-and-resources/publications/comptrollers-handbook/files/credit-card-lending/pub-ch-credit-card.pdf) | Card portfolio structure and account-management actions | card-product-graph |
| FDIC, [Estimated uninsured deposits reporting expectations](https://fdic.gov/news/financial-institution-letters/2023/estimated-uninsured-deposits-reporting-expectations) (FIL 2023) | Uninsured fraction as a reported quantity | deposit-graph-capture |
| FFIEC, [Call Report FFIEC 051 instructions and glossary](https://www.ffiec.gov/pdf/FFIEC_forms/FFIEC051_202412_i.pdf) (Dec 2024) | Nonaccrual and past-due definitions | commercial-credit-ledger |
| Federal Reserve, [12 CFR 217.10 minimum capital requirements](https://www.federalreserve.gov/frrs/regulations/section-21710-minimum-capital-requirements.htm) and [217.11 buffers](https://www.federalreserve.gov/frrs/regulations/section-21711-capital-conservation-buffer-countercyclical-capital-buffer-amount-and-gsib-surcharge.htm); [final eSLR modification](https://www.federalreserve.gov/newsevents/pressreleases/bcreg20251125b.htm) (Nov 2025); [large-bank capital requirements](https://www.federalreserve.gov/supervisionreg/large-bank-capital-requirements.htm) | Thirteen-ratio capital coverage, buffer calibration | capital-and-ftp |
| Federal Reserve, [FR Y-14Q reporting form and instructions](https://www.federalreserve.gov/apps/reportingforms/Report/Index/FR_Y-14Q), Schedules H.1 (Corporate Loan) and H.2 (Commercial Real Estate), with the [FR Y-14Q Q&As](https://www.federalreserve.gov/publications/fr-y-14-qas/fr-y-14-qas-fr-y-14q.htm) | Commercial and CRE covariate vocabulary | commercial-credit-drivers |
| Interagency, [Policy Statement on Prudent Commercial Real Estate Loan Accommodations and Workouts](https://www.federalreserve.gov/supervisionreg/srletters/sr2305.htm) (SR 23-5, Jun 2023; OCC Bulletin 2023-23; Federal Register 88 FR 43115) | Maturity-outcome framing for CRE | commercial-credit-drivers |
| CFPB, Regulation Z [§1026.53](https://www.consumerfinance.gov/rules-policy/regulations/1026/53/) ([2021 version](https://www.consumerfinance.gov/rules-policy/regulations/1026/2021-02-17/53/)) and [§1026.54](https://www.consumerfinance.gov/rules-policy/regulations/1026/54/) | Card payment allocation and interest computation limits | consumer-card, card-credit-accounting, card-credit-accounting-v2, card-product-graph |
| CFPB, Regulation DD [§1030.4](https://www.consumerfinance.gov/rules-policy/regulations/1030/4/) and [§1030.7](https://www.consumerfinance.gov/rules-policy/regulations/1030/7/) | CD disclosure and interest conventions | term-enhancements |
| CFPB, [Bulletin 2022-04 on auto repossession](https://www.consumerfinance.gov/compliance/supervisory-guidance/cfpb-bulletin-2022-04-mitigating-harm-from-repossession-of-automobiles/) | Repossession process assumptions | consumer-installment |
| CFPB consumer explainers: [card interest](https://www.consumerfinance.gov/ask-cfpb/how-does-my-credit-card-company-calculate-the-amount-of-interest-i-owe-en-51/), [repossession](https://www.consumerfinance.gov/ask-cfpb/what-happens-if-my-car-is-repossessed-en-865/), [rate vs APR](https://www.consumerfinance.gov/ask-cfpb/what-is-the-difference-between-a-loan-interest-rate-and-the-apr-en-733/), [simple vs precomputed interest](https://www.consumerfinance.gov/ask-cfpb/whats-the-difference-between-a-simple-interest-rate-and-precomputed-interest-on-an-auto-loan-en-841/), [mortgage payments](https://www.consumerfinance.gov/ask-cfpb/how-do-mortgage-lenders-calculate-monthly-payments-en-1965/), [paying down a mortgage](https://www.consumerfinance.gov/ask-cfpb/how-does-paying-down-a-mortgage-work-en-1943/), [ARM index and margin](https://www.consumerfinance.gov/ask-cfpb/for-an-adjustable-rate-mortgage-arm-what-are-the-index-and-margin-and-how-do-they-work-en-1949/), [ARM caps](https://www.consumerfinance.gov/ask-cfpb/what-are-rate-caps-with-an-adjustable-rate-mortgage-arm-and-how-do-they-work-en-1951/) | Plain-language contractual conventions used as hand-calculation anchors | consumer-card, card-credit-accounting, consumer-installment, contractual-amortization, mortgage-state, mortgage-product-graph |
| U.S. Treasury, [31 CFR Part 356 auction regulations, Appendix B](https://www.treasurydirect.gov/files/laws-and-regulations/auction-regulations-uoc/31-cfr-part-356.pdf) | Treasury price/yield formulas | dated-term-contracts |

## 2. Basel Committee and BIS

| Source | Used for | Cited in |
|---|---|---|
| BCBS, [SRP98 Application guidance on interest rate risk in the banking book](https://www.bis.org/committees/bcbs/basel-framework/standard/srp/98/inforce/2026-01-01/published/2024-07-16) (in force 2026) | Behavioural stability vs contractual liquidity; runoff, constant and dynamic balance views | deposit-state, deposit-behaviour-drivers |
| BCBS, [CRE30 IRB approach: overview and asset class definitions](https://www.bis.org/committees/bcbs/basel-consolidated-guidelines/module/cri/30) | Default definition and rating-system expectations | credit-snapshot-calibration |
| BCBS, [CRE40 Counterparty credit risk](https://www.bis.org/committees/bcbs/basel-consolidated-guidelines/module/cri/40), paragraphs 40.43 and 40.66 | Netting set and margin period of risk | saved-derivative-integration-plan |
| BCBS, [LQY10 Liquidity risk management and supervision](https://www.bis.org/committees/bcbs/basel-consolidated-guidelines/module/lqy/10) | Liquidity transfer pricing and contingent liquidity | joint-funding-validation |
| BCBS, [PAP20 Expected credit loss accounting guidance](https://www.bis.org/committees/bcbs/basel-consolidated-guidelines/module/pap/20); [2015 Guidance on credit risk and accounting for ECL](https://www.bis.org/publications/201512-guidelines-guidance-credit-risk-and-accounting-expected-credit-losses) | Separation of economic loss and reviewed allowance | dated-ledger, implementation-plan |
| BCBS, [Guidelines for counterparty credit risk management (d588)](https://www.bis.org/bcbs/publ/d588.pdf) (2024) | Closeout and collateral modelling | derivative-contracts |
| BCBS, [Working Paper 14: Studies on the validation of internal rating systems](https://www.bis.org/publ/bcbs_wp14.pdf) ([revised](https://www.bis.org/publications/studies-validation-internal-rating-systems-revised)) (2005) | Validation of PD and migration estimates, holdout discipline | credit-snapshot-calibration, credit-model, credit-path-accounting |
| BCBS, [Credit risk modelling: current practices and applications](https://www.bis.org/publications/199904-consultation-credit-risk-modelling-current-practices-and-applications) (1999) | Portfolio credit model taxonomy | credit-model |
| BIS FSI, [Paper 10: Liquidity transfer pricing, a guide to better practice](https://www.bis.org/publications/fsi-paper-10-liquidity-transfer-pricing-guide-better-practice) (2011) | FTP tenor and contingent charge design | implementation-plan, joint-funding-validation |
| CPMI-IOSCO, [Principles for financial market infrastructures](https://www.bis.org/committees/cpmi/pfmi/overview) | Settlement finality assumptions | rust-liquidity-lifecycle-plan |

## 3. Accounting standards and interpretations

| Source | Used for | Cited in |
|---|---|---|
| FASB, [ASU 2016-13 Credit Losses (CECL)](https://storage.fasb.org/ASU_2016-13.pdf) | Lifetime loss, PCD and allowance boundaries | consumer-credit-accounting |
| IASB, [IFRS 9 Financial Instruments](https://www.ifrs.org/issued-standards/list-of-standards/ifrs-9-financial-instruments/) ([2021 consolidated text](https://www.ifrs.org/content/dam/ifrs/publications/pdf-standards/english/2021/issued/part-a/ifrs-9-financial-instruments.pdf)) | Effective interest, amortised cost, derecognition | dated-income, aged-units, derivative-accounting |
| IASB, [IAS 32 Financial Instruments: Presentation](https://www.ifrs.org/issued-standards/list-of-standards/ias-32-financial-instruments-presentation/) | Gross vs net presentation of derivative positions | derivative-accounting |
| IASB staff papers, Sept 2025: [AP11A Determining the effective interest rate](https://www.ifrs.org/content/dam/ifrs/meetings/2025/september/iasb/ap11a-determining-effective-interest-rate.pdf) and [AP11B Subsequent changes](https://www.ifrs.org/content/dam/ifrs/meetings/2025/september/iasb/ap11b-subsequent-changes-effective-interest-rate.pdf); [amortised cost measurement project](https://www.ifrs.org/projects/work-plan/amortised-cost-measurement/) | Original-yield anchoring and catch-up vs prospective revision | product-units, deposit-original-income, dated-income |
| IFRIC updates: [March 2019](https://www.ifrs.org/news-and-events/updates/ifric/2019/ifric-update-march-2019/), [June 2021](https://www.ifrs.org/news-and-events/updates/ifric/2021/ifric-update-june-2021/), [February 2022](https://www.ifrs.org/news-and-events/updates/ifric/2022/ifric-update-february-2022/) | Curing of credit-impaired assets; demand deposit cash classification; EIR revision | derivative-accounting, commercial-credit-ledger, deposit-original-income |
| FASB ASC 815 hedge accounting: [ASU 2017-12 Targeted Improvements (FASB In Focus)](https://storage.fasb.org/FASB%20in%20Focus--Derivatives%20and%20Hedging%20%288-17%29.pdf) (Aug 2017); ASU 2022-01 Portfolio Layer Method (Mar 2022) and ASU 2025-09 Hedge Accounting Improvements (Nov 2025) via the [FASB ASU index](https://www.fasb.org/standards/accounting-standards-updates-issued); full texts require the FASB codification login | Hedge designation, effectiveness and recognition rules | hedge-accounting |
| FASB [ASC 320-10-25](https://asc.fasb.org) Investments—Debt Securities, HTM sale exceptions (paragraphs 25-6 and 25-14); codification access requires login | HTM taint and reclassification policy | ledger HTM accounting |

## 4. Market conventions and reference rates

| Source | Used for | Cited in |
|---|---|---|
| ISDA, [2021 Interest Rate Derivatives Definitions](https://www.isda.org/2021/10/04/2021-isda-interest-rate-derivatives-definitions/); [settlement matrix for early termination and swaptions](https://www.isda.org/book/isda-definitions-settlement-matrix-for-early-termination-and-swaptions/); [EUR swaption collateralized cash price practice](https://www.isda.org/2018/11/26/market-practice-change-for-settlement-of-eur-swaptions-to-collateralized-cash-price/); [collateral management SOP](https://www.isda.org/collateral-management-sop/) | Swap and swaption conventions, closeout, collateral operations | derivative-contracts, rust-liquidity-lifecycle-plan |
| ARRC, [SOFR bilateral business loan conventions](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2020/ARRC_SOFR_Bilat_Loan_Conventions.pdf), [syndicated loan conventions](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2020/ARRC_SOFR_Synd_Loan_Conventions.pdf) and [technical appendices](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2020/ARRC-Syndicated-Loan-Conventions-Technical-Appendices.pdf) (2020); [Term SOFR and averages conventions](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2021/Term_SOFR_Avgs_Conventions.pdf) (2021) | Lookback, observation shift, in-advance and in-arrears coupon arithmetic | floating-rate, dated-cashflows, dated-term-contracts, term-enhancements, commercial-credit-ledger |
| New York Fed, [Additional information about reference rates](https://www.newyorkfed.org/markets/reference-rates/additional-information-about-reference-rates) | SOFR publication and methodology | floating-rate, dated-cashflows, funding-contracts, funding-product-graph, term-product-graph, implementation-plan |
| New York Fed, [TOMO repo collateral schedule](https://www.newyorkfed.org/markets/domestic-market-operations/monetary-policy-implementation/repo-reverse-repo-agreements/TOMO-Repo-Collateral-Schedule); ICMA, [What is a haircut?](https://www.icmagroup.org/market-practice-and-regulatory-policy/repo-and-collateral-markets/icma-ercc-publications/frequently-asked-questions-on-repo/21-what-is-a-haircut/); Federal Reserve FEDS Note, [Proportionate margining for repo transactions](https://www.federalreserve.gov/econres/notes/feds-notes/proportionate-margining-for-repo-transactions-20250214.html) (Feb 2025) | Repo haircut and margin mechanics | funding-contracts, funding-product-graph |
| Fannie Mae, [Single-family MBS prospectus](https://capitalmarkets.fanniemae.com/media/26506/display) (Dec 2025) and [principal and interest remittance](https://singlefamily.fanniemae.com/servicing/principal-and-interest-remittance); Freddie Mac, [MBS product documentation](https://capitalmarkets.freddiemac.com/mbs/products) | Agency pass-through cash timing and remittance lag | mortgage-state, mortgage-product-graph, mortgage-credit-accounting |
| Federal Reserve FEDS Note, [Mortgage servicing right valuations under stress](https://www.federalreserve.gov/econres/notes/feds-notes/mortgage-servicing-right-valuations-under-stress-20260604.html) (Jun 2026) | Servicing strip sensitivity | mortgage-state |

## 5. Quantitative methods

| Source | Used for | Cited in |
|---|---|---|
| Longstaff and Schwartz (2001), [Valuing American options by simulation](https://escholarship.org/uc/item/43n1k4jb) | Regression-based continuation for conditional exercise | commercial-exercise, commercial-generated-exercise |
| Glasserman and Yu (2004), [Number of paths versus number of basis functions in American option pricing](https://business.columbia.edu/faculty/research/number-paths-versus-number-basis-functions-american-option-pricing) | Basis-count and path-count convergence controls | commercial-generated-exercise, commercial-generated-oas |
| Lim, Li and Linetsky (2012), [Evaluating callable and putable bonds](https://arxiv.org/abs/1206.5046) | Callable/putable bond valuation benchmarks | term-enhancements |
| Brigo and Morini (2010), [Bilateral counterparty risk with closeout](https://arxiv.org/abs/1011.3355) | Closeout valuation conventions | derivative-contracts |
| OpenGamma, [Multi-curve framework variations](https://quant.opengamma.io/Multi-Curves-Variations2.pdf); Strata [log-linear interpolator](https://strata.opengamma.io/apidocs/com/opengamma/strata/market/curve/interpolator/CurveInterpolators.html) | Multi-curve discounting and interpolation convention | market-models, aged-units |
| QuantLib, [callable bond discretisation source](https://github.com/lballabio/QuantLib/blob/master/ql/experimental/callablebonds/discretizedcallablefixedratebond.cpp) and [rate curve slides](https://www.quantlib.org/slides/rate-curves.pdf) | Independent implementation reference | term-enhancements, term-exercise-publication, derivative-contracts |
| finmath-lib, [Bachelier model](https://www.finmath.net/finmath-lib/apidocs/net.finmath.lib/net/finmath/functions/BachelierModel.html), [analytic formulas](https://www.finmath.net/finmath-lib/apidocs/net.finmath.lib/net/finmath/functions/AnalyticFormulas.html) and [Brownian motion interface](https://www.finmath.net/finmath-lib/apidocs/net.finmath.lib/net/finmath/montecarlo/BrownianMotion.html) | Normal-vol option formulas and Monte Carlo interface | market-models, market-option-convergence |
| Giles, [Multilevel Monte Carlo lecture](https://www2.maths.ox.ac.uk/~gilesm/talks/imperial_15.pdf) (2015) | Convergence diagnostics | market-option-convergence |
| NIST, [DLMF 7.9 continued fractions for the error function](https://dlmf.nist.gov/7.9) | Stable normal-integral evaluation | market-models |
| NIST, [FIPS 180-4 Secure Hash Standard](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf) | Source and identity hashing | credit-covariate |
| Fox and Glynn (1988), [Computing Poisson probabilities](https://web.stanford.edu/~glynn/papers/1988/FG88.html) | Uniformisation truncation bounds | credit-operator |
| Al-Mohy and Higham (2009), [A new scaling and squaring algorithm for the matrix exponential](https://eprints.maths.manchester.ac.uk/1218/1/alhi09.pdf) and [(2011) action of the matrix exponential](https://eprints.maths.manchester.ac.uk/1300/); SciPy [`expm` documentation](https://docs.scipy.org/doc/scipy/reference/generated/scipy.linalg.expm.html) | Transition matrix exponentials and the independent SciPy oracle | credit-operator, mortgage-credit-accounting |
| Van Dijk, van Brummelen and Boucherie (2018), [Uniformization: basics, extensions and applications](https://ris.utwente.nl/ws/files/24801192/2018_XDijkBrummelenBoucherie_S0166531617301451_main.pdf) | Uniformised CTMC transitions | credit-operator |
| Anderson (2007), [A modified next reaction method](https://web.math.wisc.edu/~dfanderson/papers/AndNRM.pdf) | Event-time simulation with time-varying intensities | credit-path |
| Vestergaard and Génois (2015), [Temporal Gillespie algorithm](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1004579) | Exact event-time credit paths on driver paths | credit-path, credit-path-accounting, driven-credit-ledger |
| Bacci et al. (2023), [MM algorithms to estimate parameters in continuous-time Markov chains](https://arxiv.org/pdf/2302.08588) | Interval-censored CTMC estimation | credit-calibration |
| Lange and Minin (2013), [Fitting and interpreting continuous-time latent Markov models for panel data](https://pmc.ncbi.nlm.nih.gov/articles/PMC3795797/) | Panel-data CTMC likelihood | credit-calibration |
| Jackson (2011), [Multi-state models for panel data: the msm package for R](https://www.jstatsoft.org/article/view/v038i08) and [msm reference](https://chjackson.github.io/msm/reference/msm.html) | Independent reference for snapshot and deposit calibration | credit-snapshot-calibration, deposit-calibration |
| Torczon (1997), [On the convergence of pattern search algorithms](https://epubs.siam.org/doi/10.1137/S1052623493250780) | Derivative-free optimiser fallback | credit-snapshot-calibration |
| Broström, eha package, [piecewise constant hazards vignette](https://www.stat.ethz.ch/CRAN/web/packages/eha/vignettes/tpchreg.html) | Poisson/piecewise-hazard likelihood equivalence | credit-covariate, driven-credit-ledger |

## 6. Behavioural and credit research

| Source | Used for | Cited in |
|---|---|---|
| Philadelphia Fed WP 26-09 (2026), [Model risk under CECL: a consumer finance perspective](https://fedinprint.org/item/fedpwp/102431/original) | Sensitivity of lifetime loss to macro forecast error | consumer-behaviour-drivers, commercial-credit-drivers (by analogy) |
| Heitfield and Sabarwal (2004), [What drives default and prepayment on subprime auto loans?](https://link.springer.com/article/10.1023/B:REAL.0000044023.02636.e6), Journal of Real Estate Finance and Economics 29(4) ([working paper](https://econwpa.ub.uni-muenchen.de/econ-wp/fin/papers/0405/0405034.pdf)) | Competing default/prepayment on auto loans | consumer-behaviour-drivers |
| [arXiv 2211.09176](https://arxiv.org/pdf/2211.09176), multistate study of auto ABS pools | Prepayment follows vehicle values and equity | consumer-behaviour-drivers |
| [Springer 2025 review of machine-learning credit scoring](https://link.springer.com/article/10.1007/s10462-025-11416-2); [arXiv 2506.02075](https://arxiv.org/pdf/2506.02075) on evaluating survival models | Challenger model and evaluation design | consumer-behaviour-drivers |
| [Taylor & Francis 2026](https://www.tandfonline.com/doi/full/10.1080/08839514.2026.2676392), transformer utilisation forecasting | Noted, not adopted | consumer-behaviour-drivers |
| Drechsler, Savov and Schnabl (2021), *Banking on Deposits: Maturity Transformation without Interest Rate Risk*, Journal of Finance 76(3) ([NBER WP 24582](https://www.nber.org/papers/w24582); [CEPR DP 12950](https://ideas.repec.org/p/cpr/ceprdp/12950.html)) | Deposit beta and franchise value | deposit-behaviour-drivers |
| Jiang, Matvos, Piskorski and Seru (2023, rev. 2024), [Monetary Tightening and U.S. Bank Fragility in 2023](https://www.nber.org/papers/w31048), NBER WP 31048; Journal of Financial Economics (2024) | Uninsured deposit concentration and run incentives | deposit-behaviour-drivers |
| Koont, Santos and Zingales (2024, rev. 2025), [Destabilizing Digital "Bank Walks"](https://www.nber.org/papers/w32601), NBER WP 32601 | Digital channel and deposit velocity | deposit-behaviour-drivers |
| New York Fed Liberty Street Economics, [Deposit Betas: Up, Up, and Away?](https://libertystreeteconomics.newyorkfed.org/2023/04/deposit-betas-up-up-and-away/) (Apr 2023); Federal Reserve [Senior Financial Officer Survey](https://www.federalreserve.gov/data/sfos/september-2023-senior-financial-officer-survey.htm) (Sep 2023, Mar 2024: cumulative retail and wholesale betas); Dallas Fed WP 2315, [Deposit convexity](https://fedinprint.org/item/feddwp/97131/original) | Observed cumulative betas by segment; threshold behaviour | deposit-behaviour-drivers |
| Belkin, Suchower and Forest (1998), *A one-parameter representation of credit risk and transition matrices*, KPMG working paper, in [CreditMetrics Monitor Q3 1998](https://www.msci.com/www/research-paper/creditmetrics-monitor-third/014211989) | Z-shift macro conditioning of migration matrices | commercial-credit-drivers |
| Vasicek (2002), [Loan portfolio value](https://www.maths.univ-evry.fr/pages_perso/crepey/Credit/Vasicek.pdf), Risk 15(12); Gupton, Finger and Bhatia (1997), [CreditMetrics Technical Document](https://www.msci.com/resources/technical_documentation/CMTD1.pdf); Wilson (1998), [Portfolio credit risk](https://www.newyorkfed.org/medialibrary/media/research/epr/98v04n3/9810wils.pdf), FRBNY Economic Policy Review (CreditPortfolioView) | Single- and multi-factor correlated default | commercial-credit-drivers |
| Jiménez, Lopez and Saurina (2009), *Empirical analysis of corporate credit lines*, Review of Financial Studies 22(12), doi:10.1093/rfs/hhp061 ([FRBSF WP 2007-14](https://www.frbsf.org/wp-content/uploads/wp07-14bk.pdf)) | Utilisation rises before default | commercial-credit-drivers |

## 7. Public data sources (not whitepapers)

Listed for completeness because the market-data and forecast documents cite them
beside the research above: [FRED API](https://fred.stlouisfed.org/docs/api/fred/series_observations.html),
[Fed GSW nominal yield curve](https://www.federalreserve.gov/data/nominal-yield-curve.htm),
[Treasury daily rates XML](https://home.treasury.gov/treasury-daily-interest-rate-xml-feed),
[NY Fed markets API](https://markets.newyorkfed.org/static/docs/markets-api.html),
[NY Fed Survey of Market Expectations](https://www.newyorkfed.org/markets/market-intelligence/survey-of-market-expectations),
[FOMC SEP tables](https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm),
[Philadelphia Fed median forecasts](https://www.philadelphiafed.org/surveys-and-data/real-time-data-research/median-forecasts),
[FHFA HPI](https://www.fhfa.gov/hpi/download/monthly/hpi_master.csv),
[Freddie Mac PMMS](https://www.freddiemac.com/pmms/docs/PMMS_history.csv),
[FDIC national rates](https://www.fdic.gov/national-rates-and-rate-caps),
[FDIC BankFind API](https://api.fdic.gov/banks/docs),
[SEC EDGAR companyfacts](https://www.sec.gov/search-filings/edgar-application-programming-interfaces),
[Eris futures files](https://files.erisfutures.com/ftp/),
[PyIceberg API](https://py.iceberg.apache.org/api/).

## Maintenance

- Add a row here whenever a model document gains an external citation; the
  "Cited in" column is the index used to update paragraph references when a
  source is revised.
- Prefer the publisher link; where only a working-paper copy is open access,
  keep both and cite the published version's volume and issue.
- The regulatory items dated 2025–2026 (Basel III re-proposal, SR 26-2, eSLR
  final rule, OCC advisory series) change faster than this list; check the
  issuing agency before quoting a paragraph in a validation pack.
