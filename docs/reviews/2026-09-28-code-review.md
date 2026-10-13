# Project code review — 2026-09-28

Remediation is documented in [2026-09-28-remediation.md](2026-09-28-remediation.md). The findings and measurements below describe the original reviewed checkout.

Reviewed checkout: `66fdb36`. Scope: quant engine, API adapters and state, optimizer and Strategy Lab, Arrow round trips, and React workspace. No application or engine source was changed. Pre-existing changes to `.serena/project.yml` and `apps/web/tsconfig.tsbuildinfo` were left in place.

The most consequential problems are incorrect numerical results and inconsistent state across layers. Address these before optimizing kernel arithmetic. The existing A-matrix separation, common random numbers, single compute worker, and kernel equivalence tests are useful foundations.

Evidence labels below distinguish executed reproductions from source analysis. P1 means a high-priority correctness issue; P2 means a narrower correctness or interaction issue. These are code findings against the project's own contracts, not an external validation of its financial assumptions or regulatory mappings.

## Validation performed

- Engine suite: **34 passed in 27.88 seconds**.
- Web production build: **passed**. Main JavaScript bundle: **1,129.42 kB**, **296.30 kB gzip**; Vite emitted its large-chunk warning.
- Executed isolated Python reproductions of vega parity, optimizer allocation replay, book serialization/save, ignored path/thread settings, cache invalidation, failed cache publication, queued-job state changes, scenario OAS recalibration, and NSFR equity reconciliation.
- Profiled a warm API KPI adapter invocation using the synthetic demo book and **four Numba threads**: **1.002 seconds**. This is a local sample, not a throughput benchmark or a production latency guarantee.
- No live service or user portfolio was modified. API reproductions called application functions in separate processes. UI findings are source-traced; browser interactions and HTTP concurrency were not exercised.

## High-priority findings

### 1. P1 — Batched MBS vegas have the opposite sign from the reference implementation

Location: `packages/portfolio-risk/src/portfolio_risk/analytics/risk.py:81–83`; compare scenario ordering at lines 46–49 and sequential vegas at lines 106–110.

The batch stores volatility scenarios as down, then up, but calculates down-minus-up. The sequential path calculates up-minus-down. Rate KRDs correctly use down-minus-up; copying that convention to volatility reverses every vega.

**Executed evidence:** On four demo pools, the first `vega_1x2` was **+2,327.52883761** in the batch and **−2,327.52883762** in the sequential reference. Across all vegas, the maximum absolute *sum* of the two implementations was `4.13e-8`, confirming they are negatives. KRDs agreed within `2.59e-8`.

**Fix:** Reverse the vega subtraction. Add a full driver parity test covering KRDs and vegas; the current batched-kernel PV test cannot catch a downstream sign error.

### 2. P1 — The optimizer credits late purchases with income outside the planning horizon

Location: `packages/portfolio-risk/src/portfolio_risk/strategy/optimizer.py:43`; compare `strategy/unitlib.py:275–282`.

The LP sums every unit's full income vector, regardless of its purchase month. Strategy evaluation shifts the vector and retains only `H - purchase_m` months. Consequently, the optimization objective and CET1 income coefficients disagree with the engine used to evaluate the returned allocation.

**Executed evidence:** A controlled 27-month library with monthly income coefficients of 1 at month 0 and 2 at month 24 caused the optimizer to allocate all 100 units at month 24 and report **5,400** of income. Replaying that allocation through `evaluate_strategy` produced **600**. Buying at month 0 would produce **2,700**, so this affects both the reported number and the selected optimum.

**Fix:** Derive LP income coefficients from the same shifted, truncated vectors used by Strategy Lab. Check objective equality and every constraint by independently replaying the returned allocation.

### 3. P1 — Robust optimization reuses base-market earnings in stressed capital constraints

Location: `apps/api/app/store.py:584, 624–626`; `packages/portfolio-risk/src/portfolio_risk/strategy/optimizer.py:55, 88`.

`run_optimize_job` computes NII once under the base market, then supplies that same monthly NII to `compute_kpis` for every stressed market. Thus stressed base-book earnings never change retained capital. Separately, the documented objective includes base-book scenario NII, but the implementation hardcodes its offset to zero and maximizes incremental NII only.

