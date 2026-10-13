# Native tagged product event ownership

`product-event-owner-1` compiles raw acquired physical installment or card requests for the existing dated daily ledger. Each path supplies `{kind: installment | card | card_v2, request: <raw primitive>}`. Each retained native receipt supplies `{kind, result: <complete native primitive result>}`. There are no caller financial intent/state arrays. Core product projectors remain the sole economic owners; the adapter reuses their original-EIR and shared credit-accounting ownership.

Owners retain unit/position/account, exact original unit face, allocated face, accounting category, economic source/revision, original-yield anchor and explicitly reviewed initial allowance. Each owner covers precisely every admitted ledger run/scenario/severity. Path IDs are unique within a run; supplied physical weights sum to one within the existing monetary tolerance. Acquisition principal equals original unit face exactly. New positions are exclusive zero placeholders without legacy cashflows, credit/stress, draw, marks, sale or liquidity overlays. Actual account opening cash/equity is supplied once. Interim regulatory/liquidity weight constraints retain the existing explicit reviewed boundary.

Before the first source projection, admission checks all raw paths, including zero weights and zero allocations, and reserves source/daily work and escaped metadata capacity. It rejects pricing-measure histories, mismatched source/currency/run/anchor, inconsistent original card terms and unsupported grace/fees/draw/CCF. The primitive repeats complete financial admission before generating its own economics. `Compiled` retains each generated native result once; boxed enum variants preserve the raw wire shapes while preventing large enum stack storage. The borrowed `OwnerSources`/`NativeView` bridge supplies fields to the existing Sim and shared `PathBalances` mapper without copying a receipt or projecting again.

The existing consumer schemas and results remain compatible: consumer `SourcePath.native` is still its bare result, and old `consumer_sources`, `consumer_daily`, `consumer_lineage` and `consumer_acquisition_lineage` retain their fields and native transaction order. New generalized runs emit `product_*` tables, complete tagged source receipts and full product-specific lineage. Card daily receipts retain actual statement, original commitment and reviewed termination records. Statement dates are indexed once, selected in source order for their exact actual date, and are not repeated when balances are held past the native source end.

The phase remains opening, acquisitions/reviewed initial allowance, dated recognition, native product intents, dated settlements, close. Raw native state is reconciled before the explicit factor `(allocated_face / unit_face) * source_weight` is applied once to postings/cash/equity. Gross accrued/default rights remain separate by path before aggregation. Purchase cash, allowance expense, default recognition, reviewed chargeoff, gross recovery and recovery costs use the existing balanced state mapper and `Sim::raw_post`; there is no second loss model, survival haircut or cost deduction. Current policy models end-of-day liquidity and does not infer intraday credit.

Reporting end and full source end are separate. Complete closing checks every live and claim principal, accrued interest, book adjustment, allowance and remaining recovery tail. Net zero carrying is insufficient if gross rights remain. The card primitive requires actual commitment retirement and an explicit zero-right closure or source default. Principal zero is not automatically payoff.

The `card` (schema1) variant preserves the shared principal-plus-basis guard. Its cost100/original-yield0 fixture with principal zero, accrued interest1 and basis-1 rejects even at zero allocation/weight. The reviewed cost101 interest-only tail may be admitted. Explicit `card_v2` invokes the separate reviewed interest-basis contract described below; schema1 never falls back to it. General fee/grace/draw/CCF, nonaccrual/modification, broader basis-right accounting, empirical fitting and joint stochastic credit remain separate open ownership gates.

## Explicit Card v2 compact sources

`card_v2` requires raw `card-credit-accounting-2` and its explicit original-yield,
non-PCD gross-cost, principal-basis transfer, disposal and paired default policies.
Its additional 64-day/64-step+cycle/16-collection primitive limits apply inside the
owner's broader window. Source allowance ownership is absent: an outer initial
allowance review rejects, including a zero-valued review. Core2 raw has no `path_id`
or invented global recovery/cost review fields. The canonical outer `Path.path_id`
and retained owner/run/path indices bind that source. Its borrowed policy view emits
only the actual accounting and rights policies. Source revision, original face,
yield/cost/date anchors, currency, physical run coverage and original card terms
remain immutable across an owner's paths.

`NativeView::CardV2` borrows the original boxed Core2 result. Optional
`rights_opening`, `rights_closing`, `rights_state(day)`, `rights_action(index)` and
`rights_actions` expose separate current principal, gross interest and live/default
interest basis. `rights_events(action)`, `rights_shared_intents(action)` and
`rights_income_steps(action)` borrow exact arena slices only when the action belongs
to that source; foreign or cloned actions are refused. Existing nine-slot principal
state summaries are not repurposed for interest basis.

`OwnerSources::postings(run,day)` yields indexed `PostingRef::Intent` for legacy
sources and `PostingRef::CardV2Action` for complete v2 atomic actions, preserving
source/path order and each native operation's original order. V2 `intents()` is
empty. Its flat events are the **sole posting stream**; the referenced shared
credit intents and native original-income steps are evidence, not second posting
instructions. This prevents double recognition of gross rights/default/collections
and preserves original EIR separately from reviewed interest-basis realization.

Every v2 source, including zero allocation/weight, must individually close its
gross rights, live/default basis and native commitment at full tail before mixed
execution. Core2 analysis alone may return incomplete claims; that is insufficient
for this compiler. Uncollected `A=1, Bi=-1` cannot disappear because net carrying is
zero or because another path has collected its claim. Actual statement indices are
interned by source/date once; a later owned snapshot never repeats the last source
statement or accrues a second statement coupon.

Admission charges the complete Core2 preflight work and reservation in addition to
outer escaped-metadata/index/report/daily execution bounds before any projection.
Generated Core2 actual retained-capacity charge is checked before moving the boxed
result into retained sources; there is no economic receipt copy or second projector.
An exclusive existing-Sim mapper must own supplemental signed live/default Bi as
contra-receivable adjustments, never classify negative Bi as deferred income.
Capital/FTP eligibility remains a separately declared and independently verified
boundary; these supplemental accounts must not be silently omitted from net carrying.

The eight compiler fixtures cover complete raw identity, borrowed compact arrays,
evidence-only shared intents, same-policy unfinished zero-weight claims, actual
statement dates, all original anchor/measure/currency/run domains, allowance/tail
limits, legacy posting-iterator compatibility and unweighted full source retention.
Native same-Sim replay, independent raw/partition verification and public publication
gates are additional to compiler validation. See
[the Core2 contract](card-credit-accounting-v2.md) for formulas and exclusions.

Bounds: 64 owners, 32 paths per run, 1080 actual days, 32MiB total raw JSON, 1MiB per tagged primitive input, 128MiB conservative owner capacity and 8 million owner-plus-dated work units. Checked arithmetic and cooperative controls apply to admission, source generation, index construction and serialization. An independent streaming byte counter checks generated retained source bytes against the reservation before retaining them. Per-primitive limits remain additional. These component limits are not aggregate RSS or 60,000-position throughput acceptance.

Native fixtures exercise one pooled GL for dated backbook, installment and card; actual cash/earnings, source-weight/allocation once, retained statements/commitment/claims, physical/run/source admission, zero-weight paths, cancellation, exact date indexing, unchanged consumer receipt borrowing and the old negative-basis rejection. Public acceptance additionally requires independent raw economic reconstruction and ordered pooled replay, persisted partition/identity verification, source/artifact/cancellation/deadline fences and current release/API/browser evidence. A native state reconciliation alone does not establish independently verified source economics or complete #19/#22/#25 acceptance.
