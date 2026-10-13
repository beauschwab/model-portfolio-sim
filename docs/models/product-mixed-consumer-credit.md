# Consumer product events in the mixed daily GL

`product-mixed-consumer-credit-1` adds acquired physical auto/personal loan paths
to the native performing `product-mixed-dated-1` source. It executes one
chronological `daily::Sim`, one account cash/equity state and one protocol-3
journal. Rust derives consumer cash, original effective-yield recognition,
defaults, chargeoffs and recoveries from immutable raw contracts and reviewed
policies. Callers cannot supply precomputed intents or financial amount overrides.

The CLI operation is `analyze_product_mixed_consumer_credit`, with envelope
`{op,schema,execution_timeout_ms,request}` and the usual `{ok:result}` or
`{error:message}` response. The request fields are `schema`, `performing`,
`combined_specification`, `consumer_owners` and `policy`.

## Actual opening and exclusive ownership

The nested performing specification and combined specification contain the same
actual accounts, opening cash/equity, existing positions and scenarios. The latter
adds exactly one zero-opening placeholder per consumer owner. Neither nested
opening nor standalone accounting seeds are added to account equity.

An owner declares its unit/position/account IDs, unit face, allocated face,
amortized-cost category, economic source, original-yield anchor, explicit initial
allowance review, and complete run/path coverage. Every path contains a raw
`consumer-credit-accounting-1` request and physical weight. Original acquisition
cost, principal, date, yield and reviewed policy must agree across the owner's
paths and runs. Each raw contract run ID matches its declared ledger run.
Weights sum to one per owner/run. Allocation face divided by unit face and the
physical weight are applied exactly once to postings. Unweighted native state
and complete source receipts remain retained, including zero allocations and
zero-weight paths.

Consumer positions cannot also have dated ownership, legacy cashflows, generic
credit/interest/draw/runoff/maturity/marks or collateral policies. The daily
generic position path skips them. Their product-event owner exclusively updates
live/default principal, signed accrued rights, basis, allowance and claims.

## Journal phases and recognition

Opening is posted once. Each day executes dated and consumer acquisitions,
dated recognition, native consumer event bundles, dated cash settlement and
closing checks. There is no new day-zero dated accrual. Consumer purchases post
principal and cost-minus-principal basis against actual purchase cash. The
primitive's opening allowance is recognized in a separate reviewed acquisition
expense/contra transaction; it is neither free opening equity nor an implicit
PD times LGD provision.

Core event bundles retain their native order: original EIR/contractual
recognition, contractual cash, reviewed BeforeCredit allowance, default
recognition, explicit chargeoffs, gross recovery, AfterCredit allowance and
reviewed recovery cost. A shared `raw_post` receives each complete balanced
transaction. Cash and earnings already present in those intents are updated
once; the helper that appends cash/P&L is not used.

Gross recovery and collection cost remain separate cash postings. Defaulted
principal, accrued interest and basis have distinct GL channels. Reviewed
allowance, accrued reversal and basis policy are independent of economic loss.
Signed live/default accrued rights split into receivable/deferred buckets within
each path before physical-path aggregation. Thus opposing paths cannot net away
gross rights. Snapshot reconciliation checks the native source against balances
derived from the journal; it does not overwrite them using a second monetary
projection.

`consumer_acquisition_lineage` retains source/path/weight/allocation, raw
acquisition and original-yield metadata, explicit allowance review and both
transaction numbers. A zero posting has a null transaction number.
`consumer_lineage` retains the Core intent index, claim/policy/source IDs,
original yield, product-event IDs and the actual mixed transaction number.
`consumer_daily` retains each unweighted actual-date state, including pre-purchase
zero state and the preserved closing state after a shorter source ends.

## Full tails and clocks

The global closing date is the largest complete performing or consumer tail,
bounded to 1,080 days. Derived dated container windows may extend to that date;
original source ends and raw economics remain retained. The Core reporting
clock is explicitly the outer reporting date clamped to the path's acquisition
and full end. This selects its pre-purchase opening or completed state without
inventing a future contract interval.

Default/payoff consumes the Core-owned live rights and stops future contractual
recognition/cash for that path. Original payment dates and the economic exit
remain in the raw/native source receipt. No consumer payments are additionally
installed into the dated store. Default exit does not erase outstanding recovery
claims. Every unweighted live/default principal, accrued, basis and allowance
channel, and every recovery tail, must close under supplied reviewed actions.
Incomplete claims reject even when their path weight or allocation is zero.
No terminal liquidation, loss plug, inferred chargeoff or replacement funding is
generated. Cash-floor admission applies to end-of-day pooled cash, not intraday
liquidity.

## Admission and verification

The slice shares the existing 512-position, eight-million-work and conservative
128 MiB retained capacity limits. It admits at most 64 consumer owners and 32
paths per owner/run. All checked cardinalities and actual escaped metadata are
admitted before consumer source generation or performing daily capture.

For each path, with days D, contractual dates plus actions A and allowance plus
chargeoff reviews R, the conservative intent bound is `N=D+2A+6R+32`. Reserved
work is `D*(A+R+64)+32N`, plus 64 units per global path-date. Retained capacity
reserves `(14N+16(D+1))*(8192+M)` per path, where M sums UTF-8 serialized lengths
of every escaped string leaf in the complete raw Core request and owner/path
keys. It additionally reserves 8192 bytes per global path-date. Reservation uses
the maximum 1,080-day window before native performing preparation determines the
actual closing date. The performing capture admission adds this reservation
before allocating its generated schedules. Initial pricing preparation retains
its separate existing budget; these are not a combined process RSS guarantee.
Raw/final JSON is independently bounded to 32 MiB.

The result exposes direct journal, trial balance, closing/reporting balances,
dated rights, event/acquisition lineage and consumer tables. Independent source
and ordered journal reconstruction must gate durable publication and a fresh
persisted reread before verified-journal/capital use. Opening identities are
supplied provenance, not proof of observed settled balances. Native
`independent_persisted_verification` remains false.

## Supported scope and open gates

This version admits the primitive's supplied non-PCD gross purchase basis and
amortized-cost installment policy, plus the existing performing dated products.
Negative original EIR/basis can be explicit; the initial installment coupon lane
is nonnegative. Card, PCD, fees/grace/CCF, generic taxes/marks/netting, reverse
severity journals, empirical scenario calibration and joint stochastic adoption
remain outside this slice. It does not supply consumer pricing/solver credit
coefficients or claim an accounting standard.

Consumer placeholder regulatory/liquidity weights must explicitly be zero, and
account capital/liquidity/HTM constraint settings are unsupported. Daily research
RWA/liquidity columns describe the admitted performing policies, without an
inferred default-claim RWA model. `interim_default_capital_adoption` is false.
Complete-tail closing capital requires independently verified GL and separately
supplied eligible capital/weight policies. Existing `None` daily callers keep
their prior financial behavior.

[The synthetic example](../../examples/models/product-mixed-consumer-credit-1.json)
adds a 100-principal consumer purchase at cost 102 to the earlier performing
mixed example. Its default at day five reverses 0.5 accrued interest, recognizes
the remaining 1.5 premium and retains principal 100. Gross recovery 60 less cost
10 arrives at day 12; an explicit 40 principal chargeoff occurs at day 15. The
consumer loses 52 in total. Both supplied scenarios therefore close with
cash/equity `1,101.2506993647357 - 102 + 50 = 1,049.2506993647357`. This is a hand
reconciliation; it is not scale, empirical or regulatory acceptance.
