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
from their own run state. Recalculation is a dependency graph (`lib/graph.ts`): `/state`
serves a content fingerprint per input node (each book, market, settings, product
assumptions, scenarios, cohorts, context), and each result node (KPIs, NII, risk and
stress per book) is current while the fingerprints of its inputs match those it was
computed at. With auto-recalculation on, only stale nodes are queued after a quiet period,
so a deposits edit reruns deposit risk, KPIs and NII but not MBS risk. Edges must stay
conservative and mirror what each `run_*` adapter in `apps/api/app/store.py` reads; when
an adapter starts reading a new input, add the edge. A failed result is not retried until
one of its inputs changes again. New panels that show a
downstream figure should call `useEngineData().results`, `isStale` and `pending`, and
should not add their own run button for a result the engine already keeps fresh.
Balance edits write back to the book, and the grid reloads only when the book changes
underneath it; it never shows a spinner for a background refresh.
