# Optional Rust product and discount backends

Version 0.28.0 provides the built-in native financial runtime: raw calibration,
seeded shared random numbers, conventions/schedules, product/scenario pricing,
fixed-OAS dependency graphs, accounting/KPIs, unit pricing and public forecast
controllers. The decision crate links this runtime to C++ HiGHS and the daily
ledger. `portfolio-workflow` accepts raw books and executes the complete financial
pipeline without Python callbacks. Python remains the default selection and an
independent reference; selecting Rust never silently falls back.

Select `RunConfig(compute_backend='rust')` or save `compute_backend: "rust"`
in API settings. The UI exposes this as **Product simulation backend**.
This differs from the existing `backend` request option, which selects only
the prepared-cashflow discount reduction. Build the decision and ledger crates
too for the public optimizer and saved-book daily replay respectively.

`portfolio_quant_abi_version()` returns 8. `portfolio_quant_call` receives an
operation number and arrays of `{data, len, shape[3]}` descriptors. Buffers are
aligned contiguous f64; integer indexes must be exact nonnegative integers.
It borrows inputs synchronously, computes owned results, validates all outputs,
then publishes them atomically. Status 1 is invalid/model/nonconvergence, 2 is
nonfinite output, 3 is pool failure. Custom Python models fail explicitly.
Float32 checkpoints round exactly as the reference; converting them at this
boundary copies memory. This is not a zero-copy Arrow ABI. Panic containment
does not validate arbitrary foreign pointers; caller lifetime and non-aliasing
obligations still apply. Reduction order within each row is deterministic.

See [current coverage and measurements](../../docs/reviews/2026-09-30-native-product-coverage.md).

From the repository root:

```powershell
uv run --project apps/api python scripts/build_native.py
cargo test --locked --manifest-path packages/portfolio-risk-native/Cargo.toml
uv run --project apps/api python scripts/benchmark_comparison.py
```

Select `backend: "rust"` on `POST /run` with `kind: "pricing"` or `"whatif"`.
The adapter discovers the release library in this checkout, or uses
`PORTFOLIO_RISK_RUST_LIB`. An absent library or rejected input produces an error;
there is no silent fallback. Installed wheels need an explicit library path.

## Prepared-discount ABI v1 (separate from product ABI)

`portfolio_risk_price` borrows contiguous aligned int64 offsets (n+1), float64
times/values (m), decimal OAS (n), and exclusive float64 output (n). Values are
already market-discounted cashflow **path sums per unit original principal**.
Times are years. Price is sum(values * exp(-OAS * time)) / path_count. No dates,
product objects, RNG or callbacks cross this boundary. No pointer is retained.

Callers must guarantee buffer sizes, lifetime, immutability and non-aliasing.
Dimension/value checks cannot establish validity of arbitrary raw pointers.
The Python adapter uses immutable byte-backed inputs and fresh output, retains
owners across the synchronous call, and releases the GIL through ctypes.CDLL.
Statuses: 0 success, 1 invalid input, 2 nonfinite result, 3 panic/pool error.

The implementation preserves reduction order within each instrument and bounds
the cached Rayon pool to one current configuration. The API runs one job at a
time and uses the configured Numba thread count for native execution. The release
binary hash participates in dependency keys. Rebuild/restart after replacing a
loaded library, especially on Windows.

Rayon 1.11.0 is pinned and all transitive dependencies are locked. This package
is MIT-licensed; Rayon is MIT/Apache-2.0. No Convex, stochastic-rs or Finstack
dependency is introduced. Only Windows x86-64 was built/exercised in this review;
Linux/macOS discovery paths exist but are not validated release artifacts.

See `docs/reviews/2026-09-28-five-step-comparison.md` in the monorepo for actual
FFI, packing, graph, HTTP and memory results. Native kernel speed is not a claim
of end-to-end application speed.

`financial-controller-1` exposes raw saved-book base calibration, spread
application, published forecast compilation, anchored forecast replay, and
empirical joint-driver fitting/selection. See `controllers.rs` and
`forecast_input.rs` for typed request fields. Full evidence and model limits are
in `docs/reviews/2026-10-01-native-owned-workflow.md`.
