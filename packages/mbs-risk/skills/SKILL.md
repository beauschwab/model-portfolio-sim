---
name: mbs-risk-engine
description: Operate the mbs_risk Python package — a shifted-lognormal LMM Monte Carlo engine for agency MBS portfolios that computes OAS, key-rate DV01s, swaption vegas, 27-month forward valuations, and 9-quarter stress-capital P&L on 10,000+ positions. Use this skill whenever the task involves MBS or mortgage pool pricing, OAS calculation, prepayment modeling, KRD/duration/vega risk on mortgage portfolios, CCAR/DFAST-style stress testing, forward-starting rate shocks, current coupon models, or any mention of the mbs_risk package, even if the user only says "run risk on the portfolio" or "what's the stress P&L". Also use it before MODIFYING the engine (prepay model changes, calibration swaps, new shock types) — it documents invariants that must not be broken.
---

# mbs-risk-engine

Drive and extend the `mbs_risk` package: OAS, risk (10 KRD01s + 9 vegas),
forward valuation, and 9Q stress capital for agency MBS passthroughs.

## Setup and verification (always do this first in a fresh environment)

```bash
pip install -e .            # from the package root (pyproject.toml present)
pytest tests/ -q            # 4 tests MUST pass before trusting any output
python -m mbs_risk 1000 bench   # throughput probe + projected wall time
```

First kernel call in any process pays numba JIT (~20-40s). The bench and
test paths include warmup; account for it when timing anything yourself.

## The three workflows

All inputs are Polars DataFrames / numpy arrays. Exact column schemas:
read `references/schemas.md` BEFORE constructing any input frame.

### 1. OAS only (price the book)
```python
from mbs_risk.scenarios import setup, solve_base_oas
models, B, abcd0, sec, tgt, face = setup(port, swap_rates, vol_pts, cc_hist, ps_hist)
oas, px = solve_base_oas(swap_rates, vol_pts, abcd0, B, models, sec, tgt)
# oas, px: per-position arrays (decimals; px per unit balance)
```

### 2. Spot risk (KRDs + vegas)
```python
from mbs_risk import run_risk
df = run_risk(port, swap_rates, vol_pts, cc_hist, ps_hist)
# adds: oas_bps, model_price, dv01, krd01_{1..30}y ($/bp), vega_{e}x{t} ($/vol-pt)
```

### 3. 9Q stress capital (forward valuation + forward-starting shocks)
```python
from mbs_risk import run_stress
pos, agg, prof = run_stress(port, swap_rates, vol_pts, cc_hist, ps_hist)
# pos:  long frame, position x 27 monthly horizons x shocks
#       (fwd_value_base, fwd_price_base, fwd_value_shock, stress_pnl in $)
# agg:  portfolio P&L by (horizon_m, shock_bp)
# prof: forward DV01 profile ($/bp per horizon) from the +/-100bp pair
```

CLI equivalents: `python -m mbs_risk <N> [bench|risk|stress|all]` (writes
`risk_results.parquet` / `stress_results.parquet`; uses demo data).

## Critical rules — violating these produces silently wrong numbers

1. **Demo data is synthetic.** `mbs_risk.demo` exists so the package
   self-tests. Production runs need real monthly histories (`cc_hist`,
   `ps_hist`), real par swaps, and a real vol surface. If the user hasn't
   supplied them, say so explicitly — do not present demo-fitted output as
   production risk.
2. **MODEL-BLOCK duplication.** The per-month prepay/cashflow model is
   open-coded in BOTH `kernels.engine` and `kernels.stress_engine`
   (deliberately — a shared inlined helper measured 28% slower). Any model
   change must be applied to both marked blocks, then `pytest` rerun:
   `test_zero_shock_invariant` is the drift guard and will fail if the two
   copies diverge.
3. **Numba freezes module constants at first compile per process.** Editing
   `config.py` (or `prepay.py` anchors/LUTs) requires a fresh Python
   process. Special trap: `PREPAY_PARAMS` travels as a runtime vector, BUT
   `BURN_LUT` is built from `PREPAY_PARAMS[3]` (burn_k) at import — changing
   burn_k silently does nothing without re-import.
4. **OAS is held fixed** across all risk and stress revaluations (standard
   spread-constant convention). Never re-solve OAS inside a scenario.
5. **Common random numbers are load-bearing.** Risk/stress differences are
   only low-variance because every scenario reuses one `CRN` object. Never
   reseed or rebuild CRN between the two sides of a central difference.
6. **Units:** rates/spreads as decimals internally; `oas_bps` in bp; KRDs in
   $ per 1bp; vegas in $ per 1 lognormal vol POINT (0.01); prices in % of
   par in output frames, per-unit decimals internally; stress P&L in $.

## Performance model (measured, single 2.1GHz core)

| operation | 10k pos x 128 paths |
|---|---|
| engine pass (A + 27-horizon FV + checkpoints) | ~8.7s |
| stress pass (1 horizon, checkpoint restart) | ~6.8s |
| path build (LMM sim + CC + PS + HPI) | ~0.1s |
| full risk (38 reval) + stress (4 shocks x 27 hz) | ~18 min |

Scales near-linearly with cores (prange over securities). Modern 12-16 core
laptop: ~1-1.5 min for the full pack. Memory: checkpoints are
`S x P x 27 x 2 x 4B` (~276MB at 10k x 128) — scale paths down before
securities up if memory-bound. Speed levers in order: reduce
`N_PATHS_SENS`; cohort-bucket the portfolio before calling (real books
collapse ~5x); raise paths only if KRD noise is observed.

## Accuracy switches (config.py)

- `RATIONAL_SIGMOID=True`: Pade(7,6) logistics; measured max 0.007bp OAS
  vs exact. Set False only to prove equivalence (e.g., model validation).
- `USE_FLOAT32=True`: path-array storage only (scalar math stays f64);
  <~0.5% jitter on 1bp KRDs. Accumulation is always f64.
- `N_PATHS_BASE=512` (OAS solve) / `N_PATHS_SENS=128` (scenarios, CRN).

## Known model limitations (disclose when reporting results)

Deterministic CC vol features (no stochastic vol — SV-LMM needed for vol
dynamics in the CC distribution); point vegas projected onto the 4-param
abcd family (smeared toward neighbors); stylized prepay spline anchors and
S-curve params (fit to loan-level before production); forward shocks are
parallel-only; frozen-weight Rebonato calibration; no payment-delay
adjustment; lognormal-equivalent ATM vol conversion is vega-approximate.

## Modifying or extending the engine

Read `references/internals.md` first — it maps modules, data flow, where
loan-level fits plug in, how to add per-pillar forward shocks or per-point
vol multipliers, and which tests gate each change. Never edit kernels
without rerunning the full test suite.
