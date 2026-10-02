# Why the native product port was slower

The regression is concentrated in the native mortgage and deposit cashflow calls.
The reference called “Python” already runs these loops as compiled, parallel Numba
code. This is a comparison of two native implementations, not Rust versus an
interpreted Python instrument/month loop. The existing Rust release build uses
optimization level 3 and thin LTO; an accidental debug build is not the cause.

## Measured attribution

Fresh processes, the same 6,000 synthetic instruments, 128 paths, 27-month income
horizon, four compute threads, warm compilation and empty 512 MiB dependency
caches. Timers wrap Numba calls, Rust FFI calls and the enclosing Python adapter.
No SQL, object storage, HTTP or solver is in this measured graph invocation.

| Work | Python/Numba seconds | Rust FFI seconds | Calls per backend |
|---|---:|---:|---:|
| Mortgage cashflows | 7.821 | 15.265 | 23 |
| Deposit cashflows | 3.279 | 6.752 | 23 |
| Rate paths | 0.307 | 0.458 | 23 |
| Corporate cashflows | 0.084 | 0.235 | 46 |
| CD cashflows | 0.012 | 0.028 | 23 |
| Entire graph | 18.512 | 28.783 | 1 |

Both evaluations computed **288,072 dependency nodes**. Mortgage and deposit
calls add 10.916 seconds, accounting for essentially all of the net 10.271-second
regression; some other native operations offset part of that difference.

All Rust product FFI calls total 22.995 seconds. Their enclosing adapters total
23.238 seconds: approximately **0.243 seconds** is outside the FFI in Python
conversion/validation/descriptor handling. Native FFI time includes native input
validation, calculation, allocations and output copying, so it is not a pure
arithmetic timer. Nevertheless, Python-to-Rust adapter overhead is too small to
explain the observed gap.

Raw data: [Python profile](2026-09-30-profile-python-6000.json),
[Rust profile](2026-09-30-profile-rust-6000.json).
Harness: [profile_native_products.py](../../scripts/profile_native_products.py).

## Controlled mortgage experiments

The same 1,920 mortgages, 128 paths, 360-month arrays and four threads; five timed
repeats after compilation/warm-up. Every variant's seven outputs matched the
reference at `rtol=1e-10, atol=1e-10`. Experimental native DLLs were built in
isolated `.data` directories; the application engine and installed native binary
were not changed.

| Variant | Median seconds |
|---|---:|
| Existing Numba, original input types | 0.315 |
| Numba with float64 inputs | 0.308 |
| Numba with strict outer-loop math | 0.313 |
| Numba with strict outer-loop math and bounds checking | 0.359 |
| Numba with strict math including scalar helpers | 0.322 |
| Existing Rust | 0.626 |
| Rust forcing the mortgage event helper to inline | 0.601 |
| Rust compiled for this host CPU | 0.625 |

Additional isolated tests removed View dimension checks, removed repeated integer
validation, reused scratch buffers, specialized base/stress modes at compile time,
and forced scalar math helpers to inline. None closed the gap; their results stayed
approximately 0.62–0.66 seconds against paired native baselines around 0.62–0.64.
Check-removal variants are diagnostic experiments on valid inputs, not deployable
fixes or a recommendation to remove validation.

These results **do not support blaming float64 conversion, fastmath alone, generic
CPU targeting, scratch-buffer allocation alone, or helper inlining alone**.
They also show why listing plausible overheads without testing them would give a
misleading explanation. The differing source structures still warrant inspection:
Numba's product-specific loops hoist instrument constants and operate directly on
typed arrays; the Rust port uses generic array descriptors and shared event
functions. Their individual instruction costs have not been established here.

Raw data: [Numba/native controls](2026-09-30-mortgage-ablation.json),
[native implementation experiments](2026-09-30-native-loop-ablations.json).
Reproduction scripts: [kernel controls](../../scripts/profile_native_kernel.py),
[isolated native builds](../../scripts/build_native_product_ablations.py).

## Conclusion and remaining uncertainty

The native port achieved the tested output parity but regressed its two dominant
compute kernels. It has not earned an end-to-end performance advantage. This is
an implementation-level performance problem, not evidence that an interpreted
Python loop outperformed Rust or that a language change guarantees acceleration.

The profile establishes **where** the regression occurs. The precise remaining
instruction-level cause is **not yet resolved** by the controlled experiments.
Further work should use native CPU sampling and generated-code inspection on the
month loops, then test targeted kernel changes against both parity and throughput
gates. A broader backend rewrite would not directly address this measured issue.

The earlier 6,000-instrument cache-reuse attempt was separately affected by cache
eviction in both backends. That does not explain the uncached native regression.
No application code or deployed backend default changed during this diagnosis.
