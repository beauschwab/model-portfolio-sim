# Native deposit graph capture

`deposit_graph_capture` converts an admitted `DepositGraph` physical source into
typed actual-date liability cash events and recognition origins without projecting
the model a second time. `prepare` accepts `deposit-graph-capture-1`; `capture`
accepts an existing immutable graph plus explicit owners and policy.

The reviewed accounting boundary is `deposit_daily_par-1` with
`par_deposit_simple_interest` and `fees: reject`. It measures gross earned deposit
expense. It does not infer effective yield, premium amortization, franchise value
or tax accounting. Each position/run must supply exactly one physical path with
weight one. Pricing, pricing-down and pricing-up histories remain explicitly
separate graph inputs and retain the graph's original OAS anchor; none posts a GL.
Supplied paired histories are not a claim of stochastic common random numbers.

Performing runoff may include competition, confidence stress, observation lag,
asymmetric repricing, floors/ceilings, withdrawal and closure. Migration policies,
external inflows, opening accrued rights and negative physical paid rates/floors
reject the entire capture. Multiple independent segments share one explicit
liability owner while retaining their original segment IDs. A future migration
extension needs explicit transfers and owner accounting, rather than cash postings
for an internal movement.

## Signs, dates and recognition

Opening principal is the positive deposit liability before day-zero actions.
Owner direction is `payment`, with `fully_scenario_generated` ownership. Bank
principal and cash-interest payments are negative. Earned customer interest is a
negative signed recognition amount that increases bank expense and interest
payable; it is not recognized again when paid. Core daily ordering applies
beginning-of-day maturity/runoff before earning on the remaining balance over
`[day, day+1]`. Thus day-zero runoff is real settlement against the opening face.

For competing withdrawal intensity `w` and closure intensity `c`, both supplied
per year, exit fraction is `-expm1(-(w+c)/365)`. Each exit receives its intensity
share of opening balance and previously earned interest. Daily interest is
remaining balance times paid rate divided by the explicitly selected 360 or 365.
Calendar ordinals include leap days; ACT/365F retains its fixed denominator.
The state model's lagged repricing and supplied stress policies are reused once.
See [deposit-state.md](deposit-state.md) for their formulas and domains.

Capture allocates earned origins FIFO independently within each segment to their
actual interest settlement. It never assumes a uniform coupon. The retained
receipt records original native cash event, original segment, dated event and
daily recognition origin IDs. Numeric ledger segments omit only origin labels;
the immutable receipt preserves them. Distinct coupon and maturity events on the
same date retain their order. Contractual maturity and explicit assumed terminal
liquidation remain different provenance; terminal liquidation retains its supplied
rationale. Residual principal/interest must reach zero at the full supplied tail.

## Admission and evidence

Identity/amount/calendar domains reuse the graph and core typed contracts. Input
serialization is capped at 32 MiB, retained capture at a conservative four-times
serialized-size estimate within 128 MiB, and recognition segments at 250,000.
Graph projection work has its own admission and cooperative checkpoints. These
component budgets are not an aggregate process RSS limit. Missing/duplicate owner,
run, source or physical history coverage rejects; no severity-scaled fallback is
introduced. Supplied physical histories exclusively control dated-owned stress.
Global legacy deposit-flight settings may govern other unowned positions and are
not reapplied to dated-owned deposits.

Native fixtures cover bank signs, zero-day competing exits, leap ACT/360,
same-date events, full tails, deterministic capture, zero-balance declarations,
explicit liquidation and unsupported admission. Independent Python tests replay
the supplied physical equations and reconstruct every event/recognition origin;
they do not invoke native pricing to produce their expected source. This is
synthetic mechanics validation, not fitted retention accuracy or full scale
acceptance.

The [Federal Reserve interagency IRR advisory](https://www.federalreserve.gov/frrs/guidance/interest-rate-risk-interagency-advisory.htm)
and [IRR FAQs](https://www.federalreserve.gov/frrs/guidance/interest-rate-risk-frequently-asked-questions-regarding-interagency-advisory.htm)
support explicit, tested deposit decay/repricing assumptions and sensitivity
analysis. They do not supply these synthetic coefficients. The
[FDIC reporting guidance](https://fdic.gov/news/financial-institution-letters/2023/estimated-uninsured-deposits-reporting-expectations)
includes accrued unpaid interest in deposit liabilities, consistent with keeping
earned rights separate from cash here. This implementation does not assert
regulatory reporting compliance.
