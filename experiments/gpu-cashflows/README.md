# Mortgage/deposit CUDA experiment

This isolated Rust DLL controls CUDA through pinned `cudarc` 0.19.10 and NVIDIA
NVRTC 12.9.86. It implements prepared-input base mortgage/deposit cashflows from
`portfolio-risk-native/src/quant.rs`. It is not linked into the production crates,
registered in API settings, or used by workers. No production financial kernel
or persistence contract is changed.

## Reproduce on Windows with the T1000

The NVIDIA driver and existing production Rust product DLL must be available.
NVRTC is supplied in an isolated uv environment; installing a CUDA toolkit or
changing the production Python dependency lock is unnecessary.

```powershell
cargo build --release --locked --manifest-path experiments/gpu-cashflows/Cargo.toml
uv run --project apps/api --with nvidia-cuda-nvrtc-cu12==12.9.86 python experiments/gpu-cashflows/validate.py
uv run --project apps/api --with nvidia-cuda-nvrtc-cu12==12.9.86 python experiments/gpu-cashflows/validate_public.py
uv run --project apps/api --with nvidia-cuda-nvrtc-cu12==12.9.86 python experiments/gpu-cashflows/benchmark.py --matrix-paths 128 512 2048 --matrix-threads 4 28 --positions 256 --repeats 5
cargo test --release --locked --manifest-path experiments/gpu-cashflows/Cargo.toml
cargo clippy --release --locked --manifest-path experiments/gpu-cashflows/Cargo.toml -- -D warnings
cargo fmt --check --manifest-path experiments/gpu-cashflows/Cargo.toml
```

Use thread counts available on the tested CPU. The local host is an i7-14700
with 28 logical processors and an NVIDIA T1000 8 GB, compute capability 7.5.
The prototype compiles explicitly for that capability. The harness currently
loads Windows DLLs and the pinned Windows NVRTC package.

## Numerical and memory contracts

* Scalar calculations and accumulators are `f64`; NVRTC disables FMA contraction,
  flush-to-zero and fast math. Existing Padé logistics, LUTs and spline equations
  are retained. The exact-logistic mortgage switch is tested separately.
* Rust generates the existing NumPy-compatible shared random tapes. The harness
  prepares all market scenarios with the same tapes and records their hashes.
* One CUDA block owns an instrument. Its lanes evolve independent paths with
  sequential monthly state. Three lanes sum current-month contributions in
  ascending path order, matching the production reduction order. No unordered
  floating-point atomics or tree reductions are used.
* Scratch is three current-month values per instrument/path. No instrument x
  path x time cashflow cube is created. Input upload uses time-major storage for
  coalesced reads. Returned values are instrument/month aggregates.
* Admission is at most 256 instruments, 2,048 paths, 360 months, 128 MiB of packed
  inputs, and 32 output horizon slots. These are component limits, not an aggregate
  process/GPU RSS cap; driver context, host arrays and transport copies are extra.
* Forward/checkpoint capture and restarted forward stress are explicitly rejected.
  Only operations 4 and 6 with `want_fwd=False` are supported; unsupported calls
  cannot silently fall back to another backend.
* Deadline and atomic cancellation checks execute between stages and bounded
  launches, before result publication. Cancellation waits for the current kernel;
  it does not preempt an individual CUDA kernel. The subprocess supervisor supplies
  a separate watchdog. Validation kills a child after an observed CUDA launch.
* Results are staged, checked for finiteness and copied to caller outputs only on
  success. Failure tests retain sentinel outputs and unchanged timing buffers.

## What the experiment measures

Each thread/path configuration executes in a fresh child process. Workloads are
synthetic demo mortgages and deposits, 256 instruments per product and 360 monthly
steps. CPU/GPU timing order alternates. Five complete prepared-array calls and
three five-scenario pricing/income pipelines are measured after warm-up.

CPU timings include the Python adapter and unchanged production Rust C ABI,
allocation and output copy. GPU call timings include the Python adapter, Rust
validation, transposition/packing, device allocation, upload, kernel execution,
download, and staged output copy. CUDA initialization/JIT is recorded separately.
CUDA-event timing of repeated resident kernels is also recorded; it must not be
reported as an end-to-end speedup.

The five scenarios are base, parallel -/+1 bp and parallel -/+200 bp. Base OAS and
mortgage accounting yield are calibrated once per backend and held across its
scenarios. Production Rust OAS/PV routines and frozen-yield income conventions
produce per-instrument dollar PV, parallel DV01, monthly NII and runoff. The
comparison tolerances are unchanged (`rtol=1e-7`, `atol=1e-5`). All base kernel
outputs also match both Rust and the independent Python kernels at `1e-10`.
Odd paths, another seed, exact sigmoid, admission, malformed inputs, deadlines,
repeat determinism, cancellation after launch and recovery are separately gated.
The separate public validation harness patches only its own process: CUDA
cashflows price all 38 mortgage and deposit spot-risk bumps through Python
reference orchestration and match the Rust public routes, including published
OAS in basis points, price, DV01, KRD, vega and monthly NII tables. Its 40 CUDA
calls per product include base and NII builds. This is a numerical validation
route, not a public GPU backend or a timed native GPU workflow.

Shared input preparation (calibration, Rust random tapes and market paths) is
recorded separately. The pricing/income stage excludes API/HTTP, full KRD/vega
sweeps, forward capture, daily ledger, Parquet writing and independent persisted
journal replay. This experiment does not establish complete-workflow speedup,
Monte Carlo convergence, production deployment, or real-book model validity.

Evidence and measured conclusions are in
`docs/reviews/2026-10-01-gpu-cashflows.md`, the adjacent benchmark JSON and
`2026-10-01-gpu-cashflows-validation.json`. Numeric results include source, binary
and random-tape identities. Reports are saved only after the comparison succeeds.
