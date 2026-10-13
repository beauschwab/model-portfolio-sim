# Transactional native aged preparation cache

`aged_units::cache::Cache` retains the canonical typed `AgedUnitsRequest` and
immutable native result. The public API is:

```rust
Cache::new(&AgedUnitsRequest) -> Result<Cache, String>
cache.update(&AgedUnitsRequest) -> Result<CacheWork, String>
cache.request() -> &AgedUnitsRequest
cache.snapshot() -> &AgedUnitsResult
```

`Cache` is `Clone`. Request, canonical identity bytes, shared identity bytes,
template pieces and combined result use immutable `Arc` buffers. Cloning copies
only the small template map and its labels; native metric/cash buffers are shared.
The decision actor can clone for staging, update the candidate, validate capital
dependencies and publish or abort without mutating its committed version. Cache
success itself is not actor publication, capital refresh or journal acceptance.

## Identity and selective preparation

Exact canonical serialization of the complete typed request is the unchanged
request identity. Repeating it returns `unchanged_request=true`, zero rebuilt
templates/units and zero pricing work. Revision labels alone never form a hit.
Canonical serialization is deterministic struct-field/input-array order, not a
lossy hash. Input domains and complete request dimensions are validated on entry.

A template piece key includes its complete typed terms, quote/original yield
anchors, weights, side/category/capital labels, and its ordered prepared and
validation purchases. Changed or new pieces prepare through the **same** native
`prepare_with_work` implementation as uncached preparation. This rebuilds all
purchase nodes and validation diagnostics for that template; it does not scale
old DV01, interpolate new risk, or replace coupon/principal cash with income.
Unchanged pieces share retained native values and events. Removed templates
require no pricing. Changing only global input/grid revision labels regenerates
current result/provenance metadata without repricing; merged contract source
revisions use the current immutable request revision.

Shared schema, model revision, CRN, valuation convention, curve-as-of/currency,
reporting grid, base/scenario ordering, complete curve content and source
identity, bump, or interpolation tolerance changes rebuild all templates. A
calendar, business-day adjustment or accrual-date policy change on an existing
template conservatively invalidates all pieces. A newly supplied template is
prepared under its own explicitly supplied calendar. Unsupported product
semantics remain rejected by the fixed-contract producer.

The merged result follows canonical **request purchase order** and scenario
order, even when template order or cross-template purchase order changes. It
copies exact dense `[S,N,10,H]` native slices. Validation diagnostics follow the
request's validation-purchase order, then scenario/metric order. Native cash
events are reindexed to global units and compiled through the shared funding
producer into a newly sorted **full-tail** union of reporting/purchase/settlement
dates. Stable same-day source order, side signs and source identities are retained.
No pricer runs during merging or coefficient-only consumption.

## Bounds, atomicity and work evidence

The existing one-million dense-value, full-tail funding and event bounds still
apply. The eight-million native pricing/replay/root work budget is checked
across **all** retained/prepared pieces, not restarted for every template.
Dirty preparation receives the remaining admitted budget before financial work.
`CacheWork.pricing_work` reports work actually rebuilt, while total retained
piece work controls admission. `units_rebuilt` counts prepared purchase nodes;
validation-node work is included in `pricing_work`.

Retained admission is 128 MiB, and logical old-plus-next staging admission is
256 MiB. Charging includes canonical identity capacities, four times serialized
request/result bytes as a conservative metadata/native-container charge, and
additional numeric-buffer, event, unit and opening-state capacities. Serialization
uses a bounded counting writer before allocating identity bytes. Template pieces
currently retain their result metadata, including repeated market descriptors;
those copies are charged and can cause admission to reject a request that an
uncached single result accepts. This favors explicit rejection over hidden
unbounded cache growth. Old and staged-clone buffers share `Arc` allocations;
staging charge conservatively counts logically retained pieces again.

These are component admission limits, not a global process/RSS guarantee.
Caller raw inputs, transport, temporary projection/serialization buffers,
solver state, other native caches and concurrently retained actor sessions are
additional. Actor/session count and aggregate runtime admission remain their
own contracts.

Updates build a candidate, checkpoint, then replace the committed cache once.
Invalid input, work/byte admission, projection/pricing failure and cooperative
cancellation return an error without changing its canonical request or result.
Immutable snapshots cannot mutate the cache's shared financial buffers.

Eleven focused Rust fixtures compare complete serialized cached results with
fresh `prepare` **exactly**, including quote/yield/weight edits, native contract
tail changes, purchase/template/scenario ordering, validation-diagnostic ordering,
input revision rebasing, shared-market/model/CRN/calendar invalidation, removals,
aggregate work and byte admission, failed updates and staged clone isolation.
Cloned request/snapshot pointers prove buffer sharing. All eleven and the
existing 23 aged analytical fixtures pass; strict all-target Clippy passes.
The 15 public aged-strategy tests subsequently pass against rebuilt native
artifacts, including coefficient-only execution and missing-input rejection.
Public actor and durable publication acceptance remain separate tests. Full
issue #23 scope in [aged-units.md](aged-units.md) remains.
