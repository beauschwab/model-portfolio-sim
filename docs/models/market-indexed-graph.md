# Unapplied named-index incremental registry branch

This candidate adds an explicit `market-indexed-graph-1` owner to the **existing** Risk `incremental.rs` handle registry. It invokes the actual indexed-contract and vanilla-swap owners and caches their dependencies. It is not a renamed analytical response. Installed sources, native libraries, and official targets remain unchanged.

The frozen predecessor at `../market-indexed-graph-stage` remains unchanged. This successor removes its artifact-only OAS solver counter. Acceptance instrumentation which compiles the untouched legacy registry is confined to the copied target and is excluded from the prospective patch.

## Public entry and ownership

The outer protocol remains `graph-1`. New operations are explicit:

```json
{"schema":"graph-1","threads":2,"request":{"op":"create_market_indexed","max_bytes":8388608,"max_entries":1024,"request":{"schema":"market-indexed-graph-1","product":"vanilla_swap","source":"complete typed market-indexed-adapter-1 request"}}}
```

The illustrative `source` string above must be replaced by the complete typed object in `market-indexed-registry-1.json`. Creation returns `{handle,owner,initial}` after successful actual pricing. `price_market_indexed` requires that handle and a complete typed graph request. `info` and `clear` dispatch by owner; market info returns `{owner,cache}`. `drop` retains the historical idempotent behavior for both owners.

Legacy `create`, `price`, and `compare` retain their original request types and financial implementation. The registry stores `Legacy(Arc<Mutex<Cache>>)` or `MarketIndexed(Arc<Mutex<Graph>>)`. A market handle rejects legacy `price`/`compare`; a legacy handle rejects `price_market_indexed` with `GRAPH_OWNER_MISMATCH`. It never casts an actual-date request into legacy quarterly `Flow`.

Pricing is outside the global registry lock. Creation checks the common 128-handle limit before expensive work and again during registration. Failed admission, pricing, cancellation, and full-registry creation leave no registered new owner. New branch caller Control is captured before Rayon dispatch and scoped on the worker. A final scoped creation failure removes the unpublished handle. Existing branches retain their None behavior. The CLI retains its existing external watchdog scope; this patch does not invent a universal CLI deadline envelope.

## Exact originals and financial owners

The graph pins product/schema, complete known contract, date/reset/calendar/index binding and review policies, full immutable original curve source and supplied original OAS. Each warm request is admitted before lookup. Current market edits are allowed; changing original input requires rebuilding the handle (`BASE_ANCHOR_CHANGED`). Float signed zero participates in native serde-byte key identity. Hashes accelerate lookup; complete serialized key and parent equality decide a hit.

`market_indexed_adapter::requests` mechanically extracts only DTO construction from the existing case function. The original case still invokes the same actual owners in the same order. The graph invokes `indexed::build_path` for complete actual-date contractual cash and `swaps::value_swap` for clean swap value. Its indexed-loan reduction calls **`term_product_graph::pricing::price` with supplied original OAS**. It does not call `TermGraph::new`, `Graph::build` calibration, or `pricing::solve`. Swap spread adjustment uses the adapter's same cash/OAS convention. Graph counters report actual projection/cash/clean/spread/loan node computations and hits. The frozen predecessor's calibration counter evidence and this branch's actual call graph establish zero OAS refits; no global instrumentation is proposed for production.

Projection and collateral curves have separate parents. Collateral-only edits reuse cash and reprice marks. Index edits change cash; a source-revision change invalidates source-dependent nodes. Original and current/bumped results keep the original OAS. All active Arc request/results survive through risk even when cache insertion is refused or eviction occurs.

## Admission

Per graph: 1 MiB serialized typed input, at most 512 periods, 16 curves per market, and 4096 nodes per curve. Cache capacity is at most 16 MiB /4096 entries. Original and current full curve/date validation occurs before serialization or generation; source validation polls active Control between admitted groups and retains the deadline reason.

Let `a` be the existing adapter admission, `b` the existing bridge admission, and `r=max(serialized current graph request, serialized retained initial graph request)`. Each call charges:

* bytes: `4*a.retained_bytes + 3*b.retained_bytes + 16*r + configured_cache_bytes + 262144`, at most 128 MiB;
* work: `a.numerical_work + b.admitted_work + 64*r`, at most 60 million;
* serialized graph result: at most 32 MiB.

The maximum initial/current metadata term retains the original anchor budget even when later current inputs shrink. Cache bytes include retained nodes/identities; the formula additionally reserves pinned product results, raw originals and transient projection/risk arrays. Registry/parser/allocator/process overhead remains separate; this is not an aggregate RSS or 60,000-position acceptance claim. Old legacy cache limits are unchanged.

## Acceptance and patch boundaries

`new/tests/market_indexed_registry.rs` covers real dispatcher/registry ownership, creation admission/cancellation/deadline, warm/current validation, curve invalidation, clear/drop, original product/version/OAS including signed zero, and real-byte nested aliases/duplicates. A separate copied-target-only test executes the unchanged installed legacy registry and proposed registry side by side, comparing serialized create/info/price/compare/clear/drop outputs and unknown-handle errors. Its captured golden output is versionable; it does not add a second production pricer.

`run_reference.py` independently reconstructs contractual cash, forward projections, swap/loan PV and risk, exact raw originals, and admission for 30 complete real-registry results and 9 raw process failures. It uses the existing independent reference, never a native financial callback. `check_transport.py` exercises the proposed additive `NativeMarketIndexedGraph` with mocked library transport and verifies captured binary identity, close/drop and no extra callback. The acceptance sequence harness buffers each record through Value, so duplicate-byte admission is established by the direct native dispatcher tests, not that harness.

The prospective patch owns narrow changes to Risk `incremental.rs`, `term_protocol.rs`, module exports/DTO helper visibility; a new graph module/tests; additive pure transport in `core/graph_native.py`; an independent reference module; and documentation. It contains no solver instrumentation, acceptance-only cloned registry, installed artifact executable, API durable publication, or UI changes.

## Remaining adoption

This is a reviewable actual registry branch proposal, not installed production adoption. It supports supplied Pricing curves for an indexed lending asset and a vanilla payer/receiver swap, with deterministic paired curve bumps. `stochastic_crn`, `physical_accounting`, `production_adoption`, `saved_library_adoption`, and `capital_ftp_eligible` remain false. The deterministic pairing policy is explicit; it does not describe stochastic LMM or joint-driver dynamics.

Root would need to apply/rebuild/version this branch, add default native/process callback-forbidden gates, and separately bind raw books to complete typed sources before the generic `price_books` dispatcher can select it. Existing saved numeric rows cannot supply missing original curves/reset/calendar parents implicitly. Saved books, candidate/library/cohort reconstruction, decision dirty/source refresh and persisted independent verification must preserve those exact parents and original OAS; they remain open. Physical observed fixings, income/accounting, conditional stochastic histories, smiles/options, behavioral/credit coupling, empirical calibration, capital/FTP and combined scale acceptance are also outside this slice.
