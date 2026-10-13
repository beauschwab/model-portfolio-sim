# Native mixed installment/card product-event GL

`product-mixed-product-credit-1` adds tagged native installment and card sources to the existing actual-date performing mixed book. Production execution uses the same ledger `daily::Sim`, `raw_post`, account cash/equity, transaction sequence, journal sink and gross rights mapper. It does not concatenate independently simulated books. The older `product-mixed-consumer-credit-1` request, bare native source receipts and table names remain unchanged; the regression fixture compares its entire serialized native result against the captured pre-change output.

## Raw and retained source contract

The standalone strategy envelope uses `op=analyze_product_mixed_product_credit`, `schema=product-mixed-product-credit-1`, an explicit execution timeout and a typed `request`. The request contains the original performing mixed request, the actual `combined_specification`, complete `product_owners`, and reviewed policy identity/phase/ownership/tail/cash declarations. Every path supplies `request={kind:installment|card|card_v2,request:<raw native accounting source>}`. There are no caller cash, exposure, allowance-intent or journal overrides.

Actual opening accounts and all original dated positions must exactly match the performing request. Product placeholders open with zero principal, accrual, basis, allowance and source-owned cashflows. Complete forward/reverse run mappings, scenario severities, original contract/purchase/EIR/policy anchors and physical source weights are required. Each native product source is retained as `{kind,result}` with raw request, exact source events, full daily state, default/recovery claims, accounting intents and source lineage.

The derived `product_request` and conservative `product_reservation` are retained in `preparation` alongside `raw_request` and the original performing preparation. New direct tables are `product_sources`, `product_daily`, `product_lineage` and `product_acquisition_lineage`; protocol3 journal/trial/closing/report/source tables remain native ledger outputs. Purchase and initial allowance lineage retain their actual transaction numbers (null for zero postings). Card statement receipts are indexed once by actual statement date. Extending the ledger past a card source end never repeats its final statement.

## Money, phases and tails

Acquisition posts actual cost/principal/basis once, with separate reviewed non-PCD initial allowance expense and contra balance. Native source accounting states remain unweighted; the single GL applies allocated face/unit face times physical path weight once. Even zero participation paths retain and validate complete state/claim tails. Recognition, actual cash settlement, before-credit allowance review, default/accrual/basis treatment, reviewed chargeoffs, gross recovery, after-credit allowance and explicit recovery costs retain the shared source phase order. Contractual future rights retire on native default/payoff under the admitted source policy.

Every unweighted principal, accrued, basis, allowance and default/recovery claim must close within the supplied complete tail. No terminal liquidation, replacement funding, net-loss cash shortcut, survival haircut or allowance inferred from expected loss is added. Shared accounts use explicit end-of-day cash constraints. Actual original source ends are retained separately from an extended ledger container. Both reporting and complete-tail closing dates are explicit.

## Admission and supported boundary

One global 128 MiB retained/output admission and 8 million work-unit budget covers performing capture plus product reservation before either heavy capture or product generation. Product reservation uses a conservative 1080-day ledger window, exact escaped metadata and per-source primitive work, retained native receipts, journal/state/lineage/statement rows and daily replay. Actual re-admission cannot exceed that reservation. Original pricing preparation has its own disclosed bound. Output serialization is bounded and cancellation/broken output cannot produce a successful completion.

The initial card slice supports supplied non-grace payment/default/zero-limit paths with original account EIR, billed/unbilled recognition, reviewed allowance/default/recovery and explicit commitment memo retirement. Grace, fees, draws/CCF, sign-changing unsupported ownership and unresolved principal-zero carrying/basis rights reject the entire request, including zero allocation/weight. Mortgage/MBS, joint stochastic pricing/credit, empirical calibration, regulatory default RWA, observed settlement and accounting-standard acceptance are not inferred. Product placeholder research regulatory/liquidity weights are zero and incompatible account constraints reject. `interim_default_capital_adoption`, `dynamic_mixed_acceptance` and `independent_persisted_verification` remain false; later independent raw-source and persisted-journal verification is a separate publication gate.

## Synthetic hand example

[Complete raw example](../../examples/models/product-mixed-product-credit-1.json) retains dated backbook, performing candidate, an acquired installment and an acquired card in baseline/stress runs. Each credit source defaults day5, recovers gross60 less explicit cost10 on day12 and receives its reviewed principal chargeoff on day12; its complete supplied horizon is day14. The performing ledger continues to day60. With candidate quote `1.0419930063526421`, pooled closing cash/equity is `1000 - 100*quote + 104.45 + 101 - 102 + 50 - 100 + 50 = 999.2506993647357`. The card initial allowance40 is explicitly expensed once and used once, never added to acquisition cost or recovery cash. This fixture is synthetic and is not scale or empirical acceptance evidence.

## Validated rebuilt integration

All three official native releases are rebuilt. The current bundle passes 1,627
engine tests with two reference-only skips, 445 API tests and 37 browser cases.
The focused release group passes 300 complete source/ordered-journal regressions;
28 new durable primitive/mixed/capital/failure-fence checks pass. Public component
coverage is 677 native tests. Independent raw and persisted verification gate
immutable publication; saved card/mixed reports and exact full-tail closing
capital reopen with their original source receipts. These are local synthetic
and application gates, separate from deployment and the wider scope above.

## Earlier component validation

At the component freeze, serial DEBUG all-target suites passed 84 ledger and 71 decision tests, and both all-target strict Clippy checks passed. The four new wrapper fixtures include full pooled hand cash/transaction balance, invalid zero-participation tails and anchors, cancellation, and complete old consumer serialized output identity against the captured versioned test baseline. A fresh DEBUG standalone CLI produced 732 journal rows, four native source receipts, admitted work302676 and exact baseline/stress closing cash999.2506993647357. These are local component gates; official artifact, durable publication and independent persisted replay acceptance are separate.

## Explicit Card2 interest-basis extension

The raw `card_v2` tag selects [compact interest-basis accounting](card-credit-accounting-v2.md).
It retains separate gross interest and signed live/default basis after principal
settlement. Complete native action events are posted once through the same Sim;
shared intents are evidence, never an additional posting stream. Original EIR
and reviewed basis realization retain separate income accounts. Gross rights
and commitments must close independently in every unweighted source before any
allocation or physical weight applies, including zero participation.

The [Card2 mixed example](../../examples/models/product-mixed-card-credit-2.json)
retains its original acquisition/yield anchors and complete monetary tail. Its
rebuilt default result has 750 journal rows, 122 supplemental daily-right rows and
44 atomic lineage rows; both scenarios close with cash 1102.2506993647357.
The existing Card1 and consumer results remain byte-identical. Ledger all-target
tests pass 97 and Decision tests pass 74 with strict Clippy; the installed
[independent verifier](product-rights-reference.md) passes 89 checks. Thirty-eight
durable source/publication/closing-capital checks pass against the explicitly
selected DEBUG executable and in the final default-release API suite. The rebuilt
Card2 bundle passes 1,842 engine tests (two reference-only skips), 493 API tests
and all 40 browser cases without a DEBUG override. The earlier release counts
above refer to Card1. Native and independent receipts retain
`capital_ftp_eligible:false` and separate `closing_capital_eligible:true`
permission. The signed live/default interest-basis asset mapping passes ten
native and six independent public hand calculations, preserving explicit supplied
weights and distinct original-EIR/basis P&L. Complete-tail closing capital requires
matching native/independent/persisted markers, ordered journal replay, the exact
final date and supplied account/capital policies. It does not authorize interim
capital, inferred regulatory weights or modeled FTP.
