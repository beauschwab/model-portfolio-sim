# Rust quant primitives: measured comparison

Date: 2026-09-28. Scope: isolated local integration and source assessment of
Convex, stochastic-rs and Finstack Quant for rates-workbench.

## Decision

Use a small, application-owned batch-pricing contract and adopt OSS components
selectively. The measurements support incremental invalidation and compiled
batch execution. They do not establish a performance reason to replace the
existing Numba engine with general-purpose Rust product pricers.

- **stochastic-rs: proceed with a simulation/numerical-component pilot.** The
  exercised simulation interfaces support shared paths and reproducible bumps.
  Its supplied LMM does not match our model specification.
- **Finstack: proceed with a bounded primitive pilot.** Its core cashflow and
  prepayment components compiled and passed the exercised checks. Its agency
  MBS MC-OAS implementation conflicts with our factorization and shared-path
  contracts, so adopting the entire pricer would require substantive changes.
- **Convex: block adoption of the exercised risk/callable surfaces pending
  fixes.** Two reproducible correctness/reliability issues were found. Its
  fixed-bond schedule and pricing components remain useful evaluation targets.
- **Production application code was not changed.** No library was added to the
  application dependency graph. No upstream changes or issue submissions were made.

## Reproduction and evidence

Runner: `scripts/oss_primitives/compare.py`; Rust probe:
`scripts/oss_primitives/main.rs`; instructions and exclusions:
`scripts/oss_primitives/README.md`. Raw samples, numerical diagnostics, versions,
source hashes and dependency-lock hash are in
`docs/reviews/2026-09-28-oss-primitives.json`.

Run from the repository root:

```powershell
uv run --project apps/api python scripts/oss_primitives/compare.py
```

| Project | Exact source revision | Compiled components |
|---|---|---|
| Convex | `acc357b8562587194a6c419bd38034b9d69dd62c` | core, curves, bonds, analytics, dependent math |
| stochastic-rs | `2d6bcc7b42927b5b9777bf237ea5ca9ec102f1ac` (`3.0.0-rc.3`) | core, stochastic, quant and their dependencies |
| Finstack | `19355232773b2c0daf81070d72a96171eebe0c23` | core, models and dependent analytics/cashflows |

All selected components compiled without upstream patches. This does not verify
the complete workspaces, Python bindings, GPU targets or upstream CI suites.
Finstack's full-workspace CI concerns from the initial review remain distinct
from the successful component build here.

Machine: Windows 11, Intel Core i7-14700, 20 physical/28 logical cores. Rust
1.95.0, Python 3.12.11, NumPy 2.4.6, Numba 0.65.1. Convex's manifest rejected
the installed default Rust 1.93.1. A separate 1.95.0 toolchain was installed;
the default was not changed. Sources/builds are outside the repository under
the system temporary directory. Transitive Rust dependencies are locked.

## Fixed-cashflow prices and batch timings

Synthetic 30-year, semiannual, face-100 bonds; valuation 2026-01-15; 41 distinct
coupons from 2% through 6%, repeated to form larger books. Each instrument has
60 future cashflows, including principal in the final payment. Discounting is
flat continuous 4% plus a 100bp spread, using actual days / 365. The common
fixture uses Convex-generated cashflow dates/amounts, so this tests discounting
agreement, not independent schedule-generation agreement.

Every instrument PV agrees with the explicit cashflow formula within 1e-9.
The existing Python adapter's maximum observed error was 4.27e-14 per 100 face.
Every timed batch checksum was checked too. This is not validation against
independent market prices or a complete model-validation suite.

Warm median milliseconds from the recorded four-worker configuration:

| Evaluated path | 1,000 instruments | 10,000 instruments |
|---|---:|---:|
| Existing NumPy `corp_pv`, prepared A | 0.278 | 7.988 |
| Convex product pricing, parallel adapter | 0.973 | 9.657 |
| Convex curve lookup over prepared numeric flows, serial | 0.593 | 6.332 |
| Finstack dated-flow NPV, parallel adapter | 0.486 | 4.984 |
| stochastic-rs cashflow-leg NPV, parallel adapter | 0.439 | 4.630 |
| Custom fused Rust control, parallel | 0.063 | 0.557 |
| Custom fused Numba control, parallel | 0.057 | 0.570 |

The NumPy baseline itself does not become parallel merely because the harness
configures four workers for Rayon/Numba. On one worker, the recorded 10,000-name
medians were 6.66ms for the existing path, 17.90ms for stochastic-rs, and 19.92ms
for Finstack. Small single-instrument Rust calls were faster in-process, but
these measurements exclude the Python/Rust boundary and service overhead.

Interpretation:

1. General Rust pricing objects have meaningful date, curve and validation
   overhead. Language choice alone does not guarantee higher throughput.
