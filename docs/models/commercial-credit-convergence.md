# Constant-default credit sampling benchmark

This independent numerical experiment tests generated commercial credit cash
against an analytic expected value. Its predeclared family has face of 100, zero
coupons, zero discounting and OAS, no exercise, a single absorbing default,
LGD of 0.4 and immediate recovery of 60. Contractual principal settles on its actual
last principal cash date,35 days after source origin. The benchmark retains the
complete native original/current contracts and existing source/credit ownership.
Python supplies an explicit independent validation oracle; native Rust generates
and prices every path.

## Exact law and sample diagnostics

For constant default intensity h and actual principal horizon T, the cumulative
default probability is p=1-exp(-hT). A surviving path pays 100; a defaulted path
pays 60. Thus exact expected cash is 100-40p. The benchmark independently derives
h=-log(1-p)/T from the declared probability and checks both possible cash
outcomes, full path coverage and the native sample mean.

The native output reports a standard error from its actual antithetic clusters:
path i pairs with path i+N/2. Credit uniforms use separate counter keys per path,
so the two credit outcomes in a market antithetic cluster need not be opposite.
The benchmark recomputes the standard deviation of those N/2 pair means divided
by sqrt(N/2). An ideal independent-credit-path standard error,
40sqrt(p(1-p)/N), is recorded separately. Every full source and native generated
path is also replayed through the existing independent stochastic verifier.

For a zero-default sample, the sample standard error can be 0 while the supplied
law still has positive expected loss. The receipt retains the normal 95% interval
as a descriptive diagnostic. It also computes a two-sided Clopper-Pearson
binomial interval from the actual default count, including zero events, then
maps that interval to expected cash with 100-40p. These intervals use the
[original Clopper-Pearson construction](https://doi.org/10.1093/biomet/26.4.404)
through [SciPy's exact binomial interval](https://docs.scipy.org/doc/scipy/reference/generated/scipy.stats.binomtest.html).

Each of 24 cells receives alpha of 0.01/24. Under the ideal IID credit-uniform model,
the union bound controls simultaneous interval failure at at most 0.01; this does
not require different matrix cells to be independent. The actual fixed-seed
counter generator, nested sample sizes and reused seeds remain explicit.
The recorded coverage describes those selected runs; it establishes no observed
bank accuracy or general uncertainty guarantee for arbitrary credit models.

## Predeclared cases and immutable evidence

The complete matrix is the Cartesian product of p in
{0,0.000001,0.01,0.05,0.5,0.95}, N in {128,256}, and evaluation credit seeds
in{11,23}. Every declared cell remains in the receipt with its exact raw request,
request hash, executed native input/artifact identity, full output, independent
verification, sample statistics and either a pass or retained failure. Sampling
intervals are separate from unchanged financial replay tolerances. There is no
monotonic error requirement across sample sizes or seeds.

The standalone harness pins a supplied executable before and after execution,
plus all benchmark and engine Python reference sources and their dependency
configuration. It records the benchmark checkout commit, Python/SciPy versions
and every matrix result. Existing receipts cannot be overwritten.

```text
python packages/portfolio-risk/tests/commercial_credit_convergence_benchmark.py \
  --native <pinned-lifecycle-executable> \
  --output <ignored-evidence-directory>/matrix-receipt.json
```

The pytest controls accept the existing explicit
`PORTFOLIO_CONVERGENCE_NATIVE` selection for a contained matching executable.
Without it, the normal installed native artifact is required. The optional
`PORTFOLIO_CREDIT_CONVERGENCE_OUTPUT` retains the full fresh matrix receipt;
regular tests otherwise use their temporary output directory.

This finite default-cash family adds rare-event, expected-loss and sample-SE
checks to the existing continuation benchmark. Nonlinear/default-dependent
exercise, nonaccrual, EAD/draws, delayed or uncertain LGD/recovery, correlated
credit uncertainty, Physical policy transfer, observed holdouts and complete
saved/cohort/strategy/journal scale adoption retain their own acceptance gates.
No financial implementation, model selection, admission cap, final financial
tolerance, worker deployment or saved-library/session identity changes here.


## Contained checkpoint (2026-10-16)

All 24 declared cells and 34 controls pass. The exact model-based intervals contain
the supplied analytic law in 24/24 cells; descriptive normal 95% intervals contain
it in 18/24. Ten cells observe no defaults, including all four p=0.000001 cases.
Shared/nested streams make these coverage ratios dependent descriptive results.
The largest sample mean deviation is 1.5625 currency units and the largest sample
cluster SE is 1.69571048; those are sampling results at 128/256 paths, and establish
no deterministic two-bp price-accuracy bound. The unchanged native financial
replay and existing continuation-policy approximation gates remain separate.

The executable is the contained 100/80-stage lifecycle binary with SHA256
`87a1bcc469fe00a1d224e831433ad35d50d99a62f5e0f2d1b9aabb30e054f94c`.
The receipt pins 207 benchmark, reference and input files before and after the
run; its SHA256 is
`3a77ac7e6c009c47392f7a4f7a3ebfdd76f658e3b533caf8aeb70a8663c32bf4`.
Evidence is retained under `artifacts/commercial-credit-convergence/`.
An initial reference paired adjacent paths; the preserved failed attempt exposed
that mistake, and a hand control now verifies the native half-sample pairing.
No native financial code or tolerances changed during the repair. Architecture,
its 10 checker controls and native/HTTP/MCP contracts pass.
