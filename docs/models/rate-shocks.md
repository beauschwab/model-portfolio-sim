# Standard EVE/NII rate shocks (`rate-shock-1`)

The standard interest-rate-risk report: economic value of equity (EVE) and net interest
income (NII) under a defined shock set, compared with board or ALCO limits. It is part of
the supported v1 scope and reuses the existing native revaluation and income runs.

It is a risk-measurement report on the saved balance sheet. It is not a regulatory filing,
and the default shapes are conventional supervisory shapes supplied as editable policy
inputs, not a claim of compliance with any rule.

## Scenario set

Every number below is a policy input (`GET`/`PUT /rate-shocks/policy`). Defaults:

| Scenario | Timing | Definition |
|---|---|---|
| `parallel_up_N`, `parallel_down_N` for N in 100, 200, 300, 400 | instantaneous | every par swap pillar moves by ±N bp |
| `short_up`, `short_down` | instantaneous | ±`short_bp` × exp(−t / `decay_years`); defaults 300 bp and 4 years |
| `steepener` | instantaneous | −0.65 × short(t) + 0.9 × long(t) |
| `flattener` | instantaneous | +0.8 × short(t) − 0.6 × long(t) |
| `ramp_up_N`, `ramp_down_N` for N in 200 | ramp | parallel shift reaching ±N bp in month `ramp_months` (12) and held |

`short(t) = short_bp × exp(−t / decay_years)` and `long(t) = long_bp × (1 − exp(−t / decay_years))`
with `long_bp` 150 by default. `t` is the pillar tenor in years (1, 2, 3, 4, 5, 7, 10, 15, 20, 30).

Conventions:

- Shocks move the par swap pillars. Volatility and every other market input are unchanged.
- **Floor.** `floor` is the lowest post-shock pillar rate as a decimal, or null for none
  (the default). A down shock stops at the floor, or at the base rate when that is already
  below the floor; a floor never raises a rate. The report shows the applied shift and
  whether the floor bound.
- **Model domain.** A shocked market the model cannot value is never clipped or replaced.
  That scenario is reported with no values and the engine's reason, its limits are
  `unavailable`, and the other scenarios still report. The known case is a floor of
  exactly 0% reached by a large down shock: a flat zero curve has a zero forward swap
  rate, which the lognormal volatility calibration refuses. A failure of the base run, a
  cancellation or a deadline stops the whole report.
- **Ramps** are income only. In month `m` (counting from 1) the mean simulated short and
  swap rates move by `shift × min(m / ramp_months, 1)`. Each path's deviation is rescaled
  around the moved mean in shifted-rate terms, the same convention as conditional
  forecasts, so no path crosses the model rate floor. Mortgage rates move by the same
  amount at the model's incentive lag and zero basis. A zero ramp is exactly the base.
  A ramp is floored once, at the shortest pillar. A ramp has no EVE, by construction: the
  shifted paths are not a pricing measure.
- Scenario names are fixed by the magnitudes. Fractional magnitudes use `p` for the
  decimal point (`parallel_up_12p5`).

## Metrics

- **EVE**: market value of assets less liabilities, plus the mark-to-market of the swap
  and swaption hedge book, revalued at each scenario curve. Base OAS is solved once on
  the base market and held for every scenario, and all scenarios share the base random
  draws. Non-maturity deposits are valued with their modelled behavioural runoff at that
  fixed OAS; equity is excluded. The KPI board's EVE level leaves the hedge book out, so
  the base EVE here differs from it by the hedge mark-to-market (`base_hedge_value` in
  the result); the board's first-order DV01 sensitivity already includes hedges. This
  report is a full revaluation, so it also captures convexity.
- **NII 12m, NII 24m**: the sum of the first 12 and 24 projected months on the unchanged
  book (no new business, no balance-sheet growth). Accounting yields and deposit opening
  rates are anchored to the base run, so a shock changes floating coupons, deposit
  repricing, prepayment and runoff but never rewrites the yield of the existing book. The
  24-month metric needs `horizon_months` of at least 24; a shorter horizon reports it as
  unavailable.
- **Delta %** is the change as a percent of the base metric. It is undefined when the base
  is not positive.

## Limits and status

A limit is the largest permitted decline of one metric under one scenario, as a percent of
the base metric: `{metric, scenario, applicability, max_decline_pct}`.

| Status | Meaning |
|---|---|
| `within_limit` | an internal limit exists, the metric is defined, and the decline does not exceed it |
| `breach` | the decline exceeds the internal limit |
| `unavailable` | an internal limit exists but the metric is undefined (ramp EVE, short horizon, non-positive base, or a scenario the model could not value) |
| `not_applicable` | the policy explicitly marks the pair as not applicable |
| `unconfigured` | no limit entry for the pair |

No configured limit is never a pass. The report status is the worst row: breach, then
unavailable, then within limit; with no internal limit at all it is `unconfigured`.
Limits apply to declines only; an increase is always within an internal limit.

A limit that names a scenario outside the policy's set is rejected when the policy is
saved, so changing the magnitudes cannot silently orphan a limit.

## Running it

`POST /rate-shocks/run` with the current `expected_revision` queues one durable job. The
result (`GET /jobs/{id}/result`) contains:

- `rows`: one row per scenario and metric with base, value, delta, delta %, limit,
  headroom and status;
- `scenarios`: the applied shift by pillar, whether the floor bound and whether the
  scenario was evaluated;
- `nii_monthly`: monthly NII for the base and every scenario;
- `conventions`, `policy`, `status`, `counts`, `base_eve`, `base_hedge_value`.

The Rate shocks page shows the table and exports it as CSV. The policy is stored with the
workspace assumptions, so it is revisioned and part of each run's inputs.

Cost is one base run plus one EVE revaluation and one NII projection per instantaneous
scenario and one NII projection per ramp: 15 scenarios by default.

## Ownership and checks

Rust owns the scenario construction, deltas and limit status
(`portfolio-risk-native/src/risk/rate_shocks.rs`) and the ramp path shift
(`income/forecast/paths.rs`). `portfolio_risk.analytics.rate_shocks` only sequences native
calls. `rate_shocks_reference.py` and `forecast.ramp_paths` are an independent Python
reference used by tests: the engine test compares EVE, NII and ramp NII against the
Python reference engine, and the golden banks record the full set.

Limits of this version: no dynamic balance sheet or new-business assumptions; no
basis, volatility or spread shocks; no currency dimension; EVE is not reported as a
percent of Tier 1 capital. Synthetic fixtures do not establish empirical accuracy.
