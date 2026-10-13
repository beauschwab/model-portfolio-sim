# Actual-date scheduled recognition and daily publication

Version 0.29.6 introduces an explicitly selected `dated-ledger-1` request and
`analytics.dated_ledger.run_dated_ledger`. Rust owns admission, active recognition,
settlements, daily GL reconciliation and closing reports. Python transports raw
inputs and independently verifies persisted source events and postings before
atomic publication. Existing `balance-stress-2` requests keep protocol 2 and their
historical economic/posting behavior. The dated route emits protocol 3.

This implements actual-date **scheduled** recognition. Shared credit propagation
and explicit credit-accounting intents exist as separate modules; their automatic
adoption into this daily route remains required. This is not a new credit,
prepayment, deposit-retention, reinvestment or accounting-standard calibration.

## Request and supported ownership

The strict request requires:

```text
schema: dated-ledger-1
monetary_unit: currency_units
recognition_policy:
  policy_id, policy_revision
  recognition_model: supplied_calendar_segments-1
  fees: immediate_earnings
specification: raw balance-stress-2 specification
dated: dated-store-1 source, owner, run and recognition plans
```

The recognition choice and immediate fee treatment are explicit supplied policies.
Fees requiring deferral, servicing allocation or effective-yield treatment need
their own supported policy; selecting this schema does not establish that all
fees should be immediately recognized.

Dated owners are opening positions with compatible account/currency/direction
and accrued anchors. Their generic interest, legacy default/migration, prescribed
draws, deposit flight/runoff, proportional survival and synthetic maturity branches
are bypassed. Other positions in the same book keep the legacy route. Overlapping
legacy cashflows, sale/collateral policies and opening pledged/eligible collateral
are rejected. Forward originations, active commitments/allowances/PD/migration,
floating-beta overrides and AFS/trading marks require explicit native integration
and are rejected by this initial route. These are remaining issue scopes, not
silent approximations or completed capabilities.

Runs must exactly cover baseline, each scenario and every reverse severity.
Regular and reverse executions at severity one share the same declared economic
source case; a reverse scenario cannot borrow an unrelated baseline schedule.
Full principal settlements cannot exceed opening face. New draws require explicit
state/exposure events and are not inferred from signed cash or CCF.

## Daily phases and gross rights

1. Post supplied opening principal, basis and accrued anchors; reconcile the
   source's earned rights at the valuation date before day-zero events.
2. Settle day-zero events once, apply the existing explicit tax policy to day-zero
   earnings, and close/report that day's activity before beginning day one.
3. For each actual elapsed day, execute disjoint legacy position-day events,
   recognize active supplied calendar segments, then settle that date's events
   in stable source order.
4. Execute remaining legacy cashflows, liquidity actions and closing checks under
   the existing policy sequence; a dated owner cannot also receive legacy economics.

The implementation currently executes legacy position-day events before dated
interval recognition on each day; they have disjoint ownership. A joint credit
coordinator must introduce its separately versioned interval/boundary phase order
before enabling credit or sale modifiers on dated owners.

Interest recognition posts the signed change in the appropriate right and
offsetting income/expense. Settlement posts signed principal/cash coupon/fees
without recognizing the whole coupon again. Basis recognition changes carrying
value and income separately. Principal may reach zero while interest rights
continue to accrue or await settlement.

Before cash settlement, positive/negative signed rights are receivables/payables.
After early settlement, positive/negative residuals are prepaid expense/deferred
income. Four distinct GL accounts preserve gross assets and liabilities rather
than netting early and late coupons. Closing statements derive solely from
replayed postings. The independent verifier classifies these GL accounts anew.

## Bounded runtime and immutable evidence

The active recognition cursor follows supplied segment boundaries; it does not
scan every cashflow each day. Owner/legacy overlap and coverage checks use indexed
sets rather than quadratic book scans. Position-day work and active segment-day
work are admitted together. Public transport keeps the existing 2,000-position /
3-million-work tier; explicit engine-only large-book transport allows up to
60,000 positions and 90 million combined work units. Reporting remains 30–1,080
days. Store/component limits are separate from aggregate process memory.

`event_lineage` links each dated recognition/settlement to run/input revision,
position/event/source/provenance, policy, date, transaction and five monetary
components. `dated_rights` retains all selected events, settled and future cash,
earned and unearned recognition, and the original full contract end. The raw
request is stored immutably as `dated-input.json` with a hash. DLL/executable and
Python verifier source identities are fenced before publication.

The independent publication gate verifies persisted balanced GL, once-only source
phases, exact calendar recognition, source/provenance identity, transaction-level
postings, principal/basis roll-forwards, gross rights, full tails and independently
classified closing/consolidated statements. A balanced but economically altered
transaction therefore fails. Cancellation/deadline checks cover native execution,
persisted replay and final publication; failure publishes no successful manifest.

Validation fixtures include leap/off-cycle/day-zero/same-day cash, early coupons
after principal maturity, delayed interest, mixed legacy ownership, immutable
source corruption and cancellation. Empirical credit/behavior validation, native
graph capture, saved-book/candidate adoption, API/UI exposure and complete
graph/credit/solver/journal 60,000-position acceptance remain open.

## Research and boundaries

The [Basel ECL guidance](https://www.bis.org/committees/bcbs/basel-consolidated-guidelines/module/pap/20)
supports separating credit-risk estimates from accounting measurement policy.
The [interagency allowance statement](https://www.federalreserve.gov/frrs/guidance/interagency-policy-statement-on-allowances-for-credit-losses.htm)
addresses accrued-interest treatment. These references inform explicit policy
boundaries; this scheduled-recognition route does not claim CECL/IFRS9 compliance.
Research was checked on 2026-10-01. See [credit accounting](credit-accounting.md)
and [the integration requirement ledger](daily-integration-plan.md).

Rebuild the ledger DLL/executable and dependent decision/workflow artifacts,
then restart API/workers before using the new schema. No services were restarted
and no production deployment is implied by local validation.
