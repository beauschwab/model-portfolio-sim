# Optional ledger reduction pilot

## Full daily state and partitioned output (0.26)

The crate now also builds `portfolio-balance`, a separate-process daily state
engine. Use it through `analytics.balance_stream.run_streamed_balance_stress`;
its private stdin protocol expects Python-validated, fully defaulted typed inputs.
It does not replace the older C ABI reduction pilot described below.

The executable owns opening state, daily recoveries, collateral/funding claims,
position events, dated cashflows, derivative/netting loss and posted VM, policy
scheduling/execution, operating income, dividends, taxes, daily GL checks and
research limit calculations. Scenarios and reverse severities are sequential and
independent. Product Monte Carlo/pricing/calibration is outside this state loop.

Output batches contain at most 16,384 total rows. Journal columns use shared
string dictionary IDs and numeric arrays over a local pipe; other report tables
use bounded row batches. This is a columnar JSON protocol, not Arrow IPC or
zero-copy transport. Python writes immutable Parquet partitions, independently
replays saved lines in order, compares the closing GL, derives closing statements
and attribution, then atomically publishes a local manifest directory. Pipe
backpressure bounds queued output. The Rust journal retains GL balances, not all
journal lines. Dynamic recovery/claim state still grows with distinct events;
the native GL has a two-million-key limit.

```python
from portfolio_risk.analytics.balance_stream import run_streamed_balance_stress
manifest = run_streamed_balance_stress(
    specification, ".data/stress-runs", backend="rust", large_book=True)
```

`large_book=True` is an explicit engine-only tier of at most 60,000 positions and
90 million work units. Default/API admission remains 2,000 positions and three
million work units. The runner does not register a new API job, change the
production default, publish to S3 or implement an Iceberg catalog. Native failures,
timeouts, cancellation, corrupt partitions and failed replay prevent manifest
publication. Interrupted processes restart from inputs; checkpoint resume is not
implemented. The default native timeout is 30 minutes. Cancellation callbacks
can be polled from a watchdog thread and should be thread-safe.

Run `scripts/benchmark_streamed_balance.py` for the three-repeat accrual matrix
and `scripts/benchmark_mixed_state.py` for a mixed 60k daily book. Both include a
2 GiB combined process-tree RSS guard. They measure saved journal verification,
not just compute. See `docs/reviews/2026-09-29-native-state-streaming.md`.

## Earlier reduction-only ABI

This crate validates balanced transactions, applies their lines in supplied order,
and verifies daily GL checkpoints. It does not generate product cashflows, choose
policies, calculate regulatory limits or persist results. Python still owns those
operations and the label-to-integer mapping. This is a stateless batch kernel,
not a Rust-owned financial state machine.

```powershell
uv run --project apps/api python scripts/build_ledger_native.py
cargo test --locked --manifest-path packages/portfolio-ledger-native/Cargo.toml
cargo clippy --locked --manifest-path packages/portfolio-ledger-native/Cargo.toml --all-targets -- -D warnings
uv run --project apps/api python -m pytest packages/portfolio-risk/tests/test_ledger_native.py -q
uv run --project apps/api --with psutil python scripts/benchmark_ledger_backends.py
```

```python
from portfolio_risk.analytics.balance_stress import run_balance_stress
result = run_balance_stress(specification, journal_backend="rust")
```

The default remains `python` (original journal). `columnar` is a Python control
with the same compact buffers and output conversion as Rust. Neither pilot is
selectable through the API/UI. `execution` reports the backend and Rust binary
SHA-256. An explicit unavailable Rust request fails; it never falls back.

ABI version 1 accepts dense previous/expected balances and CSR transaction
offsets, GL-key indices and signed debit-positive amounts. One call processes a
day, including opening day; another replays the full journal from zero at close.
Empty days use offsets `[0]`. Key IDs map to (account, GL account, instrument)
inside a single scenario. Posting order is preserved; each transaction belongs
to exactly one entity/currency account. No native pointer survives a call.

The Python adapter copies inputs before releasing the GIL; this cost is included
in measurements. Rust stages changes privately and writes output only after
transaction and checkpoint validation. Errors leave the output buffer unchanged.
Status codes: 1 invalid input, 2 unbalanced transaction, 3 checkpoint mismatch,
4 nonfinite arithmetic, 5 caught panic. Callers must still honor buffer ownership,
alignment and lengths. Rust rejects more than 2 million GL keys or 20 million
lines per call; current engine request limits remain unchanged.

Transaction checks use cancellation-aware expansion summation and the reference
`1e-9 * max(1, sum(abs(lines)))` tolerance. GL balances use sequential floating
addition in exact event order. Daily checkpoints use relative `1e-9` / absolute
`1e-8`; closing replay additionally checks `1e-12` / `1e-9`. This is floating-point
simulation accounting, not a currency-rounded production GL.

Failed daily validation aborts the simulation before a result can be returned.
The journal buffer is retained on the local object for diagnostics, but is not
a recoverable publication log. Durable revision/attempt fencing remains the API
worker's responsibility; the pilot is not promoted into that path.

Parity tests compare every output frame against the original journal, then use
the original Python replay on native-produced lines. Production runs with the
pilot perform native daily validation and native closing replay, not an extra
Python replay. That validation-scope distinction matters when interpreting time.
