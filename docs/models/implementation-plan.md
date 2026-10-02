# Product model enhancement implementation and evidence

Tracking scope: [GitHub issue 25](https://github.com/beauschwab/model-portfolio-sim/issues/25).
This document records implementation stages, not a reduced definition of completion.
Every child issue's acceptance criteria remain required. A numerical implementation
is not an empirical calibration, a regulatory methodology approval or a deployment.

## Ownership and order

The common `portfolio-model-core` crate holds reusable typed financial contracts.
Product runtimes depend on it; it must not depend on API, persistence or product
crates. Inputs are explicit, bounded and versioned. Calculation stays in Rust;
Python adapters transport raw inputs and explicit independent Python references
remain test support. New contracts do not implicitly overwrite old model economics.

1. Define and validate shared credit transitions and dated cashflow contracts.
2. Integrate whole loans, deposits and corporate contracts with these foundations.
3. Implement auto, unsecured personal and revolving-card models and full product
   adapters, views, cohort audits and applicable strategy templates.
4. Complete CDs, derivatives/netting and contractual markets/funding flows.
5. Integrate accounting, aged forward coefficients and capital/FTP, preserving
   independent allocation and persisted-journal publication gates.

Independent contract improvements may run alongside foundational work. Model
and calibration dependencies are explicit; no product silently receives a newly
estimated coefficient or a different amortization convention.

## Requirement ledger

| Issue | Scope | Current stage | Completion evidence still required |
|---|---|---|---|
| #11 | Credit states, term PD/LGD/EAD, competing exits and recoveries | Shared expectation contract and exact observed-spell fitter implemented | Product/ledger adoption, stochastic mode, covariate/term/LGD/EAD fits, real-history evaluation, complete issue gates |
| #12 | Actual-date scenario cashflows | Shared contract and fixed corporate/CD dated producer implemented | Behavioral/credit scenario graph generation, daily journal integration, bounded mixed-book acceptance |
| #13 | Mortgage/MBS prepayment, credit, ARM and guarantee mechanics | Planned | Entire issue scope and empirical holdout evidence |
| #14 | Deposit retention, migration and unified stress | Planned | Entire issue scope and observed retention validation |
| #15 | Corporate/commercial schedules, resets, exercise and credit | Fixed level-payment pricing; shared supplied-fixing coupon stage | Forward-index/reset integration, optimal exercise, credit integration and full issue gates |
| #17 | Auto lending | Planned | Native product engine and complete end-to-end/calibration gates |
| #18 | Personal lending | Planned | Native product engine and complete end-to-end/calibration gates |
| #19 | Revolving credit cards | Planned | Statement/payment/EAD state engine and complete end-to-end/calibration gates |
| #16 | CDs | Fixed nonoptional actual-date contractual producer | Withdrawal, penalty, exercise, renewal and complete issue gates |
| #20 | Derivatives and dealer counterparties | Planned | Pricing, legal netting/CSA, margin/default capture and full issue gates |
| #21 | Markets/repo/funding | Planned | Contractual rollover/collateral integration and full issue gates |
| #10 | Market drivers | Research stage | Versioned model implementation, convergence/calibration and full issue gates |
| #22 | Income accounting | Planned | Explicit policies, exact accrual and full issue gates |
| #23 | Forward strategies | Planned | Aged native coefficients, interpolation error and full issue gates |
| #24 | Capital/FTP | Planned | Product exposure/funding preparation and full issue gates |

## Research basis and design decisions

Research checked 2026-10-01. Primary sources guide model structure; the project
does not copy supervisory parameters or claim supervisory suitability.

- The Federal Reserve's [2025 detailed model descriptions](https://www.federalreserve.gov/publications/2025-june-supervisory-stress-test-methodology-descriptions-supervisory-models.htm)
  motivate product-specific delinquency transitions and competing exits. The
  [2026 methodology](https://www.federalreserve.gov/publications/files/2026-february-supervisory-stress-test-methodology.pdf)
  documents the subsequent annual scope and adjustments. We use these as dated
  modeling references, not substitutes for a bank's data or applicable policy.
- The [Basel credit/ECL guidance](https://www.bis.org/publications/201512-guidelines-guidance-credit-risk-and-accounting-expected-credit-losses)
  supports keeping credit risk estimation, expected cash shortfalls and accounting
  policy explicit. A supplied economic hazard is not an accounting staging rule.
- Current [SR 26-2 guidance](https://www.federalreserve.gov/supervisionreg/srletters/SR2602.htm)
  supersedes SR 11-7. Validation records must use the current, risk-based framework
  while preserving independent financial reference tests and documented limits.
- [New York Fed SOFR methodology](https://www.newyorkfed.org/markets/reference-rates/additional-information-about-reference-rates)
  distinguishes actual value-date day weights and overnight compounding. Contracts
  therefore require explicit observation intervals; no hidden conversion from
  three-month simulated forwards to contractual overnight fixings is permitted.

The shared deterministic credit stage uses explicit continuous-time hazards and
absorbing default/prepayment states, with cures among active states. This is a
project design choice: it prevents independently applied exit probabilities from
spending the same surviving exposure twice. Piecewise intervals, exposure inputs,
loss severity and recovery timing are supplied assumptions until calibrated.

## Validation levels

For each model retain controlled-path hand calculations, explicit independent
references, domain/failure admission, conservation and zero-change identities.
For stochastic models also retain seeded shared-draw, convergence and stress
comparisons. Fresh-process ownership tests must forbid Python financial callbacks.
Product-to-ledger adoption additionally requires independent replay of immutable
postings and failure/cancellation publication tests. Empirical outcomes use separate
chronological holdouts with source hashes and never borrow training observations.

## Open input requirements

Empirical prepayment, deposit retention, consumer transition/payment and recovery
fits require dated observed histories. The repository's synthetic fixtures cannot
prove their predictive accuracy. Regulatory eligibility/requirements and accounting
policy require explicit reviewed inputs. Code progress can continue without them;
those acceptance gates remain open and must not be marked complete.

## Delivered foundation stage

Detailed contracts: [credit](credit-model.md), [observed credit fitting](credit-calibration.md),
[dated events](dated-cashflows.md), [dated term production](dated-term-contracts.md),
[fixed level payments](contractual-amortization.md), [floating coupons](floating-rate.md)
and [native entrypoints](native-contracts.md). The new shared crate has no API,
persistence or product dependency. Model versions and complete supplied assumptions
remain explicit. No issue is closed by delivering these foundational stages.