**Source-traced impact:** A scenario with materially worse existing-book earnings can appear to satisfy CET1 and can be treated as less adverse than it is. Omitting scenario-specific objective offsets can also change which scenario binds and which allocation wins. This is separate from the purchase-timing defect above.

**Fix:** Compute base-book NII per scenario, use it for that scenario's capital path, and include its total in the maximin constraint if the product promises total balance-sheet NII. If incremental NII is intentionally the objective, explicitly label it as such while still fixing scenario-specific capital.

### 4. P1 — Named scenario runs re-solve OAS instead of preserving the base calibration

Location: `apps/api/app/main.py:151–164`; `analytics/risk.py:18–20`; `products/corp.py:280–283`; corresponding CD/deposit/stress entry points.

The API shifts the market and invokes ordinary top-level risk drivers. Those drivers solve OAS against the unchanged book prices under their supplied market. They preserve OAS for internal derivative bumps, but not between the application's base and named scenario. This violates the root instruction that scenario runs never re-solve OAS.

**Executed evidence:** A demo loan retained its model price of **98.73489102095** after a +100 bp curve scenario while its OAS changed from **310.1797 bp** to **310.8277 bp**. The new solve absorbs market movement into spread rather than retaining the original calibration.

**Fix:** Separate base calibration from scenario revaluation and pass the original per-position OAS into scenario drivers. Test zero-shock identity and nonzero-shock changes at unchanged OAS. Preserve common random numbers throughout.

### 5. P1 — Saving an unchanged book destroys the schema needed for subsequent runs

Location: `apps/api/app/main.py:54–62`; `apps/web/src/lib/api.ts:107–124`; `apps/api/app/store.py:111–125`.

The browser converts Arrow dates to ISO strings. `put_book` reconstructs a DataFrame from untyped JSON without restoring dates. Object-backed call schedules are also exported as JSON strings without a matching typed reconstruction path.

**Executed evidence:** A read/serialize/save round trip changed `maturity` from `Date` to `String`. Loan and debt deck construction then failed with `TypeError: '>' not supported between instances of 'str' and 'datetime.date'`. CDs failed with `'str' object has no attribute 'weekday'`. KPI maturity filters also failed.

**Fix:** Validate and normalize per-product input schemas before replacing stored frames, including nested schedule dates. Reject malformed submissions atomically. Add unchanged-book round-trip tests for every product, including optional schedules and empty books.

### 6. P1 — Strategy caches remain valid-looking after their inputs change, and rebuilds publish partial state

Location: `apps/api/app/main.py:54–92, 110–126`; `apps/api/app/store.py:533–562`.

Book, market, settings, and assumption writes do not invalidate `UNITLIB` or `BASE_KPIS`. A library rebuild assigns `UNITLIB` before calculating `BASE_KPIS`, permitting evaluations against a new library and old base KPIs. If the KPI calculation fails, that mixed pair remains installed.

**Executed evidence:** Cache sentinels survived settings and book edits. A controlled KPI failure during rebuilding left the newly built library alongside the previous base KPI object.

**Fix:** Key caches by immutable market/book/model/settings revisions. Build both objects privately and publish one complete cache bundle atomically. Return a rebuild-required response on revision mismatch. Keep the synchronous evaluation path free of engine calls.

### 7. P1 — Simulation settings do not control the simulation advertised to users

Location: `apps/api/app/store.py:323, 369–394, 463–494`; `core/config.py:12–14`; `core/scenarios.py:165`; `analytics/accounting.py:144–146`.

The API reads `SETTINGS.n_paths` for progress and planning, while the engine drivers instantiate CRN from fixed defaults. Stress horizons use `STRESS_HORIZONS_M`; library builds omit the configured horizon and retain 27 months.

**Executed evidence:** NII runs requested with **32** and **2,048** paths both instantiated CRN objects with **128** paths and produced the identical total, **1,250,986,939.9074564**. The path setting changes the displayed work estimate, not the calculation.

**Fix:** Introduce an explicit run configuration passed through drivers, including distinct base-calibration and sensitivity counts where needed. Report actual counts. Either propagate the chosen horizon consistently or separate the fixed 9Q settings from configurable NII settings. Validate even path counts or repair antithetic generation: `CRN(33, 7)` currently reports 33 paths but produces only 32 rate paths and 33 mortgage-model noise paths.

