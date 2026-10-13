# Raw financial instruction validation

Additive product requests reject fields that their selected instruction does not
own. An ignored narrowing or payment field can express a different instruction
from the one executed. Shared historical enum definitions and financial formulas
remain unchanged; strict deserializers sit on the new request fields.

| Request field | Complete unit-variant fields | Rejected examples |
|---|---|---|
| Card1/Card2 `termination` with `source_default` | `kind` | A supplied settlement `amount` or `ordinal` |
| CSA/MPOR `disposition` with `retain_to_closeout` | `kind` | An ignored settlement date or amount |
| Credit replay boundary `action` with `maturity` | `kind` | Repayment fractions or a partial payment amount |
| Product strategy `selector` with `all_assets`/`all_liabilities` | `kind` | A supplied unit or template narrowing key |

Struct variants retain their documented required fields and reject extra fields.
Kinds use explicit strings. Valid requests retain the existing serialized wire
forms. The native helpers reject duplicate top-level instruction fields when
their deserializer receives those keys. Some existing outer boundaries first
parse `serde_json::Value`, which can erase duplicates; this stage does not claim
complete duplicate-byte rejection at those boundaries.

Independent product-strategy verification checks every selector before financial
source reconstruction, including infeasible results. Independent credit replay
checks every supplied boundary before reading persisted rows or constructing a
financial operator, including dates beyond the reporting horizon. Neither gate
changes coefficients, accounting policy or numerical tolerances.

The separate [acquired mortgage contract](mortgage-credit-accounting.md) preserves
duplicate keys recursively until its new original/current contract and interest
policy deserializers reject them. It also rejects fixed-rate extra fields and
unit-enum object aliases while preserving historical mortgage parsing.

The local native/source/application acceptance status is recorded in
[the validation report](../reviews/2026-10-02-product-model-stages.md). Component
checks and independent preflight checks do not substitute for rebuilt-release
process, durable publication or complete child-issue acceptance.
