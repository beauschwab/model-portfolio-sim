# Acquired whole-loan mortgage source and accounting carry

`mortgage-credit-accounting-1` is a bounded Rust analytical contract. It retains an immutable original mortgage source, a supplied current Physical scenario, one actual acquisition cost and original effective yield, exact interval carrying transport, and complete economic/book claim closeouts. It does not produce balanced journal entries or a daily mixed-book ledger. A review identity is supplied provenance, not accounting approval or certification of an observed balance.

The implementation is `packages/portfolio-model-core/src/products/mortgages/sequential_carry/mod.rs`, with private source, coupled-transport and default-interest helpers. The actual native mortgage loop remains in `mortgage_state.rs`; `mortgage_receipts.rs` provides borrowed observations from that same loop. The [synthetic request](../../examples/models/mortgage-credit-accounting-1.json) has explicit policies and complete original/current sources. It is an arithmetic example, not a calibrated loan or approved accounting policy.

Read this alongside [the native mortgage state contract](mortgage-state.md), [the shared credit/earned-interest operator](credit-operator.md), [actual-date income](dated-income.md), and the [Rust production contract](../rust-production-contract.md). Their respective financial ownership and compatibility contracts remain in force.

## Public API and schema versions

```text
mortgage_sequential_carry::admit(&Request) -> Result<Admission, String>
mortgage_sequential_carry::admit_with_check(&Request, check) -> Result<Admission, String>
mortgage_sequential_carry::evaluate(&Request) -> Result<ResultData, String>
mortgage_sequential_carry::evaluate_with_check(&Request, check) -> Result<ResultData, String>
mortgage_sequential_carry::run[_with_check](&Request, writer[, check]) -> Result<(), String>

mortgage_state::project(&MortgageRequest) -> Result<MortgageResult, String>
mortgage_state::project_with_receipts(&MortgageRequest, sink, check)
    -> Result<MortgageResult, String>
```

`check` is a cooperative `FnMut() -> Result<(), String>`. A sink implements `MortgageReceiptSink`. Callback values borrow actual native data and cannot outlive the callback; the interface supplies no setters or generator, rate, cash or state overrides.

| Layer | Schema |
|---|---|
| Public acquisition/source owner | `mortgage-credit-accounting-1` |
| Original and current raw mortgage contracts | `mortgage-state-1` |
| Captured interval DTO | `mortgage-loop-interval-1` |
| Borrowed callback contract | `mortgage-loop-receipts-1` |
| Private exact carrying operator | `mortgage-coupled-carry-1` |

The earlier artifact's `mortgage-sequential-carry-draft-1` remains a distinct proof-tool schema. It is not an alias accepted by this public owner. Schema/model changes require explicit selection and rebuilding; historical inputs are not silently relabeled or refitted. Python may independently verify results or transport raw requests but is not the production financial backend.

## Required raw inputs and admitted ownership

The typed request rejects unknown fields.

The new owner also rejects duplicate JSON object keys and extra fields in a fixed
rate object: its complete wire form is `{"kind":"fixed"}`. Unit enums use their
documented string forms; singleton-object aliases are rejected. These checks apply
to both original and current contracts and the reviewed interest policy. They are
confined to this new owner; historical mortgage parsing and financial formulas
retain their existing behavior.

Its required groups are:

| Field | Ownership and meaning |
|---|---|
| `contract` | Complete current native Physical mortgage request, including actual payment/reset/path dates and full native recovery tails. |
| `original_contract` | Complete immutable original Physical request used to validate the original acquisition quote. |
| `economic_source`, `original_economic_source` | Separate explicit source `id`/`revision`; each revision must match its corresponding raw mortgage source revision. |
| `path_id` | Explicit outer supplied-path identity. No source generation or empirical probability estimate is inferred. |
| `pairing` | Reviewed identity, exact original/current run IDs and `supplied_physical_original_contract_unchanged-1` selection. |
| `acquisition` | Acquisition identity, original-yield anchor, income-review identity, purchase ordinal, actual cost, original annual effective rate, opening gross carrying `[3]`, and `supplied_non_pcd_gross_cost-1`. |
| `policy` | Explicit supported accounting, cash, economic-right and full-tail policies, each with its required review identity. |

