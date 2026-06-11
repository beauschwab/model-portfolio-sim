# mbs_risk internals and extension guide

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

## v0.5 additions

### Swappable components
ModelSuite (interfaces.py) carries cc/ps/hpi/prepay_step; pass
`suite=ModelSuite(...)` into run_risk/run_stress/build_paths. Custom CC/HPI
models MUST implement shock_response/shock_multiplier or stress templates
break. Custom prepay: jitted step fn (signature in interfaces.py docstring)
-> generic engine, ~25-30% slower; run_stress raises until the model is
promoted into both MODEL-BLOCKs and the invariant test extended.

### Conventions
conventions.py: year_fraction(d1,d2,DayCount), Calendar("US") with rule-
generated SIFMA-style holidays, adjust(date,BDC), gen_schedule, MBS delay
via `pay_delay_days` portfolio column (applied at OAS-discounting layer).

### Corporates
corp.CorpDeck(contracts_frame, asof) + run_corp_risk. Contract columns:
id, face, maturity(date), freq_months, daycount(str), is_float, 
coupon_or_spread, price; optional: cap, floor, amort_type
(bullet|annuity|sink), sink_schedule [(date,frac)], call_schedule /
put_schedule [(date,px)], call_threshold. Exercise is RULE-BASED (issuer
calls when 5y sim rate < coupon - thr) -- disclose that callable OAS is
not option-exact; the EXERCISE-BLOCK in corp_engine is the LSMC seam.
Coupon amounts are day-count exact AND discounting is exact-time:
within pay month m, df(t) = df[p,m-1]/(1+short[p,m]*(t-m/12)); the OAS
layer uses per-period exact pay times via the CSR A-vector
(corp_pv / corp_solve_oas, NOT the monthly pv_from_A). No 9Q stress for
corp v1. Pricing helpers: _corp_A -> Acsr; corp_pv(deck, Acsr, oas, n).
Floater fixings read the index at the EXACT fixing date (linear
interp between bracketing monthly observations, deck.fix_m/fix_w).
Remaining index limitation: front 3m forward used regardless of
accrual frequency (no 1m/6m index curves).

### Deposits (v0.6)
deposits.py: LogisticBetaECM (registered "deposit_rate"); fit = JOINT NLS
over the simulated asymmetric-ECM recursion (never use a static levels
fit -- measured b_max 0.22 vs true 0.60). deposit_engine mirrors the MBS
A-matrix on the monthly grid (deposit cycles are monthly; no exact-time
mapping needed) + undiscounted principal runoff for WAL. Attrition
anchors in SEGMENTS/AGE_/SIZE_ are the panel-fit seam. OAS bracket
widened to -15% (franchise premia => negative liability OAS).
9Q deposit stress IS wired (run_deposit_stress): shocked deposit-rate
paths are EXACT re-runs of the ECM recursion on the shifted short path
(no linearized template); velocity recomputes (flight under shock);
deposit_stress_engine restarts from balance-only checkpoints. Sign
convention: eve_pnl = -stress_pnl (liability). The duplicated
DEPOSIT-MODEL-BLOCK is gated by test_deposit_zero_shock_invariant --
same rule as the MBS kernels: change BOTH blocks, rerun pytest.
shock_response on the rate model remains as the documented linearized
approximation; the implementation uses the exact recursion instead.

### CDs (v0.8)
cds.py: CDDeck duck-types corp pricing (per_off/t_pay/tgt/n -> corp_pv /
corp_solve_oas reused unchanged). cd_engine = corp-style CSR period loop
+ WITHDRAWAL-BLOCK (depositor put: hazard S-curve in short - rate -
penalty/remaining_term; bank pays principal minus forfeited interest) +
issuer-call rule. CD_EW_PARAMS is the panel-fit seam. Cross-engine test
(test_cd_matches_corp_bullet) pins CD-with-options-off to the corp bullet
on identical paths -- the consistency gate between the two engines.

### NII accounting (v0.9)
accounting.py: run_balance_sheet_nii(model_balance_sheet(), ...) ->
monthly NII by product, book yields, model NIM. Engines' accrual outputs
appended LAST. MBS/corp = effective interest (static level yield on
expected cashflows); CDs contractual smeared; deposits at rate paid.
demo.model_balance_sheet cites WFC 1Q26 figures in its docstring; it is
synthetic and excludes trading/repo/cash/cards.

### KPIs (v0.11)
kpis.compute_kpis(bs, sr, vp, dep_hist, nii_monthly) -> EVE/duration gap
(first-order parallel dv01), LCR, NSFR, CET1 9Q path. Weight tables in
kpis.py are stylized module data -- the internal-mapping seam. EVE ==
demo equity plug by construction (balanced sheet). IRRBB flag fires on
demo: no hedge book exists yet.
