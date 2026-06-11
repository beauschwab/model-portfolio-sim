# AGENTS.md — rates-workbench (monorepo root)

Three layers, three contracts:
- `packages/mbs-risk` — the quant engine. Its OWN AGENTS.md hierarchy
  (root/src/tests) is authoritative for model assumptions, invariants,
  and extension recipes. Never modify kernels without reading it.
- `apps/api` — FastAPI over the engine. Holds NO quant logic: adapters in
  `app/store.py` call engine drivers and JSON-ify Polars frames. Long
  runs are jobs on a single worker thread (kernels saturate cores).
- `apps/web` — Vite/React. Talks only to the API via `src/lib/api.ts`.
  UI primitives are hand-rolled shadcn-style in `components/ui.tsx`;
  theme tokens in `tailwind.config.js` (Supabase dark: zinc + emerald).

Commands: `make install | dev-api | dev-web | test-py | build`.

Rules that cross layers:
1. Engine invariants (fixed-OAS, CRN, numba constant freezing) surface in
   the API/UI as behavior: scenario runs never re-solve OAS; prepay
   assumption edits report RESTART_REQUIRED rather than silently no-op.
2. Adding a product = engine recipe first (packages/mbs-risk/src AGENTS),
   then a `run_*` adapter in store.py, then a book tab + KRD key in web,
   and a unit template in unitlib.TEMPLATES if it should be available to
   the interactive Strategy Lab.
2b. The interactive loop: POST /run kind="unitlib" builds the unit
   tensor + caches base KPIs; POST /strategy/eval is SYNCHRONOUS and
   must stay sub-ms -- never put engine calls in that path; anything
   slow belongs in the unit-library build.
3. The API store is in-memory; anything stateful you add must keep the
   repository shape (functions over module dicts) so persistence can be
   swapped in without route changes.
