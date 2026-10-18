# Swap strategy units

Issue #20 asks for strategy coefficients from the derivative owner, and issue #35
stage 7 asks the interactive strategy loop to see hedging instruments. The monthly
unit library now admits two explicitly selected templates:

| template | side | meaning |
|---|---|---|
| `receive_fixed_swap_5y` | +1 | receive the forward par rate, pay the short rate |
| `pay_fixed_swap_5y` | -1 | pay the forward par rate, receive the short rate |

Neither is in the default template set. A library built without naming them is
unchanged, byte for byte.

## Coefficients

Each unit starts at a grid month `h` with unit notional through its tenor.

- The fixed rate is the forward par rate at `h` for the tenor plus `spread_bp`.
- Monthly NII is `(fixed - mean base short rate) / 12` on the shared base paths.
  It is also the unit's cash interest. There is no principal runoff.
- DV01 is `(PV(down 25 bp) - PV(up 25 bp)) / 50`, where PV is the path mean of
  discounted net settlements through the tenor. These are the same common random
  number paths as every other unit.
- The supplied `rwa` weight multiplies notional. It is a research counterparty
  weight, not an SA-CCR exposure.

A swap is off balance sheet. Its balance, asset and liability rows are zero, and
liquidity and stable-funding weights must be zero. Notional still scales DV01 and
RWA. The pay-fixed template is the exact negative of the receive-fixed template.

## Solver

Rust prepares the coefficients and HiGHS solves. A unit carries a `kind: "swap"`
marker that must agree with its template kind. Swap units never count toward
`max_total_assets`, purchase-cost caps, funding rows, or the `ALL_ASSET` and
`ALL_LIAB` commercial selectors. A template selector can still bound a swap
template directly. Historical units serialize without the marker, so existing
sessions and replays keep their bytes. The Python optimizer and decision replay
apply the same rule as the independent reference.

## Boundaries

- A strategy allocation is an economic position. It does not designate a hedge.
  Hedge accounting stays in the durable `hedge-accounting-1` and
  `saved-hedge-mixed-1` lanes, which need reviewed documentation.
- Swap units carry no collateral, margin, CVA or DVA cost, and no netting set.
- Only fixed-for-floating swaps on the existing short-rate model are offered.
  Options and cross-currency swaps have no unit template.

Tests: `packages/portfolio-risk/tests/test_swap_strategy_units.py` checks native
generation against the reference, symmetric off-balance-sheet coefficients,
coefficient-only interactive evaluation, solver parity, cap exclusion and the
marker check.