### 8. P1 — NSFR omits the money-market book when estimating equity

Location: `packages/portfolio-risk/src/portfolio_risk/analytics/kpis.py:284–288`.

The equity contribution to ASF uses assets minus liabilities from MBS, loans, debt, deposits, and CDs only. The same model includes money-market assets and liabilities elsewhere, and the supplied `bs['equity']` is ignored. This is an internal balance-sheet inconsistency independent of whether the regulatory weights are stylized.

**Executed evidence:** The seeded balance sheet has equity/full EVE of **$1.80313 billion**, while this partial proxy is **−$779.38 million**, clipped to zero. Holding other terms fixed, including the omitted equity changes modeled NSFR from **142.18%** to **161.18%**.

**Fix:** Define one authoritative equity input/reconciliation including all modeled books, and reuse it consistently. Add a reconciliation test with the markets book enabled.

### 9. P1 — Risk Desk's “net dv01” adds liability sensitivities to asset sensitivities

Location: `apps/web/src/pages/Dashboard.tsx:75–77`; compare `analytics/kpis.py:191–198`.

The dashboard sums every book's positive-value DV01 directly. Engine results for deposits, CDs, and debt are liability-value sensitivities; the KPI engine subtracts them to compute net EVE sensitivity. The displayed badge therefore misstates the direction and size of net risk. KRD chart aggregation also needs an explicit distinction between instrument-value exposure and signed balance-sheet exposure. The risk adapter additionally omits hedge overlays, so its total should not imply parity with the hedged KPI book.

**Fix:** Apply explicit book-side signs and include the agreed hedge scope, ideally in a shared API-level aggregate. Test an equal-and-opposite asset/liability fixture and reconcile with the KPI definition.

## Additional actionable findings

### 10. P2 — Queued jobs read mutable state rather than their submission inputs

Location: `apps/api/app/store.py:280–298, 369–394, 489–494`.

The single worker serializes compute jobs but does not serialize API edits. Adapters consult `BOOKS`, `SETTINGS`, histories, and scenarios while executing. A queued job can use inputs changed after submission; a multi-book job can even mix revisions. `_LOCK` protects progress writes, not the input state.

**Executed evidence:** A job submitted while the seed was 7 observed seed **999** after an edit made before it reached the worker.

**Fix:** Capture one immutable input snapshot and its revision at submission; pass it into every adapter. Store the revision in the job result. This is also the safe basis for performance caches.

### 11. P2 — Returning thread settings to “all cores” keeps the previous limit

Location: `apps/api/app/store.py:295–297`.

The persistent worker sets Numba's thread count only when the setting is positive. A subsequent zero setting never resets the worker's thread mask.

**Executed evidence:** Jobs configured first with 1 thread and then with 0 reported actual thread counts of **1 and 1**.

**Fix:** Resolve zero to the process-supported maximum and set the count for every job. Validate against that maximum rather than allowing arbitrary values up to 256 that may fail only after queueing.

### 12. P2 — Batched risk drops custom CC/PS/HPI models during revaluation

Location: `packages/portfolio-risk/src/portfolio_risk/analytics/risk.py:54–55`.

Base OAS construction receives `suite`, but the batched `build_paths` call omits it. A suite that customizes CC, spread, or HPI while retaining the default prepay kernel gets base pricing from one model and risk revaluations from another. Only custom prepay triggers the sequential fallback.

**Fix:** Pass the same suite through every scenario path. Also ensure setup uses the chosen components' fit methods rather than hardcoding default fitters. Add a distinctive custom-model test at driver level, not just a custom-prepay kernel test.

### 13. P2 — The emitted 30-year swap path silently shortens beyond year 10.25

Location: `packages/portfolio-risk/src/portfolio_risk/core/lmm.py:13–19, 38`; `core/config.py:17`.

`_par_rate` clips `eta + n_q` to `N_FWD`. The grid ends at 40.25 years, but the simulation runs for 30 years and labels the fourth output as a 30-year par swap throughout. At simulation year 20 it uses approximately 20.25 years of remaining forwards, not 30. This output feeds mortgage current-coupon forecasts and can be selected by strategy/hedge code.

