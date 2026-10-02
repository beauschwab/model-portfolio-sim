# Rust production contract and Python calculation deprecation

Effective engine version: **0.29.4**. This supersedes earlier instructions to keep
Python as the default or to select Rust only after workload-specific measurement.

## Execution policy

- Rust is the default built-in financial runtime for `RunConfig`, context-free
  public engine dispatch, new API settings and settings missing the backend field.
- The application accepts only Rust for new calculation execution. Python, NumPy
  and Numba execution choices are removed from production requests and controls.
  The engine's explicit Python path remains an independent validation oracle.
- Production pricing and what-if use the native dependency graph and coordinator.
  Adapters must pass no custom discount-backend object: such an override previously
  selected the Python graph even when product computation was Rust.
- Implement new built-in product, simulation, dependency/coordinator, capital/FTP
  and strategy financial logic in Rust. Keep HiGHS in C++; Rust constructs its
  financial problem, invokes it and validates allocations.
- Missing or incompatible native artifacts fail explicitly. Never silently fall
  back to Python, weaken parity tolerances, or substitute a simplified calculation.
- FastAPI, the worker service, persistence, artifact transport, cohort audit
  coordination and independent verification still contain Python. This contract
  deprecates the selectable Python calculation backend; it does not claim those
  application services have been rewritten. Further service migration is separate.

## Existing workspaces

Explicit historical Python settings and immutable results remain readable.
Execution rejects them with `PYTHON_BACKEND_DEPRECATED`; change the workspace
setting to Rust through Settings or `PUT /settings`, then rebuild unit libraries
and decision sessions. This is a revisioned user edit, not a silent mutation of
saved snapshots. New requests attempting to save Python receive HTTP 422; jobs
submitted with historical Python workspace settings receive HTTP 409. The worker
also rejects Python snapshots independently of API admission. Old-build queued
jobs may first fail the existing `MODEL_VERSION_CHANGED` identity check.

Do not reuse Python-prepared interactive libraries. Synchronous evaluation remains
coefficient-only and explicitly requires a Rust library. Do not rewrite previous
job manifests or report results to give them a different backend identity.

## Build and deploy

Build the product, ledger and decision/workflow artifacts on the target platform:

```text
uv run --project apps/api python scripts/build_native.py
uv run --project apps/api python scripts/build_ledger_native.py
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py
```

`make build-native` provides these steps; `make build` also builds the web app.
Deploy matching source and artifacts, restart API and workers, and rebuild cached
libraries/sessions. Do not replace native libraries inside running processes.
Database portability (SQLite/PostgreSQL) and immutable local/S3 Parquet storage
remain unchanged. No live database rewrite is part of the default change.

## Agent and test requirements

The root and nested AGENTS files, GitHub Copilot instructions, engine operator
skill and its packaged references all carry this policy. Historical reviews retain
their dated findings; they are not current execution instructions.

Keep independent Python expectations explicit. The engine test harness enters an
explicit Python reference context; native ownership/parity cases explicitly enter
Rust. Pre-existing reference `RunConfig` instances name Python, so a changed default
cannot make a parity test compare Rust against itself. Fresh-process production
tests run outside that reference context and check defaults, native execution and
failure when required artifacts are absent. API tests forbid Python graph/coordinator
callbacks and verify rejected options, worker fencing and persisted configuration.

Reference Python is test support, not a supported production deployment option.
