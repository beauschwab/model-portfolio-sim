# Bounded mortgage/deposit CUDA experiment

## Decision

The T1000 can accelerate sufficiently large mortgage/deposit batches relative to
four Rust CPU threads. It does not provide a general improvement over this
machine's full 28-thread CPU configuration. Keep the prototype isolated and
retain the production CPU backend. A later experiment should target an explicit
CPU-constrained worker or representative scenario workload before promotion.

The GPU is competitive for some scenario mixes: the repeated five-scenario
mortgage stage at 512 paths was 1.29x faster than 28 CPU threads. Base-only
cashflow calls at every tested path count were slower than 28 CPU threads, and
deposits did not improve in that configuration. These results do not establish
complete-workflow acceleration or a benefit on another GPU.

## Workload and implementation

Hardware: Intel Core i7-14700, 20 physical/28 logical cores; NVIDIA T1000 8 GB,
compute capability 7.5, NVIDIA driver 596.51; Windows 11. This is an actual local
CUDA execution, using NVRTC 12.9.86 and pinned cudarc 0.19.10.

Synthetic demo books: 256 mortgages and 256 deposits, 360 monthly steps,
128/512/2,048 paths, seed 7. Each path/thread configuration ran in a fresh
process, with five warmed base-call samples and three five-scenario pipeline
samples. CPU/GPU timing order alternated. A separate 32-position probe measured
the small-batch behavior.

The new Rust DLL in `experiments/gpu-cashflows` invokes a CUDA port of the
existing base mortgage/deposit kernels. Each block owns one instrument, evolves
its paths in parallel, and sums each current-month metric in ascending path
order. Arithmetic is f64; FMA contraction and fast math are disabled. Existing
logistic/LUT/spline logic is retained. No path x instrument x time cashflow cube
is created. Numeric device allocations were at most **36.68 MiB** in this matrix;
this excludes the driver/context and does not represent peak aggregate VRAM/RSS.

Packed inputs are bounded to 128 MiB, batches to 256 instruments and paths to
2,048. Forward/checkpoint capture is explicitly unsupported. Production crates,
API settings, workers, persistent backend identities and financial tolerances
were not changed. Runtime compilation is supplied through an isolated uv
environment rather than a system CUDA installation.

## Base cashflow call timings

Milliseconds below include Python/Rust adapters, validation, allocations and
output copying; GPU time additionally includes transposition, packing, upload
and download. Initialization/JIT is recorded separately. A speedup above 1
means the GPU won. Values are medians of five warmed calls.

| CPU threads | Paths | Mortgage CPU ms | Mortgage GPU ms | GPU speedup | Deposit CPU ms | Deposit GPU ms | GPU speedup |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 4 | 128 | 80.91 | 74.31 | 1.09x | 104.44 | 51.65 | 2.02x |
| 4 | 512 | 566.75 | 214.56 | 2.64x | 458.14 | 185.64 | 2.47x |
| 4 | 2,048 | 2,233.30 | 1,240.13 | 1.80x | 1,438.00 | 1,084.96 | 1.33x |
| 28 | 128 | 27.51 | 75.72 | 0.36x | 43.23 | 49.81 | 0.87x |
| 28 | 512 | 121.02 | 214.73 | 0.56x | 133.38 | 186.30 | 0.72x |
| 28 | 2,048 | 832.18 | 930.96 | 0.89x | 559.18 | 817.56 | 0.68x |

The 32-instrument, 128-path follow-up was slower on the GPU: mortgage call
speedup **0.51x**, deposit **0.63x**, against four CPU threads. Batch occupancy
matters; this experiment does not establish an automatic dispatch threshold.

## Prepared scenario pricing/income stage

The five scenarios are base, parallel -/+1 bp and parallel -/+200 bp. Each
backend calibrates base OAS once and holds it across revaluation; mortgage
accounting yields are also frozen. Production Rust OAS/PV and existing income
conventions produce dollar PV, parallel DV01, monthly NII and runoff.

