# Liquidity lifecycle contract (`liquidity-lifecycle-1`, stage A)

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

## Not in this stage

Opening balance-sheet reconciliation from carrying values, native acquisition and unwind
coordination, voluntary funding and derivative exits, margin recalculation, the shared journal, the
independent Rust verifier, the Rust API and worker, saved-book adoption, long-term policies,
prepared allocations and scale are stages B to I (#44 to #51).
