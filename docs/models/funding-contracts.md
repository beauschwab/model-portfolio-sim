# Dated funding and exclusive repo collateral

`portfolio-risk-native::funding_contract::evaluate(&FundingRequest)` implements
schema `funding-contract-1`, model version `funding-contracts-1`. The native JSON
route uses `{schema, threads, request}`; the inner request has the same schema.
Rust owns the arithmetic. The module is split into immutable `types`, admission
`validation`, collateral arithmetic, a chronological `timeline`, and orchestration.

This is an explicit contractual/scenario capability. It preserves the legacy
spread-to-short, constant-principal rollover model under its existing identity.
Adoption requires new contract inputs and a rebuilt graph/unit library. A
successful component test is not deployment, empirical calibration or full #21
integration acceptance.

## Dates, direction and rates

Each contract identifies its entity, counterparty, currency, contractual source,
rate path, lending/borrowing direction, funding kind, start and actual maturity.
Supported kinds are interbank, unsecured funding and repo. Currency must equal
the request currency; FX conversion and same-entity counterparties are rejected.

`New` produces an initial cash advance and an explicit lender-perspective
origination fee. `Opening` receives existing principal without repeating that
advance or historical fee. Opening contracts supply only remaining interest
periods; their first remaining accrual anchor can follow original origination but
must be at/before asof. Historical settlements are rejected rather than replayed.
Remaining full coupons retain their original accrual anchors. They are cash
settlements, not automatically the current day's earned interest.

Interest periods reuse `term_enhancements::indexed::ContractPeriod`. Fixed coupons
and explicitly supplied observed/conditional projected index paths therefore
share actual day counts, index tenor, exact UTC reset/selection cutoffs, immutable
source revisions, lookbacks, compounding, margin and cap/floor conventions.
Observed publications cannot follow the declared valuation snapshot. Projected
information must be available at its selected reset/observation. No future
published fixing, front-quarter index proxy, or rate fallback is synthesized.
Indexed business-day mappings remain supplied and are explicitly reported as
unverified. See [term-enhancements.md](term-enhancements.md) for exact equations
and supported conventions.

Funding principal is a bullet: final principal and the last coupon settle at
actual maturity. Intermediate coupon cash and fees are separate. From the lender
perspective, a new advance has negative principal cash, later repayment positive
principal and interest. Borrowing reverses these cash signs. Signed and zero
interest are supported; principal and total claim-factor domains remain bounded.
There is no OAS solve or random-number generation here.

## Partial rollover and funding access

A maturity election can request a positive principal amount no larger than the
old principal. Available access requires separately identified `New` replacement
terms starting at the old maturity, with explicit replacement coupon/spread,
counterparty and collateral policy. Entity, currency and lending/borrowing
direction remain compatible; counterparty switching is explicitly represented by
the replacement input. Old principal is repaid in full, then the new principal is
advanced. The same-day net funding reflects those two distinct contracts.

One replacement generation is supported. Recursive future rollover is rejected.
An unavailable-access election omits executable replacement terms and reports the
requested amount as a funding-access shortfall. It does not remove, delay or
reduce the old contractual maturity obligation. Automatic constant-balance
extension is absent. Rollover is a modeled election and rejects `Contractual`
measure; its scenario/assumption/source must be explicit.

## Collateral inventory and margin

Repo requires a collateral agreement, eligible asset classes, owned inventory,
exactly dated valuation/haircut snapshots and initial pledge quantities. Unsecured
and interbank rows reject attached repo collateral semantics. Negative haircuts,
haircuts of one, and implicit initial-margin ratios are unsupported. This policy
uses haircut `h` as a discount from collateral market value:

```
eligible collateral credit = sum(quantity * market_price * (1 - haircut))
required exposure = principal + (include_accrued_interest ? supplied_accrued_right : 0)
```

The accrued right and its immutable source come from the state-aware coordinator;
the collateral module does not infer them from a whole coupon or haircut.
Required exposure must be nonnegative. Initial eligible credit must cover it.

Global inventory records owner, currency, asset class, quantity and quantity
encumbered outside this request. That external encumbrance is never released by
one of this request's contracts. Pledges belong to the cash borrower: own entity
for a borrowing book, counterparty for a lending/reverse-repo book. Received
collateral is rejected as pledgeable inventory. No legal title transfer,
rehypothecation permission or re-use right is inferred.

