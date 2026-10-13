# Supplied indexed-term and CD product graph

`term-product-graph-1` adds Rust graph analysis for explicit term contractual
paths and supplied CD elections. It is the graph/API stage of #15/#16, with
joint credit/market optimal exercise, empirical behavior, complete library,
saved-book/verified ledger and treasury acceptance still separate gates.
The public API is `term_product_graph::analyze(&Request)` and immutable
`TermGraph::{new,request,snapshot,base_anchor,reprice,repriced}`.

Each position supplies typed known terms: `indexed_term` has a contractual
`indexed::ContractPathRequest`; `cd_election` has `CdKnown` with explicit source
model version, currency, direction, policy, opening state and complete tail.
The graph consumes per-scenario supplied P/Q/Q-down/Q-up requests with explicit
path IDs/probabilities and native run IDs. P can have independent paths/weights;
ordered Q identities/weights match across all scenarios and bumps for each
position. Weights sum to one within `1e-12` without normalization, and zero-weight
paths still execute. The supplied paired-history domain generates no random
draws and does not fit a Physical-to-Pricing probability conversion.

## Original contract and rate-source preservation

Indexed histories preserve known contract/source version, original principal
plan/payment, dates, direction, fees, fixed coupon rates, index descriptor,
daycount, margin/bounds, reset/selection convention and observation geometry.
Only explicitly conditional index rate/source/path slots may vary. Observed
fixings are immutable by ID/revision/index/fixing date/rate/publication across
the graph. Projected information must be available at its selected reset;
future publication is never synthesized as an observed fixing. Complete native
coupon audits preserve selected sources and supplied calendar mappings remain
reported as unverified. No front-quarter proxy or curve-implied fallback exists.

Original supplied amortization or retained payment remains original. A scenario
does not recompute payment using remaining tenor. Saved legacy `annuity` remains
the existing equal-principal convention under its old identity; this new graph
does not reinterpret saved contracts. See [term-enhancements.md](term-enhancements.md)
and [contractual-amortization.md](contractual-amortization.md) for the existing
explicit contract equations and supported model boundaries.

The new CD helper `cd::step_cd_pricing` requires explicitly declared Pricing
requests and uses the same private arithmetic with native cash events tagged Q.
Original `cd::step_cd` remains Physical-only with identical financial output,
admission, source and disclosure. This additive helper does not take a Physical
request and relabel its behavior as Q: each graph pricing history must independently
declare its supplied election, reviewed policy/source and pricing probability.

## Supported CD path and owned full tails

The graph supports **one supplied CD election per path**, followed by the
native component's complete fixed remaining maturity/renewal/switch tails.
It preserves known contractual policy and asof opening state; it does not infer
later withdrawals, automatic renewal, issuer optimization or borrower liquidity.
Known `last_accrual_ordinal` must equal graph asof. Source reporting handoff is
`max(graph final report,election date)` because the existing election component
reports at/after its modeled election. Graph cash reports still use their own
actual `(left,right]` boundaries, including reports ending before the election.

Opening and generated CD accrued rights cannot be negative/prepaid; a terminal
zero-principal state cannot retain unowned accrued rights. Remaining maturity
interest cannot represent negative prepaid rights in this graph tier. The
standalone Physical CD API's signed-interest scope is unchanged. Indexed term
cash supports signed coupons and either receipt/payment direction.

Every new renewal/switch position has an explicit contract/source/date/rate and
belongs to its originating graph position. New IDs cannot collide with existing
positions, another child in the same election or children assigned to another
parent. Unused/contradictory contracts and out-of-tail maturities fail under the
native election validator. Additional future actions are not accepted/pruned.

Partial withdrawal retains the remaining original principal/accrual in its full
native maturity tail. Penalty separates accrued-interest forfeiture from
principal-funded cost and gross face extinction from principal returned. Native
cash records **gross constructive allocated interest and principal plus the
full negative penalty fee** (sign reversed for a liability). Graph principal/
interest/fee fields preserve those exact components. Separate signed
`cd_interest_cash_paid`, `cd_principal_cash_returned` and `cd_net_external_cash`
diagnostics retain the actual after-penalty settlement and are never added again
to cash or PV. `cd_interest_accrued_through_election` is a native diagnostic on
the election date, not a daily earned-income allocation or effective-yield NII.

