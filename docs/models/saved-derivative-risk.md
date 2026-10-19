# Saved derivative graph risk (`saved-derivative-risk-1`)

Issue [#90](https://github.com/beauschwab/model-portfolio-sim/issues/90), for #20 and #35 stage 7.
Saved raw swap trades become cached nodes in the native pricing graph, with clean value and
paired curve risk per trade, netting set and hedge relationship.

## Request

| Field | Meaning |
|---|---|
| `source` | Identity of the saved derivative source. A graph owner is bound to it. |
| `positions` | 1-256 trades: `position_id`, `netting_set_id` and a complete Pricing `SwapRequest`. One valuation date and one currency. |
| `relationships` | Up to 64 hedge relationships: instrument position IDs and an optional fixed-rate hedged item with its benchmark rate and hedged notional. Each instrument belongs to at most one relationship. |
| `bumps` | A reviewed identity, a bump size in (0, 100] basis points and up to 32 increasing key-rate dates after valuation. |
| `projection_policy` | Required: `named_projection_curves` or `additive_projected_observations`. There is no default. |
| `projection_curves` | One curve per index under the named policy: index ID, source identity, day count and exact points. Refused under the additive policy. |

## Bumps

Each risk case shifts every curve by `s(t) = sign * bump * w(t)`:

- Parallel: `w(t) = 1`.
- Key rate `k`: a triangular weight that is one at knot `k`, falls linearly to zero at the
  neighbouring knots, and stays at one before the first and after the last knot. The weights sum
  to one, so key-rate risk adds up to the parallel risk for linear trades.

The collateral discount curve moves as `P'(t) = P(t) * exp(-s(t) * (t - asof) / 365)`.

Projected fixings move under the selected policy:

- **Named projection curves.** Each projected fixing is the simple forward of its index curve over
  the observation's interest period, `(P(start) / P(end) - 1) / tau`, after the same bump. At the
  base case the forward must reproduce every supplied fixing at the standard tolerances, or the
  request fails.
- **Additive.** Each projected fixing moves by `s(fixing date)`.

Observed fixings never move. Nothing is interpolated: every date used must be an exact curve point.

Risk is the central paired difference per basis point, `(V(down) - V(up)) / (2 * bump)`, so a
position that gains when rates fall has positive DV01. Hedged-item risk uses the hedge owner's
benchmark valuation on the first instrument's bumped collateral curve. A relationship reports
instrument, item and net DV01, net key-rate DV01, and `dv01_offset_ratio = -instrument / item`.

## Graph ownership

`graph-1` operations `create_saved_derivative_risk` and `price_saved_derivative_risk` own a bounded
cache keyed by the complete bumped swap or item inputs. Cached values are copied out at once, so
eviction cannot change results; a constantly evicting cache returns identical output. A request
for a different `source` fails with `BASE_SOURCE_CHANGED`. Operations from other graph owners fail
with `GRAPH_OWNER_MISMATCH`. Runtime deadlines and cancellation reach these operations like the
indexed graph operations.

Durable jobs use `POST /models/runs` with schema `saved-derivative-risk-1`. Each job builds a
fresh owner in the native model process. The independent Python reference
(`saved_derivative_risk_reference`) revalues every bumped swap with the derivative verifier and
every hedged item with the hedge reference, and the job publishes only if they agree.

## Boundaries

- Pricing measure only. No curve calibration, convexity adjustment, XVA, accounting designation,
  capital or FTP output. Designation never changes a trade's economic risk.
- Swaps only. Options have no graph node yet.
- Hedged items are fixed-rate instruments. Closed-portfolio layers are refused here.
- Work is bounded at 20 million units and output at 32 MiB. These are component limits, not an
  aggregate memory guarantee or the 60,000-position acceptance (#51).

## Evidence

- Native: `tests/saved_derivative_risk.rs` covers totals, key-rate additivity, hand values for the
  example's fair-value relationship, warm, cold, cleared and evicting caches, both projection
  policies and refusals.
- Engine: `tests/test_saved_derivative_risk.py` checks native output against the reference for
  both policies and five kinds of edits, and catches tampering.
- API: `tests/test_saved_derivative_risk_jobs.py` covers durable publication and tamper fences.