All contract actions run on one chronological timeline. Opening holdings are
established first; old maturity release/liquidation occurs before separately
supplied new pledges on the same date; later margin transfers follow. This allows
explicit same-day refinancing while rejecting incompatible overlapping pledges.
Each asset's pledged quantities plus outside encumbrance cannot exceed its owned
remaining quantity. Report-horizon encumbrance is captured independently from the
complete tail closing inventory.

Margin snapshots explicitly supply changed prices, haircut and eligibility.
Every held/delivered asset requires a mark. Ineligible held collateral contributes
zero credit; new deliveries must be eligible. Positive quantity transfers deliver
owned collateral; negative quantities return that contract's existing pledge.
Substitution is an explicit delivery/return pair. Releases execute first within a
failure-atomic request, followed by deliveries and a final coverage check. No
cash funding is generated by these security movements.

The minimum transfer value controls whether a deficit creates a material call.
The audit retains required exposure, before/after eligible credit, call value and
unmet value even when a small gap falls below the transfer threshold. `Reject`
fails a material unmet call; `Report` retains the explicitly disclosed breach.
With management access unavailable, proposed transfers remain pending and do not
change encumbrance. Future maturity payment and contractual release remain in the
projection. Management outage is not automatically a settlement-system failure
or borrower default. Same-date margin/default ambiguity is rejected: margin
actions must precede default.

Snapshot ID/revision pairs cannot be reused with different contents in one
request. Stable inventory-index reduction and movement ordering preserve
repeatable output. No collateral cash, external fee, or funding principal is
added for an ordinary pledge, substitution, margin transfer or release.

## Counterparty default and realization

The separate default policy supports `Physical` cash-lender counterparty claims
before maturity. New origination and default on the same day are rejected. An
opening already-defaulting claim can be recognized at asof with its explicit
initial holdings. Own-entity borrowing default, counterparty-lender default,
cross-contract legal netting and closeout enforceability are unsupported.

The default source supplies actual nonnegative accrued-interest claims, separate
economic principal/interest loss amounts, immutable recovery review and complete
dated residual recovery schedules. Unsupported signed prepaid accrued rights at
default are rejected; ordinary signed coupons remain supported. Whole future
coupons are stopped, including principal at contractual maturity; scheduled cash
through the declared default date survives. No allowance, chargeoff timing,
accounting P&L or legal liability extinguishment is inferred.

A defaulted repo requires explicit collateral realization. All held collateral
remains encumbered until realization. A supplied liquidation mark, sale discount,
cost and principal-first or interest-first waterfall determine the retained
recovery:

```
gross_sale = held market value * (1 - sale_discount)
net_sale = gross_sale - sale_cost
retained = min(net_sale, principal_claim + accrued_interest_claim)
surplus_to_owner = net_sale - retained
```

The contractual haircut is not applied a second time to actual modeled sale
proceeds. Sale cost cannot exceed proceeds. Immediate same-date surplus return is
required; a delayed custodial payable needs a separate reviewed state/policy.
The chosen waterfall allocates the retained amount between principal and
interest, capped by each claim. Funding cash records that retained recovery once.
Gross sale, cost and surplus fields are explanatory audit amounts marked
`cash_already_in_funding_schedule=true`; consumers must not add them as another
funding cash event. The asset inventory is disposed, not merely released for reuse.

Separate conservation holds over the entire declared tail:

```
principal_claim = collateral_principal_recovery + residual_principal_recoveries + economic_principal_loss
accrued_claim = collateral_interest_recovery + residual_interest_recoveries + economic_interest_loss
```

Principal LGD does not include accrued interest and is not an accounting allowance.
Recovery tails may extend beyond both original maturity and report horizon, but
must fit the explicit full-tail domain. The result distinguishes live principal,
gross unsettled default claim, and expected principal recovery after reporting.
The gross default claim still includes economic loss until separate reviewed
accounting/legal recognition; it is not silently written off here.

## Admission, identity and integration