| CPU threads | Paths | Mortgage CPU ms | Mortgage GPU ms | GPU speedup | Deposit CPU ms | Deposit GPU ms | GPU speedup |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 4 | 128 | 815.73 | 303.10 | 2.69x | 553.36 | 259.66 | 2.13x |
| 4 | 512 | 2,962.48 | 1,333.51 | 2.22x | 2,301.22 | 1,186.44 | 1.94x |
| 4 | 2,048 | 11,141.67 | 4,884.04 | 2.28x | 7,103.77 | 4,369.43 | 1.63x |
| 28 | 128 | 354.80 | 284.43 | 1.25x | 228.51 | 255.03 | 0.90x |
| 28 | 512 | 1,404.06 | 1,087.92 | 1.29x | 664.66 | 940.56 | 0.71x |
| 28 | 2,048 | 4,290.09 | 4,926.92 | 0.87x | 2,784.59 | 4,377.67 | 0.64x |

These stage timings exclude shared calibration/market-path preparation, full
KRD/vega sweeps, API/HTTP, daily ledger, Parquet writing and persisted replay.
They are medians of three samples, not statistical cross-machine guarantees.
There was material timing variation: the 28-thread/128-path mortgage CPU stage
ranged from 149.43 to 363.04 ms; its GPU samples ranged from 278.77 to 287.17 ms.
Do not infer a reliable improvement from that 1.25x median. At 512 paths all
three measured GPU mortgage stage samples were below all three CPU samples,
but production-sized and real-book acceptance remains untested.

Resident CUDA-event measurements isolate kernel execution on retained buffers;
they exclude transfer and allocation costs and are not end-to-end speedups.
The JSON retains both these values and full-call timings. Large-book repeated
batch scheduling with persistent market buffers is a future experiment, not a
measured feature of this prototype.

## Numerical and failure validation

* All 12 product/thread/path combinations match all Rust base kernel outputs
  and the independent Python kernels at rtol=1e-10/atol=1e-10. Maximum base
  Rust/CUDA cashflow difference: **2.90e-11**.
* Per-instrument PV, parallel DV01, monthly NII, runoff and base OAS comparisons
  pass the existing combined rtol=1e-7/atol=1e-5 gates. Maximum dollar difference
  in the timed matrix: **2.24e-7**. Base/scenario use shared native-generated
  random tapes whose hashes are retained.
* The separate public validation runs 16 positions per product at 128 paths,
  with 40 CUDA calls per product. It compares all public spot-risk columns,
  including OAS basis points, price, DV01, all KRDs and vegas, plus monthly NII,
  summary and book-yield tables against the Rust routes. **71 numeric columns /
  1,189 values pass** the same combined final-output tolerances. Its maximum
  absolute difference is **1.32e-5**, accepted by the relative-plus-absolute
  tolerance. Python reference orchestration is used only by this validation
  harness; it does not prove a native GPU public controller or worker route.
* **20 device checks pass**: odd paths, seed 19, exact sigmoid, exact repeat
  determinism, invalid operation/shape/index, admission, cancellation and recovery.
  Benchmark-side checks additionally gate nonfinite input, unsupported forward
  capture and expired deadlines with unchanged sentinel output/timing buffers.
* Cancellation is observed after an actual kernel launch and discards output.
  A separate child is forcibly terminated after an actual launch, with no result
  published. Cooperative cancellation waits for the current bounded kernel; it
  does not preempt a running CUDA kernel.
* One Rust unit test, strict release clippy and formatting checks pass. Six
  existing production native product regressions pass, covering mortgage output,
  deposit/corporate/CD cashflows and OAS roundtrip/failure.

## Evidence and reproduction

* [Main matrix](2026-10-01-gpu-cashflows.json)
* [Small-batch probe](2026-10-01-gpu-cashflows-smoke.json)
* [Device/cancellation validation](2026-10-01-gpu-cashflows-validation.json)
* [Public financial-output validation](2026-10-01-gpu-cashflows-public-validation.json)
* [Prototype and reproduction commands](../../experiments/gpu-cashflows/README.md)

Source, native binary and shared-random-tape hashes are retained. The main matrix
and validation use the same GPU DLL. No financial artifacts, production
manifest, live-book result or backend deployment is claimed.
