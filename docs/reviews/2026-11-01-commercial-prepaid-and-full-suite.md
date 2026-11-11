# Prepaid commercial source serialization and complete test routing

This continues local `94bbe9d`, with main `01d0254` included. It closes selected
source-record traversal funding and corrects the local regression test matrix.
Successful cut admission/publication and the full requirements of issues #15
and #25 remain open.

## Source record traversal

The Rust borrowing serializer adapter prepays a JSON cell and its string/key
UTF-8 scan before downstream escaping, and a cell plus a 32-byte formatting upper
before finite native-domain numeric formatting. Compound and variant layouts
retain their existing representation. Opaque RawValue/Number, unbounded Display,
nonfinite/oversized numbers and excessive nesting reject. No new source clone,
financial query, projection, calibration or repricing is introduced.

The additive `capture_prepaid` component emits the exact previous complete JSONL
stream while returning receipt version 2. Historical version-1 capture is
unchanged. It retains original loan/fee anchors, all source uses, zero allocations
and complete declared tails. The existing writer/record/footer charges and all
source admissions and caps remain intact.

| Genuine complete fixture | Result |
|---|---:|
| Source records | 27 |
| Encoded bytes, unchanged | 1,070,233 |
| Additional source traversal work | 1,510,239 |
| Total capture work | 3,653,185 |
| Reported joined capture-component work | 30,454,141 |
| Reported retained component bytes, unchanged | 115,257,808 |

The independent reference reconstructs the additional counter from decoded JSON
cells, UTF-8 key/string lengths and numeric widths. It replays all 27 financial
sources and three original loan/fee quote pairs; sixteen tampering controls
reject and all 24 current sources also replay with zero weights. Sixty portable
reference controls pass. The specimen has no realized defaults or recovery
receipts; declared policies/empty tails are complete, and inserted recovery money
rejects. This is not positive realized-default/recovery evidence.

The new adapter covers the audited built-in record serializers, not arbitrary
work hidden inside caller Serialize implementations. Complete raw/private
allocator/Sim/output codec admission and aggregate RSS are not certified. The
original typed economic owners are not recovered. The component continues to
grant no cut Sim, admission or publication authority. See the
[source capture contract](../models/commercial-source-proof-capture.md).

## Browser test ownership and full targets

The previous default browser configuration started a memory-only API while
discovering tests that require durable storage and a separate worker. The
dedicated durable configuration also omitted hedge accounting. The shared
`playwright.suites.ts` inventory now routes those tests to their required stacks.
All **22 test files** are assigned across memory, durable, cohort and native
service configurations; settings persistence is deliberately exercised in both
memory and durable modes. The durable group contains **76 tests in ten files**.
No test case is deleted, skipped or weakened by this routing correction.

Both root full-test commands include all four browser configurations, MCP/API
integration and the Rust service checks. Make serializes shared-port test targets.
The native browser configuration defaults to the ordinary built model-ledger
path and still accepts an explicit test executable override. All test stacks use
isolated temporary state. Existing user workspaces and running services are not
test data.

## Required positive cut acceptance remains failing

The committed cut API regressions establish safe refusal and ownership fences.
They do not establish successful cut publication. The explicit required-positive
acceptance check in `artifacts/full-suite-2026-11-01/` submits the unchanged full
example through a temporary durable repository and the actual worker. It fails:

```text
native cut joined32M/128MiB before dual graph generation:
work=36844632,bytes=223261940
```

No result is published. The request identity and failure are retained in
`positive-cut-acceptance.json`; the assertion remains a real failure, with no
xfail/skip, smaller fixture or relaxed cap. The successful source-capture
component's smaller dimensions cannot substitute for this complete route's
admission. A green existing regression matrix cannot close this acceptance gate.

## Validation records

`artifacts/commercial-prepaid-2026-11-01/` retains focused compilation/setup logs,
the complete byte-identical capture and independent replay/mutation receipts.
`artifacts/full-suite-2026-11-01/` retains full-source inventories, commands,
stdout/stderr, binary identities, matrix discovery and full regression records.

All 24 native formatting/all-target/strict-all-feature-Clippy/doc gates pass,
covering **1,655 tests across six crates**. The initial 4,033-case engine run
reports 4,026 passes, four staging failures and three skips. Two successfully
built standalone executables had been omitted from the isolated test directory.
After staging their exact matching binaries, the four affected standalone cases
pass. Native counted controls emit a fresh receipt and its independent Python
check passes, resolving the third skip. The engine coverage is therefore
**4,031 passing cases and two intentional Python-variant ownership skips**.
Nineteen counted native controls are a separately recorded repeat of existing
native cases, not additional unique native coverage. The dedicated MCP-to-real-
API/worker target also passes all ten cases.

The browser grouping fix changes only four existing Playwright configurations
and adds a shared inventory plus aggregate command wiring. Financial/engine
sources and all engine test files remain byte-identical across the fix. The
continuation verifies that exact source relationship and retains the original
native/engine logs before continuing API/browser/service validation. Initial
failed/setup records remain retained. All ten financial native release artifacts
are staged and identified, including the two initially missing standalone tools.

The completed matrix has these results:

| Gate | Result |
|---|---|
| Six native crates | 1,655 passing tests; formatting, strict Clippy and docs pass |
| Rust workbench service | 30 passing tests; formatting, strict Clippy and default build pass |
| Engine | 4,031 passing cases, two intentional Python-variant ownership skips |
| API full run | 1,451 passes, one failure, two prerequisite/module skips |
| API cross-engine database prerequisite follow-up | One pass after the native service binary was built |
| Dedicated MCP-to-API/worker integration | Ten passes, including the module omitted by ordinary API collection |
| MCP | Forty passes |
| Browser memory/cohort/durable/native stacks | 25 / 4 / 76 / 1 passes after output-directory corrections |
| Architecture and contracts | Eleven fixtures plus boundary/native/HTTP/MCP checks pass |
| Web production build | Pass |

The full API failure is the original, unchanged generic model-run success gate:
`test_model_runs.py::test_explicit_models_run_on_worker_with_source_identity_and_restart[sqlite-commercial-cut-mixed-journal-1]`.
It reports the same 36,844,632-work / 223,261,940-byte admission refusal as the
separate required-positive check. It is not skipped, xfailed, narrowed or changed
to expect refusal. The full acceptance result remains **failed**.

Initial parallel browser groups shared Playwright's output directory, causing
three trace-file cleanup failures after their application assertions. The memory
and native groups were rerun completely with separate output folders; the
affected hedge-accounting case was rerun alone against the durable stack with its
own output folder. Every original browser case is accounted for. Preliminary
failed logs remain retained. The parallel wrapper also encountered a Windows
CP1252 error while printing a failure tail; it had already saved command results
and log hashes. The final aggregate checks the command-level records and all
source/binary identities explicitly, rather than treating wrapper exit as proof
of success. Optional live PostgreSQL/S3 validation is not established by these
local checks.

None of these results overrides the failing required positive cut check. Remaining engineering is
complete raw-derived source/private-allocation/Sim/codec admission, actual
replacement of repeated preparation/Value work, exact verified published
predecessor binding and positive persisted cut publication. Installed regression
results do not establish observed model accuracy, professional policy approval
or every original product/scale/adoption requirement. Synthetic data and example
policies retain their declared status. No push, deployment or worker restart is
part of this local validation.
