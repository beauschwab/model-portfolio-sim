# Dated replay and credit foundations: local 0.29.6 checkpoint

Date: 2026-10-01. This report describes the local shared working tree. No commit,
production deployment or worker restart is implied. All fifteen model issues
#10–#24 and tracker #25 retain their full acceptance scope.

## Implemented behavior

The explicitly selected `dated-ledger-1` / ledger protocol 3 route consumes a
bounded, immutable actual-date source store. Rust admits owner/run/measure
identities, recognizes supplied calendar segments, settles dated principal,
coupons and fees, reconciles daily GL and emits closing statements. Dated owners
bypass the historical credit/runoff/interest/maturity overlays. Other positions
can retain those historical economics in the same book.

Early and late coupons retain separate gross interest rights, including after
principal settlement. Full source tails survive the reporting horizon. Day-zero
settlements, explicit fee earnings and their supplied tax treatment close on day
zero; their activity cannot carry into day one's report. Combined position-day
and segment-day admission cannot exceed the existing explicit engine tier of
60,000 positions / 90 million work units.

The publication adapter retains immutable raw input, hashes, source/event lineage
and closing rights. An independent persisted verifier checks once-only phases,
source amounts and dates, transaction postings, principal/basis/right roll-forwards
and gross closing/consolidated statements. Balanced journal tampering fails.
Cancellation and deadline fences extend through replay and final publication.

Shared `credit_operator` compiles reusable CTMC transition and earned-interest
operators for separate probability, principal, undrawn, signed accrual and basis
vectors. `credit_accounting` executes supplied physical daily policies and returns
balanced intents with reviewed allowance/chargeoff amounts and complete recovery
tails. These are separately tested components; automatic coupled product/daily
adoption is still open.

## Validation

| Gate | Result |
|---|---|
| Shared model core | 61 Rust tests passed |
| Native product crate | 42 Rust tests passed |
| Native ledger crate | 34 Rust tests passed |
| Native decision/workflow crate | 8 release Rust tests passed |
| New dated publication boundary | 19 Python tests passed against rebuilt artifacts |
| Full engine suite after final rebuild | 777 passed; 2 intentional reference-only skips; 204.60 seconds |
| Clippy / formatting | Passed for changed shared/ledger and dependent native crates; warnings denied |
| Python/Rust/TypeScript architectural boundaries | Passed; 104 documented cyclic edges |

The Rust total is 145 tests. The 19 dated boundary tests are part of the engine
suite, not an additional engine count. The earlier engine run passed 776 tests
with two intentional reference-only skips before the final day-zero and admission
fixes; only the subsequent rebuilt run is final evidence for this checkpoint.
The final stable-source run passed 777 tests with those same two skips. Both skips
are Python-reference parameterizations of Rust-only ownership cases in
`test_decision.py`; their native counterparts passed. No engine sources or release
artifacts changed during that final run.

A subsequent run completed with 776 passes, two intentional skips and one
publication refusal (`engine source changed during run`). Concurrent architecture
work changed generated ABI metadata and a Python contract while the run was
active. The source-identity fence correctly rejected that attempt. Final evidence
requires rebuilt deployment binaries and a stable-source rerun; the fence and
financial tolerances were not relaxed.

Representative fixtures cover leap dates, early/late/day-zero/same-day settlements,
mixed source ownership, signed negative coupons, separate fee/basis postings,
opening anchors, named physical scenarios, immutable input corruption, source/GL/
statement tampering, and cancellation before execution and after persisted replay.
A fresh production subprocess defaults to Rust and forbids Python financial
callbacks. Credit component fixtures cover analytical competing exits, cures,
integrated interest, atomic failures, contingent default draws, reviewed allowance
timing, signed accrual/basis treatment and beyond-horizon recovery claims.

Commands:

