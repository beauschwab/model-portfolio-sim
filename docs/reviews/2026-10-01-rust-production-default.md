# Rust default and Python calculation deprecation — 2026-10-01

Version **0.29.4** makes Rust the production default and removes Python calculation
as an application execution option. This is a calculation-backend deprecation;
the Python API, worker, storage adapters and independent verifier still exist.

## Implemented

- Rust defaults in API settings, `RunConfig` and context-free product dispatch.
  What-if calls without an explicit config also enter native coordination.
- New settings cannot select Python. Production run requests cannot select
  Python/NumPy/Numba reducers or the Python daily engine. Missing native capability
  disables execution; no automatic fallback remains in the partitioned UI.
- Historical Python settings and results remain readable. New jobs, workers and
  interactive libraries reject that backend until explicitly migrated/rebuilt.
  Immutable prior requests are not rewritten.
- Fixed a production ownership gap: `store.run_pricing` used to always pass a
  discount reducer, selecting Python graph coordination even with Rust products.
  Pricing and what-if now pass no override and use the complete native graph.
- Root/nested AGENTS, Copilot, canonical/packaged operator skill and references
  contain the current contract. Historical notes are explicitly superseded.
- `make build` and the root package build include native artifacts; CI jobs which
  previously built only the ledger now build the required product/workflow runtime.
- Existing numerical/parity tests explicitly retain their Python oracle. Fresh
  subprocess tests exercise actual unconfigured production defaults separately.

## Validation

Full API suite: **122 passed** in 106.00 seconds. Includes native coordinator
ownership checks, rejected backend choices, historical settings visibility,
legacy library rejection and worker failure without result publication.

Full engine suite: **735 passed, 2 skipped** in 305.66 seconds. The two skips are
the Python parameter cases of native-only ownership tests; their Rust cases run.
The fresh-process default tests execute outside the explicit Python reference
harness. Full browser acceptance: **13 passed** across the isolated settings,
tape/treasury and daily/partitioned ledger workflows, including missing-native
capability without fallback. Web production build passes.

Agent-context coverage, skill mirror equality, packaged skill integrity, updated
CI YAML syntax and build-script JSON checks pass. Historical comparison scripts
now explicitly select Python for their labelled reference/SciPy measurements;
their syntax was checked, without rerunning scale benchmarks. No financial
tolerance was relaxed. HiGHS remains C++.

## Deployment and remaining scope

Build/deploy matching native artifacts and source, restart API/workers, then select
Rust in any existing Python workspace and rebuild its libraries/sessions. Fresh or
backend-omitting settings use Rust. Existing Python-selected revisions remain
explicitly historical until a revisioned edit; old queued requests may first hit
`MODEL_VERSION_CHANGED`.

Validation used isolated local services and SQLite. CI configuration is updated;
live PostgreSQL/S3 deployment and hosted CI execution were not performed here.
No native financial formulas changed in this step. Web bundle-size and API
TestClient deprecation advisories are pre-existing.

See [the production contract](../rust-production-contract.md) for the authoritative
agent and operational policy. Replacing FastAPI, worker services, Python artifact
transport and cohort-audit orchestration is separate migration work.
