# Full Rust calculation lifecycle: implementation and acceptance ledger

Status: **built-in execution migration complete and locally validated (0.28.0)**.
The [current report](2026-10-01-native-owned-workflow.md) records ownership, final
60,000-instrument measurements, regression gates and model boundaries. Historical
sections below describe earlier incomplete checkpoints. The objective is one Rust-owned calculation
lifecycle from financial inputs and assumption edits through scenario simulation,
strategy optimization and downstream balance-sheet results. Porting numerical
kernels alone does not satisfy it. Python may remain an outer API/data adapter and
an independent validation reference; production numerical orchestration must not
call back into Python.

The user explicitly confirmed that **HiGHS stays in C++**. Rust owns problem
construction, solver invocation and result validation; replacing the numerical LP
primitive with a Rust implementation is neither required nor planned.

## Acceptance requirements

| Required ownership | Current evidence | Completion evidence / retained model boundary |
|---|---|---|
| Typed input contracts, conventions and instrument schedules | **Rust raw mortgage, corporate/CD, deposit, hedge, accounting and unit contracts** | Rust raw daily defaults/validation, typed graph contracts, raw forecast period/unit compilation and saved-book calibration controller implemented |
| Curve, volatility and behavioral calibration | **Rust curve bootstrap, PCA, current-coupon/PS fits, bounded abcd volatility and joint deposit fits** | Broaden real-data validation; retain fitted-function and final-risk parity gates |
| Shared seeded random numbers and scenario scheduling | **Rust owns seeded CRN and built-in product risk/stress scheduling, shared market paths and forward programs** | **Rust cross-book edit/scenario graph added in 0.27.9** |
| Dependency graph and bounded immutable caches | **Rust cross-book dependency resolution, immutable instrument/market caches, complete parent identity, batched misses, bounded eviction and compact results** | Native decision coordinator calls the graph directly in 0.28.0 |
| All product simulation and pricing | **Rust raw mortgage, corporate/CD, deposit and hedge lifecycles; forward programs** | Graph integration and all existing raw built-in product drivers implemented |
| Risk, accounting and KPI aggregation | **Rust cross-book NII/accrual, conditional forecast paths, parallel risk and all current KPIs** | Native graph integration implemented; retain independent model validation |
| Unit-library pricing and assembly | **Rust raw template construction, unit pricing and coefficient preparation** | Native scenario/edit coordinator owns unit builds and selective template merges; approximations remain explicit |
| Strategy problem construction, solve and allocation validation | **Migrated public optimizer** to raw-library Rust entrypoint | Keep independent Python test oracle; solver primitive is still C++ HiGHS invoked from Rust |
| Interactive strategy evaluation and transactional edits | **Rust public coefficient evaluator and complete native repricing actor** | Rust owns full update/reprice/solve/publish loop; retain coefficient-only interactive latency |
| Daily ledger and candidate replay | **Rust daily state loop, saved-book/candidate mapping, posting replay and closing/consolidated/attribution reports** | Native raw spec normalization and unified streamed workflow implemented; retain persisted-artifact verification |
| End-to-end runtime boundary | `portfolio-workflow` runs raw graph/edit/unit/HiGHS/accounting/mapping/daily-ledger stages in one native process | Implemented with bounded native batches/schedules, deadline checkpoints and a transport watchdog; 60,000-instrument combined run and independent persisted replay completed |
| Compatibility and scale | Existing SQLite/PostgreSQL metadata and Parquet/S3 storage | Atomic publication/restart gates passed (99 API tests); complete native 60,000-instrument workflow and separate equivalent Python/Rust graph and full term-risk comparisons passed, with cold/warm/edited and RSS evidence |

Financial acceptance retains fixed base OAS, shared CRN, A-matrix factorization,
exact-time schedule behavior, accounting reconciliation, LCR/NSFR/EVE/CET1/funding
and commercial restrictions. Existing HTM restrictions live in dynamic replay;
category-aware strategy integration must not be silently replaced by a coefficient
proxy. Existing model approximations and unsupported saved hedge mappings remain
explicit until implemented and validated.

