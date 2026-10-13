# Actual daily credit replay

`analytics.credit_ledger.run_credit_ledger` transports this explicit route and publishes
`credit-balance-partitions-1` after independent persisted verification. Python's
`credit_verification` uses SciPy matrix exponentials, explicit accounting-policy
replay, complete claim tails, once-only lineage and original posting order. It
checks all source states, journal entries, daily balances, closing trial balance
and summary. It is a publication gate, not a selectable financial execution backend.

The durable API exposes `POST /balance-stress/credit` with `expected_revision`,
`specification` and `large_book`; backend selection is absent. Native validation
enforces standard/large limits. Queued executable and validation-library hashes,
source identity, worker lease, attempt token and cancellation fence publication.
The raw input is retained as a storage-managed object reference and remains rooted
through restart and garbage collection. The partitioned panel supports credit
state/claim/lineage paging and reopening. It labels regulatory limits unevaluated:
this route does not infer capital policies from credit balances.

`credit-ledger-1` is an explicit native loan-credit execution route exposed by `portfolio-balance`. It compiles raw `balance-stress-2` accounts and loan opening anchors, complete run-specific physical credit contracts, actual dated boundaries and reviewed allowance/chargeoff decisions. It streams actual journals and independently constructed credit subledgers. The existing `dated-ledger-1` and legacy protocol-2 routes retain their existing economics and execution. The [synthetic example](../../examples/models/credit-ledger-1.json) demonstrates the exact wire schema; it is not empirical evidence.

The Rust API is `CompiledCreditReplay::compile_with_limits(request, max_positions, max_work)`, `normalized_spec()`, `admitted_work()` and `credit_replay::run(compiled, writer)`. `compile(request)` uses the explicit maximum 60,000-position/90-million-work tier. Callers cannot raise those limits. The ordinary API tier must pass its 2,000-position/3-million-work limits through the native compiler. There is a separate maximum of 62,000 run-position contracts across the supplied scenario grid. Thus a 60,000-position baseline plus a complete 60,000-position stress run exceeds this route's current contract tier. The module does not assert combined 60,000-position graph/solver/ledger scenario acceptance or aggregate process RSS bounds.

Requests require `schema`, `monetary_unit="currency_units"`, `as_of_ordinal`, `specification` and `runs`. Each run declares unique `run_id`, `scenario_id`, `severity` and complete `positions`. Each position declares `account_id`, `contract`, `opening`, `boundaries` and `reviews`. Baseline severity one, every named scenario severity one and every requested reverse severity must be supplied exactly; no interpolation or multiplication of hazards by scenario severity occurs. Duplicate severities collapse only when their economic identity is identical, including positive/negative zero. All native built-in credit coefficients are compiled once per position/run.

Opening live monetary arrays plus outstanding recovery-claim assets reconcile to the normalized position's principal, book adjustment and accrued-interest anchors. Allowance and live undrawn reconcile separately. Account opening equity is independently reconciled by native specification admission; no balancing plug is inferred. Claims retain unique position namespaces, economic and reviewed-policy identities and complete dated tails. The route rejects overlapping legacy default, migration, draw, runoff, floating repricing, marks, collateral, legacy cashflow/policy ownership and account fee/cost/tax/dividend policies. Only owned loan positions are admitted in this schema. Regulatory proxies, mixed funding books and capital/FTP reports require their separate integrated routes.

Every interval is exactly one actual day under ACT/365. The [joint state coordinator](credit-state.md) first transports live absolute probability and independently weighted money, including continuous earned-interest occupation, then emits reviewed default, recovery and accounting intents. Economic prepay settles only its own transported allocation. The replay then executes the supplied boundary events in stable input order. This documented closing-boundary approximation does not assign intra-day loan cash settlement times. Source and policy identities remain attached to every interval and boundary. Each credit policy review has its exact native recognition date, with allowance review before/after credit defined by `credit_accounting`.

Boundary actions are explicitly tagged objects:

- `repayment` declares three `principal_fraction`, `accrued_fraction` and `cancel_undrawn_fraction` values. Each is finite in 0..1 and applies to that state's surviving actual exposure. Book basis retires in the principal fraction, with signed recognition in earnings. The amount is not computed from opening scheduled notional and is not survival-scaled again. Principal can fully retire while accrued rights remain until a later settlement event.
- `draw` declares three nonnegative `amount_by_state` values. Each must fit its current surviving undrawn commitment. Cash funds once, principal increases and undrawn falls; the draw earns from the next interval. Default-contingent CCF draws remain separate and occur through credit accounting.
- `maturity` settles all live principal, signed accrued rights and signed basis and cancels live undrawn. It moves live probability to a separate `matured_probability` diagnostic. It does not increment prepaid probability, dispose of default claims or erase recovery tails. Every normalized in-report `maturity_day` anchor requires exactly one explicit Maturity boundary on that actual date. Missing, duplicate or mismatched boundaries fail native compilation; no reviewed term-modification override is admitted by this schema. Out-of-report maturity retains principal, rights and claims without inventing a payment. No legacy synthetic maturity is applied to these owners.

