# Pinned OSS pricing comparison

This is an isolated experiment. It adds no Rust dependency to the API, web app,
or production quant engine. Sources and build products live in the system
temporary directory. The selected upstream trees are verified clean at exact
revisions before execution; the harness does not patch them.

Run from the repository root:

```powershell
# Once, if this toolchain is not installed. Does not change the default.
rustup toolchain install 1.95.0 --profile minimal
uv run --project apps/api python scripts/oss_primitives/compare.py
```

Windows requires the MSVC linker/build tools. The checked-in Cargo.lock pins
transitive dependencies. The Python environment uses the existing API lockfile.
The first run downloads public sources and compiles selected library crates.
The installed default Rust compiler was 1.93.1; Convex's pinned manifests require
1.95 despite older README guidance. The other projects are compiled with the
same compiler for this experiment.

`--source-root PATH` chooses a different scratch directory. A directory with
different revisions or dirty upstream source is rejected instead of reset.
`--output PATH` preserves a separate measurement run. Otherwise the dated JSON
in `docs/reviews` is replaced.

Contents:

- `Cargo.toml.in`: local source paths substituted only in the scratch manifest.
- `Cargo.lock`: dependency resolution used for the recorded measurements.
- `main.rs`: Rust integrations, assertions, diagnostics and warm timings.
- `compare.py`: clone/build runner, existing `corp_pv` baseline, benchmark-only
  Numba control, and a selective single-spread-edit diagnostic.

The harness checks 10,000 synthetic fixed-bond PVs against an explicit formula,
aggregated outputs from every timed adapter, OU moments/reproducibility/CRN,
callable price ordering where a result is returned, and prepayment response.
Expected upstream failures are recorded as evidence: Convex's callable panic
is caught in this experiment, and its spread-DV01 discrepancy is reported.
These catches are not a proposed production fallback. A successful harness run
does not mean all upstream libraries are correct or their full suites pass.

Interpret timings carefully:

- Warm in-process microseconds; three warmups and eleven samples. Reported p95
  is the nearest-rank percentile, which is the maximum with eleven samples.
- One- and four-worker runs are sequential; no simultaneous benchmark processes.
- Construction, compilation, serialization, FFI and application graph overhead
  are excluded. Each batch produces an instrument PV vector and a checksum.
- Convex's product entry point regenerates cashflows inside pricing. Its separate
  prepared-flow measurement avoids that work and uses precomputed year fractions.
- Finstack and stochastic-rs consume dated flows and perform date mapping and
  curve lookup. The current NumPy path consumes already base-discounted A values.
- Custom Rust/Numba controls perform a fused flat-rate discount calculation.
  They are not third-party pricers, and their speed does not measure a migration.
- Callable probes use different rate models and tree-lifecycle costs; do not
  rank libraries by their callable timings. OU is not our shifted three-factor LMM.
- Finstack's behavioral probe is survival under prepayment, without scheduled
  amortization, interest, payment delays or a full MBS valuation.
- The selective Python adapter edits a spread overlay and patches a cached
  total. It does not implement an incremental graph or rebuild behavioral flows.

See `docs/reviews/2026-09-28-oss-primitives.md` for findings and adoption gates.