Acceptance gates cover native standalone execution from raw books/market/history/
config, complete small-book final-output and edit/invalidation parity, failure/
restart behavior, and measured 60,000-instrument performance/memory. The final
native workflow took 372.824 s versus 793.275 s before optimization, with 31.3%
lower native peak RSS and 15.1% lower process-tree peak RSS. All 10,497,266 journal
rows independently reconciled. The full-size native before/after comparison is
not a full-size Python daily-workflow speedup; see the report for exact scope.
These gates complete execution migration, not production model certification.

## First implementation: native strategy ownership

`portfolio-decision-native/src/library.rs` accepts `strategy-library-1`: raw
unit-grid metadata, unshifted income and opening balances, unit DV01s, template
weights, base KPIs and constraints. Rust constructs purchase-shifted allocation
tensors and monthly financial constraints, invokes HiGHS, replays the allocation,
checks financial/commercial limits and labels the validation scope. Python's
`optimize_balance_sheet` native branch only serializes inputs and returns results.
It does not call `_kpi_vectors`, `allocation_vectors`, SciPy or Python allocation
replay. Precomputed Python `vectors` caches are not sent or trusted.

The existing decision actor and this entrypoint share one native financial solver.
An omitted public asset cap is represented by `null`, not replaced with the actor's
default $100 million cap. Infeasible/unbounded outcomes do not claim validation.
The native replay remains coefficient-only; it does not imply daily-ledger or HTM
validation. Commercial row identifiers follow the native actor's indexed labels.

`portfolio-strategy` is a standalone executable accepting one bounded JSON request
on stdin and returning an `ok`/`error` envelope. It requires no Python interpreter.
Its inputs are still a **previously priced unit library**, not raw instruments;
the remaining rows of the acceptance table are not claimed complete.

Build: `uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py`.
Focused gates: `uv run --project apps/api python -m pytest packages/portfolio-risk/tests/test_native_strategy_owner.py -q`.
Full engine, Rust unit tests, formatting and clippy remain required gates.

## Native market context (0.27.2)

`portfolio-risk-native::market::MarketContext` now owns the complete path-building
stage from explicit par quotes, fitted parameters, factor loadings and a shared
draw tape: annual-leg curve bootstrap, quarterly forwards, abcd volatility table,
LMM, forward-volatility features, CC/PS/HPI recurrences, incentive lag and YoY HPI.
`ParallelShock` owns default-model forward parallel stress templates. Rust stages
call each other directly; Python no longer orchestrates these internal stages.
The typed Rust API is exercised without Python by a controlled deterministic run.

The Python `build_rate_paths`, `build_paths` and `shocked_paths` native branches
serialize a complete stage request and convert returned storage dtypes. Bootstrap
uses bounded bracketed bisection against the same annual par residual and root
bracket as the reference; random smooth curves and final outputs are parity-gated.
The product ABI is now version 2, requiring a rebuild and worker restart.

This does **not** complete calibration or global scenario orchestration: Python
still fits PCA/abcd/behavioral parameters, generates the NumPy-compatible CRN tape,
dispatches multi-scenario risk, and holds the bounded run-local market-result
cache. Recalibration remains an explicit caller choice. No process-global cache
or implicit reseeding/recalibration was introduced. Higher-level raw-book routing,
native cache ownership, unit-library pricing and final ledger reporting remain
open in the acceptance table. HiGHS stays C++ by explicit user instruction.

## Native linear calibration controllers (0.27.3)

`portfolio-risk-native::calibration` now owns current-coupon design construction,
SVD regression, adjustment-speed estimation and fit diagnostics; PS-spread AR(1)
fitting, clipped persistence and residual volatility; and exponential-correlation
PCA. Native adapters pass raw history columns and return fitted parameters. The
linear algebra primitive is pinned `nalgebra=0.34.2`; HiGHS remains unchanged.

