# Commercial original generated OAS follow-through

Local engine version **0.29.12**. Full issues #15 and #25 remain open. This stage adds
explicit retained-Pricing original quote calibration, native per-trial continuation
refitting, actual stopped-cash residual acceptance and calibrated acquired units.
Physical acquisition remains a separate cost/EIR authority. Current scenarios
retain the selected original OAS and shared Pricing randomness.

Four native calibration controls cover known spread/full day35 settlement,
actual exercise/refit/repeatability, unreachable/work/unresolved refusals, and
ambiguous endpoints plus a real unobserved-fixing policy-switch price gap.
Six generated graph controls include native acquired coefficients and historical
locked behavior. Independent raw-source, acquired coefficient and durable
publication/tamper controls pass against the matching official release.

## Accepted local validation

| Gate | Result |
|---|---|
| Official product / ledger / decision-workflow builds | Passed |
| Model Core / Risk / Ledger / Decision all-target Rust suites | 377 / 498 / 196 / 140 passed; 1,211 total |
| Strict all-target/all-feature Clippy | All four crates passed |
| Decision documentation compilation | Passed; no documentation test cases |
| Installed commercial engine / durable API checks | 165 / 117 passed |
| Full engine regression | 2,553 passed; two skipped |
| Full API regression | 1,082 passed; one dependency deprecation warning |
| Fresh-process Rust default / C ABI ownership | Passed with Python financial callbacks forbidden |
| Four complete 0.29.11 public outputs | Byte-identical after rebuild |
| Architecture, HTTP and native ABI contracts / operator package | Passed; seven boundary fixtures and exact packaged Markdown bytes |
| Final source / artifact / log verification | 1,041 source/input files, eight binaries and 25 retained logs rehashed |

Evidence directory: `artifacts/issue15-generated-oas-validation/`.
Frozen source aggregate SHA-256:
`ba634abee0032457293ffdf1ea3e0ca00514642e56c27b2dc2e8d6e729edda69`.
The [immutable validation receipt](../../artifacts/issue15-generated-oas-validation/validation.json)
has SHA-256 `20c3e1a3f231a93d742b1f17567393543c9042e93e049c76fe3e8b637b4a694b`.
The exact tested source/input ZIP has SHA-256
`7dbc1349daaa55ab92e38db0b61ce62923ba1db436cfbd02242c805f733fc6b1`.
All-target native suites use the standard test profile; engine/API checks use the
official optimized artifacts. Selected and full API checks report the existing
Starlette/httpx dependency deprecation. This dependency tree includes existing
dirty-checkout work and is not a commit change set.

The source snapshot, official artifact hashes, complete historical byte controls,
commands and logs are retained. The validated 0.29.11 receipt and exact source ZIP remain
unchanged in `artifacts/issue15-followthrough-fixture-validation/`.

Full remaining scope is preserved in
[the completion plan](../models/commercial-completion-plan.md).
See [the new math/ownership contract](../models/commercial-generated-oas.md).
The stage remains local; deployed workers have not been restarted.