The initial slice admits whole loans only, with `remittance_lag_days=0`, equal opening borrower/investor principal, zero acquired borrower/investor/contractual accrued interest, purchase on the source as-of date, and actual `currency_units`. Opening gross carrying is nonnegative, belongs only to origins with nonzero opening principal, and sums to actual acquisition cost under the unchanged financial reconciliation tolerance. Cost is positive and respects the shared monetary domain; original annual effective yield is finite in `[-0.99, 10]`. Negative yield is supported without clipping. Signed basis is retained separately.

Both sources must have a complete contractual/payment/live-right tail. The owner does not infer an opening allowance, PCD adjustment, fee, nonaccrual-income convention, legal disposition, funding transfer or agency guarantee. The source's whole-loan recognized-accrual fractions remain supplied assumptions; a zero fraction stops new recognized contractual accrual but does not itself change the selected original-EIR policy.

## Original versus current source

Pairing requires exact typed equality of contractual terms, original loan/cohort/currency/model/source identities, as-of and opening state/probability vectors, ownership, day count, payment calendar and reset terms. Only these differences are admitted:

- Supplied `path` points, including current credit and prepayment drivers.
- Borrower collection fractions, LGD and complete native recovery tails.
- Explicitly paired run and assumption IDs.
- ARM reset `input`, while reset dates, index descriptors, calendar identities, margins, caps/floors and payment-recast rules remain unchanged.

Both complete fixing/projection inputs are retained. Availability/publication and selected observed-versus-projected semantics remain those of `mortgage-state-1`; no future fixing is inferred. Its calendar-mapping disclosure also remains in force.

Let original effective yield be `y`, `k=log1p(y)`, purchase date `d0`, and original actual cash rows be `(dj,Cj)`. Quote validation is

```text
original_quote_PV = sum_j Cj * exp(-k * (dj-d0)/365).
```

`Cj` includes original native principal/interest cash and separately reviewed original default-interest collections on their actual dates. Economic disposal is not cash and is not added to this quote. Quote PV must reconcile to the supplied actual cost. The policy is recorded as `complete_immutable_original_actual_cash_only-1`.

The current scenario never fits cost, yield or opening gross carrying. Original receipts do not post into current cash. Neither yield nor OAS is solved by this contract, and the Physical quote is not a pricing-measure conversion. Pricing-graph adoption still requires its separate fixed-original-OAS and paired-history gates.

## Exact coupled interval transport

Use row vectors and seven states: performing, watch, delinquent, default from each of those three origins, and prepaid. Let `Q` denote the actual same-loop transition generator, not a pricing measure. Exit states are absorbing. Probability and monetary vectors are distinct; expected monetary amounts already contain their population weighting and are never multiplied by probability again.

For one interval of `t` ACT/365F years, let `P` be principal, `A` recognized accrued interest, `G` gross carrying, `E` accumulated earned original-EIR reward, `R` the diagonal native recognized-contractual accrual rates, and `K=diag(k,k,k,0,0,0,0)`. The selected `stop_at_exit-1` policy gives

```text
probability' = probability Q
P' = P Q
A' = A Q + P R
G' = G (Q+K)
E' = E Q + G K
signed_basis = G-P-A.
```

The shared credit operator supplies

```text
T = exp(Q t)
F = integral_0^t exp(Q s) R exp(Q(t-s)) ds
P_out = P_in T
A_out = A_in T + P_in F.
```

Gross carrying and accumulated reward use the augmented exponential of

```text
[ Q+K  K ]
[  0   Q ].
```

