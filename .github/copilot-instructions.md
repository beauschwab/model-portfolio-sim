# GitHub Copilot Instructions

This repository is Rates Workbench, a monorepo with a Python quant engine,
FastAPI API, and Vite/React trader dashboard.

Follow the repository `AGENTS.md` hierarchy first. The root file defines the
cross-layer contract; `packages/portfolio-risk/AGENTS.md` and its nested files are
authoritative before changing engine code, tests, or skills.

Use these local commands:

- `bun run setup` installs Python environments with `uv` and frontend packages with `bun`.
- `bun run dev:api` runs FastAPI on port 8000.
- `bun run dev:web` runs Vite on port 5173 and proxies `/api` to port 8000.
- `bun run test:py` runs the engine test suite.
- `cd apps/web && bun run build` runs the production web build.

The Makefile mirrors these commands for systems where `make` is available.

Keep quant logic inside `packages/portfolio-risk`. The API in `apps/api` should adapt
engine outputs and keep state in the existing in-memory store shape. The web app
in `apps/web` talks to the API through `src/lib/api.ts` and should preserve the
existing Supabase-dark, zinc/emerald design language.

Preserve engine invariants when they surface through API or UI behavior:
scenario runs keep base OAS fixed, common random numbers matter for risk, and
prepay assumption changes should report restart-required semantics rather than
silently mutating frozen numba constants.