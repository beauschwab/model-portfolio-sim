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
