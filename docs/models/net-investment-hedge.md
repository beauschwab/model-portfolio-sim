# Net-investment hedges and the dual hedge (`net-investment-hedge-1`)

Issue [#89](https://github.com/beauschwab/model-portfolio-sim/issues/89), for #35 stage 6. A
reporting entity translates one foreign operation and hedges its net investment under ASC 815-35.
The ASU 2025-09 dual hedge (815-20-55-38) is supported behind a reviewed entity adoption.

## Request

| Field | Meaning |
|---|---|
| `reporting_currency`, `foreign_currency` | Two distinct ISO codes. Spot rates are reporting units per foreign unit. |
| `dates` | 2-4096 increasing reporting dates. Each has a closing spot, its source and, while a forward is live, both currencies' discount curves from that date. Every contractual or event date must be a listed date; nothing is interpolated. |
| `foreign_operation` | Opening net assets (foreign currency), reviewed opening translation CTA, and income, contribution and distribution flows with supplied translation rates. |
| `instruments` | `fx_forward` (sell foreign currency, notional, forward rate, trade and maturity) or `foreign_debt` (notional, maturity), outstanding from the first date. |
| `relationships` | One per instrument: designation date, hedged net investment (foreign currency), method `spot` or `forward`, and for forwards under the spot method the excluded-component recognition `amortization` or `mark_to_market`. Debt may carry a `dual_fair_value_hedge`. |
| `policy` | Assessment bounds around one, and the reviewed ASU 2025-09 dual-hedge adoption with its date. |
| `events` | `dedesignate` one relationship, or `disposal` of a fraction of the remaining investment with its proceeds. |

## Recognition

**Translation.** Flows post at their supplied rates. Net assets are translated at closing spot,
and the difference is translation CTA:

```
adjustment = net assets * closing spot - prior carrying - flows at their rates
```

**Designated portion.** Designated amounts are scaled down when together they exceed the net
assets at the start of the period. Each relationship's designated fraction is
`min(1, hedged / instrument amount)`. Only that fraction of the hedge goes to the hedge CTA; the
rest goes to earnings as `pnl:net_investment_hedge_excess` (815-35-35-13 and 35-14).

**Forwards.** The forward's fair value comes from covered interest parity on the supplied curves,
and at maturity it settles in cash at `notional * (forward rate - spot)`. An off-market value at
trade is a cash premium. Under the spot method the included component is the undiscounted spot
change, `notional * (previous spot - spot)`; the excluded forward points are either marked to
earnings or amortized straight-line by days. The amortized total is fixed at designation:
`notional * (forward rate - designation spot) - value at designation`. Under the forward method
the whole change is included.

**Debt.** The liability is remeasured at spot (830-20). The designated fraction of the
transaction gain goes to the hedge CTA.

**Assessment.** A cumulative dollar offset compares the designated change with a hypothetical
instrument sized to the effective hedged amount. Outside the bounds the period's change goes to
earnings and the relationship is discontinued. Discontinued, undesignated or matured instruments
post to earnings (`pnl:fx_forward_mark`, `pnl:fx_transaction`).

**Dual hedge (ASU 2025-09).** The debt is also the hedged item in a fair value hedge of interest
rate risk, embedded as a complete `hedge-accounting-1` request in the foreign currency. The
fixed-rate debt item must be the whole liability. Its basis adjustment comes from that run's daily
roll-forward. While the fair value hedge is active, the basis is excluded from the net-investment
hedge and its spot remeasurement goes to earnings (`pnl:fx_basis_remeasurement`). After it is
discontinued, the basis joins the hedging instrument prospectively. The rule is not applied by
analogy to other cases.

**Disposal.** Selling a fraction of the investment releases the same fraction of translation CTA
and hedge CTA to `pnl:cta_reclassification` (830-30-40), and derecognizes that fraction of the
carrying amount against the proceeds. CTA is never released otherwise.

## Output and verification

The result holds the ordered balanced journal, translation rows, per-period hedge rows (status,
effective hedged amount, designated fraction, included and excluded change, CTA, earnings, basis
remeasurement and cumulative ratio), instrument values, relationship closings and the closing trial
balance. The independent reference `net_investment_reference` recomputes every table and posting,
taking the dual-hedge basis from the independent hedge-accounting reference. Durable jobs run
through `/models/runs` and publish only after that replay matches.

## Boundaries

- One foreign operation and one foreign currency per request. No cross-currency swaps or options.
- Spot components are measured undiscounted. Discounted spot measurement is not offered.
- Debt interest, the foreign-currency fair value hedge journal itself, deferred tax and
  consolidation eliminations are outside this owner. `debt_interest_included` is false.
- Not an accounting-standard approval or observed evidence. Capital and FTP are not fed.

## Evidence

- Native: `tests/net_investment_hedge.rs` checks hand values for translation (−20.2, 40.8, −30.6,
  15.52), debt hedge CTA (12, −24, 18, −9.312) with the 0.776 over-hedge scale after the 20%
  disposal, amortized forward points, settlement, CTA release, the dual hedge before and after the
  fair value hedge ends, and refusals.
- Engine: `tests/test_net_investment_hedge.py` compares native output with the reference for both
  examples and seven policy variants, and catches tampering.
- API: `tests/test_net_investment_hedge_jobs.py` covers durable publication and tamper fences.
