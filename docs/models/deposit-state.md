# Daily native physical deposit state

`portfolio_model_core::deposit_state::simulate` accepts strict `deposit-state-1`
inputs and returns daily offered rates, cohort/segment balances, migrations,
earned interest, external cash, and closing rights. This is the supplied-policy
state foundation for #14. Its parameters are explicitly **supplied assumptions**.
It does not fit retention from paid-rate histories, establish observed account
accuracy, or complete pricing/ledger/FTP/public-product adoption by itself.

The existing production deposit fitter already supports logistic beta and
asymmetric adjustment. This version does not reinterpret it as a fixed-beta
model or silently replace saved monthly economics. Instead it provides a
separate versioned daily contract that can carry linear or logistic-beta offered
rate policies and one shared balance transition policy. Adopting it requires
explicit library/session rebuild and coordinated graph, daily stress, journal
and FTP ownership. The old monthly final payout and separate daily flight formula
must not be applied again to these generated balances.

## Research basis and evidence boundaries

The [OCC Interest Rate Risk handbook](https://www.occ.gov/publications-and-resources/publications/comptrollers-handbook/files/interest-rate-risk/pub-ch-interest-rate-risk.pdf)
treats deposit pricing sensitivity and runoff as separate assumptions. It calls
for documentation appropriate to model complexity, historical customer
behavior, competition and economic conditions, and review of key assumptions.
Recovering a synthetic rate-fit parameter therefore does not validate retention.

[Basel's current application guidance, SRP98](https://www.bis.org/committees/bcbs/basel-framework/standard/srp/98/inforce/2026-01-01/published/2024-07-16)
distinguishes contractual demand liquidity from behavioral stability, and
different runoff, constant-balance and dynamic-balance viewpoints. It also
distinguishes the information used for economic value and earnings. This model
accordingly separates withdrawals, internal migration, new business, residual
balances and continuation-value assumptions. It is not regulatory core-deposit
classification, prescribed standardized slotting, or proof of stability in a run.

## Identity, ownership and supplied policy

Every request supplies entity, cohort, currency, source revision, model version,
assumption ID and rationale, run identity and physical measure. Amounts are
actual monetary units without an implicit multiplier. Segment names are unique
and immutable within the request. The cohort's customers can migrate between
its deposit product/segment states without changing the bank's total principal.
External inflows explicitly enter that cohort; they are not inferred as payments
or renewal of lost balances. A request describes expected **balance mass**, not
account counts or a reconstruction of individual closures. Account lineage and
representative/cohort approximation must remain attached upstream.

`parameter_status` is always `supplied_assumption`. No caller can set a boolean
claim that this kernel fitted retention. The optional `ObservedRange` carries a
caller-declared dated history revision, reference-rate range and whether a
logistic plateau was identified. It is support metadata, not an audited fit.
Each daily point reports whether that segment's selected observation is outside
the declared range, and whether a range was supplied at all. Unknown regime
support does not mean an in-regime pass. The caller must decide publication
policy for unsupported extrapolation and retain parameter-sensitivity scenarios;
the kernel neither fabricates a confidence interval nor suppresses the flag.

Only physical measure is admitted. Economic-value pricing can consume these
same conditional physical state cashflows under its explicitly selected
discount/valuation convention, but that does not convert the probabilities or
policy into pricing measure. There is no separate kernel-side stress-flight
multiplier. The same function and inputs determine state for runoff, paid-rate
NII and liquidity projection. Full integration must verify that consumers do
not post the principal reduction, interest or migration a second time.

## Actual observations and offered repricing

Drivers are daily exact Gregorian rows, strictly contiguous from
`as_of - maximum_observation_lag - 1` through `horizon_end - 1`. The extra
previous day supports actual reference-rate velocity. Each row supplies
reference rate and per-segment competitor rates, offered-spread stress,
confidence stress, and external inflows. Historical rows must have zero
external inflow because opening balances already reflect history. Missing
history/forecast days, dimensions or dates fail. There is no forward filling,
interpolation, guessed fixing, calendar adjustment, or future observation used
before its lag expires.

For segment `i` on date `d`, repricing observes the complete row at
`d - lag_i`. A linear target before floor/ceiling is:

`intercept + beta * observed_reference + competitor_weight * (observed_competitor - observed_reference) + observed_offered_spread_stress`.

The logistic variant replaces beta with:

`beta_min + (beta_max-beta_min)/(1 + exp(-steepness*(observed_reference-midpoint)))`.

The sigmoid uses a numerically stable branch in extreme regimes. Beta bounds,
midpoint and steepness are supplied and validated. Logistic target economics
are not claimed to be identified when rate history never reaches a plateau.
Competitor premium and offered spread affect the **actual paid target**, not
just a withdrawal score.

The target is clamped to the supplied floor and ceiling. The applicable
upward/downward response half-life is selected against the previous paid rate:

`adjustment = 1 - exp(-ln(2)/half_life_days)`;

`paid_today = previous_paid + adjustment * (bounded_target-previous_paid)`.

Zero half-life is explicitly immediate adjustment. Positive half-life uses real
calendar days, and the final paid rate is bounded again. The model carries the
actual offered rate at each point. Floor/ceiling policies can be negative within
admission; signed expense/cash rights then remain signed. This is a policy for
administered rate changes, not an inferred market-rate fixing contract.

## One competing daily transition

The date's current competitor gap and reference velocity enter retention after
the offered rate has changed. For one origin segment:

`gap = max(current_competitor - paid_today, 0)`;

`velocity = abs(reference_today-reference_yesterday)`;

`stress_exposure = uninsured_coefficient*uninsured_fraction + concentration_coefficient*concentration + digital_coefficient*digital_fraction - relationship_protection*relationship_fraction - operational_protection*operational_fraction`;

`withdrawal_intensity = max(0, base_withdrawal + gap_sensitivity*gap + velocity_sensitivity*velocity + confidence_stress*stress_exposure)`;

`closure_intensity = base_closure + closure_stress_sensitivity*confidence_stress`.

Each named internal migration has supplied base intensity plus a sensitivity to
the positive destination-minus-origin offered-rate gap. Migration destinations
past a true contractual maturity are unavailable. Their intensity is zero, and
is not reassigned as an external withdrawal or synthetic reinvestment.

All active origin exit intensities compete once. Let `H` be withdrawal, closure
and migration intensity summed for that row:

`daily_exit_fraction = 1-exp(-H/365)`;

`fraction_to_cause = daily_exit_fraction * intensity_to_cause/H`.

When `H=0`, all exit fractions are exactly zero. The implementation uses
`expm1` for small intensities. The retained share is `1-daily_exit_fraction`.
Annual intensity time uses ACT/365 elapsed days; it is distinct from the selected
contract interest denominator. The admitted combined row intensity is at most
100 per year; an extreme driver/policy combination that exceeds it fails rather
than being silently clipped to an arbitrary runoff cap.

Rows read the same opening vector and accumulate a new vector simultaneously.
Newly migrated principal cannot migrate, withdraw or close a second time that
day. Accrued rights move in the same fraction as origin principal; withdrawals
and closures settle their fraction of accrued interest. Internal migration is
a transfer report, not external bank cash. Withdrawals, closures, true maturity
and assumed horizon payout have distinct cash event identities and totals.

This is a **daily single-transition convention**, not an exact continuous-time
multi-jump process or an intraday withdrawal model. In particular, migrating
balances cannot traverse several product states within a day. Its daily timing
is an explicit approximation to observed intraday behavior. If data require
multiple jumps, the transition engine must be replaced or refined under a new
policy/version rather than described as continuous-time exact.

## Event timing, interest, maturity and horizon

At date `d`, the model settles any true contractual maturity, adjusts offered
rates, allocates the competing transition, and adds separately supplied inflows.
Transitions are placed at the **start** of `[d,d+1)`. New inflows enter after
those transitions and do not immediately run off in the same daily step.
Post-transition balances accrue simple deposit interest at their destination's
new offered rate over `[d,d+1)` using supplied ACT/360 or ACT/365F. Interest is
not added to principal. A supplied end-of-day settlement date pays accumulated
interest as cash and clears that payable without recognizing it twice. No
monthly interest-payment date is guessed. Opening accrued rights, daily expense,
cash interest and closing accrued rights independently reconcile.

Cash uses the bank's perspective: new deposits received are positive principal;
withdrawals, closures and maturity payouts are negative principal; interest
expense settlement is normally negative cash interest. Recognition amounts use
the depositor's accrued-right/expense sign. Negative paid rates therefore create
negative expense and positive interest cash when settled.

A true contractual maturity is a separately supplied exact ordinal, never
inferred for an NMD. It stops that segment's remaining principal/interest and
disables later inflows. Maturity at the closing horizon still settles explicitly.
This schema supplies no automatic rollover/renewal fraction; a renewal model
must define its source and new contract separately.

Terminal treatment is mandatory:

| Policy | Physical closing behavior |
|---|---|
| `retain_residual` | Leave principal and unpaid accrued rights visible; no invented final payout. |
| `assumed_liquidation` | Explicitly repay remaining principal/interest at horizon with a supplied rationale and distinct assumed event. |
| `franchise_value_only` | Retain physical balances and report a supplied continuation value separately; no cash settlement or inferred discounting. |

A continuation value's sign/unit matches the caller's valuation convention and
currency label. It is not an added physical deposit liability or income. Full
horizon-sensitivity work must compare actual residuals, cash, NII and the
selected continuation convention across horizons. Extending a horizon does not
alter the prior state when policies, drivers and cash-payment dates retain the
same prefix.

## Conservation, admission and acceptance

The principal identity is:

`opening balance + external inflows = withdrawals + closures + contractual maturity payouts + assumed terminal payouts + closing balance`.

Migration does not appear as external loss. The independent rights identity is:

`opening accrued rights + earned interest expense + bank-perspective cash interest = closing accrued rights`.

The engine checks conservation without renormalizing state or relaxing published
portfolio output tolerances. It admits at most 16 segments, 36600 calendar days
subject to 250000 daily segment/migration/work units, and at most one named
migration rule per distinct pair. Segment IDs are at most 64 bytes; other
identity/history labels are at most 256 bytes and supplied rationales 1024.
Observation lag is at most 366 days. Driver dimensions/history, payment dates,
rate/coefficient/fraction domains and opening/closing/cumulative monetary
admission are validated. Amount and gross accrued/interest/cash-interest bounds
use shared `1e15`. Work counts include potential output rows. Component bounds
are not an aggregate process RSS guarantee or regulatory capital admission.

`tests/deposit_state.rs` contains 21 independent analytical fixtures. They cover
actual leap interest, residual without payout, competing withdrawal/closure/
migration, simultaneous two-way principal/accrual transfer, destination-rate
interest, explicit new inflow, lag coverage, asymmetric half-lives, floor/ceiling,
competitor/spread-driven offered rates, logistic midpoint/extremes, regime
flags, gap/confidence/velocity retention, true maturity, residual/franchise/
liquidation, horizon extension, exact same-policy replay, signed interest rights,
and invalid source/domain/money/work/overlap admission. The persisted
`examples/models/deposit-state-1.json` also checks two actual days of interest:
`1000 * 0.365 * 2 / 365 = 2`, bank cash interest `-2`, and retained principal `1000`.

These tests do not establish fitted account-level retention, stress-run accuracy,
cohort representativeness or regulatory stability. Required follow-through is
observed dated retention/migration/inflow/closure histories with customer/source
linkage, jointly identified paid-rate behavior, chronological training and
held-out regime diagnostics, parameter uncertainty and plateau identification,
account-versus-cohort PV/DV01/NII/runoff error tables, full graph/daily/FTP
single-ownership integration, and independently replayed immutable journals.
Those are retained #14 acceptance gates; no paid-rate synthetic fitter test can
substitute for them.
