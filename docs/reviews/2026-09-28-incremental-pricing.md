# Incremental pricing foundation — 2026-09-28

Implemented an application-owned pricing dependency graph and a numerical batch
boundary, using the existing product models. This is engine version 0.19.0.
No Rust or OSS pricer has been added to the production runtime. The preceding
[OSS comparison](2026-09-28-oss-primitives.md) remains the basis for selective
future adoption behind this boundary.

## Delivered behavior

The graph covers spot prices for MBS, loans, debt, deposits and CDs:

```text
base market / histories / seed / path count
  -> shared paths
  -> instrument cashflows (batched by product on cache miss)
  -> instrument base OAS (target price calibration)
  -> instrument PV (scenario cashflows + held OAS + spread override)
  -> notional-scaled values and selected-book totals
```

| Changed input | Recomputed work |
|---|---|
| One instrument spread override | That instrument's PV; totals |
| Notional | Scaling and totals; no numerical cache node |
| Target price | That instrument's base OAS and PV |
| Contract coupon or other cashflow terms | That instrument's cashflows, OAS and PV |
| Deposit segment assumptions | Cashflows/OAS/PV for instruments in that segment |
| CD withdrawal assumptions | CD cashflows/OAS/PV |
| Temporary curve/vol scenario | Scenario paths/cashflows/PV; base OAS retained |
| Saved base curve/vol, seed or relevant path count | Dependent paths/cashflows/calibration/PV |
| Row order or deletion | Output ordering and totals; surviving nodes reused |

Independent nodes are identified by SHA-256 content keys containing their explicit
inputs and parent identities. The graph is evaluated on demand by the pricing
request; it is not a background push scheduler. A cache hit does not depend on
the current global application revision. Queued older snapshots therefore cannot
overwrite numerical results for newer inputs. Job results carry their queued
revision, and the existing strategy-library publication rules remain separate.

Missing instrument nodes are computed together for their product, preserving
NumPy/Numba batch execution. Read-only cached arrays use immutable byte owners,
and the cache serializes nested resolution with a reentrant lock. The default
limits are 128 MiB of accounted retained node data and 50,000 nodes, with LRU
eviction. This is not a process RSS limit: temporary kernel buffers, in-flight
values, run-local caches, interpreter overhead and output frames are additional.

## Calibration policy and correctness fix

Saved contract, target-price, assumption and base-market edits establish a new
calibration baseline. Consequently, a saved coupon edit recalibrates its OAS to
the saved target price. Use a temporary scenario/spread override to study a mark
change while holding the original calibration fixed. General temporary
per-instrument behavioral overrides are not exposed by this first interface.

Named scenarios preserve unshocked base OAS. The scenario spread leg applies to
securities, matching the existing engine, and excludes deposits. An explicit
per-instrument deposit spread override is supported. The same CRN object for a
given path count feeds base/scenario path generation within the evaluation.
MBS uses n_paths_base for base and scenario spot prices; other products use
n_paths. Prepay constants/LUT changes still require process restart.

The initial parity tests found batch-dependent OAS stopping behavior: a row that
had converged could keep moving while another row converged. That made regrouping
instruments change the reported spread slightly. Both monthly and exact-time
solvers now freeze converged rows individually. Dedicated tests compare joint
and separate solves, and all existing pricing/product tests pass.

## API usage

Submit a normal background job, using actual position IDs from the selected book:

```json
{
  "kind": "pricing",
  "books": ["loans"],
  "spread_overrides_bp": {"loans": {"CML0000": 25}}
}
```

Overrides add basis points to base OAS (and any named scenario spread), without
editing the saved book. They are limited to +/-2000 bp and must name selected,
existing instruments. Other run kinds reject this field. Optional `scenario`
selects the existing named curve/vol/spread scenario. Omitting `books` selects
the five supported products. Explicit money-market scope is rejected.

Poll `/jobs/{id}` and fetch `/jobs/{id}/result` as the existing Arrow envelope.
Position frames report ID, base/applied OAS in bp, model price as percent of par,
notional and market value. Results include selected-book totals, input revision,
model/backend identities, cache occupancy and actual computed/reused nodes.
`scope_net_value` is MBS+loans minus debt+deposits+CDs within the selected scope;
it must not be presented as full balance-sheet EVE.

## Batch boundary and extension path

`core.batch.CashflowBatch` owns contiguous immutable CSR arrays: instrument
offsets, cashflow times in years, discounted cashflow path sums, and path count.
The `DiscountBackend` protocol takes those buffers plus one decimal OAS per
instrument and returns one finite per-unit PV. Backend identity participates in
the cache key. The current implementation is NumPy; a Rust/FFI implementation
can replace this component without changing API routes or product conventions.

This boundary prices prepared cashflows only. A complete OSS product-pricer
adapter would additionally need a validated cashflow-generation/model adapter;
the protocol does not claim arbitrary OSS engines are drop-in replacements.

## Measured validation

Run `uv run --project apps/api python scripts/benchmark_incremental.py`.
[Raw samples and graph counts](2026-09-28-incremental-pricing.json) are checked in.
Windows 11, Python 3.12.11, four Numba threads, 128 paths, seven warm samples:
375 synthetic positions (120 MBS, 90 loans, 25 debt, 100 deposits, 40 CDs).

| Operation | Median |
|---|---:|
| Unchanged pricing request | 6.73 ms |
| One instrument spread edit | 6.85 ms |
| One instrument coupon edit | 7.65 ms |
| Full graph rebuild | 178.25 ms |

The measured spread edit is about 26x faster than rebuilding this book. Timings
include input fingerprinting, dependency lookups, frames and totals; they exclude
HTTP, queue delay, Arrow serialization and JIT warmup. Every measured edit was
compared with a fresh full rebuild; maximum absolute output difference was zero.
This does not extrapolate to production book sizes or certify model accuracy.

Validation includes independent calibration against the existing engine,
an independent corporate scenario PV calculation, per-instrument invalidation,
notional scaling, product-assumption isolation, seed/path count changes, row
reordering/deletion, immutable bounded cache behavior and invalid backend output.
API tests cover real pricing jobs, validation, reuse and a book edit while an
older queued snapshot is being priced. The existing complete engine and API
suites also pass; see the final task response for current counts.

## Scope remaining

This foundation is exposed through the engine and API, not a new UI panel.
Risk/KRD/vega, stress, NII/KPIs, hedges/MM and strategy-library rebuilds do not
yet use this graph. The synchronous Strategy Lab endpoint stays free of engine
calls. A request still scans/fingerprints the selected book and reconstructs its
output/totals, so warm request overhead remains O(number of selected positions).
Graph nodes/cache are process-local and nonpersistent; multiworker persistence
and distributed scheduling are not implemented. Existing product approximations
and the synthetic nature of demo inputs are unchanged.