2. The two custom compiled controls have similar large-batch performance. The
   main opportunity shown here is fused loops, bounded allocation and batching.
3. These are different preparation boundaries: Convex product pricing generates
   cashflows in the timed call; the other paths start from prepared flows. The
   existing A vector already contains base-market discounting, while its OAS
   exponential remains inside the timed call. The Rust controls specialize a
   flat curve. This is an integration comparison, not a universal library ranking.
4. Timings exclude construction, serialization, FFI, graph traversal and path
   simulation. Eleven warm samples are retained; no CPU affinity was imposed.
   Allocation/cache/scheduler noise was visible, particularly in the large NumPy
   batch. Do not extrapolate exact ratios to a live mixed-product book.
5. Python prepared input arrays occupy approximately 9.76MB at 10,000 names.
   Peak process memory and comparable Rust object memory were not measured.

## Reproduced Convex findings

### Spread DV01 uses the wrong spread coordinate

At 100bp spread on the 30-year 2% bond:

- Direct price difference for a +1bp spread move: **0.10248367563095684**.
- `ZSpreadCalculator::spread_dv01`: **0.1327907843086109**.
- Relative discrepancy: **+29.57%**.

`Spread::as_decimal()` already divides basis points by 10,000. The helper divides
the resulting decimal by another 10,000, evaluating the sensitivity near zero
spread. The experiment leaves upstream untouched and records the discrepancy.
Use a locally checked finite difference or an upstream-fixed revision before
adopting this helper.