Regression uses the reference minimum-norm SVD cutoff (machine epsilon times the
larger design dimension times the largest singular value), including deficient
rank. Factor orientation is explicit: first-row signs (-,-,+) preserve the
existing built-in factor/CRN pairing and avoid arbitrary eigensolver signs. Both
backends use this convention. Backend-specific factor caches prevent an earlier
Python call from silently supplying a native run's loadings. Product ABI 3
requires a rebuild and restart. These controllers have independent NumPy oracles
and tests that forbid Python linear-algebra calls during native execution.

Volatility and deposit nonlinear least-squares controllers, seeded CRN generation,
the overall setup/risk dependency graph, unit-library pricing and final reporting
remain Python-owned. This step does not satisfy the full lifecycle objective.


## Native nonlinear calibration (0.27.4)

Rust now owns bounded trust-region-reflective least squares for both abcd and
logistic-beta/asymmetric-ECM deposits. `calibration::fit_abcd` consumes raw quote
triples, curve/forward/loadings inputs and optional warm start; `fit_deposits`
consumes raw fed-funds/deposit-rate history pairs and owns both fitting stages.
Public Rust calls need no Python interpreter. The Python native branches make
one call per fit and only format outputs/diagnostics. Product ABI is now 4.

The dense controller preserves the reference's bounded TRF strategy, linear
loss, unit scaling, 1e-8 stopping tolerances and 100*n evaluation cap. Volatility
uses analytic derivatives; deposits retain bounded two-point differentiation.
The Rust controller returns an error on evaluation exhaustion, while the older
Python adapter returns SciPy's unchecked result. Neither path proves parameters
are identified. The native crate retains SciPy's BSD license attribution.

Ownership tests forbid Python least_squares and model callbacks. Independent
reference tests compare quote fits, warm starts, dynamic/equilibrium deposit
rates across seeded histories, invalid inputs and thread determinism. Existing
final risk/stress/NII/KPI/strategy/ledger tolerances are unchanged. Results are
recorded in `2026-09-30-rust-nonlinear-calibration-validation.json`.

Remaining lifecycle work includes seeded CRN generation, raw-book scheduling,
global scenario/dependency/cache orchestration, unit-library pricing and native
ledger mapping/report assembly. HiGHS stays C++. No new full-lifecycle performance
claim or 60,000-instrument result follows from this calibration migration.


## Native seeded random tapes (0.27.5)

`random::SharedDraws` owns integer-entropy SeedSequence, PCG64 XSL-RR and the
float64 Ziggurat normal generator. The Rust backend's public CRN adapter makes
one native request; no NumPy seed mixing or random generation runs there. Rate
antithetic ordering (including odd counts) and spread/HPI offsets +101/+202 are
preserved. Standalone Rust tests cover NumPy golden values, carry and admission.

The adapter and Rust both admit at most 1 GiB of retained random values, with
an additional caller output copy at the synchronous FFI boundary. This bounds
tapes, not total process memory. Seed serialization retains up to 32768 bits
without f64 integer rounding. The native crate includes source hashes and the
upstream NumPy/PCG/SeedSequence licenses. Product ABI is now 5.