**Fix:** Extend or explicitly extrapolate the forward grid for all requested tenors and horizons, or reject unsupported combinations. Disclose and test any approximation. Assess runtime and memory impact before increasing grid size globally.

### 14. P2 — Nested React component definitions reset active editors

Location: `apps/web/src/pages/Optimizer.tsx:143`; `pages/Positions.tsx:214, 235`.

`F`, `SideRows`, and `BookGroup` are recreated as component types on parent renders. Updating an optimizer field remounts its input; changing a position balance remounts the row subtree containing `BalEdit` and its popover. This can lose focus, close the editor, and interrupt dragging. Optimizer elapsed-time updates also recreate `F` while a solve runs.

**Fix:** Move component definitions to module scope and pass explicit typed props. Verify multi-character number entry, uninterrupted slider dragging, and retained popover state in a browser interaction test. This also removes unnecessary DOM recreation.

### 15. P2 — Strategy evaluation can display a response for an older allocation

Location: `apps/web/src/pages/Strategy.tsx:40–48`.

The debounce clears pending timers, but does not cancel started requests or discard obsolete responses. Once request A is in flight, a later B can finish first and then be overwritten by A. The effect also has no unmount cleanup.

**Fix:** Add a request generation check and cancellation support, plus effect cleanup. Test controlled out-of-order responses. Keep the displayed result associated with the allocation revision it evaluates.

### 16. P2 — Strategy KPI timing is inconsistent with its cashflow timing

Location: `packages/portfolio-risk/src/portfolio_risk/strategy/unitlib.py:279–316`.

Income and DV01 are delayed until purchase, but HQLA, ASF, RSF, and RWA add all allocation notionals immediately. Even an allocation beyond the forecast horizon, skipped by the cashflow loop, still changes those ratios. Conversely, the scalar DV01 remains flat after purchase even as the balance amortizes or a CD matures, despite the UI describing cohorts amortizing.

**Fix:** Specify the evaluation date for each KPI, then apply active, outstanding cohort balances at that date. Precompute the required monthly vectors in the library so evaluation remains cheap. Gate purchase-after-horizon and post-maturity cases. If a flat DV01 proxy is retained, label it explicitly in the UI.

### 17. P2 — Most panel runs bypass the shared run and telemetry channel

Location: `apps/web/src/pages/Dashboard.tsx:37–43`; `pages/Kpis.tsx:25`; `pages/MorningSheet.tsx:59`; `pages/Strategy.tsx:32`; `pages/Optimizer.tsx:125–138`; compare `lib/engine.tsx:101–142`.

The EngineProvider drives global running state, pipeline telemetry, and shared KPIs, but the principal pages call `api.run` or fetch the optimizer directly and poll in local state. Running calculations in those panels does not update the global channel; the masthead can appear idle, shared KPI state stays stale, and another panel can enqueue duplicate work. The server's one-worker design still serializes computation, but the promised single-flight UI is not enforced across these paths.

**Fix:** Route all submission and observation through one shared job registry, including optimizer jobs, while keeping panel-specific result selectors. Show queued state explicitly.

## Optimization priorities

