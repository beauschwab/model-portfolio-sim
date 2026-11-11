# Fitted consumer states and daily product paths

This source checkpoint supplies the daily product bridge required by #91 after
the separately committed panel/specification component. It is analytical only;
it does not enable durable journal, saved-book, capital or FTP adoption.

The shared credit owner generates states with zero monetary openings. Explicit
reviewed live-state reduction and ceiling-to-day mapping feed the existing daily
product owner. Original fractional times, complete raw/compiled contracts and
gross recovery tails are retained. Monetary credit paths cannot be supplied as a
second owner. Conflicting actions and same-day transition collisions reject.

Card closure is an additive owner requiring every gross right and pending minimum
to be exactly zero. It cancels undrawn commitment, preserves zero daily closing
points and rejects later supplied actions. Ordinary card JSON retains its prior
open-line behavior. No cash, waiver or terminal plug is invented.

Local evidence in `artifacts/consumer-behaviour-2026-11-10/`:

- Six daily-bridge tests pass, including all three products through panel fit,
  CTMC projection and product default/recovery; hand actual-day coupon accrual;
  exact path identity; source/collision/admission refusals; cancellation; card
  closure; and refusal to truncate a recovery tail.
- Four closure tests and all 19 existing card-mechanics tests pass. The final
  joined-budget change reran all six bridge and four closure tests successfully.
- Core strict clippy passed before the final joined-budget metadata change.
  Native all-target compilation checking passed before that change. Architecture
  and generated MCP contracts pass.
- The public protocol integration tests are written but have not run locally:
  compilation stopped with Windows error 112 while writing the native archive.
  No installed binaries were replaced. Full installed regression and hosted
  acceptance remain pending; the failed build is not a test pass.

The synthetic source is not an observed panel. Existing behavioral driver,
LGD/CCF, cash-collection and policy assumptions remain explicit. See
[the complete contract](../models/consumer-behaviour-product-paths.md).
