# Commercial verified modeled prior opening - local0.29.15

The supported stage is accepted on matching official local builds. Full
issues15/25 remain open. Fresh validation passes:

- 1,220 all-target native tests: Core377, Risk503, Ledger196 and Decision144.
- Strict all-target/all-feature Clippy across all four crates and decision doc checks.
- 210 commercial engine and184 commercial API checks.
- 2,598 full engine checks with two existing reference-only skips;1,157 full API checks.
- 29 MCP tests, one real-MCP/API/durable-worker integration, and the web build.
- Four complete historical public outputs byte-identical to0.29.14.

The API environment omits the optional MCP package and reports one module import
skip; the dedicated dependency-enabled integration passes with no skip. API runs
retain the existing Starlette/httpx warning. The web build retains its existing
large-chunk warning. No financial tolerance or existing finance admission cap
is relaxed. All419 native source/Cargo inputs are unchanged from0.29.14.

The new source-ID route resolves an authorized completed holding journal,
independently replays its raw/full persisted source, and matches the exact published
cut opening and every completed posting to the new complete-origin native journal.
No caller-supplied opening or permission flag is accepted. Separate execution
evidence records modeled prior-publication verification; observed bank reconciliation,
cut-started mixed replay and native external-authority flags remain false.

The0.29.14 worker selection omitted both holding schemas from the shared final SQL
commercial guard. This successor corrects that gap and adds late-source/output,
request/attempt/cancellation/deadline tests under the lock. Existing verification
and financial admission/tolerance policies are retained. Exact integer millisecond
reconstruction also prevents a floating round trip from corrupting command hashes.

Focused checks pass37 prior-opening/clock cases and132 combined holding/source/FTP
cases. The first broader development run reported111 passes and one deposit
bundle-change rejection because application sources were edited during a guarded
job. That rejection was correct; the stationary-source rerun passes132. No finance
tolerance, permission grant or publication fence was bypassed to obtain acceptance.
Architecture retains110 documented cyclic edges; shared metadata transport was
extracted to prevent new cycles rather than extending the baseline.

The separate successor evidence directory is
`artifacts/issue15-prior-opening-validation`. Accepted0.29.14 source/artifact
receipts remain immutable. All three official builds and standard native/engine/API
regressions pass. The source freeze binds1,075 source/input files with aggregate
`6cf8c3dbe2f70de15fb42de06ad2e22b49bcfcd9355cd56a7b9551dac6cc8a47`.
Final HTTP/native/MCP/operator gates pass, including seven architecture fixtures
and110 unchanged documented cyclic edges. Independent rehash confirms eight native
artifacts,29 logs and four historical outputs. All1,075 tested source/input bodies
are preserved in a verified ZIP snapshot. The receipt records `issue_complete=false`.

- Acceptance receipt:`1dd16a16ce6a1d1e46c220178c7fdd227c8f657a0d8e936d8e8f487b445aa2b8`.
- Exact source snapshot:`40845f93dc1ffa3c5917d0c3bf20d24dbd5d1fe1b9deb6777bff3f400d3988b8`.

The snapshot includes the changed generated HTTP client and MCP catalog, with
existing unrelated local application changes preserved. Generated MCP model
references are separately checked against current documentation. This evidence
is a dirty local dependency/input snapshot, not a scoped commit or deployment.

Original remaining scope includes arbitrary closing/observed bank opening,
cut-started mixed and seasoned graph/cohort/strategy adoption, secondary conveyed
rights, nonintegral fees, daily in-arrears indexed earning, generated draw/review
streaming, general convergence, observed histories/reviewed policies and the full
60000-position mixed graph/credit/solver/journal acceptance. This local stage does
not close full issues, commit/push changes, deploy or restart workers.

See [the contract](../models/commercial-prior-opening.md) and
[complete implementation ledger](../models/implementation-plan.md).