This is a supplied boundary model. Fractional repayments do not by themselves constitute a fitted consumer minimum-payment, amortization or utilization model. Full recovery of a previously charged-off default with negative retained book discount may require an explicit reviewed basis recognition; omitting that review can violate nonnegative carrying-value admission. The runner fails rather than infer a recognition policy.

Protocol 4 outputs bounded newline-delimited `{table, rows}` batches with at most 1,024 retained rows across tables, followed by `{complete:true, protocol:4}` only on success. Journal rows are explicit records rather than the protocol-2/3 dictionary-column representation. The transport must opt into this distinction; the old journal representation is unchanged. Transaction numbers are global, monotonically increasing and balanced per account/run. Cash has instrument `@cash`, opening equity `@equity`, live balances retain position IDs, and default assets retain claim IDs.

GL labels are `cash`, `asset_principal`, `credit_default_principal`, `accrued_interest`, `credit_default_accrued_interest`, `book_adjustment`, `credit_default_book_adjustment`, `allowance`, `interest_income`, `credit_loss_expense`, `basis_recognition_expense`, `recovery_income` and `equity_opening`. Signed journal values are debit-positive. Allowance and opening equity have credit balances; revenues have credit balances, expenses debit balances. Daily source assets equal live principal/basis/accrual plus retained claim principal/basis/accrual plus cash minus allowance. Daily equity equals opening equity minus the signed earnings GL balance. Both source-to-GL and asset-to-equity reconciliations gate successful execution.

Output tables retain these contracts:

| Table | Identity and financial contents |
| --- | --- |
| `journal` | transaction_number, run_id, scenario, severity, day, ordinal, account, event, gl_account, instrument_id, value |
| `credit_states` | run/day/account/position, contract_id/revision, absolute probability[7], matured_probability, live_principal/undrawn/accrued_interest/book_adjustment[3], allowance, default principal/accrued/basis totals |
| `credit_claims` | run/day/account/position/claim identity, economic source/revision, recovery review/revision, default ordinal, initial principal/loss, outstanding principal/accrued/basis, charged-off principal, collected/applied recovery cash, remaining recovery cash and tail after reporting |
| `credit_lineage` | run/day/account/position/event identity, source/revision, policy/revision, optional claim ID, earned interest, economic principal loss, contingent draw cash, prepay cash, recovery cash |
| `daily_balance` | run/day/account/entity/currency, cash, assets, equity, allowance, reconciliation_error |
| `trial_balance` | run/account/gl_account/instrument_id, final signed balance |
| `summary` | run/account, final cash/assets/equity and maximum reconciliation error |

The shared normalized specification retains its 30..1,080-day horizon and opening carrying-value constraints. Amounts reuse the 1e15 currency-unit bound. The replay admits at most one million boundary/review dates, shared bounded identity metadata and at most 62,000 retained recovery claims per run. Base work counts position-days, account-days and supplied events; actual retained-claim scans consume the same supplied work budget during execution. Cancellation/deadline checkpoints run between compilation batches, days, position batches and boundary batches. Failed writes, invalid dynamic draws, exhausted work, incompatible reviews and cancellation emit no final completion; immutable publication must reject every such partial stream.

Independent native tests hand-calculate competing exit flux, stopped coupon accrual, cash-funded EAD, signed basis recognition and maturity/recovery balances; cure timing; actual surviving repayment; delayed signed interest settlement after principal retirement; dated reviewed provision/chargeoff; scenario/reverse coverage; ownership/anchor admission and failed-completion behavior. Legacy dated tests run alongside the new tests. Python persisted-journal and credit-state economic verification, public durable API/UI adoption, complete scale acceptance and empirical calibration remain separate acceptance gates. Numerical tests and this synthetic example make no accounting-compliance or empirical-accuracy claim.

Primary modeling references and numeric equations are documented in [credit state](credit-state.md), [credit operator](credit-operator.md) and [credit accounting](credit-accounting.md). The supplied transition, LGD and CCF assumptions must retain calibration source/training/holdout evidence before empirical release.
