# Rates Workbench

Monorepo: a production-grade fixed-income/balance-sheet risk engine
(`packages/portfolio-risk`), a FastAPI service (`apps/api`), and a Vite/React
trader dashboard (`apps/web`).

```
apps/
  api/        FastAPI wrapper: books, market, assumptions, scenarios, runs
  web/        Vite + React + TS + Tailwind + recharts (Supabase-dark theme)
packages/
  portfolio-risk/   LMM Monte Carlo engine: MBS OAS, corporates, NMD deposits,
              CDs, money markets, ASC 815 hedges (swaps/swaptions);
              KRD/vega risk, 9Q stress capital, NII accounting with
              amortized-cost basis, EVE/LCR/NSFR/CET1 KPIs, forward
              strategies, and the unit library powering interactive
              strategy analysis. Self-documenting via a layered
              AGENTS.md hierarchy (root / src / tests / skills).
```

Docs: PRODUCT.md (what/who/journeys), ARCHITECTURE.md (layers and
invariants), DESIGN.md (UI system), AGENTS.md hierarchy (modification
contracts at every layer).

## Quick start
```bash
bun run setup     # uv Python envs + bun workspaces
bun run dev:api   # :8000 — seeds a WFC-1Q26-proportional model balance sheet
bun run dev:web   # :5173 — proxies /api to :8000
bun run test:py   # 33 engine tests (the change gates)
```

Toolchains: Python environments are managed with `uv`; frontend packages
and scripts are managed with `bun`. The Makefile exposes the same targets for
systems that have `make` available.

## What the UI does
- **Dashboard** — book balances, stacked KRD-by-pillar across all five
  books, 27-month NII forecast, 9Q stress P&L lines.
- **Balance Sheet** — browse/edit each book (MBS pools, commercial loans,
  LT debt, NMD deposits, CDs); PUT replaces wholesale for auditability.
- **Market & Scenarios** — par-curve editor with live preview; 9Q scenario
  builder in trader space (10y level, 2s10s twist around the 5y pivot,
  spread, vol), each quarter revaluing the full balance sheet (base OAS
  held fixed — the engine's global invariant).
- **KPIs** — EVE & duration gap (IRRBB 15% outlier test), LCR, NSFR,
  CET1 9Q projection; stylized weight tables are the calibration seam.
- **Strategy Lab** — build the unit library once (~20s; new origination
  of every product through the live behavioral engines), then drag
  allocation sliders: every edit recalcs ALL top-level KPIs in real time
  (~sub-ms sync endpoint).
- **Assumptions & Settings** — paths/seed/horizon/shock grid; deposit
  attrition segment editor (the panel-fit seam); prepay vector is
  displayed read-only because numba freezes constants at first compile
  (restart required — engine AGENTS.md invariant 5).

## Honesty notes
The seeded book is **synthetic**, sized to Wells Fargo's published 1Q26
mix (sources cited in `model_balance_sheet`'s docstring); it is not WFC's
positions. The API store is in-memory single-process — repository-shaped
on purpose so Postgres/Redis can replace it without touching routes.
