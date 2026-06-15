# portfolio_risk internals and extension guide

## Data flow

```
swap_rates --bootstrap--> DFs/forwards --+
vol_pts ---calibrate abcd (Rebonato)-----+--> LMM sim (CRN.Z) --> df, 4 swap paths
                                                  |
cc_hist --fit--> {beta, lambda} --> CC paths <-- vol features (deterministic)
ps_hist --fit--> OU params ------> PS paths (CRN.eps_ps)
                                   mtg = CC + PS, 2m incentive lag
s10 path --> HPI paths (CRN.eps_h) --> hpi, yoy
                                                  |
            kernels.engine(mtg,hpi,yoy,df, sec attrs)
              -> A[s,t]  (pricing: PV at any OAS = cheap einsum)
              -> FV/BAL[s,h] (27 monthly horizons)
              -> ck_bal/ck_burn[s,p,h] (stress restarts)
                                                  |
   pricing.solve_oas_from_A (vectorized Newton, bisection bracket)
   risk.run_risk: fixed-OAS central diffs, CRN, abcd fixed for curve bumps,
                  recalibrated (warm) for vol bumps
   stress.run_stress: shocked_paths templates -> stress_engine per (shock,h)
```

## Why it is fast (do not undo these)

1. **A-matrix factorization**: OAS enters only discounting, so cashflows are
   generated once; pricing/Newton never re-runs the kernel. Never
   materialize a (paths x secs x months) tensor.
2. **Transcendental-free inner loop**: Pade(7,6) logistics, LUTs for SMM
   12th root and burnout exp, annuity factor by recursion (one pow per
   security). Adding an exp/log/pow inside the month loop costs ~15-25%.
3. **Open-coded MODEL-BLOCK**: shared inlined helper with tuple return
   measured 28% slower (LLVM register allocation lost across pack/unpack).
   The duplication is guarded by test_zero_shock_invariant.
4. **prange over securities**: each thread owns its output rows -> no
   contention; path arrays are small enough to stay cache-resident.
5. **CRN everywhere**: one draw set across all 39+ revaluations; deltas of
   means, not means of deltas.
6. **Forward-starting shocks need no re-simulation**: deterministic
   templates in (t-h) — CC partial-adjustment ramp, deflator
   (1+d*dt)^-(t-h+1), HPI drift beta — applied to base paths
   (scenarios.shocked_paths).

## Extension recipes

### Plug in loan-level prepay fits
Replace anchors in `prepay.py`: LTV/FICO/SIZE (x, y) arrays feed
`nat_spline` / CubicSpline directly; STATE_MULT / CHANNEL_MULT dicts.
S-curve, turnover, burnout, lock-in live in `config.PREPAY_PARAMS`
(runtime vector — no recompile, EXCEPT burn_k which bakes into BURN_LUT at
import; rebuild process after changing it).

### Real swaption calibration
`vol.calibrate_abcd` is the seam: keep its signature, swap the objective
(e.g., add per-forward phi multipliers: extend the parameter vector and
multiply into `sig_tab` in `lmm.simulate_rates`). For crisp bucketed vegas,
add per-point multipliers on top of abcd and bump those instead of
recalibrating — `risk.run_risk`'s vol loop is the only caller to change.

### Per-pillar (non-parallel) forward shocks
`scenarios.shocked_paths` currently builds parallel templates. For a
pillar/curve-shape shock at horizon h: compute the shocked par curve, take
d_swaps = shocked minus base pathwise swap-rate effect via the CC beta
vector (per-tenor instead of summed), and the deflator template from the
short-rate portion of the shock. Same plumbing; stress_engine is unchanged.

### New shock dimensions (vol, HPI, spread)
- HPI stress: multiply `base["hpi"]` by a scenario path, rebuild yoy.
- Spread (OAS) stress: pass a bumped `oas` vector into stress_engine.
- Forward vol shock: requires re-simulation with bumped sig_tab rows >= h
  (cheap, ~0.1s) but breaks the "no re-sim" shortcut; use build_paths with
  a modified abcd and splice paths at h.

### Adding output (e.g., effective duration/convexity at base)
Derive from existing scenario PVs — never add columns to the kernel for
quantities computable from A or FV downstream.

## Change gates

| change | must rerun |
|---|---|
| anything in kernels.py | full pytest (zero-shock invariant + signs) |
| prepay anchors/params | test_oas_roundtrip + eyeball CPR sanity |
| calibration/vol | calibration RMSE print < ~50bp vol on real surface |
| config dtype/sigmoid switches | test_rational_sigmoid_oas_accuracy |
| new shock template | extend test_zero_shock_invariant pattern: zero-
  magnitude shock must reproduce base FV exactly |

## Module sizes / where things live

config (constants, switches) | curve (bootstrap) | vol (calibration,
features) | lmm (simulation) | models (CC/PS/HPI fits+paths) | prepay
(model DATA) | kernels (engine, stress_engine) | pricing (PV/OAS from A) |
scenarios (CRN, paths, shocks, setup) | risk | stress | demo | __main__.
