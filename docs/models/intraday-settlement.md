# Intraday settlement replay (`intraday-settlement-1`)

A deterministic replay of one or more business days at minute resolution: timestamped
payment requests queue and settle in discrete cycles from available cash, collateral
moves between locations take time, and facility draws settle after their own lag. It
answers a narrow question the daily ledger cannot: **was cash available before the
payment had to be made?** A collateral value is not cash until a draw has settled.

It is a research replay with supplied timetables and capacities. It is not a model of
any specific payment system's rules, and it does not predict withdrawal requests.

## Inputs

Times are whole minutes from the start of day 0; day `d` is minutes `1440d` to
`1440d + 1439`. Windows are minutes of the day. Money is in one currency.

| Input | Fields |
|---|---|
| Top level | `schema`, `opening_cash`, `horizon_minute` (at most 14 days), optional `stop_minute` |
| `systems` | `id`, `open`, `close`, `cycle_minutes`, optional `capacity_per_cycle`, `closed_days` |
| `payments` | `id`, `minute`, `amount`, `system`, optional `netting_set` |
| `receipts` | `id`, `minute`, `amount`; a netted receipt also names `system` and `netting_set` |
| `netting_sets` | ids of sets with legally enforceable netting |
| `lots` | `id`, `value`, `location`, `haircuts` by facility |
| `moves` | `id`, `lot`, `to`, `minute`, `transfer_minutes`, `window_open`, `window_close` |
| `facilities` | `id`, `location`, `open`, `close`, `settlement_lag_minutes`, `capacity`, `outages` |
| `draws` | `id`, `facility`, `minute`, `lots`, optional `amount`, optional `only_if_cash_below` |

## Rules

**Order.** Work is processed by minute. Within a minute the order is: collateral
arrivals, draw settlements, plain receipts, payment requests, move instructions, draw
requests, settlement cycles (systems in input order). Within one kind, input order.
Nothing at or after `stop_minute` is processed; without a stop, work through
`horizon_minute` is.

**Payments.** A request joins its system's queue at its minute. A system settles only
at its cycle times: `open`, `open + cycle_minutes`, ... up to `close`, on days not in
`closed_days`. A request that arrives after the day's last cycle is marked
`after_cutoff` and waits for the next open day.

At each cycle the system pays its queue strictly first in, first out (by request
minute, then input order). Each payment receives the smaller of its unpaid amount,
available cash and the cycle's remaining capacity, so partial payment is possible. The
cycle ends at the first payment left partly unpaid: later payments do not jump the
queue. If cash ran out the cycle records a `shortfall` with the system's total unpaid
amount; if only capacity ran out it records `capacity_limited`. Cash never goes
negative.

**Legal netting.** Before the gross queue, each listed netting set with at least one
due netted receipt in that system settles as one: all its queued payments against all
its due receipts. If receipts cover payments the difference is added to cash. Otherwise
the net amount must be available in cash and capacity, or the set waits whole
(`netting_waiting`, with detail `cash` or `capacity`) and is retried at the next cycle:
none of its payments is paid gross in that cycle, and they do not hold up the gross
queue behind them. A set with no due receipt is not netted; its payments go through
the gross queue. A plain receipt (no set) is cash at its minute.

A cycle counts as a shortfall cycle when cash ran out in the gross queue or a netting
set waited for cash; `first_shortfall_minute` is the first such cycle.

**Collateral moves.** An instruction inside the custodian window starts at once;
outside it, it waits for the next window opening. From start to arrival
(`transfer_minutes` later) the lot is in transit and cannot be pledged anywhere. A
pledged or in-transit lot cannot be moved (`move_blocked`).

**Draws.** A draw is declared in advance and decided once, at its own minute, on the
state at that minute only. It is blocked, with a reason and no retry, when:

| Reason | Condition |
|---|---|
| `not_needed` | `only_if_cash_below` is set and cash is not below it |
| `outage` | the minute is inside a facility outage |
| `facility_closed` | the minute of day is outside the facility window |
| `already_pledged` | a named lot is pledged |
| `collateral_in_transit` | a named lot is in transit |
| `collateral_not_at_facility` | a named lot is somewhere other than the facility's location |
| `ineligible` | a named lot has no haircut for the facility |
| `no_capacity` | nothing is left of the facility capacity |

Otherwise every named lot is pledged for the rest of the replay, so no lot is pledged
twice, and the draw amount is the smallest of the lots' value after haircuts, the
optional `amount` and the facility's unused capacity. Cash arrives
`settlement_lag_minutes` later. A draw that would settle at or after the stop never
arrives and is reported in `draws_unsettled`.

## What follows from the rules

- A draw cannot cure an earlier shortfall: the `shortfall` events already recorded
  stay, and its cash pays only from its settlement minute.
- Nothing is anticipative. Adding later payments, receipts, moves or draws cannot
  change any event before their own minute.
- Requested, paid and unpaid are separate: `requested = paid + unpaid` for requests
  that arrived before the stop, and requests dated at or after the stop are never
  counted as requested.

## Output

- `events`: the ordered journal, one row per event with `minute`, `event`, `id`,
  `amount`, `cash` and `queued` (total unpaid requested) after it, and a `detail`.
- `payments`: per request: `requested`, `paid`, `unpaid`, `first_paid_minute`,
  `settled_minute`, `after_cutoff`.
- `summary`: opening and closing cash, requested, paid, unpaid, received, drawn,
  `draws_unsettled`, `draws_blocked`, `first_shortfall_minute`, `shortfall_cycles`,
  peak queue and its minute, and `reconciliation_error`.

The run fails rather than report if opening cash plus receipts and settled draws less
payments does not equal closing cash, or if requested does not equal paid plus unpaid.

## Ownership, bounds and checks

Rust owns the replay (`portfolio-risk-native/src/treasury/intraday.rs`, reached through
`financial-controller-1`). `portfolio_risk.analytics.intraday_settlement` calls it;
`intraday_settlement_reference.py` is an independent Python reference written from
this page. The engine test compares the full event journal from both on hand-built
cases for cash exhaustion, partial settlement, collateral movement delay, missed
cut-offs, outages, double-pledge prevention, legal netting and the stop, and on random
cases.

Bounds: 14 days, 100,000 payments and receipts each, 200,000 settlement cycles, 16
systems, 64 facilities, 10,000 lots, moves and draws. The replay checks cancellation
and the caller's deadline every 1,024 events.

Not in this version: an HTTP route, saved publication or a link to the daily ledger;
multiple currencies; intraday credit or overdraft lines from the system operator;
payment priorities or queue reordering; partial netting; lot substitution or release;
repayment of draws; and any calibration of request timing.
