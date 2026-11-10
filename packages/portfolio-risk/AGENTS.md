# AGENTS.md — portfolio-risk (root)

## Production execution contract

The authoritative policy is `../../docs/rust-production-contract.md`.
Follow its deployment, compatibility and validation rules. Rust owns built-in
pricing, graph/coordinator, simulation, capital/FTP and solver financial logic.
HiGHS remains C++; Rust constructs/invokes/validates its financial problem.
Python API/storage adapters and independent verification remain application services.

Rust is the new/omitted backend default. Historical Python inputs/results remain
readable unchanged but need an explicit Rust selection and library/session rebuild
before execution. Missing/mismatched native artifacts fail without fallback.
Production pricing/what-if enters the native graph without a custom discount override.
Never add Python/NumPy/Numba financial options to production requests or UI.

Preserve fixed base OAS, shared CRN, unchanged final-output tolerances, immutable
verified publication and coefficient-only interactive evaluation. Python test
oracles are explicit and independent; fresh-process default/ownership tests run
outside their reference context and forbid Python financial callbacks.

The root current contract governs cross-layer ownership and product/publication
rules. Read `../../docs/models/native-contracts.md` for the selected schema and
`../../docs/models/implementation-plan.md` for remaining adoption gates.

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

Deeper docs: `src/portfolio_risk/AGENTS.md` (model assumptions + extension
recipes), `tests/AGENTS.md` (what gates what), `skills/portfolio-risk-engine/`
(the operator skill for LLM agents using — not modifying — the engine).

## Commands

```bash
pip install -e .                      # editable install (pyproject)
pytest tests/ -q                      # full suite; MUST pass before any ship
python -m portfolio_risk 10000 bench        # throughput probe + projection
python -m portfolio_risk 1000 all           # MBS risk + 9Q stress, demo data
```

The explicit Python reference kernel pays numba JIT on its first call (~20-40s). `cache=True` on
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
embedded skill (`skills/portfolio-risk-engine/` mirrors the source skill) and
update `README.md` + skill references on any behavior change. Performance
claims in docs are MEASURED on a single 2.1GHz core — re-measure before
restating them.

## Architecture and interface maintenance

Current architecture: `../../docs/architecture.md`. Interface/transport and dependency rules are in
`../../docs/contracts/README.md`. Run `make check-architecture` from the root
for boundary/contract changes. Dated migration sections are archived in `../../docs/changelog/agents-history/`;
current production and shared-model contracts supersede their defaults.
Preserve independent reference tests and all applicable numerical/publication gates.

## Current product and publication rules

Follow the repository root `AGENTS.md` product-specific rules and linked documents
before changing a financial lane. The operator reference
`skills/portfolio-risk-engine/references/current-runtime.md` covers current API
and coefficient workflows. Historical notes are preserved unchanged in
`../../docs/changelog/agents-history/packages--portfolio-risk--AGENTS.md`;
use them as dated evidence only, not backend or adoption instructions.