Sources: [spread helper](https://github.com/sujitn/convex/blob/acc357b8562587194a6c419bd38034b9d69dd62c/crates/convex-analytics/src/spreads/zspread.rs),
[spread units](https://github.com/sujitn/convex/blob/acc357b8562587194a6c419bd38034b9d69dd62c/crates/convex-core/src/types/spread.rs).

### Callable pricing panics under ordinary grid refinement

Probe: 10-year 6% semiannual bond, face 100, flat 4% curve, par Bermudan calls
from year two, Hull-White mean reversion 0.1, volatility 0.01, zero OAS.

- 100 requested steps returns a price.
- 200 and 400 requested steps panic with an out-of-bounds index in
  `convex-bonds/src/options/trinomial_tree.rs:257`.
- A second 200-step probe aligning calls to the bond's generated payment dates
  also panics. The finding is not limited to unadjusted weekend call dates.

The failing index wraps to a very large unsigned value. The root cause has not
been fully diagnosed. Catching the panic keeps this experiment running; it is
not an acceptable production pricing fallback. The successful 100-step result
has not been independently validated.

Source: [tree implementation](https://github.com/sujitn/convex/blob/acc357b8562587194a6c419bd38034b9d69dd62c/crates/convex-bonds/src/options/trinomial_tree.rs).

## Simulation, callable and behavioral probes

**stochastic-rs OU:** 4,096 paths × 361 points took approximately **1.40ms** with
four workers, including random generation and terminal extraction. Recreated
seeded processes returned identical values; one/four threads matched; retaining
all paths and mapping terminal values matched. A long-run-mean bump preserved
the expected per-path CRN difference to **8.16e-17**. Terminal mean/variance
passed checks against the Euler recursion. These checks concern this CPU OU
process and configuration, not every process or GPU backend. Its `Deterministic`
seed advances state: recreate/reset it deliberately for repeatable evaluations.

**stochastic-rs callable:** the constant-theta short-rate tree returned prices
103.517159, 103.523047 and 103.521274 at 100, 200 and 400 steps. Each price was
positive and below its straight-bond counterpart. This is a useful smoke and
refinement check, not independent price certification. The implemented drift
is `a * (theta - r)` with constant theta; the name HullWhiteTree does not mean
this entry point fits an arbitrary initial term structure. It differs from
Convex's curve-fitted tree and our rule-based exercise on shifted LMM paths.

**Finstack Richard-Roll primitive:** connected directly to retained stochastic-rs
paths. At a 5% pool coupon, fully seasoned SMM was 0.7744%, 0.5143%, and 0.2615%
for market rates of 3%, 5%, and 7%. At-the-money SMM matched the independent
`1 - (1 - CPR)^(1/12)` conversion. A 512-path, 360-month survival calculation
took approximately **2.59ms** on four workers. Increasing base CPR from 6% to 8%
on the same paths reduced mean surviving fraction from **0.17159 to 0.09341**.
This is a prepayment-survival diagnostic, excluding scheduled amortization,
interest, fees, payment delays and valuation. It is not a full MBS benchmark.

**Settlement policy:** on a single 100-unit cashflow due today, Finstack's
`npv_amounts_with_curve` returns 0; stochastic-rs's `CashflowPricer` returns 100.
Both policies can be intentional. The adapter must specify payment cutoff
explicitly; otherwise a migration can change valuations on payment dates.

## Product and execution compatibility

| Requirement | Finding | Application consequence |
|---|---|---|
| Our shifted three-factor LMM, abcd volatility, supplied CRN | stochastic-rs's inspected LMM uses unshifted log-Euler, constant per-forward volatility and a full correlation matrix | Do not substitute it on the basis of the LMM name; preserve or port our model explicitly |
| Reuse shared paths across products | Low-level simulation outputs and Finstack prepayment trait compose successfully | Keep scenario generation outside product valuation |
| MBS A-matrix factorization | Finstack agency MC-OAS reruns `price_on_path` inside each solver objective, including behavioral projection | Reuse lower-level models; its high-level implementation would undo our cashflow-once optimization |
| User-supplied shared MBS paths | The inspected Finstack agency MC-OAS path/config functions are private/crate-private and generate paths internally | A public adapter/extension or application-owned batch pricer is required |
| Our prepayment specification | Finstack's exercised Richard-Roll uses arctangent incentive and prepayment-relative burnout; ours has incentive accumulation, CLTV/HPI and static loan multipliers | Treat adoption as a model change requiring calibration, not a transparent optimization |
| Callable exercise | OSS trees use continuation-value exercise; ours currently uses rate thresholds | A potential analytics upgrade, with separately validated economics |
| Non-maturity deposits / retail CDs | No matching implementation established for our beta/ECM, attrition and withdrawal models | Retain application-owned kernels; credit-default swaps are not certificates of deposit |
| Incremental dependencies | Finstack has market-factor selective repricing; Convex has a reactive graph | Source assessed only; neither graph was built/benchmarked here for assumption edits |
| Core strategic evaluation | Cached coefficient/vector operations remain the right boundary | Keep pricer calls out of synchronous Strategy Lab evaluation |

Pinned source evidence:
[stochastic-rs LMM](https://github.com/rust-dd/stochastic-rs/blob/2d6bcc7b42927b5b9777bf237ea5ca9ec102f1ac/stochastic-rs-stochastic/src/interest/lmm.rs),
[constant-theta tree](https://github.com/rust-dd/stochastic-rs/blob/2d6bcc7b42927b5b9777bf237ea5ca9ec102f1ac/stochastic-rs-quant/src/lattice/short_rate/hull_white.rs),
[Finstack agency MC-OAS](https://github.com/jeickmeier/finstack-quant/blob/19355232773b2c0daf81070d72a96171eebe0c23/finstack-quant/valuations/src/instruments/fixed_income/mbs_passthrough/metrics/mc_oas.rs),
[prepayment trait](https://github.com/jeickmeier/finstack-quant/blob/19355232773b2c0daf81070d72a96171eebe0c23/finstack-quant/models/src/credit/pool/prepayment/traits.rs),
[selective valuation](https://github.com/jeickmeier/finstack-quant/blob/19355232773b2c0daf81070d72a96171eebe0c23/finstack-quant/portfolio/src/valuation.rs).

## Incremental edit experiment and next implementation boundary

Changing one instrument's spread overlay by +25bp in a 10,000-name book:
full NumPy reprice **8.13ms**, selective one-instrument reprice plus cached-total
patch **4.84 microseconds**. The totals agree within the asserted tolerance.
This is an application-owned Python adapter with a known affected instrument;
it excludes dependency discovery, publishing, persistence and behavioral-flow
rebuilding. It is evidence for selective recomputation, not a measured Rust DAG
speedup. A prepayment edit has more downstream work than this spread-only edit.

Recommended implementation order:

1. Define immutable calculation inputs: model version, instrument terms,
   resolved assumptions, valuation date, market snapshot, scenario identity,
   seed/path count, numerical precision and base OAS calibration identity.
2. Invalidate at instrument granularity, then group affected instruments by
   product/model for execution. Reuse the existing shared paths and compact
   cashflow representations. Preserve fixed OAS across scenarios.
3. Put replaceable primitive adapters behind a batch interface accepting shared
   path buffers and explicit assumptions. First pilot: simulation components
   from stochastic-rs and prepayment components from Finstack, with model changes
   isolated from performance comparisons.
4. Optimize the existing discounting boundary with fused compiled loops before
   attributing benefits to a language migration. Benchmark realistic mixed books,
   dirty-set sizes and actual data-transfer costs.
5. Require independent product golden cases, scenario/CRN equivalence, explicit
   settlement conventions, missing-input errors and deterministic aggregation
   before any component is enabled in production.

Rust remains a viable long-term home for a typed calculation core. This experiment
supports choosing it for architectural/ownership benefits and measured kernels,
while retaining the current engine as a reference and avoiding wholesale adoption
of an unvalidated general-purpose pricing stack.
