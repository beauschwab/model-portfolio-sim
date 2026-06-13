# ARCHITECTURE.md — Rates Workbench

```
apps/web (Vite/React/TS) ──HTTP──> apps/api (FastAPI) ──import──> packages/mbs-risk
   pages: Dashboard, KPIs,            routes -> store.py             (the engine)
   Balance Sheet, Market &            adapters; jobs on a
   Scenarios, Strategy Lab,           single worker thread;
   Optimizer, Settings                in-memory repository-
                                      shaped state
```

## Engine layering (packages/mbs-risk)
1. **Paths**: curve bootstrap → abcd vol calibration → 3-factor shifted-
   lognormal LMM → df/short/4 par-swap paths, under CRN (one draw set per
   run; all scenario revaluations are differences of means).
2. **Products**: numba kernels per family emit discounted A-matrices
   (monthly grid) or CSR Acsr (exact-time), PLUS undiscounted accrual
   outputs. Behavioral models (prepay, attrition, withdrawal, exercise,
   deposit-rate ECM) live INSIDE kernels as duplicated, test-gated
   MODEL-BLOCKs (a shared helper measured 28% slower).
3. **Pricing**: A-factorization — cashflows once, OAS only in
   discounting; vectorized Newton OAS solve. Duck-typing lets new decks
   reuse corp pricing (proven 5×: CDs, swap legs, …).
4. **Risk/stress**: fixed-OAS CRN scenario loops; v0.15 batches all 38
   KRD/vega revaluations into ONE kernel launch (PV[38,S]).
5. **Accounting**: effective-interest book yields (or amortized-cost
   overrides), smeared CSR accruals, NII + runoff vectors.
6. **KPIs**: EVE/duration gap (parallel dv01s), LCR/NSFR (deck
   maturities + stylized weights), CET1 projection.
7. **Strategy stack**: strategies.py (per-path forward fixing) |
   unitlib.py (unit tensor through live engines; linear eval ~300µs) |
   optimizer.py (robust LP over the tensor; HiGHS, duals).

## Performance contract
No transcendentals in hot loops (Padé(7,6) + LUTs); float32 storage
only; numba freezes constants at first compile (restart to change
prepay); prange everywhere — thread count via API settings. Measured
single-core figures live in the engine AGENTS.md; re-measure before
quoting.

## Data flow invariants (cross-layer)
Fixed-OAS revaluation; one CRN per run; engine accrual outputs appended
LAST in tuples; /strategy/eval must stay sub-ms (no engine calls in that
path); quant logic never leaves the package.
