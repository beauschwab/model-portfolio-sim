# Separate nonintegral fee owner: local validation

This local0.29.16 stage adds `nonintegral-fee-accounting-1` to the shared Rust
model core and standalone native lifecycle. It exposes supplied physical fee
histories through the existing analytical Model Lab/durable worker. It does not
close issues15/25. The complete contract and remaining integrations are in
[nonintegral-fees.md](../models/nonintegral-fees.md).

The source explicitly supplies signed fee entitlements, service/classification
identities, reviewed actual-date earning, settlement, prepaid refund, waiver,
cancellation and recovery. The native result separates receivables/payables and
deferred income/prepaid expense by fee, retains ordered balanced intents, reporting
and complete-tail closing, and changes no loan G/EIR. There is no modeled LGD
or coupon recovery appropriation, terminal plug, caller monetary checkpoint or
loan-default reset. Unsettled supplied claims remain visible after the tail.

Independent Python replay verifies every raw identity, actual date/order, money,
review, posting and closing state; it is not a selectable financial runtime.
Before/after persisted verification and the common commercial SQL guard retain
the original accepted deadline, source/executable hashes, exact request/attempt,
workspace revision, lease/token/cancellation and fresh object bytes. API and
native strict scalar/duplicate checks preserve supplied source identity.

The fee owner retains `native_credit_event_binding`, `mixed_ledger_adoption`,
`pricing_unit_ftp_adoption`, `empirical_validation` and `accounting_approval` false.
Native credit/exercise/tranche binding, separate fee GL adoption in daily Sim,
pricing/acquisition fee rights allocation, aged units, capital/FTP, secondary or
earned-refund semantics, observed evidence and the full joint60000 gate remain.
The historical commercial compiler still rejects nonintegral fees. Historical
cash-based income schemas have not been silently reinterpreted. No deployment,
API/worker restart, commit/push or issue closure is performed.

## Evidence

All three matching official release builds pass. The four native all-target
suites pass1232 checks (core389, product503, ledger196, decision144), all strict
all-target/all-feature Clippy checks and decision docs pass. Focused commercial/
fee engine244 and API200 checks pass, with dedicated fee engine34/API10 checks.
The full engine suite passes2632 with two existing reference-only skips. Full API
validation passes1174 checks with one optional MCP module skip and the existing
Starlette/httpx warning; both dependency-enabled integration journeys pass
separately without skips. Both MCP/API/worker journeys and40 MCP checks pass;
the web build passes with its existing500k chunk warning. Architecture retains
110 documented cyclic edges and all8 checker fixtures pass. HTTP/native/MCP
contracts and exact operator-package bytes pass. Four complete historical product
outputs are byte-identical.

The exact accepted source/log/artifact evidence is sealed under
`artifacts/issue15-fee-owner-validation-final`. The initial release build completed
before a Python reference correction for valid integer-valued money; its first
freeze and interrupted orchestration remain preserved in the predecessor local
validation directory. No Rust source changed during that build. The corrected
matching build freeze binds1083 source/input files. An existing concurrent MCP
integration test changed before suite execution; only that test differs in the
separate validation freeze. Application and native financial inputs are identical
between these two snapshots. Neither prior evidence nor concurrent work is
overwritten. The stationary validation freeze passes after the full API gate.

## Hand and rejection controls

Fee5 earned day10/paid day20 retains receivable5 at day15 and no second fee income
at payment. Prepaid positive cash5 before service retains deferred income-5; the
negative-fee counterpart retains prepaid expense5. Unpaid negative earned fees
retain payable-5. Partial earning3 then payment5 transfers the unsettled right to
prepaid ownership2 without netting another fee. A reviewed default waiver2 from
earned5 leaves right3, with separate actual recovery2 on its complete future date.
Exercise cancellation of future service retains the prepaid obligation until
actual reviewed refund. No terminal settlement is invented for an unpaid right.

Native intake rejects unsupported measures/checkpoints, foreign source identity,
invalid signs/reasons, repeated IDs/clocks, money exceeding separate rights,
unearned waivers/recoveries and unsupported refunds. Deadline checks fence
validation and complete-tail generation. Independent/persisted replay rejects
rehashed rights, money, dates, order, review IDs, tails and adoption evidence, and
boolean monetary/retained-native-timeout scalars. Original integer-money inputs normalize only
the DTO money fields; clocks/booleans/provenance remain exact. Final SQL tests
reject late source/artifact/cancellation changes and queued request/attempt or
producer-deadline changes.


## Sealed identities

Validation source SHA256: `698d5095e203ab94a46b69031f54e57b433dfa5a74c1d886a140e43e83e8f92f`.

Receipt SHA256: `405f8130e261df044f33c556a54fd574dccf619ee36994378e02a431fb04ed13`.

Exact source ZIP SHA256: `f0545fc1b197e534d5fd6cc9f50afd52013cacca72c1e53f60c23cbec72189c7`.

The receipt rehashes eight official artifacts, all retained logs, the operator
package and the exact source archive. Original issue acceptance is unchanged;
full15/25 completion and deployment are not recorded.
