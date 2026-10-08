# Production execution policy

Follow the root AGENTS.md and docs/rust-production-contract.md. Rust is the
required production financial runtime; Python calculation is deprecated and
retained only as an independent engine test reference. Never add Python, NumPy
or Numba execution options or silent fallbacks to production routes or UI.
Existing Python API/storage adapters are not a Rust service; preserve their
durable contracts until explicitly migrated. HiGHS stays C++.

# UI design system

Styling follows the Aperture Risk design system; read the repository root DESIGN.md
before changing UI. Use the token classes and CSS variables from
`src/styles/aperture.css` (never hard-coded hex), the primitives in
`src/components/ui.tsx`, the chart theme in `src/components/charts.tsx`, and Lucide icons.
Green/red signal direction only; KPI deltas need an explicit reference.

## Downstream results (live recalculation)

Panels read downstream results (KPIs, risk, NII, stress) from `lib/engine.tsx`, never
from their own run state. Each result carries the input revision it was computed at. An
input write (settings, assumptions, books, market, scenarios) bumps the revision, marks
results stale and, with auto-recalculation on, queues the stale ones after a quiet period.
A failed result is not retried until the inputs change again. New panels that show a
downstream figure should call `useEngineData().results`, `isStale` and `pending`, and
should not add their own run button for a result the engine already keeps fresh.
Balance edits write back to the book, and the grid reloads only when the book changes
underneath it; it never shows a spinner for a background refresh.
