# Architecture

Rates Workbench is built around one unusual premise: a bank balance sheet
can be made interactive without turning every user gesture into a new
distributed pricing run. The system does that by separating expensive
path-dependent valuation from cheap portfolio recomposition and by
preserving quant invariants all the way through the API and UI.

The result is not a generic compute grid wrapped in a dashboard. It is a
specialized rates and balance-sheet engine with a few deliberate
contracts:

- One common rate-path universe per run.
- Base OAS solved once and held fixed for scenarios.
- Product engines emit reusable cashflow factors and accounting vectors.
- Long jobs are coarse-grained backend runs.
- Interactive controls operate on prebuilt unit tensors and cached KPI
  components.

## What Makes It Unique

Most risk systems start from a product pricer. They ask the pricer the
same question many times: price this bond, price it again under a curve
bump, price it again under a vol bump, price it again at a forward
horizon, then repeat for every position and every scenario. That is a
natural architecture for a library of independent instruments, but it is
not ideal for a balance sheet whose numbers only make sense when products
share the same rate paths, runoff, funding, accounting, and regulatory
constraints.

Rates Workbench starts from the balance sheet instead. MBS, loans,
deposits, CDs, money-market lines, and hedge overlays are valued on the
same simulated rate state. That makes EVE, NII, liquidity ratios, capital
projection, hedge effects, and strategy deltas internally consistent.
The system is not just collecting product greeks; it is producing a
coherent balance-sheet state.

The second distinctive choice is the A-matrix factorization. Hot kernels
generate path cashflow factors once. OAS is applied later through
discounting. For mortgage-style monthly products this is an `A[s,t]`
matrix; for schedule-driven products it is a CSR-like exact-time vector.
That separation matters because cashflow behavior is expensive while
discounting is cheap. Once the cashflow factors exist, the engine can
solve OAS, reprice fixed-spread scenarios, and reduce path means without
rerunning behavioral logic for every spread trial.

The third choice is treating interactivity as a first-class quant
requirement. The Strategy Lab does not call the engine on every slider
move. It first builds a unit library: hypothetical new production across
MBS, commercial loans, CDs, deposits, and other templates is priced
through the same live engines, then stored as per-unit NII, runoff,
balance, DV01, liquidity, funding, and capital contributions. A strategy
edit becomes a time-shifted dot product against that tensor plus a
closed-form KPI recalculation. That is why `POST /strategy/eval` can stay
synchronous and sub-ms while still reflecting the expensive behavioral
models.

Finally, the engine deliberately keeps simplifications visible. Synthetic
demo data, stylized regulatory weights, rule-based exercise, prepay and
attrition anchors, and frozen-vol approximations are documented seams.
That is an architectural decision too: the UI and API do not hide model
status behind polished output.

## Why It Works

The architecture works because it aligns the computation with the real
dependencies in the model.

The most expensive dependency is the path state: simulated discount
factors, short rates, par swap rates, mortgage current coupon, HPI,
primary-secondary spread, deposit-rate recursion, and behavioral state
such as balances and burnout. If each instrument owned its own paths,
portfolio risk would be noisy and inconsistent. Rates Workbench instead
creates one common random number object for a run and reuses it across
base, bumps, and shocks. Central differences become differences of means
under shared draws, which sharply reduces Monte Carlo noise in KRDs,
vegas, EVE, and stress P&L.

The second key dependency is base spread calibration. OAS is a way to
explain today's price; it is not supposed to be re-solved every time the
market moves in a risk scenario. Holding base OAS fixed prevents scenario
P&L from being absorbed into a new spread solve. That makes the numbers
more interpretable: a shocked P&L is the effect of rates, vol, spread leg,
and behavioral response at the same solved base spread.

The third dependency is behavioral cashflow generation. Prepayment,
deposit attrition, withdrawal, exercise, and forward runoff are not
simple scalar adjustments. They are pathwise processes. The engine keeps
those processes in numba kernels, stores only the reduced factors needed
for pricing and accounting, and avoids materializing massive
path-by-position-by-month tensors where possible.

The fourth dependency is user latency. Full risk, stress, NII, and unit
library builds are honest jobs with progress telemetry. They run on one
backend worker thread because numba kernels already use parallel regions
internally and can saturate cores. The interactive path is separate and
must not call the engine. This split keeps the API predictable: long
operations are polled jobs; live controls operate on cached structures.

## Traditional Distributed Pricing Comparison

A traditional distributed system often looks like this:

1. A scheduler creates one task per position, scenario, bump, or horizon.
2. Workers load market data and position data.
3. Each worker runs an instrument pricer.
4. Results are written to a store.
5. A reducer aggregates PVs, greeks, stress losses, and reports.