| Priority | Change | Evidence and expected benefit | Required guardrail |
|---|---|---|---|
| 1 | Create an immutable, run-scoped market context with calibrated parameters and shared rate paths | One warm KPI call made **21 rate simulations**, **16 `build_rate_paths` calls**, **5 `build_paths` calls**, and **4 calibrations**. Calibration consumed **0.2783s** and simulation **0.2489s** of the **1.002s** sample. Reuse avoids repeated work across products and NII/KPI adapters. | Cache by curve, vol inputs, seed, path count, model revision, and numerical configuration; retain CRN and fixed-OAS semantics. Do not blindly cache mutable global inputs. |
| 2 | Remove duplicate cashflow computation in NII | `analytics/accounting.py:160–164` solves MBS OAS, discards it, then rebuilds the same paths and cashflows for accounting. Lines 229–232 invoke `_deposit_A` twice to retrieve outputs returned together. | Reuse the existing results; compare every NII/runoff output before and after. Avoid changing the model itself. |
| 3 | Reuse base/up/down paths inside unit-library builds | `strategy/unitlib.py` repeatedly invokes `rp(sr)` for corp, CD, and deposit products with identical curve/vol/CRN inputs. | Use the shared run context above; keep product-specific behavioral paths distinct. |
| 4 | Split telemetry state from stable market/configuration state | `lib/engine.tsx:108,145` updates a single context every 100ms, invalidating all consumers, including market/config-only consumers. Dashboard also reconstructs Arrow-derived aggregates on elapsed updates. | Separate contexts or use selected subscriptions; memoize numerical reductions by result identity. Profile before claiming a frame-rate improvement. |
| 5 | Lazy-load workspace panels and heavy optional dependencies | All panels are eagerly imported by `workspace/panels.tsx`; the verified build emits **1.13 MB** of main JS before gzip. | Code-split panels with suitable loading/error states; confirm docking, restoration, and direct navigation still work. Measure startup separately from compute time. |
| 6 | Bound job retention and cache result serialization | `JOBS` retains raw results indefinitely; each `/jobs/{id}/result` fetch serializes the result tree again. This grows memory with session length and repeats Arrow construction. | Add TTL/count/byte limits, protect active jobs, and reuse serialized bytes when appropriate without retaining unnecessary duplicate representations. |
| 7 | Make tables scale with visible rows | `components/ui.tsx:106` renders every cell; Positions recomputes repeated balance/NII/KRD arrays during aggregation. | Memoize per-position vectors and aggregates; introduce virtualization based on measured book size and interaction cost. Preserve keyboard and screen-reader behavior. |

These are opportunities, not measured post-change speedups. Start with path/calibration reuse and duplicate accounting calls; kernel arithmetic is already carefully optimized and changes there have a higher numerical-validation cost.

## Model and boundary issues to resolve before expanding scope

- **Funding conservation:** The optimizer has asset caps and regulatory ratio rows, but no explicit cash/funding conservation constraint or priced cash/ST-funding residual. Decide whether allocations must be self-financing or consume an existing cash pool. Otherwise an optimizer can select new assets without a corresponding funding cost. This needs a product/model decision, not a guessed constraint.
- **LCR composition:** Unit-library evaluation adds the full template HQLA weight without applying the base model's Level 2A composition cap. The optimizer discloses a linearization, but its usable range should be bounded and solutions replayed through the intended KPI calculation before claiming compliance.
- **OAS convergence:** Monthly and corporate solvers return a price after exhausting iterations without surfacing convergence or bracket failure. Invalid or extreme edited prices can be accepted and presented as solved. Return convergence/residual information and reject unsupported cases before producing risk.
- **Input validation:** Markets, allocations, optimizer options, assumptions, and programs have weak or absent shape/domain validation. Validate finite values, column schemas, schedule terms, known templates/scenarios, and legal purchase months at the API boundary.
- **Nine-quarter scenario meaning:** `run_scenario_grid` runs nine independent full-horizon forecasts on the unchanged book and original as-of date. It does not roll balances and cohort state through the scenario path; the spread leg is also unused. Label it as separate instantaneous forecasts or implement the intended stateful quarterly path.
- **Dependency compatibility:** The engine permits NumPy `>=1.26` but calls `np.trapezoid`, which is not available in the full declared version range. Align minimum dependencies with used APIs and test the declared lower bound. The passing local environment alone does not establish installation compatibility.

## Recommended implementation and regression sequence

1. Fix batched vega signs and optimizer timing. Add full-driver parity and optimizer-to-Strategy-Lab replay tests first.
2. Repair typed book round trips, immutable job snapshots, and atomic revisioned cache publication. Add API-level tests for edits, failed builds, queued runs, and actual applied settings.
3. Correct named-scenario OAS handling, scenario-specific earnings/capital, equity reconciliation, and signed dashboard aggregation.
4. Resolve custom-model propagation and long-tenor coverage. Extend the existing controlled-path and zero-shock invariant test patterns.
5. Consolidate frontend run state, move nested component definitions, and make evaluation responses revision-aware. Add browser tests for editing and concurrent response order.
6. Introduce shared computation and lazy panel loading, then benchmark cold JIT, warm execution, peak memory, serialization, and UI response separately on representative book sizes.

The 34 passing tests mostly cover engine numerical invariants and expected directions. They do not establish API round-trip correctness, UI behavior, full risk-driver sign parity, or optimizer allocation replay. Those missing cross-layer checks explain how the confirmed failures coexist with a green suite.
