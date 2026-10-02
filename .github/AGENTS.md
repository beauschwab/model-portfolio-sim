# Production execution policy

Follow the root AGENTS.md and docs/rust-production-contract.md. Rust is the
required production financial runtime; Python calculation is deprecated and
retained only as an independent engine test reference. Never add Python, NumPy
or Numba execution options or silent fallbacks to production routes or UI.
Existing Python API/storage adapters are not a Rust service; preserve their
durable contracts until explicitly migrated. HiGHS stays C++.

## Architecture and interface maintenance

Current architecture: `../docs/architecture.md`. Interface/transport and dependency rules are in
`../docs/contracts/README.md`. Run `make check-architecture` from the root
for boundary/contract changes. Dated version sections document historical migration
stages; current production and shared-model contracts supersede their defaults.
Preserve independent reference tests and all applicable numerical/publication gates.
