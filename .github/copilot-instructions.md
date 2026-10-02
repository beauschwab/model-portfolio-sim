# GitHub Copilot Instructions

## Production execution contract (0.29.4; supersedes historical defaults)

Rust is the required production financial runtime. Python calculation is deprecated;
it is retained only as an independent test/reference oracle, not an API/UI backend.
New and omitted `compute_backend` settings resolve to `rust`. Historical explicit
Python snapshots remain readable, but must be switched to Rust and their libraries/
sessions rebuilt before production execution. Never silently change saved economics
or fall back when native libraries are missing.

Implement new built-in pricing, graph/coordinator, simulation, capital/FTP and solver
financial logic in Rust. HiGHS stays C++; Rust owns LP construction, invocation and
allocation validation. Production pricing/what-if must enter the native graph with
no custom discount-backend override. Do not reintroduce Python/NumPy/Numba execution
choices in application requests or controls. Keep Python oracles explicit in tests
so parity cannot accidentally compare Rust with itself. Defaults/ownership tests
must run outside the reference test context and forbid Python financial callbacks.

The existing FastAPI/worker, persistence/artifact transport and independent verifier
still use Python; this change does not claim a Rust HTTP/storage service. Keep quant
logic out of adapters. See `docs/rust-production-contract.md` at the repository root
for deployment, compatibility and validation rules. Preserve fixed OAS, CRN, final
output tolerances, immutable publication and the coefficient-only interactive path.

This repository is Rates Workbench, a monorepo with a Rust financial runtime (C++ HiGHS), a Python
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

Implement production quant logic in the native Rust crates. `packages/portfolio-risk`
contains adapters and independent references. The API in `apps/api` adapts engine
outputs and uses its existing repository facade backed by SQLite/PostgreSQL revisions
and immutable Parquet artifacts; do not add a second mutable source of truth. The web app
in `apps/web` talks to the API through `src/lib/api.ts` and should preserve the
existing Supabase-dark, zinc/emerald design language.

Preserve engine invariants when they surface through API or UI behavior:
scenario runs keep base OAS fixed, common random numbers matter for risk, and
prepay assumption changes should report restart-required semantics rather than
silently mutating frozen numba constants.