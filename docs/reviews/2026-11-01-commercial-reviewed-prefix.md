# Reviewed credit fields in the typed prefix census

The typed cut diagnostic previously refused every present ReviewedSnapshot.
It now maps all five snapshot fields and the complete RecoveryClaim,
RecoveryPoint, AccountingPolicy and LegalWaiver types. Four binary policy enums
are exhaustively matched. Every struct mapping exhaustively destructures the
native type, so new fields or variants require an explicit mapping review.

The mapping retains reviewed source/policy identities, legal/model/allowance
stocks, signed accrual and basis, chargeoff and collection stocks, settled
interest identities, the complete ordered recovery tail and separate legal
waiver clocks/dispositions. It observes existing amounts and does not perform
credit projection, accounting calculations or policy approval.

The native controls compare the typed field layout with independently serialized
real native values. They mutate all34 serialized claim leaves and all eight
waiver leaves independently; check all16 policy combinations; and cover every
snapshot scalar, signed zero, optional-state presence, recovery/settlement order,
tail truncation, nonfinite refusal, work-ticket spending and exhaustion. Existing
unreviewed and complete specification/prefix controls remain unchanged.

The diagnostic retains its existing string/materialization limits, source work
floors, incomplete private-source/Sim/codec flags and publication refusal. This
closes reviewed-state field mapping only. No admission discount, successful cut
publication, predecessor binding, empirical/accounting approval or complete
issue15/25 acceptance follows.

Final source-fenced validation is retained under
`artifacts/commercial-reviewed-prefix-2026-11-01/`. All15 format/test/Clippy/doc/
debug-binary gates pass across core/ledger/decision, totaling940 native tests.
Ten full commercial journal results remain exactly equal to the retained
pre-index baseline. All276 targeted commercial engine tests and40 MCP tests pass.
The original positive cut worker test remains a failure, while its
nine refusal/authority controls pass at the original admission totals. No full
combined suite, release installation, deployment or service restart is claimed.
