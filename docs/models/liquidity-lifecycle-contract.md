# Liquidity lifecycle contract (`liquidity-lifecycle-1`, stages A to E)

Stage A of the [Rust liquidity lifecycle plan](rust-liquidity-lifecycle-plan.md), issue
[#43](https://github.com/beauschwab/model-portfolio-sim/issues/43). This stage defines the typed
relationship, action and settlement-obligation records, their strict admission, and a walk over the
declared settlement obligations. It posts no journal entries and computes no valuations or margin.
The schema is proposed and opt-in: no public route or durable job is registered for it yet, and
historical schemas are unchanged.

Code: `packages/portfolio-model-core/src/lifecycle/` (types, admission, settlement walk) and the
Decision preflight `packages/portfolio-decision-native/src/workflows/lifecycle.rs`. All code and
tests are Rust; there is no Python dependency.

## Records

| Record | Fields | Notes |
|---|---|---|
| Request | schema and model version `liquidity-lifecycle-1`, workspace and saved-book revisions, entity, currency, `currency_units`, as-of date, settlement policy | One entity and currency per plan in this stage. |
| Source | id, revision, sha256, measure (`physical` or `pricing`) | Hashes establish identity, not economics. |
| Opening account | id, cash, equity | Supplied and posted once; no balancing plug. |
| Position lot | lot and position ids, Physical source, account, kind (`security`, `loan`, `derivative`, `funding`), accounting category, positive quantity, acquisition date, original cost, effective yield and OAS, outside encumbrance | Original anchors stay with the lot and any surviving part. |
| Hedge allocation | id, exposure lot, derivative lot, amount, risk basis, half-open effective interval, review, unwind treatment, optional formal relationship id | An economic link. It never qualifies a relationship. |
| Funding allocation | id, funding lot, attributed asset lot, amount, interval, repay/retain/replace instruction | Attribution alone does not pledge inventory. |
| Pledge | id, security lot, agreement (a funding lot or a margin agreement), holder, quantity, eligibility, haircut in [0, 1) | |
| Margin agreement | id, counterparty, legal review, settlement lag, IM mode (`none` or `supplied_requirements`), segregation, VM treatment at termination, calendar, derivative lots in scope | A derivative lot belongs to at most one legal scope. |
| Facility | id, provider, commitment, drawn, access policy, expiry | |
| Formal relationship | relationship id and revision, Physical accounting source, adoption policy, hedging lots, hedged lots or one forecast exposure | The handoff to the ASC 815 owner (below). |
| Action | id, kind (acquisition, unwind, offset, rebalance, refinance), trade, effective and settlement dates, lots and quantities, policy, accounting treatment (`economic` or `formal`), action dependencies | |
| Settlement obligation | id, exclusive source event, account, kind, signed cash, optional lot/quantity, pledge, facility, action, margin agreement and collateral kind, due date, phase, sequence, dependencies, state (`pending`, `settled`, `failed`) | Cash is the signed movement in the account's cash; deliveries and releases carry none. |

Every record refuses unknown fields and unknown variants. Original bytes are parsed with
duplicate-key refusal (`strict_json`).

## Admission rules

1. **Selection and domain.** The schema and model version must be `liquidity-lifecycle-1`, and the
   unit `currency_units`. Amounts are finite and within the shared monetary domain. Text is bounded.
   Input is at most 8 MiB, with at most 16,384 records of each kind and 64 dependencies each.
2. **Unique identities.** Ids of every record kind are unique. Each source event has exactly one
   owning obligation.
3. **Exact mappings.** Every reference resolves: lots to Physical sources and accounts, allocations
   to lots of the right kind, pledges to security lots and a known agreement, obligations to their
   account, lot, pledge, facility, action and agreement. A missing mapping refuses with
   `LIFECYCLE_MAPPING_REQUIRED`.
4. **Kind and category.** Derivative lots use the derivative category, funding lots the funding
   liability category, and securities and loans an asset category. There is no `fvh`/`cfh`
   category, so a legacy hedge label cannot migrate silently.
5. **Shared resources.** Over time the concurrent hedge allocations on an exposure lot, and those of
   a derivative lot, never exceed the lot quantity. The same holds for funding on a funding lot and
   for the attributed lot. Intervals are half open, so an allocation that ends on a date frees
   capacity for one that starts on it. Pledges plus outside encumbrance never exceed a security
   lot. A facility is never drawn beyond its commitment. Unwinds never sell more than a lot holds.
6. **Formal treatment.** An action selecting formal treatment needs, for every hedge allocation on
   its lots, a formal relationship that names those lots and a listed Physical accounting source.
   There is no fallback to economic treatment (`FORMAL_OWNER_REQUIRED`).
7. **Settlement graph.** Obligation and action dependencies are acyclic. Within an account an
   obligation may only depend on an earlier `(due date, phase, sequence)` coordinate, and
   coordinates are unique.
8. **Collateral credit.** Termination and collateral-return obligations name their margin
   agreement. Under `apply_against_consideration`, a VM return alongside a termination is refused:
   the same VM cannot be credited against consideration and returned again.

Refusal codes: `MALFORMED_INPUT`, `BOUND_EXCEEDED`, `DUPLICATE_IDENTITY`,
`LIFECYCLE_MAPPING_REQUIRED`, `OVERALLOCATION`, `UNSUPPORTED_POLICY`, `FORMAL_OWNER_REQUIRED`,
`SETTLEMENT_DEPENDENCY_CYCLE`, `SETTLEMENT_ORDER`, `DUPLICATE_COLLATERAL_CREDIT`, and from the walk
`INSUFFICIENT_SETTLEMENT_LIQUIDITY` and `INVENTORY_UNAVAILABLE`. Malformed input is distinguished
from a valid plan that is financially infeasible.

## Settlement walk

`lifecycle::walk(plan, account)` applies an account's obligations in coordinate order, starting
from the supplied opening cash:

- A failed obligation does not settle, and every obligation depending on it is blocked. An unsettled
  receipt never becomes usable cash.
- A financing draw needs facility headroom; a facility repayment reduces the drawn amount.
- A collateral release reduces its pledge. A security delivery needs unencumbered, unpledged,
  undelivered quantity.
- Cash may never go negative after an event. A plan that would need cash it does not yet have is
  refused at that obligation. Whole-day netting is not used.

The walk checks declared signed amounts only. Valuation, margin calculation and journal posting
belong to stages B and C and to the daily Sim.

## ASC 815 handoff (frozen against #35 stages 0 and 1)

A formal relationship carries the relationship id and revision, the immutable `hedge-accounting-1`
source (its sha256 and Physical measure), the entity's adoption policy, and its hedging and hedged
lots or forecast exposure. Lifecycle facts for those lots enter the ASC 815 owner, and typed intents
come back to the shared journal. This stage never posts formal basis or OCI. An economic hedge
allocation supplies no designation, effectiveness or adoption permission, and removing a position
row does not erase a formal relationship. Redesignation is a new relationship identity under #35's
rule matrix.

## Source-to-rule-to-test matrix

Tests are in `packages/portfolio-model-core/tests/lifecycle.rs` unless noted.

| Plan source | Rule | Tests |
|---|---|---|
| Worked unwind fixture | Bridge-funded repay, release, sale, bridge repayment and termination; VM and IM returns later; final cash equals equity 6 | `worked_unwind_with_a_bridge_reaches_cash_six` |
| Worked fixture, bridge unavailable | Infeasible despite a positive final net cash | `without_the_bridge_the_dependency_chain_is_infeasible` |
| Second fixture: VM against consideration | Same final cash; VM never returned twice | `applying_vm_against_consideration_reaches_the_same_cash_once` |
| Shared collateral state | Pledged inventory is not delivered before release | `pledged_inventory_cannot_be_delivered_before_release` |
| Event ordering, failed legs | Failed legs block dependents; no netting of a shortfall | `a_failed_leg_blocks_its_dependents_and_keeps_expected_cash_out`, `a_failed_leg_with_insufficient_cash_is_refused_rather_than_netted` |
| Dependency graph | Acyclic, earlier-only dependencies | `dependency_cycles_and_order_violations_are_refused` |
| Many-to-many relationships | No overallocation over time; facility and unwind limits | `shared_resources_cannot_be_overallocated` |
| One exclusive owner | Unique source events, ids and legal scopes | `every_event_has_one_exclusive_owner_and_identities_are_unique` |
| ASC 815 alignment | Economic links never confer formal treatment; formal source identity | `economic_links_never_confer_formal_treatment` |
| Strict input; Physical provenance | Duplicates, unknown fields, legacy labels, Pricing sources and signs refused | `strict_input_refuses_duplicates_unknown_fields_and_legacy_labels` |
| Bounded admission | Record and byte bounds | `admission_is_bounded` |
| Decision preflight | Refusal before financial generation | `workflows::lifecycle::tests` in the Decision crate |

## Stage B: native acquisition replay (#44)

Stage B replays a security purchase, its financing advance and pledge, a hedge entry with cash
variation margin, and supplied exact initial margin, and posts the result in the existing daily
Sim beside a reviewed dated backbook. Code: `portfolio-model-core/src/lifecycle/acquisition.rs`
(engine) and `journal.rs` (shared kinds), `portfolio-ledger-native/src/owners/lifecycle.rs` and
`replay/daily/lifecycle_hooks.rs` (owner), and the Decision coordinator
`workflows/lifecycle/acquisition.rs`, exposed by the `portfolio-model-ledger` process as schema
`liquidity-lifecycle-1`. All code and tests are Rust.

### Request

`{schema, dated, plan, acquisitions[], hedge_contracts[]}`: the reviewed `dated-ledger-1`
backbook, the stage A plan (admitted from its own bytes), one acquisition block per dated run,
and the raw swap contract for every hedge entry. Every plan account is a dated account with the
same opening cash and equity, so the opening is posted once by the Sim.

| Instruction | Fields | Rule |
|---|---|---|
| Security purchase | new security lot, reviewed quote, trade and settlement dates, clean price per 100, accrued interest, fees, delivery versus payment | The lot's original cost must equal clean cost plus fees. Carrying is face plus a book adjustment of cost less face: no purchase-price gain is manufactured. Accrued interest is a receivable; fees are expensed. |
| Financing advance | new funding lot, advance date, principal, plan pledge | Principal equals the lot quantity and is advanced once; an opening contract is never advanced again. The pledge must secure that lot and deliver an owned, unencumbered quantity. |
| Hedge entry | new derivative lot, margin agreement, entry date, upfront consideration, VM terms (thresholds, minimum transfer) | The clean value is priced natively from the raw swap at the entry date. Upfront consideration must equal it, so no day-one gain or loss exists. Exposure beyond a threshold is called at entry and settles after the agreement's lag as posted or received VM. |
| Supplied initial margin | agreement, portfolio lot ids, scenario, date, posted and received requirement, review | Bound to the exact live portfolio, scenario and entry date, else `MARGIN_REFRESH_REQUIRED`. Posted IM is cash to a posted-IM asset; received IM is custody memo only. |

Reservation (trade date), obligation creation and settlement are separate: intents post on the
settlement day in phase order (advance, purchase, pledge, entry, VM, IM). Cash may never go
negative after an event (`INSUFFICIENT_SETTLEMENT_LIQUIDITY`), a pledge needs owned unencumbered
inventory (`INVENTORY_UNAVAILABLE`), and no cash plug or emergency facility is invented.

Received VM under a segregated agreement reduces usable cash once, in the daily closing; the GL
keeps the gross received stock. Under a reusable agreement it is usable.

### Journal

Kinds and GLs: `cash`, `opening_equity`, `asset_principal`, `book_adjustment`,
`dated_interest_receivable`, `funding_principal`, `derivative_asset`, `derivative_liability`,
`derivative_posted_vm_asset`, `derivative_received_vm_liability`, `derivative_posted_im_asset`,
`pnl:lifecycle_fees`, and the custody memo pair. Instruments are namespaced `@lifecycle:<lot or
agreement id>`; a dated position cannot alias one. The owner reconciles its book to the engine's
daily closings every day, and the Sim's daily balance-sheet identity includes the lifecycle
stocks. Lineage rows: `lifecycle_journal_lineage` and `lifecycle_custody_journal`.

The engine's receipt reports the cash bridge, inventory and collateral conservation, gross
derivative stocks and fees; the process header carries the native entry valuations with their
pricing source and the plan's SHA-256.

### Hand fixture

Opening cash 100 (the dated account). Day 1: a 95 repo advance; a bond bought at 99.5 clean plus
0.5 accrued and 0.25 fees for a 100 lot (cost 100.25); the bond pledged to the repo; a payer swap
priced at -4 and entered for -4 upfront; 3 IM posted and 2 received in custody. Day 2: 4 VM posted.
Cash 100 → 195 → 94.75 → 98.75 → 95.75 → 91.75. Assets less liabilities fall by exactly the fee.
Tests: `portfolio-model-core/tests/lifecycle_acquisition.rs` (engine) and
`portfolio-decision-native/tests/lifecycle_acquisition.rs` (coordinator, shared Sim and the
native process).

### Not in stage B

Formal hedge accounting of an acquisition refuses with `FORMAL_OWNER_REQUIRED`: the
`hedge-accounting-1` intents are not yet bound to this journal, and there is no fallback to
undesignated treatment. Unwinds, voluntary exits and offsets are stage C below; post-entry
derivative marks and coupons, dynamic IM and the independent Rust verifier are later stages. The acquisition is one Physical plan: the same intents post in every
dated run, and the supplied IM must be bound per scenario.

## Stage C: unwinds, voluntary exits and settlement dependencies (#45)

Stage C extends the execution engine and the same coordinator, owner and process entry. An
execution block may now also carry sales, funding repayments, derivative terminations and bridge
facility draws and repayments. Every instruction has an optional `instruction_id` (defaults are
`sale:<lot>:<date>`, `repay:<lot>:<date>`, `terminate:<lot>`, `draw:<facility>:<date>`,
`facility_repay:<facility>:<date>`, `purchase:<lot>`, `advance:<lot>`, `entry:<lot>`), a
`depends_on` list and a `state` (`pending` or `failed`).

| Instruction | Fields | Rule |
|---|---|---|
| Security sale | lot, quantity, quote, trade and settlement dates, clean price, accrued, fees, mechanism | Delivers only owned, released, unencumbered quantity (`INVENTORY_UNAVAILABLE` otherwise). Removes face and book adjustment pro rata; realized gain or loss is consideration less removed carrying, and marks already in earnings are never counted again. The surviving quantity keeps the lot's original anchors. |
| Funding repayment | lot, date, principal, actual accrued interest, break cost, consent, pledge releases | Full or partial. Accrued interest and break cost are expensed with the principal. Each release is a separate event that depends on the repayment; maturity is never shortened in place. |
| Derivative termination | lot, agreement, effective and settlement dates, consideration source (`model_clean_value` or `execution_quote`), fees, consent | Rights stop at the effective date: carrying is removed, consideration plus the contractual cash due that day becomes an unpaid receivable or payable, and consideration less carrying is realized. The claim settles on the settlement date. Under `return_separately` existing VM returns after the lag; under `apply_against_consideration` it is netted at settlement exactly once and never returned. |
| Facility draw and repayment | facility, date, amount | Within the facility's commitment and expiry; repayments cannot exceed the drawn amount. |
| Offset | a hedge entry with `offsets_lot_id` | A second contract. The original lot, its carrying, rights and margins survive; nothing is extinguished. |

Initial margin binds to every population change: an entry or termination on a date needs a
supplied requirement for the exact live portfolio of the agreement after the change
(`MARGIN_REFRESH_REQUIRED`). Proportional release is never assumed. When the scope empties, its
posted IM returns after the lag following the last termination settlement.

### Settlement replay

Events of a day run in phases: advances and draws, repayments, releases, delivery versus payment,
pledge delivery, derivative entry and termination, facility repayment, VM, IM. Dependencies must
name known events earlier in that order (`SETTLEMENT_ORDER`). An event whose dependency did not
settle is blocked; a `failed` instruction does not settle. Nothing that settled is undone: a failed
sale leaves the settled bridge debt, the released bond and the unterminated swap exactly as they
are. Cash still never goes negative; the plan is refused at the first obligation it cannot fund.

A reporting cutoff does not extinguish outstanding claims: the receipt's `open_claims` flags
unsettled termination claims and collateral not yet returned, and the stocks stay on the closing.

### Accounting facts for #35

Each sale, partial sale and termination emits a fact: lot, quantity sold and retained, effective
and settlement dates, consideration, removed carrying, realized amount, and the economic
allocations and formal relationship ids that touch the lot. The ASC 815 owner decides basis
disposal, retained-basis amortization, OCI and reclassification under its reviewed rules; this
engine posts none of them, and a termination's cash alone never forces OCI recycling. Formal
treatment of an executed action still refuses with `FORMAL_OWNER_REQUIRED`.

### Hand fixture

The plan's worked unwind. Opening cash 0, bond carrying 100 pledged to a 95 repo, swap liability
4, posted VM 2 and posted IM 3 (equity 6). Day 1 in phase order: 95 bridge draw; repo repaid
(depends on the draw) and the bond released; the bond sold for 100 (depends on the release); the
swap terminated at its native value −4 and settled the same day; the bridge repaid (depends on the
sale). Day 2: VM 2 and IM 3 return. Cash 0 → 1 → 6, no gain or loss.

- Without the bridge the repayment is refused at cash 0, before the pledged bond could be
  delivered; selling the pledged bond first is `INVENTORY_UNAVAILABLE`.
- VM applied against consideration pays 2 at settlement and returns nothing: cash 3 → 6.
- A failed sale blocks the bridge repayment and the termination; the bridge debt, the released
  bond, the swap and its collateral remain.
- An execution quote of −4.5 realizes −0.5; a coupon due on the termination date joins the claim
  with its sign, including negative amounts.
- A later settlement date leaves the −4 payable open until it settles; a horizon before the
  returns leaves VM and IM open.
- A partial sale of 40 retains 60 on the original anchors; an offset is a second contract.

Tests: `portfolio-model-core/tests/lifecycle_execution.rs` (engine, 10 cases) and the
`worked_unwind_*` and `unwind_without_the_bridge_*` cases of
`portfolio-decision-native/tests/lifecycle_acquisition.rs` (shared Sim and native process, with
the swap priced natively at the termination date).

## Stage D: independent Rust verification (#46)

`packages/portfolio-verification-native` is the second implementation. It verifies a
`liquidity-lifecycle-1` run from two inputs, the raw request bytes and the native process stream,
and returns a receipt only when every check passes. Its dependency boundary is sealed: Cargo
dependencies are `serde`, `serde_json` and `sha2` only, and a test refuses any production
financial crate in the manifest or the sources. It cannot call Risk pricers, Decision preparation
or Ledger replay to construct expected values.

| Check | What is compared |
|---|---|
| Raw identity | The request parses as strict JSON (duplicate keys refused, bounded bytes); the producer's `plan_sha256` equals the hash of the raw plan; lifecycle rows name that plan; the opening cash of each plan account matches the receipt bit for bit, so a signed zero or a retyped number is refused separately from any tolerance. |
| Native valuations | Every entry and termination valuation in the receipt is recomputed from its raw swap contract with the verifier's own equations (fixed and term-simple indexed legs over the supplied periods and exact discount points, cash due on the date) within 1e-12 relative and 1e-9 absolute. Every date an instruction needs must have a valuation. |
| Independent replay | For each execution block and account, the verifier re-derives every posting from the raw instructions: purchases, sales, advances, repayments and releases, entries, terminations and settlements, facility draws and repayments, VM calls and returns, IM and custody. It applies the declared phases and dependencies, blocks dependents of failed legs, refuses negative cash, unavailable inventory and unbound IM. |
| Ordered journal | The expected intents match the `lifecycle_journal_lineage` rows and, separately, the ordered `native_lifecycle_intent` transactions of the columnar journal: same events, dates, accounts, instruments and amounts, in the same order. Custody memo rows match the same way. |
| Closing stocks | Every lifecycle instrument balance in the trial balance equals the independent closing, including stocks that closed to zero; a nonzero lifecycle balance no posting owns is refused. |
| Receipt | Closing cash, settled movements, collateral stocks, fees, realized amounts, the open-claims flag, every event status and every accounting fact (kind, quantities, consideration, carrying, realized, lineage) match. |
| Completion | Exactly one `{"complete": true, "protocol": 3}` line, last. |

Supplied IM is checked for its scope, binding and the custody treatment of received amounts; the
methodology behind the amounts is not certified (`methodology_certified: false`).

`portfolio-lifecycle-verify <request.json> <stream.jsonl>` runs the same verification as a
process and exits 1 with the refusal. Tests (`tests/verify.rs`, `tests/boundary.rs`): the frozen
worked-unwind stream verifies, and these are refused: a balanced but economically changed posting,
a duplicated, reordered or altered row, an altered journal value, an altered closing stock, a
forged, missing or misplaced completion, an altered native valuation, a changed plan hash, an
altered receipt amount or status, a formal-accounting claim, a changed raw instruction, a signed
zero opening, a duplicate key, and an IM requirement not bound to the live portfolio. The Decision
crate's `lifecycle_acquisition` tests pipe a live run into the verifier, so producer and verifier
cannot drift unnoticed.

Not in stage D: the formal ASC 815 lanes (no intents are bound yet), Parquet partition rereads
(the Python API persists them today; the Rust service of stage E will own that), and any claim of
empirical or accounting-standard validity.

## Stage E: Rust service, durable worker and verified publication (#47)

`apps/api-rust` (crate `workbench-native`) is one binary with four roles. `api` is same-origin HTTP
admission, `worker` is the leased durable worker, and `admin` handles migration, initialization,
cutover and status. `contract` generates the HTTP contract. It runs the native
`portfolio-model-ledger` process and the stage D verifier, and contains no financial calculation and
no Python. Every scenario below is an actual process test (`tests/service.rs`, `tests/fencing.rs`)
on SQLite and, with `TEST_POSTGRES_URL`, PostgreSQL. Every service process runs with a cleared
environment and an empty `PATH`.

| Concern | Behavior |
|---|---|
| Shared storage | The existing `workspaces`, `revisions` and `jobs` tables and the object layout of `apps/api/app/artifacts.py` (workspace-hashed prefix, content-addressed keys, `{key, sha256, bytes, format}` descriptors) are reused unchanged. A fresh environment creates them with the SQLAlchemy definitions (`schema_version` 2). Rust-owned tables (`worker_routes`, `native_workspaces`, `native_job_authority`, `native_publications`) are versioned in `native_migrations` with a SHA-256 per migration; drift or a newer database refuses. |
| Scope | One process serves its configured tenant and workspace; no request selects either or a storage prefix. |
| Admission | `POST /native/lifecycle/runs` takes the raw body (strict JSON, duplicate keys refused, 8 MiB) or `?document=` at the expected revision, requires `X-Expected-Revision` (a stale value is refused), and stores the raw bytes before the job row commits. A repeated `Idempotency-Key` returns the original job and original deadline for byte-identical input and refuses anything else, across API restarts. |
| Revisions | Native workspaces commit immutable named documents with `PUT /native/documents/{name}` and `If-Match`; snapshots are reread with checksums. Workspaces whose snapshots the Python service owns are readable but refuse native commits. |
| Authority | Admission writes one immutable `native_job_authority` row: raw descriptor and hash, admitted revision, original deadline, producer executable name and SHA-256, verifier, and the digest of all of them. The worker reads authority only from it; a job view that disagrees, or an authority row edited after admission, is refused (`AUTHORITY_MISMATCH`). |
| Routing and cutover | Native jobs use `native_queued`/`native_running`/`native_done`/`native_error`/`native_cancelled`; the legacy oldest-job claim selects only `queued` and requeues only `running`, so it can neither consume nor reset them. `admin cutover --to rust` requires the legacy queue drained and no live legacy lease; `--to legacy` requires the native queue drained and no live native lease. Each change advances an epoch that fences leases and publication. The Python worker refuses the lease of a workspace routed to Rust. |
| Lease and attempts | The workspace lease is the only compute ownership, taken only while routed to Rust. A new holder requeues work a previous holder left running; that attempt's token cannot publish. Three attempts at most; a job past its original deadline fails at claim without running. Retries never reset the deadline. |
| Execution | The producer receives `{"execution_timeout_ms": <time left to the original deadline>, "request": <raw bytes>}`. A watchdog kills it on the deadline, lease loss, cancellation or an output above 256 MiB. The executable hash is checked before the run and again before publication (`EXECUTABLE_CHANGED`). |
| Publication | The stream is verified independently; then the verbatim stream (4 MiB whole-line chunks), bounded table pages and the manifest are written; the manifest, every chunk and page are reread with checksums and the reread stream is verified again. Only then does one transaction, under the workspace lock, check lease, epoch, authority digest, attempt token (not cancelled or superseded) and the original deadline, and insert the completed manifest. Cancellation or expiry during final I/O leaves no completed manifest. |
| Reopen | `GET /native/jobs/{id}/manifest` and `/tables/{table}?offset&limit` read only the pages that overlap the window, each checksum-verified; journal dictionary columns are decoded. Local files and, with the `s3` feature, S3-compatible buckets are supported (`tests/s3.rs`, MinIO in CI). |
| HTTP guard | A browser `Origin` other than the configured one, or a cross-site fetch, is refused; a bearer token is required when configured and always in production. |
| Contract | `contract/openapi.json` and `apps/web/src/lib/native-api.generated.ts` are generated by the binary from one route table; a test fails when either is stale. Tests also check that the service links no production financial crate and invokes no Python, and that the shared DDL matches `repository.py`. |

`tests/live.rs` runs the real producer through the whole path when `WORKBENCH_TEST_LEDGER_EXE` names
a built `portfolio-model-ledger`; CI builds it with Cargo alone.

Not in stage E: the ASC 815 lane (#35) on this service. Its jobs need an independent Rust hedge
verifier, so `hedge-accounting-1` stays on the Python durable route and the shared-path acceptance
item remains open. Also not in stage E: Parquet and Iceberg delivery of lifecycle tables (JSON
pages are published, and the verbatim stream is what verification replays), Rust-side object
collection (the offline Python maintenance tool now counts native active jobs) and any browser
workflow (stage F).

## Not in stages A to E

Post-entry derivative marks and coupons, dynamic IM, saved-book adoption, long-term policies,
prepared allocations and scale are stages F to I (#48 to #51).
