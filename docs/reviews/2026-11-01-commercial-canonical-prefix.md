# Exact cut comparison with one retained encoding

The native cut comparator retains only the smaller complete JSON encoding and
streams the other serializer against its immutable bytes. It retains both
complete size scans, both complete serializations, the original per-operand and
combined32MiB limits, and exact field/order, numeric-category and signed-zero
semantics. It does not use a digest or reconstruct financial values. A mismatch
still completes serialization; late serializer errors, size changes and
cancellation cannot become a successful comparison. Each write is probed in
chunks of at most32768bytes.

The staging Vec reserves exactly the smaller encoded size before writing and
refuses excess allocator-reported capacity. This removes a complete second
encoding owner. It is not a complete serializer-allocation, source/Sim or RSS
proof. Historical admission charges, limits and refusals remain unchanged.
The ordinary holding raw/table comparators retain their existing two-pass
implementation, avoiding additional unbudgeted sizing work in that route.

Validation on the final source:

- All15 format, all-target test, strict all-feature Clippy, doc and debug-binary
  gates pass across model-core, ledger and decision: 938 native tests in total.
- Five new core controls include324 independently encoded value pairs, both
  operand orders, exact capacity edges, signed zero, escaped/unicode strings,
  late serializer failures, changing serialization sizes and every cancellation
  point in a100000-byte unescaped-string comparison.
- All276 targeted commercial engine tests and40 MCP tests pass.
- Ten complete commercial journal results match the retained pre-index baseline
  exactly, including both zero-allocation controls.
- The unchanged original positive cut API test still fails before graph
  generation at work36844632/bytes223261940. Its nine refusal/authority controls
  pass. No test, history, monetary tolerance or request limit is weakened.
- Architecture checks,11 boundary fixtures and native/HTTP/MCP contracts pass.

Logs, source fences, binary identities and parity records are under
`artifacts/commercial-canonical-prefix-2026-11-01/`; `native-final-v2` is the final
native matrix. Preliminary runs remain retained. No release installation,
deployment, service restart or full combined regression acceptance is claimed.

The original positive test and dedicated refusal test currently use the same full
request. Repository instructions preserve its historical source-work floors and
receipts. Resolving that conflict requires an explicit contract decision as well
as complete resource and independent publication proofs; a smaller buffer alone
cannot grant admission. Reviewed-credit census mapping, all other private source/
Sim/codec bounds, verified predecessor publication and the remaining issue15/25
adoption, observed/policy and combined-scale gates remain required.
