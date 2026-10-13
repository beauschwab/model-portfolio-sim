# Supplied CD election sequence graph (proposal)

`term-product-graph-2` selects chronological CD sequence pricing in the existing native term graph. `term-product-reprice-2` supplies `{base,current}`. The nested graph schema is `term-product-graph-2`; the history schema is `cd-election-sequence-2`. These are an unapplied proposal, requiring rebuilt native libraries before use. The Python methods only transport the raw request.

## Sources and immutable original economics

Every position has known opening terms, a complete authorized renewal/switch term universe, reviewed policy identities and a locked original OAS. `original_base` contains the complete original scenario and ordered original physical/pricing/down/up histories for every position. `original_quotes` contains one ordered reviewed signed future-cash quote per position. Creation checks that quote against the original pricing cash and original collateral curve with the supplied original OAS. It never solves an OAS. As-of settlement is retained as opening cash and excluded from that future-cash quote.

Current scenarios and histories are separate editable inputs. Their ordered pricing/down/up path IDs and weights must match each other and the original pricing pairs. Physical histories are supplied independently; there is no Physical-to-Pricing transformation or inferred election behavior. Zero weights remain retained and validated. Contract, action and policy identities may not be reused with different content. Authorized terms and policy ownership must match exactly, including negative-zero financial identity. A renewal uses its actual supplied maturity, coupon and contract identity; no automatic renewal is inferred.

`TermGraph::new` prepares all original and current native sources and binds the original receipt. `TermGraph::reprice` validates the current request on every call, retains the original source/anchor, and prepares only the current sources. Its immutable base key includes schema, model/pairing parent identity, opening calendar, known terms, locked OAS, complete original parents and quotes. Original quote, curve, path or term edits require a rebuild; editable current collateral/history changes cannot rewrite them.

## Cash and pricing ownership

The sequence owner uses the existing `cd::step_cd`/`step_cd_pricing` contractual arithmetic and shared pure `cd::project_cd_tail`. No-election sequences use that tail projector directly. Each action accrues its live owner to the election date, settles the actual instruction and retains live closing/new owners. Stable supplied same-date action order is explicit precedence. Final pricing cash contains every actual election settlement plus only the final unsuperseded live tails. Intermediate step preview tails are evidence, never additional priced cash.

For each future event, the existing native pricing function computes

`cash * DF_collateral(actual_days/365) * exp(-original_OAS * actual_days/365)`.

Cash is the signed sum of principal, interest and fees. The existing within-path compensated sum and supplied path-order weighted reduction remain unchanged. Supplied-history DV01 prices the paired down/up histories with the declared collateral/OAS bumps. This can include supplied election differences; it is not an optimal-exercise Greek or a calibrated behavioral model.

Physical reports distinguish principal, interest, fees and action diagnostics. Penalty/gross-extinguished/new-principal diagnostics are not additional cash. Reporting intervals are `(left,right]`; as-of cash has a separate opening amount. Pricing and retained physical tails continue past the last report. These are contractual cash metrics, not reviewed original-EIR daily NII or journal recognition.

## Signed and post-principal-zero rights

Negative contractual coupons and signed accrued rights are retained. Principal reaching zero does not retire a nonzero earned-interest right. An exact regression retains `8.881784197001252e-16` after full withdrawal and pays its signed amount at the actual maturity date; it is neither clamped nor used as a balancing plug. The preceding frozen tail-stage proof was insufficient for this residual-right edge. Residual states are explicitly before their separately retained terminal settlement, not certified accounting closing balances.

## Admission and runtime limits

The graph retains the existing limits: 256 positions, 13 scenarios, 120 reporting intervals, 128 paths per role, 4,096 joined original/current runs, 250,000 conservative/generated rows and 8,000,000 work units. Sequence histories allow at most 256 actions and 256 live policy owners, and at most 1 MiB typed raw bytes each. The complete graph raw input is bounded at 32 MiB. Full-tail/calendar checks reject inadequate source tails rather than invent terminal cash.

Before projection, checked conservative admission reserves original and current source rows, retained raw/source metadata, original/current curve nodes, metric/risk calls and transient index/risk work. The component bound is `12*typed_raw_bytes + 32768*reserved_rows + 8*reserved_work <= 128 MiB`. It intentionally retains the historical conservative pricing-call reserve even though the new branch does not calibrate OAS. After generation, actual retained serialization and key bytes are checked again against 128 MiB. This is a component capacity contract, not an aggregate process-RSS or 60,000-position claim.

The new strict selected boundary rejects duplicate nested fields and non-string aliases of declared unit enums before financial preparation. Historical graph1 admission uses its mechanically copied DTO-only parser; explicit new null/empty fields and sequence variants remain historical unknown-field/variant errors. Its shared native financial owner is unchanged. Raw requests that have already been collapsed to JSON Value cannot prove duplicate-key absence retroactively.

The contained proof carries an active native Control through parsing, admission, source generation, pricing and checked output writes/flushes. The proposed dispatcher propagates active control through Rayon only for this new selected branch. Existing public `term_call` has no per-call controlled capability for these schema names; default-library process watchdog or future durable-job fences must not be described as cooperative caller cancellation. Default native, persisted, API and browser execution remain pending installation/rebuild.

## Validation and remaining gates

Contained evidence: 35 native tests and strict all-target Clippy; 9 independent tests reconstruct CD sequence cash, native source receipts, signed PV, paired DV01, original quote/anchor and physical cash. There are 26 accepted and 5 rejected serialized process cases, plus a real duplicate-key byte rejection. Test-only native counters prove zero solver calls at creation and warm repricing. Twelve historical native fixtures and a complete nonempty legacy output/errored mutation comparison preserve graph1 behavior.

Authored installed-default gates forbid Python financial callbacks and use the ordinary default library in fresh processes. They are proposals, not executed acceptance. The independent reference currently admits CD-only graph2 books; indexed-term coexistence is supported natively but needs a composed independent source gate before publication. Intermediate preview settlement tails are not independently certified accounting inputs. The separate [CD public publication proposal](cd-sequence-publication.md) adds complete raw/persisted preview/cash/risk checks through the real native owner and copied durable worker; installed default acceptance remains a distinct gate. Real incremental registry, saved-book/library/cohort mapping, joint stochastic exercise, empirical calibration, mixed daily journals, capital and FTP adoption remain open. The result explicitly marks joint optimal exercise, accounting, empirical validation, saved/library/cohort adoption and capital/FTP eligibility false.

Contractual/math foundations: [term enhancements](term-enhancements.md), [existing term graph](term-product-graph.md), and [floating-rate conventions](floating-rate.md). No accounting-standard or depositor-behavior approval is inferred from supplied research policies.
