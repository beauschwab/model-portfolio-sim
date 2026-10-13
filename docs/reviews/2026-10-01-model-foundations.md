# Product model foundations: 0.29.5 local validation

This is a local implementation checkpoint for
[tracker #25](https://github.com/beauschwab/model-portfolio-sim/issues/25).
The complete backlog remains open. Changes in this shared working tree have not
been published as a commit or deployed; existing unrelated changes are preserved.

## Delivered behavior

- Shared Rust `portfolio-model-core`: strict credit/date/floating contracts,
  monetary/metadata admission and reusable calendar/amortization arithmetic.
- `credit-model-1`: deterministic multi-state cures and competing default/payoff,
  state-specific EAD/CCF/LGD, explicit physical/pricing measure and complete
  recovery tails. Recoveries are timed from interval-end default recognition.
- `observed-credit-calibration-1`: exact transition-time hazard estimation and
  chronological holdout diagnostics. Missing exposure requires explicit assumed
  rows or rejection; observed no-event exposure is distinguished from no data.
  Estimates are physical, preserve source/policy/split identities and never
  automatically become pricing hazards. No bank dataset was supplied or fitted.
- `dated-cashflow-1` and `dated-term-1`: identity-preserving dated events and a
  bounded fixed corporate/CD producer, with separate scheduled/accrual/settlement
  dates, explicit accrual conventions and cash direction. Leap dates, stubs,
  supplied holidays and preceding settlement retain their actual dates.
- Fixed corporate `level_payment`: true equal total payments through existing
  native pricing. Saved `annuity` remains exactly equal principal. Floating,
  negative amortization, ambiguous sinking and terminal truncation fail explicitly.
- `floating-rate-1`: supplied-fixing term/simple/compounded coupon arithmetic,
  explicit index/all-in bounds, margin treatment and observation weights.
  Intraday publication/reset cutoffs prevent unavailable fixing use. Calendar
  mapping is explicitly unverified; no future index rates are invented.

Python analysis adapters transport inputs/results only; both the DLL and native
standalone lifecycle use these Rust contracts. No financial fallback or API/UI
backend selector was added. Product ABI 8 and existing ledger protocol remain;
new additive schemas require rebuilt artifacts. Product and decision/workflow
release artifacts were rebuilt locally, without restarting a running service.

## Validation

| Gate | Result |
|---|---|
| Shared Rust crate | 50 tests passed |
| Native product crate | 42 tests passed (20 existing, 7 amortization, 15 dated term) |
| Native decision release crate | 8 tests passed |
| New DLL/standalone/default adapter gates | 20 tests passed |
| Initial full engine regression | 751 passed; 2 intentional Python-reference skips |
| Final full engine regression on rebuilt artifacts | 755 passed; the same 2 intentional reference-only skips |

The two full-suite skips are the Python-reference parameterizations of native
actor/standalone ownership tests in `test_decision.py`; their Rust counterparts
pass. They do not represent unavailable native artifacts. Shared/product/decision Clippy
denies warnings, formatting passes, model document links/fences are checked and
the three operator skill mirrors are identical; the skill archive was regenerated.

Independent expectations include competing-risk closed forms, SciPy matrix
exponentials, exact exposure/count MLEs, chronological holdout-only mutations,
hand-generated dates, annuity payments and independently rooted OAS/fixed-spread
marks. Fresh default-process tests forbid Python financial callbacks. Standalone
execution confirms the new calculation contracts do not need a Python runtime.

The final rerun took 342.57 seconds. These checks establish implementation behavior; they do not
establish empirical predictive accuracy, regulator approval, live deployment or
acceptance of new schedules in persisted daily journals.

Final release artifact SHA-256 identities:

| Artifact | SHA-256 |
|---|---|
| Product DLL | `9dc5138e64ccb6c6a8c96b528df3f691dff6596830476970fa44b68f9cb9e63e` |
| Native lifecycle | `db1d9099b2ab2a9479fce14af22011a1dc863b3750c0e21cb1985cae626567ef` |
| Decision DLL | `d24754668fb144d2af0d064b4429f18de182e3b9c1da502a12fc328b9ad79a2f` |
| Native strategy | `9032336557b63dc2c9a9bdba554f17eb45c22374174052242b3be262aa928ed3` |
| Native workflow | `1c5e2fc1d1bcbdd535476895f6f32fdc65a5f1773656c39f9c6cfa845540770b` |

## Required follow-through

#11 remains open for covariate/term/LGD/EAD calibration, correlated credit paths,
product cashflows, reviewed allowance/nonaccrual/writeoff policy and daily replay.
#12 remains open for scenario graph generation, typed event ownership and bounded
daily journal integration. #15 remains open for future index projection/reset
integration, optimal exercise and credit. #16 remains open for withdrawals,
penalties, exercise and renewal. Other product and accounting/capital/strategy
issues retain their entire acceptance scope.

Daily integration must assign one explicit owner of each position's economics.
The existing loop proportionally rescales scheduled amounts and separately
applies default/withdrawal behavior. Routing scenario-generated events into that
loop unchanged would double-count reductions. The existing monthly bridge stays
readable under its old version; using the enhanced state/event engine requires
an explicit rebuilt model and independently replayed journal publication.

[The implementation ledger](../models/implementation-plan.md) and
[native contracts](../models/native-contracts.md) link complete model documentation,
research sources, input domains and remaining assumptions.