Renewal/switch preserves one gross old settlement and separate reinvestment,
followed by the new fixed tail. The graph iterates settlement and remaining
schedules once each; it does not also add `net_external_cash` or retain the old
unmodified maturity after its extinguishment. Complete principal/cash rights and
all fixed tails survive the reporting horizon. No terminal cash is invented.

## Pricing and immutable base

PV uses future native declared-Q principal/interest/fees, the scenario's supplied
collateral DF and original per-position `exp(-base_oas*actual_days/365)` spread.
`after_opening_actions` excludes same-asof settlements from future PV and
`(left,right]` reporting but retains them in `opening_actions`. Receipt direction
gives asset-positive cash; Payment gives liability-negative cash. Curve sources,
log-DF interpolation and reject/flat-zero/flat-forward tails remain explicit.

Locked OAS lies in [-5,5]. `calibrate_positive_cash` targets positive magnitude
in the position's explicit direction, preserving signed liability PV/DV01. Any
opposing future event, including constructive renewal funding, requires locked
OAS; the positive solver does not silently net events or change target semantics.
Original base calibration retains complete known terms/base Q histories, target
source, base curve/scenario, model/source/paired identity and timing convention.
Changed dependencies report `BASE_ANCHOR_CHANGED`. Nonbase scenarios, P/down/up
histories, input revision and compatible report boundaries may change while
retaining that exact original anchor. Scenario pricing never re-solves OAS.
Signed `supplied_history_dv01` combines native +/- curve bumps and supplied paired
Q down/up cash with the original OAS.

The separate [conditional exercise DAG](term-enhancements.md) remains its own
native reviewed route. This graph does not claim market-curve consistent joint
call/put/credit optimal exercise adoption or treat a threshold as optimization.
Those generation/integration limits remain open explicitly.

## Bounds, fixtures and example

Before projection, admission bounds 256 positions, 13 scenarios, 120 report
periods, 128 paths per role, 4,096 runs and 4,096 periods/observations per supported
indexed field; CD paths have one election and bounded waiver windows. Worst-case
native audit/source/cash rows are limited to 250,000 and report/calibration/source
work to 8 million units. Streamed input byte counting precedes canonical
dependency comparisons at 32 MiB; full raw/native result/base identities use a
128 MiB retained capacity estimate. These are component limits, not aggregate
RSS or a combined 60,000-position lifecycle claim. Native financial domains,
full principal/tails and chronological election feasibility fail closed.

The [synthetic raw example](../../examples/models/term-product-graph-1.json) has
a 100-principal loan at 0.36 ACT/360 for 29 days: future receipt is 102.9.
The CD liability starts at 100 with 0.365 ACT/365F and withdraws 40 on day 5;
earned accrued interest is 0.5, withdrawal allocation 0.2, penalty
`40*0.05*90/365 = 0.4931506849315068`, and principal-funded penalty
`0.2931506849315068`. Remaining principal 60 and interest 0.6 settle on day 10.
With collateral rate 0.05 and locked OAS -0.05 cancelling, CD signed PV is
`-100.30684931506849`. Its gross cash components are principal -100, interest
-0.8 and fee +0.4931506849315068; the after-penalty withdrawal cash is
-39.70684931506849. This is a synthetic hand fixture, not observed behavior.

Tests independently cover actual cash/signs, original retained payment,
Physical-only compatibility and declared-Q event tags, partial penalties,
constructive renewal once-only cash, closed-form OAS and frozen repricing,
explicit indexed measures, paired identity, negative/unowned rights and budget
failure. Reference tolerances stay `1e-12` relative and `1e-9` absolute.

The [New York Fed reference-rate methodology](https://www.newyorkfed.org/markets/reference-rates/additional-information-about-reference-rates)
motivates exact observation/publication and revision identities. The underlying
contract/exercise/CD primary sources and limitations are in
[term-enhancements.md](term-enhancements.md). Inputs still require reviewed
applicable contract/election policies; this graph makes no universal issuer,
accounting-standard or empirical market-probability claim.