[NumPy 2.4.6 source](https://github.com/numpy/numpy/tree/v2.4.6/numpy/random)
provided the compatibility algorithm and tables. Long seeded tapes match the
local NumPy reference exactly; the cross-platform gate permits 2e-15 absolute
roundoff for rare libm tail differences while rejecting any stream drift.
See `2026-09-30-rust-crn-validation.json` for test evidence.

**Full goal remains incomplete.** Global scenario scheduling, raw-book schedule
normalization, native dependency/cache ownership, unit-library pricing, native
ledger mapping/reporting, unified cancellation/deadline entrypoint and the full
60,000-instrument equivalence/performance gates remain required. HiGHS stays C++.


## Native mortgage risk and forward-stress lifecycles (0.27.6)

`MortgageRiskRequest::run` now owns the complete built-in mortgage spot-risk
calculation from raw numeric book fields, histories, par/vol quotes and explicit
immutable prepay tables. It normalizes static multipliers/HPI/payment delays,
fits models, creates base/sensitivity CRNs, solves base OAS once (or accepts fixed
OAS), revalues all curve/volatility scenarios and computes dollar risk columns.
The Python `run_risk` native branch does no financial calculation or orchestration.
MBS-style whole-loan rows use the same driver; other book lifecycles remain open.

`portfolio-mortgage-risk` accepts named `mortgage-risk-1` JSON fields defined in
`src/bin/portfolio-mortgage-risk.rs`, with a 64 MiB input cap and a typed result
or error envelope. It requires no Python interpreter. The request carries raw
book rows and model tables, not precomputed prices/calibrations. Base cashflows
are processed in 256-position chunks, and one scenario's paths are retained at
a time, with immutable market-stage retention bounded separately to 128 MiB.
Cross-book dependency/cancellation/deadline orchestration remains unimplemented.

A wider parity test found an existing hybrid/reference 5y KRD difference of
$0.0000953. Tightening the Python curve root from default 2e-12 to 2e-15 removed
its float32 path-boundary amplification (largest reproduced difference below
$0.0000001). No risk acceptance tolerance was relaxed. ABI 6 requires rebuild
and restart. The standalone schema rejects unknown fields and malformed books.

Validation is recorded in `2026-09-30-rust-mortgage-lifecycle-validation.json`.
The full goal is still incomplete: other product risk/stress drivers, unit-library pricing,
cross-book accounting/KPIs, dependency/cache ownership, ledger mapping/reporting,
unified runtime controls and full 60,000-instrument comparisons remain required.
HiGHS remains C++.

### Forward stress ownership and review corrections

The same raw-input contract now powers native `run_stress`: Rust owns base OAS,
forward checkpoints, horizon/shock scheduling, fixed-OAS revaluation, position
P&L, portfolio aggregates and the forward DV01 profile. `mortgage-stress-1` adds
explicit horizon and shock arrays to the standalone executable. No Python
financial callback or portfolio aggregation executes in this branch. State and
channel multiplier tables are explicit inputs, including user-added categories.

Stress checkpoints/cashflow buffers are admitted in chunks of at most 256 rows
against a 64 MiB scratch estimate, independently of total book size. Result
buffers have a 1 GiB admission ceiling checked before FFI allocation and in Rust.
These are component budgets, not total process memory limits: paths, inputs,
transport copies and formatted output use additional memory. Horizons must be
positive, increasing and inside the simulation; shocks must be finite and unique.
Existing forward valuation conventions and float32 checkpoint quantization remain.

The expanded regression suite rejects Python calibration, RNG, cashflow, shock
and aggregation callbacks during native execution. It covers fixed/solved OAS,
odd path counts, delayed payments, explicit original HPI, matured/zero-face rows,
chunk boundaries, optional DV01 profiles, standalone execution, thread invariance
and malformed/oversized request rejection. An impossible two-month price in a
new fixture was corrected; acceptance tolerances remain unchanged.

### Native market-stage cache and measured regression

Native mortgage stages now share a process-local immutable LRU cache, capped at
128 MiB of retained key/value accounting and 512 entries. It stores CC/PS/PCA/
curve/abcd calibrations and mortgage market paths, keyed by complete length-tagged
input bytes. Hash collisions cannot alias financial inputs. Only successful
stages publish; concurrent borrowers use immutable Arc values and eviction is
safe recomputation. Clear also prevents earlier in-flight work from repopulating
the cache. Book terms, prepay cashflows, target/OAS solves and final position
results are not cached. Price/notional/category edits therefore reuse market
stages but recompute instrument results. Seed, paths, market/history and relevant
model/grid changes alter stage identity. Component retention excludes active
borrowers and transport copies; it is not a total RSS cap. ABI operation 30 exposes
entries/bytes/hits/misses/evictions and optional clear for diagnostics/testing.
This is mortgage market-stage reuse, not the full cross-book dependency graph.

The first scoped benchmark found warm Rust recomputation slower than cached
Python. It is preserved in `2026-09-30-rust-mortgage-before-native-cache.json`.
The same reproducible workload is rerun after native stage caching; see
`2026-09-30-rust-mortgage-lifecycle-benchmark.json`. Neither is the complete
60,000-instrument mixed-book/strategy/daily-ledger benchmark.

## Native corporate and CD spot-risk lifecycles (0.27.7)

Rust now owns raw corporate/CD normalization, Gregorian date arithmetic,
calendar adjustment, day counts, coupon/amortization/exercise schedules, market
calibration, shared seeded draws, base OAS and fixed-OAS KRD/vega/DV01. Public
`run_corp_risk` and `run_cd_risk` make one raw-batch native call in Rust mode.
`CorpDeck` and `CDDeck` also use native normalization for their other consumers.
Python retains transport/column formatting and the independent reference path.

`portfolio-term-risk` runs without Python and accepts `term-risk-1` or
`term-deck-1` JSON: an envelope with `schema`, `threads` (1..256) and `request`.
Typed request fields are defined in `term_deck.rs` and `term_risk.rs`; dates are
Gregorian ordinals (0001-01-01 = 1). Missing/unknown fields and invalid contracts
fail explicitly. Native product ABI 7 requires rebuilding and restarting workers.
There is no silent Python fallback or callback into Python financial code.

Corporate/CD cashflow kernels write into preallocated CSR buffers through
disjoint row slices, avoiding per-instrument allocation and assembly copies
while preserving path reduction order. Risk uses 256-contract chunks.
Schedules admit at most
4096 periods per contract and 1,048,576 per constructed deck. The shared immutable
128 MiB market-stage cache now also retains rate-only paths keyed by complete
market/grid/seed inputs. Book edits recompute cashflows/OAS/risk; market edits
invalidate the affected stages. Failed requests return an error envelope; only
successful stages can enter the cache. Raw JSON transport is bounded to 64 MiB
input and 128 MiB output for the standalone/deck protocols. Python spot-risk
uses the same raw JSON input but receives contiguous numeric buffers, removing
bulk result JSON encoding/decoding. All outputs publish only after validation;
result buffers are admitted up to 1 GiB. Random/path budgets are separate. These
are component limits, not a total process RSS cap or zero-copy Arrow transport.

This preserves existing model approximations: adjusted accrual endpoints,
simplified US holiday rules, linear-principal "annuity", monthly exercise
mapping, rule-based exercise, CD withdrawal heuristics and terminal-grid clamping.
The reference calendar's year-scoped New Year observation behavior is retained;
explicit extra holidays can represent missing dates. Custom Python calendar
subclasses are rejected. Parity does not certify these as production conventions.

`test_native_term_owner.py` gates full schedule parity across all four day counts
and business-day conventions, holidays/stubs/leaps, raw-batch ownership, fixed
and solved OAS, odd paths/large seeds, withdrawal overrides, option-rich 257-row
chunk boundaries, empty decks, standalone/thread equivalence, malformed inputs,
warm reuse and curve/volatility/seed/path invalidation. Final risk tolerances
remain rtol=1e-7/atol=1e-5; warm/cold rebuild results are checked exactly.

Remaining lifecycle work includes deposit/hedge and other drivers, cross-book
accounting/KPIs, native unit-library pricing, the global dependency graph,
ledger mapping/reporting and unified cancellation/deadlines. Full mixed-book
60,000-instrument performance acceptance is still open. HiGHS stays in C++.

## Raw mixed-book lifecycles (0.27.8)

Rust owns deposit risk/stress, hedge risk, cross-book effective-yield accounting,
conditional forecast paths, parallel risk/KPIs, raw unit-library pricing/coefficient
preparation, forward programs, saved-book/candidate mapping and daily closing,
consolidated and attribution reports derived from replayed journal postings.
Product ABI 8 and ledger protocol 2 require rebuilding and restarting workers.
`portfolio-lifecycle` exposes bounded raw JSON requests without Python. HiGHS
remains C++; Rust owns its financial problem construction and allocation validation.

Adapters transport raw inputs and format tables. API interactive evaluation keeps
the library's selected backend and uses prepared coefficients; it never reprices.
Caller-supplied unprepared libraries also support native coefficient construction.

Both deposit fitters use analytic derivatives; zero-shock deposit stress is an exact
identity; conditional-forecast initial means accumulate in f64. Accounting yield
powers use a native recurrence. Final-output tolerances are unchanged. Existing
research regulatory weights, forward-unit approximations and monthly mapping remain.
Saved hedge-to-netting-set mapping is still explicitly unsupported.

Full migration remains open: the cross-book incremental graph, what-if/decision
repricing coordinator and raw daily-spec normalization are still Python-owned.
The native entrypoints remain separate. Unified deadline/cancellation and complete
graph/solver/ledger 60,000-position acceptance are still required. Independent
persisted-journal verification remains a publication gate. The migration ledger is
`docs/reviews/2026-09-30-rust-lifecycle-migration.md` at the monorepo root.

## Native workflow ownership (0.28.0)

Rust now owns the transactional decision repricing coordinator: dirty-row
selection, scenario dispatch, fixed-base-OAS repricing, KPI contribution deltas,
selective unit-template rebuilding/merging, HiGHS invocation and staged state.
Python retains raw table transport, independent coefficient replay and the API
publication guard. Constraint-only updates invoke no pricers. The complete
what-if baseline/revised graph and difference calculation also execute natively.

`portfolio-strategy` accepts `run_owned` with raw books/markets/histories and edit
steps. `portfolio-workflow` combines those stages with bounded accounting capture,
explicit saved-book/candidate mapping and daily journal/report execution in one
native process. `analytics.owned_workflow.run_owned_workflow` transports inputs,
writes immutable Parquet partitions, independently replays their postings, and
atomically publishes a manifest only after verification. It stores the immutable
raw request and its hash. The existing native saved-book route also uses this
coordinator; its materialized output remains limited to 250,000 cashflow rows.

The native streaming workflow captures at most 256 contracts per batch and retains
numeric schedules with dictionary position indices in a 128 MiB capacity budget.
It admits at most 60,000 backbook instruments plus generated candidates within
62,000 total ledger positions and 120 million daily work units. This is a separate
native generated-schedule tier; ordinary raw-spec API admission and the existing
60,000-position/90-million-work tier remain unchanged. No multi-million-row
cashflow JSON document crosses the runtime boundary.

Native cooperative deadlines run between stages/batches/days. The transport also
has a process watchdog for cancellation, deadlines, broken pipes and failed
writes. HiGHS retains its C++ implementation and bounded solve time. Component
cache/schedule limits do not constitute an aggregate RSS cap. Custom Python model
extensions remain explicitly separate from the built-in Rust path.

Model scope is unchanged: monthly capture uses 30-day reporting months, regulatory
weights are research assumptions, forward unit pricing remains approximate,
saved hedge-to-netting mapping is explicitly rejected, and HTM restrictions are
checked by daily replay rather than a category-aware unit-template LP. Offline
empirical-driver calibration and held-out diagnostics remain independent analysis
tools. See `docs/reviews/2026-10-01-native-owned-workflow.md` for acceptance evidence.


## Final public-controller and optimization audit (0.28.0)

The final audit also moved saved-book base calibration, spread application,
published forecast compilation, anchored forecast replay, and empirical joint
scenario fitting/selection into native controllers. These public application
paths no longer orchestrate financial stages in Python. Python backtesting and
persisted-journal verification remain independent checks.

The first complete 60,000-instrument workflow optimization reduced total time
from 793.275 to 373.183 seconds, and native peak RSS from 903.836 to 620.801 MiB.
Both runs emitted and independently replayed 10,497,266 journal rows. Their 510
retained financial summary/solver values agreed within 2.4e-10 maximum absolute
difference. These are before/after Rust workflow measurements, not a claim of
that speedup over Python. Subsequent flat OAS/PV output and small-grid market
reuse optimizations have separate final validation and comparison evidence in
[the current report](2026-10-01-native-owned-workflow.md).

The earlier chronological sections describe intermediate ownership gaps. Read
the current report and the acceptance table for present implementation status.