The book admits 1..256 total old plus replacement contracts, at most 4096 assets,
4096 margin steps per contract and 4096 points per principal/interest recovery
tail. A combined 250,000-item budget counts periods, index observations, snapshot
values, allocations, recoveries and timeline actions. Complete dated domains are
bounded to 36,600 days within Gregorian years 1..9999. Monetary/quantity/price
inputs use the shared finite 1e15 admission; products and sums are admitted again.
Metadata IDs/revisions are 1..128 control-free bytes with a combined 32 MiB budget.
The native JSON layer adds its own request/output bounds. These limits do not
constitute an aggregate process RSS cap.

Every contract, rate path, source revision, collateral snapshot, eligibility
policy, loss/recovery review, election, entity, currency and measure belongs in
cache/model identity. Results retain the complete coupon-source audits. Saved
legacy constant-balance economics stay under their historical identity. Model
changes require explicit source preparation and native library/worker rebuilds.
Pricing callbacks or Python financial execution are not used.

The result's schedule is complete through `tail_end_ordinal`; report horizon does
not clip obligations or recoveries. Entity/counterparty lineage, margin calls,
inventory, realization and market-access diagnostics are separate typed outputs.
Native risk, liquidity, NII, daily accounting, capital and FTP must consume these
outputs through their own declared policies. Persisted journal replay remains a
publication gate. This component does not by itself establish those integrated
gates, optimal funding management, empirical rollover calibration or legal rights.

## Independent example and evidence

`examples/models/funding-contract-1.json` is a complete native protocol request.
It advances 100 on January 1, 2026 at 0.36 ACT/360, settles 100 plus interest 1
January 11, then explicitly advances replacement principal 60 at 0.72 until
January 21. Same-day January 11 net lender cash is `100 + 1 - 60 = 41`;
replacement interest is `60 * 0.72 * 10 / 360 = 1.2`. Total net cash is 2.2 and
all cash principal advances/repayments sum to zero.

Initial collateral quantity 112 at price 1 and haircut 0.1 gives credit 100.8.
On January 3, price 0.9 and haircut 0.2 give credit 80.64; supplied accrued claim
0.2 makes the call `100.2 - 80.64 = 19.56`. Explicit delivery of 40 adds credit
28.8, leaving pledged quantity 152 and credit 109.44. The old pledge is released
at maturity before the replacement pledges quantity 70. All tail encumbrance
returns to zero; report-horizon encumbrance remains 152.

`tests/funding_contracts.rs` contains twelve independent fixtures covering
funding signs/fees/zero/negative interest, no re-advance of opening balances,
partial roll and outages, collateral haircut changes/substitution/exclusivity,
outside encumbrance, deterministic output, default losses and delayed tails,
sale/waterfall/surplus and no duplicate cash, strict domains, and the checked-in
example. Existing indexed-coupon fixtures independently gate the reused reset,
compounding and observation-availability arithmetic.

## Primary sources and supported-policy limits

[ICMA's haircut explanation](https://www.icmagroup.org/market-practice-and-regulatory-policy/repo-and-collateral-markets/icma-ercc-publications/frequently-asked-questions-on-repo/21-what-is-a-haircut/)
distinguishes the collateral-value haircut from an initial-margin ratio. The
implementation explicitly chooses the haircut convention and separately models
valuation changes, transfers and realized sale proceeds.

The [Federal Reserve's repo margining research](https://www.federalreserve.gov/econres/notes/feds-notes/proportionate-margining-for-repo-transactions-20250214.html)
describes cash/securities exchange, haircuts and variation margin. Negative
haircuts exist in practice; this supported subset rejects them rather than
reinterpreting the sign. Its margin method is a supplied contract policy, not an
assertion of universal market practice.

The [New York Fed collateral schedule](https://www.newyorkfed.org/markets/domestic-market-operations/monetary-policy-implementation/repo-reverse-repo-agreements/TOMO-Repo-Collateral-Schedule)
contains specific eligibility/margin terms and can change. This model does not
hardcode that facility's terms for arbitrary private repos; callers supply
reviewed, versioned eligibility and haircut snapshots. The
[reference-rate methodology](https://www.newyorkfed.org/markets/reference-rates/additional-information-about-reference-rates)
distinguishes observation and publication and allows revisions, motivating exact
available-time and immutable fixing identity.

Issue #21 additionally requires native graph risk/runoff and scenario adoption,
daily journal/liquidity/FTP integration with verified publication, supported legal
collateral/closeout policies, scale gates and observed conditional funding-access/
rollover evidence. Those criteria remain open until their own evidence exists.