Rust evaluates a bounded uniformized exponential. For negative `k`, it evaluates the corresponding nonnegative `|k|` reward block and restores the signed reward. It does not use Euler steps, closing-principal/full-period interest or proportional basis allocation. The independent reference uses a separate scaling-and-squaring matrix exponential; the [Al-Mohy–Higham paper](https://eprints.maths.manchester.ac.uk/1300/) and [official SciPy `expm` documentation](https://docs.scipy.org/doc/scipy/reference/generated/scipy.linalg.expm.html) describe that reference numerical method, rather than this Rust uniformization implementation.

The owner carries one `G` and cumulative live `E` continuously through all source intervals. Ending default/prepay reward slices and completed live-origin rewards are retired exactly once into a conservation accumulator. Intervals cannot receive new caller cost, yield or carrying anchors. Native principal/accrued transport must match the captured source exactly at every interval, including origin-specific default slices and the post-boundary live state.

For an independent one-state hand check with default intensity `h`, no prepayment/migration and recognized rate `r`, define `phi(z,t)=expm1(z*t)/z`, with `phi(0,t)=t`. Before the boundary's contractual settlement,

```text
live_P = P0*exp(-h*t)
default_P = P0*(1-exp(-h*t))
live_A = P0*r*t*exp(-h*t)
default_A = P0*r*((1-exp(-h*t))/h - t*exp(-h*t))
live_G = G0*exp((k-h)*t)
default_G = G0*h*phi(k-h,t)
new_EIR = G0*k*phi(k-h,t).
```

At zero hazard, default `P/A/G` are zero by their continuous limits; at `k=h`, the carrying/reward integral uses `t` rather than a division by zero. These formulas illustrate exact occupation/exit transport, not a new event-date convention.

The canonical zero-credit example has principal 100, a 3.65% ACT/365F note rate and one actual 30-day remaining maturity settlement. Native contractual cash is 100.3. With original effective yield 2%, original cost is `100.3*exp(-log1p(.02)*30/365) = 100.13688317735347`; earned EIR is `100.3-cost = 0.16311682264652935`. Contractual accrual is 0.3, so basis runoff differs from earned EIR. Actual cash closes carrying without a new terminal plug or scenario yield fit.

## Native dates, settlement and boundary approximation

The receipt observes actual native operators/rates, source clocks, opening/transported monetary states, default claim ranges, cause-specific payoff fractions, scheduled collections, ARM resets, final state and entitlement-to-cash IDs. The existing order is

```text
interval transport → default capture → turnover/refinance payoff
→ scheduled collection → ARM reset → snapshot.
```

A payment on an ARM reset date uses the preceding interval's rate; the reset applies to later accrual. Turnover/refinance shares come from the actual native source, without reimplementing its seasoning, incentive or burnout formulas.

The native source assigns expected default/payoff events to the interval end and measures recovery lags from that ending date. Continuous-time occupation and original-EIR transport are exact for the frozen interval coefficients; borrower exit cash/default birth timing remains this explicit interval-end approximation. Refining supplied boundaries can change behavioral coefficients, cash timing and recovery dates. This is not a continuously timed realized-borrower simulation.

Scheduled actual principal plus interest reduces live `G`. At complete native live-origin settlement, remaining signed carrying is disposed under the reviewed exit policy, with signed accounting gain/loss and no added cash. A partial settlement that produces negative live gross carrying rejects. Prepaid carrying is split by the source's actual turnover/refinance fractions; each exit recognizes actual payoff cash less exact exit `G` once.

The result contains interval endpoints, not complete daily states or arbitrary report-date income snapshots. Absorbing default/prepay monetary slices exist inside a multi-day interval before the native source creates its boundary claims or payoff cash. A future daily owner must explicitly retain those pending exit rights and derive exact subinterval cumulative rewards from the same captured generator. It cannot interpolate endpoints, discard pending assets, move claim birth dates or post both daily rewards and full interval income.

## Explicit accounting and economic-right policies

| Request selection | Supported value |
|---|---|
| `policy.original_income` | `stop_at_exit-1` |
| `policy.remittance_float` | `reject_nonzero_remittance_lag-1` |
| `policy.live_cash` | `actual_native_cash_reduces_carry-1` |
| `policy.exit_basis` | `signed_native_exit_cash_minus_exact_carry-1` |
| `policy.default_basis` | `dispose_signed_basis_at_default-1` |
| `policy.default_principal_book` | `chargeoff_native_loss_at_default-1` |
| `policy.tail` | `reject_unclosed_gross_rights-1` |
| `policy.interest.accounting_accrued` | `reverse_income_at_default` |
| `policy.interest.economic_remainder` | `reviewed_economic_disposal` |

For each actual default-origin claim, the owner separately retains gross contractual principal/interest, recognized accrued interest, exact default `G`, signed basis, native loss, complete native principal cash, reviewed interest cash, accounting actions and dated economic dispositions.

The admitted book policy reverses recognized accrued `A`, disposes signed basis `G-P-A`, and charges off the native principal loss. These are distinct signed accounting-income actions. They do not retire the borrower's gross economic debt. Native principal recovery is owned exclusively by the native mortgage cashflow source; no second recovery, generic survival haircut or recovery-cost lane is added.

Default-interest cash is generated from the native contractual interest claim, a supplied origin-specific collection fraction and complete dated weights. Accounting interest reversal does not prevent separately reviewed later collection income. Remaining contractual interest requires its own economic-disposal review/date. Principal economic disposal likewise has its own review/date and must follow every native principal recovery. For gross claim rights `Pclaim` and `Icontract`,

```text
principal_disposal = Pclaim - sum(actual native principal cash rows)
interest_disposal  = Icontract - sum(actual reviewed interest cash rows).
```

Economic disposal is neither cash nor accounting income. Amounts use actual emitted sums, not normalized weights or an intended collection target. Negative disposal from overcollection rejects; a waiver policy cannot silently become a refund policy. The original economic-only quote receipts may close reviewed rights after actual cash while the current book still has unpaid principal. Such current book claims reject full-close execution; no extra writeoff, cash or terminal plug is inferred.

The native principal-tail unit-sum domain remains eight machine EPSILONs; reviewed interest weights use `1e-12`. They are not normalized. Final financial reference/reconciliation tolerances remain `1e-9` absolute plus `1e-12` relative. Exact economic residual and complete live-right closure checks remain exact where the implementation requires them. Scale-aware native principal conservation remains a separate source rule; it does not relax book claim closure.

## Result, full tails and identity

`ResultData` retains the complete typed `raw_request`, admission receipt, `source` and `original_source`, source intervals/boundaries/cash links, exact carry requests/operators/transport, ordered analytical `actions`, current and original reviewed interest cash, current book/economic closeouts, and separate original economic closeouts. Actions are scalar analytical receipts, not balanced posting instructions. Stable ordering uses actual ordinal and interval index.

It separately reports original quote PV, acquisition cost, cumulative earned EIR, total signed accounting income, actual current cash, original/full end ordinals, closing live gross carrying and conservation errors. Original and current raw/receipt hashes are separate SHA-256 values of compact, sorted-object, native-typed JSON. Retained normalized raw inputs, original/current immutable contract metadata and original-EIR/policy copies require exact type/value identity; dates and order remain exact. Financial tolerances apply to calculated results. A hash establishes receipt identity, not correctness by itself.

Full end is the maximum of both original/current source ends, all native recovery cash, reviewed interest collection dates and reviewed economic-disposal dates. Live contractual principal, borrower/investor principal/accrued and live `G` must fully close; every current claim must close its book and gross economic rights individually. Net-zero carrying is not permission to hide a gross claim. Each native entitlement must have one actual cash owner. No tail is truncated at a report horizon or replaced by terminal liquidation.

The result explicitly retains `sequential_carry_adoption=false`, `production_adoption=false`, `installed_independent_verification=false` and `mixed_ledger_adoption=false`; the private carrying receipt also retains `native_mortgage_generator_verified=false`. These markers do not negate the native source linkage, but they do prevent treating source-generated receipts as independent acceptance. A public independent analytical verification receipt is a separate publication gate. Mixed journal, capital, FTP and empirical eligibility remain false by scope; no accounting-standard approval or observed-source certification follows from successful arithmetic.

## Compiled generator hash and None behavior

`BASE_FINANCIAL_SOURCE_SHA256` labels the reviewed preinstrumentation body as provenance. The instrumented generator has different bytes. `generator_source_sha256()` hashes each file name and compiled file bytes with separate little-endian u64 length prefixes, in this order:

```text
mortgage_state.rs
mortgage_receipts.rs
consumer_contract.rs
credit_operator.rs
credit.rs
amortization.rs
cashflow.rs
floating_rate.rs
limits.rs
```

The declared scope is `length-prefixed-core-mortgage-generator-and-dependencies-1`. At this reviewed Core checkpoint, the encoded hash input is **141,669 bytes** and the digest is `cb467bb6260f6882d84a55a335c7bc91d377e228791b09a8a38d68c79d2e583a`. Rebuilding different source bytes changes these values. This nine-file digest does not claim to hash the full acquisition owner or executable: queued, executed and current bundle/source/artifact identity fences must independently bind public execution.

`mortgage_state::project` calls the same native loop with no receipt sink/check. The None path does not compute this digest or allocate receipt DTOs. Existing monetary formulas, reduction/event order, domains and guard errors remain unchanged. Versionable historical fixtures compare complete None and sink serialized native output bytes, including fixed/ARM, Physical/pricing and agency source cases. The analytical owner still rejects agency ownership and pricing measures; receipt compatibility does not extend its admitted accounting policy.

## Admission, capacity and cancellation

Admission finishes before either heavy original/current capture begins. Both native raw sources are validated without generating schedules, then their complete calendars, paired terms, policy identities, actual escaped metadata and output cardinality are admitted together. Limits are:

| Boundary | Limit |
|---|---:|
| Complete typed raw request | 1 MiB encoded bytes |
| Current intervals / original intervals | 16 each; aggregate work can be tighter |
| Aggregate owner work | 9,000,000 work units |
| Conservative retained/output capacity | 64 MiB |
| Contractual source horizon | 1,080 days each |
| Reviewed interest collection tail | 32 points per origin |
| Reviewed economic disposal lag | 1,080 days, after the respective complete cash tail |
| Explicit review/source/path identity text | 256 bytes, nonempty and control-free |

Each native mortgage projection retains its separate two-million-work/component domains. Those source bounds do not substitute for this combined admission. These are small admitted analytical slices, not 360-payment mortgage or 60,000-position workflow acceptance, and not an aggregate process RSS guarantee.

Let `I/Io` be actual current/original calendar interval counts, `RP/RPo` their summed native recovery-tail cardinalities, `IR` the summed reviewed interest-tail cardinality, `M` the sum of encoded JSON string-leaf lengths in the complete typed request, `Eraw` the complete encoded request length, and `Hcode=source_code_bytes()`. The checked conservative bounds are

```text
Rrows = I*(8*RP + 4*IR + 128) + Io*(8*RPo + 4*IR + 128)
Bbytes = Rrows*(M+4096) + (I+Io)*(M+65536) + 4*Eraw
Wwork = 4,000,000 + I*(140*14^3 + 4096) + Io*4096 + Hcode.
```

The fixed four-million source allowance charges both native projections; `Hcode` charges the compiled generator digest once even when cached. Numeric matrix/output overhead and source/cash/action/closeout replication are charged before generation. Arithmetic overflow, over-budget metadata/cardinality, invalid dates/amounts or unsupported policy return errors. The completed serialized result is checked again against its admitted byte bound. For the canonical public example, work is 4,534,021 and retained/output bound is 2,323,814 bytes.

Cancellation checks run during request counting, between admission/source stages, each actual source interval, native recovery/cash-conversion loops, sequential intervals/default claims/recoveries, and final output fences. Matrix exponentials, validation, hashing, some bounded short scans and final JSON serialization have boundary checks rather than an inner check for every arithmetic or byte operation. Each such phase remains bounded by the admitted contract; the public process watchdog handles a blocking transport/write. Writer/newline/flush errors or the final cancellation check return failure. Partial bytes may have reached a failed writer, but they cannot count as a complete result or authorize immutable publication.

## Validation and remaining adoption gates

The acquired-source Core stage passed 281 all-target native tests; the additional shared instruction guards bring the current Core total to 286, with strict Clippy. Its 23 acquired-source hand/domain fixtures cover original=current identity, changed default/prepayment/refinance/ARM with fixed anchors, signed yields and basis, migration/cure/default-origin conservation, sequential semigroup, actual cash/tails, large-scale near-unit weights, cancellation and failed writes. Four receipt tests cover historical eight-source None/sink byte equality, all 32 corrected accepted/error snapshots and compiled generator hashing/admission. Eight additional native cases cover strict new-owner fields, enum forms and duplicate keys. Official-default artifacts pass 185 independent/public-process mortgage checks and 34 durable mortgage checks, including exact retained-input tamper rejection. The complete bundle passes 788 component native, 2,066 engine (two reference-only skips), 531 API and 41 browser tests. The separate frozen artifact's 57 native and 206 independent-reference checks remain component evidence; they are not public persisted-journal acceptance.

Public analytical execution must be independently reconstructed from raw original/current sources, then reverified after immutable persistence with current bundle/executable and request fences. Existing mortgage valuation guards remain until their separate supported cash/claim valuation gates are implemented. Daily same-Sim adoption additionally needs acquisition cash once, complete balanced atomic P/A/basis/default transfers, explicit pending-source-exit ownership, exact interim/report-date native checkpoints, and independent ordered/persisted journal replay.

Unsupported policies include agency guarantees/advances, remittance float, acquired accrued rights, PCD, allowance, state-specific nonaccrual EIR, incomplete retained claims, new collection-cost ownership, pricing-measure conversion, empirical calibration and capital/FTP adoption. Freddie Mac's [mortgage security documentation](https://capitalmarkets.freddiemac.com/mbs/products), already cited in the local foundation, concerns security guarantees; those are distinct from whole-loan borrower collections. No agency terms or accounting approval are imported into this whole-loan contract.