That design is flexible and familiar. It handles heterogeneous product
libraries and very large books by adding workers. It is also expensive in
coordination overhead. For Monte Carlo rates risk, it has several
specific problems:

- Common random numbers are harder to guarantee across workers and
  product types.
- Repeated worker startup, serialization, and market-data hydration can
  dominate smaller runs.
- OAS solve loops can repeatedly regenerate cashflows unless the pricer
  has its own factorization layer.
- Product-level aggregation often happens before balance-sheet KPIs, so
  NII, runoff, LCR, NSFR, CET1, and hedging effects become downstream
  reconciliations rather than native outputs.
- Interactive analysis usually becomes a queueing problem: every slider
  change either waits for a distributed run or uses a separate simplified
  approximation.

Rates Workbench chooses fewer, larger units of work. A risk run builds
shared path sets and prices the relevant books in product batches. The
v0.15 MBS risk path goes further by stacking all KRD and vega path sets
into one scenario axis and launching one batched kernel rather than
thirty-eight separate launches. That is a different optimization target:
minimize orchestration and kernel-launch overhead first, then use
parallel loops inside the compiled kernel.

The comparison is not that distributed systems are bad. A distributed
grid is still appropriate when the book is too large for one host, when
independent desks must run isolated pricers, or when overnight regulatory
production has strict batch scheduling requirements. The difference is
that Rates Workbench is designed for ALM exploration where consistency,
latency, and explainability matter as much as raw throughput. It avoids
using a cluster to compensate for avoidable repeated work.

## Scalability

The engine scales along four axes: vectorized product batches, compiled
parallel kernels, cached factorization, and coarse-grained job
orchestration.

**Vectorized product batches.** Product drivers pass whole books into
engines instead of pricing one position at a time in Python. The kernels
loop over positions, paths, and months in compiled code. That keeps
Python overhead out of the hot path.

**Compiled parallel kernels.** The heavy work is numba-compiled with
parallel loops. The API exposes `n_threads`, where zero means all
available cores. Because each job can saturate local CPU resources, the
backend uses a single worker thread rather than starting multiple
competing runs in the same process.

**Common path reuse.** A single path build feeds multiple products and
scenario revaluations. Product-specific shocks reuse the same random
draws. For MBS risk, bumped path sets can be stacked and reduced in one
kernel launch. The same design can extend to additional products when a
new product can emit the same monthly `A` or exact-time `Acsr` pricing
surface.

**A-matrix and CSR factorization.** Pricing work scales with the smaller
discounting problem after cashflows are generated. This is why OAS solve,
fixed-OAS scenario repricing, and exact-time schedule products can share
pricing functions instead of embedding spread loops inside every product
kernel.

**Unit library caching.** The Strategy Lab moves the scalability problem
from "price every edited portfolio" to "price one representative unit
tensor, then linearly combine it." That makes strategy evaluation scale
with the number of templates and purchase months, not with the number of
raw instruments in every hypothetical allocation.

**Repository-shaped API state.** The current backend is in-memory, but it
is intentionally shaped as functions over dictionaries: books, market,
scenarios, jobs, settings, programs, and cached unit libraries. That
shape can be backed by Postgres, Redis, object storage, or a work queue
without changing the route contract.

The practical scaling path is incremental:

1. Add products by making them emit the same pricing and accounting
   surfaces.
2. Batch more scenario axes into product kernels where repeated launches
   dominate runtime.
3. Persist books, market snapshots, run inputs, and outputs behind the
   existing store surface.
4. Move job execution from the local single worker to an external queue
   only when one host is no longer enough.
5. Preserve CRN seeds, base OAS, and model-version metadata so distributed
   workers reproduce the same run semantics.

In other words, the system can distribute later without becoming a
different model. The core scalability contract is not "run everything
everywhere"; it is "do the expensive pathwise work once, preserve the
state needed for consistent portfolio math, and make every downstream
operation as small as possible."

## Architectural Tradeoffs

This design has tradeoffs. It is less plug-and-play than a generic
instrument-pricing farm because products must conform to the engine's
pricing surfaces and invariants. The numba kernels are fast, but duplicated
model blocks require strict tests. The in-memory API store is convenient
for the workbench but must be replaced for multi-user production. The
unit library is linear by design, so it is excellent for rapid strategy
exploration but not a substitute for a final full revaluation of a very
large nonlinear balance-sheet change.

Those tradeoffs are deliberate. The architecture optimizes for a treasury
workflow where users need coherent balance-sheet numbers, visible model
assumptions, and interactive strategy feedback before deciding which
full run is worth paying for.
