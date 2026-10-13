# Acquisition-aware mixed dated replay

`product-mixed-dated-1` combines a reviewed, performing dated backbook with selected performing product-unit purchases. Rust prepares original product pricing/income sources, captures complete physical paths, and executes one chronological daily cash and general ledger replay. This is an additive expected-journal contract. It does not certify observed settlements, accounting standards, empirical calibration, or joint stochastic/credit behavior.

The standalone executable accepts `{ "op": "analyze_product_mixed_dated", "schema": "product-mixed-dated-1", "execution_timeout_ms": 30000, "request": ... }` and returns one `{ "ok": ... }` after complete native execution. Errors and cancellation during preparation/replay produce no partial successful result. Production adapters transport this raw contract; Python is an independent verifier, never an execution fallback.

## Opening and ownership

The raw request contains a dedicated `candidate` selection without standalone account balances, a `backbook` dated-ledger request, an actual `combined_specification`, reviewed mixed `policy`, and supplied `opening_identity`. Combined accounts, equity, cash, scenarios and existing positions must exactly match the normalized backbook opening. Candidate positions are additional, disjoint zero-opening placeholders. Candidate source identities cannot overlap backbook sources. No standalone candidate equity or cash is added to the actual opening.

Every existing position must have explicit dated ownership. Backbook accrual segments and signed basis recognition are supplied reviewed inputs, reconciled to original accrued and basis anchors. This route does not calculate or certify an original backbook effective yield. Candidate original yield and locked base OAS/quote are checked through native product-units preparation. The original physical source, source revision, path, weights, acquisition date, actual price, reviewed policy and yield anchor remain in the preparation receipt and acquisition lineage.

## Chronology and monetary conservation

Opening cash/equity are posted once. On each actual purchase date, acquisitions precede that day's recognition and cash settlement. Purchase principal and signed premium/discount are recognized against actual purchase cash. Allocation face divided by unit face and the separately supplied physical path weight are applied exactly once. These factors remain metadata in the ledger acquisition store and are not multiplied again. Zero allocations and zero-weight histories remain retained.

Recognition before acquisition, principal repayment before acquisition, conflicting purchase dates for an owner/run, missing path ownership and unretired principal tails reject. No synthetic `start_day`, inferred borrowing, fees or terminal payoff is inserted. Supplied account cash floors apply to the actual end-of-day pooled cash; this contract does not simulate intraday liquidity.

Each candidate physical path pools its signed earned-minus-paid contractual interest right. Gross receivable/payable/prepaid/deferred rights remain separate across physical paths. Immutable coupon direction is derived from the complete unweighted earned and cash source, independently of principal direction and original EIR. Uniform negative coupons are supported; sign-changing coupons require separate gross obligation ownership and reject here. Backbook event rights retain their original gross event ownership. At complete closing, every backbook event right, candidate path right, principal and basis must reconcile; opposing unretired backbook rights cannot cancel to pass admission.

## Reporting and complete tails

The requested reporting end and complete closing end are distinct scalar result fields. Sources are replayed through the largest supplied complete tail, capped at 1,080 days. Shorter source containers are extended only as derived ledger windows; immutable original backbook container ends and raw requests remain retained. This extension creates no cash or recognition. Incomplete captured contractual tails reject rather than retire residual balances artificially.

`reported_daily_balance` contains the actual reporting-day balance, including dates between monthly snapshots. `reporting_rights` contains backbook event rights and one candidate row per physical path at that date. Later scheduled cash/recognition remains captured and executed in full-tail journals. `closing_statements` and `trial_balance` are complete-tail closing balances, not reporting-date liquidation values.

Direct `journal`, `trial_balance`, `closing_statements`, `daily_balance`, `dated_rights`, `event_lineage`, `acquisition_lineage`, `exposures` and `ledger` tables retain native protocol-3 fields and transaction order. Transaction numbers start at one separately for each admitted scenario. Journal postings are signed actual currency amounts. Heterogeneous raw stream messages are decoded once and not duplicated in the result.

## Admission and independent verification

The slice admits at most 512 positions, 4,096 acquisition events, eight million checked aggregate work units and a conservative 128 MiB retained capacity. Aggregate admission occurs before daily candidate capture and generated schedule allocation. It includes escaped source/policy/path metadata, origin lists, every physical path, full-tail recognition/journal/state cardinality, index work and native/prepared artifacts. Raw, prepared and final JSON each have separate bounded capacity. Cooperative checkpoints cover source preparation, index construction, events, days and output completion.

The reported `admitted_work` bounds the candidate capture, generated dated store and mixed journal/report stage. Initial native product-units pricing/preparation uses its separate existing aggregate admission. These component budgets are not a combined process RSS cap or a claim of unified large-book acceptance.

The independent verifier reconstructs original native preparation economics from raw inputs, generated dated events and acquisition amounts, then independently replays the full ordered journal and closing state. Supplied opening provenance alone is not verification. Result flags `independent_persisted_verification` and `dynamic_mixed_acceptance` are false at this native seam. Durable publication must independently verify persisted artifacts before claiming verified journals or admitting a capital bridge.

Supported candidates are the performing consumer-installment, card, fixed-mortgage and deposit product-units policies. Generic cashflows, taxes, fees, marks, defaults, netting and mixed legacy owners reject. Reverse severities are explicitly unsupported until detailed run/severity-keyed journals are integrated. No supplied physical-to-pricing inference occurs. Full joint market/credit/behavioral replay and empirical acceptance remain open.

## Hand example

[The synthetic raw example](../../examples/models/product-mixed-dated-1.json) starts with cash 1,000, an existing loan principal 100 and equity 1,100. A 100-face candidate costs `100 × 1.0419930063526421 = 104.19930063526422`. Its supplied complete principal/coupon receipts total 104.45; the reviewed backbook pays principal 100 and coupon 1 at day 60. Both explicit scenarios therefore close with cash/equity `1,000 − 104.19930063526422 + 104.45 + 101 = 1,101.2506993647357`. Candidate effective income is 0.25069936473578 and backbook reviewed income is 1. The example is a hand reconciliation, not scale, empirical or accounting-standard acceptance.
