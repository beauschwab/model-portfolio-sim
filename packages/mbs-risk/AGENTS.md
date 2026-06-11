# AGENTS.md — mbs-risk (root)

Shifted-lognormal LMM Monte Carlo engine pricing the full bank book on
shared rate paths: agency MBS + whole loans, corporates/commercial
loans, non-maturity deposits, CDs, money-market/markets balance sheet,
and ASC 815 hedge overlays (swaps/swaptions). Layers above the pricing
core: NII accounting (effective-interest book yields, amortized-cost
basis), top-level KPIs (EVE/duration gap, LCR, NSFR, CET1 projection),
forward-starting strategies (per-path at-market fixing), and the unit
library (new origination through the live engines, then linear scaling
for ~300us interactive strategy evaluation with full KPI recalc).
Outputs: OAS, KRD01s, vegas, 27m forward valuation, 9Q stress P&L,
monthly NII, regulatory ratios.

Deeper docs: `src/mbs_risk/AGENTS.md` (model assumptions + extension
recipes), `tests/AGENTS.md` (what gates what), `skills/mbs-risk-engine/`
(the operator skill for LLM agents using — not modifying — the engine).

## Commands

```bash
pip install -e .                      # editable install (pyproject)
pytest tests/ -q                      # 33 tests; MUST pass before any ship
python -m mbs_risk 10000 bench        # throughput probe + projection
python -m mbs_risk 1000 all           # MBS risk + 9Q stress, demo data
```

First kernel call per process pays numba JIT (~20-40s). `cache=True` on
module-level kernels persists across processes; factory-built kernels
(generic prepay engine) recompile each process.

## Global invariants — break these and numbers go silently wrong

1. **Fixed-OAS revaluation.** OAS is solved once on base and held fixed
   across every risk/stress scenario. Never re-solve inside a scenario.
2. **Common random numbers.** One `CRN` object per run feeds all
   revaluations. Central differences are deltas of means under shared
   draws; reseeding between bump sides destroys them.
3. **Duplicated kernel model blocks are test-gated.** The per-month model
   logic is deliberately open-coded in fast/stress kernel pairs (a shared
   inlined tuple-returning helper measured ~28% slower). Edit BOTH blocks
   (markers: MODEL-BLOCK, DEPOSIT-MODEL-BLOCK) and rerun pytest — the
   zero-shock invariant tests fail on any divergence.
4. **No transcendentals in hot loops.** Padé(7,6) logistics, LUTs for
   12th-root/burnout, annuity-factor recursion. One added exp costs
   15-25% of a 1.8B-iteration kernel.
5. **Numba freezes module constants at first compile.** Config or anchor
   changes need a fresh process. Trap: `BURN_LUT` bakes `PREPAY_PARAMS[3]`
   at import — changing burn_k without re-import silently does nothing.
6. **A-matrix factorization.** Cashflows are generated once; OAS enters
   only through discounting. Never materialize a (paths × secs × months)
   tensor; never add per-OAS work to a kernel.
7. **Demo data is synthetic.** Fitters self-test on known generators.
   Production output requires real histories/market data — say so when
   they're absent.

## Architecture in one diagram

```
curve.bootstrap ─┐
vol.calibrate ───┼→ lmm.simulate (CRN) → df / short / 4 swap paths
                 │        │
models (CC,PS,HPI) → mtg/hpi/yoy paths ─→ kernels.engine ──→ A[s,t] (MBS)
                          │                kernels.stress_engine (9Q)
build_rate_paths ─────────┼──→ corp.cd engines → Acsr[j] (exact-time)
                          └──→ deposits engines → A[s,t] + runoff
pricing: pv_from_A / solve_oas_from_A (monthly) | corp_pv/_solve (exact-time)
risk / stress / corp / deposits / cds / hedges / mm: product drivers
accounting (NII + runoff) -> kpis (EVE/LCR/NSFR/CET1)
strategies (per-path fwd programs) | unitlib (unit tensor, linear eval)
```

## Versioning & shipping

Bump `pyproject.toml` + `__init__.__version__` together. Refresh the
embedded skill (`skills/mbs-risk-engine/` mirrors the source skill) and
update `README.md` + skill references on any behavior change. Performance
claims in docs are MEASURED on a single 2.1GHz core — re-measure before
restating them.