```powershell
cargo test --locked --manifest-path packages/portfolio-model-core/Cargo.toml
cargo test --locked --manifest-path packages/portfolio-risk-native/Cargo.toml
cargo test --locked --manifest-path packages/portfolio-ledger-native/Cargo.toml
cargo clippy --locked --all-targets --manifest-path packages/portfolio-ledger-native/Cargo.toml -- -D warnings
uv run --project apps/api python scripts/build_native.py
uv run --project apps/api python scripts/build_ledger_native.py
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py --test
uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py --clippy
uv run --project apps/api --extra dev python -m pytest packages/portfolio-risk/tests/test_dated_ledger.py -q -rs
uv run --project apps/api --extra dev python -m pytest packages/portfolio-risk/tests -q -rs
uv run --project apps/api python scripts/check_architecture.py
```

## Artifact identity

SHA-256 values recorded after rebuilding the final ledger and dependent
decision/workflow artifacts, before the final engine suite:

| Artifact under its package's `target/release` | SHA-256 |
|---|---|
| `portfolio-risk-native/portfolio_risk_native.dll` | `4290EF2279FBA67D3B9A779CA13BC20C0DB31F66DB9B6F4B8CC0F5908BF47852` |
| `portfolio-risk-native/portfolio-lifecycle.exe` | `2A103A5F10FFAFFDE0B7FA18135B225C0E0470A8A174F26DB9CBC1116CFDF3BD` |
| `portfolio-ledger-native/portfolio_ledger_native.dll` | `503E7E7F4C36B58FBE1142D1BE07AA9FBCC4FE29F82EEBA2C39B890EB18EB6A4` |
| `portfolio-ledger-native/portfolio-balance.exe` | `7DE3F07404F2E56F984BE2A2D086777B3825429D89E3685A37927DF819D1113F` |
| `portfolio-decision-native/portfolio_decision_native.dll` | `A65D8BCE77E0455BA4E1B7C1FFE564505C037C5E86310FDF7FA7188E9FD8A443` |
| `portfolio-decision-native/portfolio-strategy.exe` | `2A0B2715C8CB194D8FA5B894FF5CCD70A432F5AD44BD94C2932C97BE9F66BD96` |
| `portfolio-decision-native/portfolio-workflow.exe` | `AEB07575C1948AED95ED0A5470C4F09332F6EE0799C0038D63057B773084B917` |

## Remaining assumptions and required adoption

Recognition is uniform within each explicitly supplied calendar segment. Immediate
fee earnings are an explicitly selected policy, not a general accounting rule.
The producer supplies complete baseline/scenario/reverse schedules; the route
does not calibrate behavior or generate joint credit/prepayment/withdrawal paths.
Unsupported draws, active commitments/allowances, floating-beta overrides,
mark-to-market, sale/collateral ownership and incompatible opening anchors fail
admission rather than receiving implicit economics.

The credit operator's default-origin slots refer to the live state immediately
before default. A coupled coordinator must conserve and regroup migrated live/
default/prepay monetary state before applying origin-based accounting policies;
it cannot subtract a migrated default from an unrelated opening origin. Prepay
settlement, contractual maturity and sale are distinct exits. There is no second
probability weighting or live-mass normalization.

Required next stages include native product graph/capture, saved-book/candidate
mapping, joint credit/default/prepay/draw state, reviewed accounting policy
mapping and persisted credit subledger verification, durable API/UI exposure,
empirical histories and holdouts, and complete graph/credit/solver/journal
60,000-position acceptance. Component admission limits are not aggregate RSS
guarantees. Legacy monthly routes and saved economics remain separately readable.

The [OCC August 2026 Bank Accounting Advisory Series](https://www.occ.treas.gov/publications-and-resources/publications/banker-education/files/pub-bank-accounting-advisory-series.pdf),
[Basel ECL guidance](https://www.bis.org/committees/bcbs/basel-consolidated-guidelines/module/pap/20)
and [interagency allowance statement](https://www.federalreserve.gov/frrs/guidance/interagency-policy-statement-on-allowances-for-credit-losses.htm)
inform explicit separation of credit estimates, interest rights and reviewed
accounting decisions. No synthetic fixture establishes empirical predictive
accuracy or CECL/IFRS 9/regulatory compliance.

Contracts and full issue gates: [implementation plan](../models/implementation-plan.md),
[daily integration plan](../models/daily-integration-plan.md),
[dated ledger](../models/dated-ledger.md), [source store](../models/dated-store.md),
[credit operator](../models/credit-operator.md) and
[credit accounting](../models/credit-accounting.md).
