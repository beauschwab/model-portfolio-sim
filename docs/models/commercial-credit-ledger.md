# Commercial contractual rights, credit carry and shared daily ledger

The explicit `commercial-credit-ledger-1` contract joins complete original/current
Physical term sources, one event-time credit path, coupon-level legal/recognized
rights, original-yield carrying value and the existing daily `Sim`. It requires an
explicit source selection and compatible native artifacts. It never changes a
historical saved contract or selects this model from an old `annuity` label.

## Financial owners

`term_enhancements/indexed.rs` owns principal and coupon cash, including supplied
fixings, observation/reset/payment dates, contractual day counts, caps, floors,
margins and compounding conventions. Its optional native segment capture retains
the actual pre-observation accumulator and exact full factor endpoints. Native
factor descriptors have no deserialization or caller restoration constructor.

`commercial_factor_source` resolves the captured segment at actual fractional
credit-clock endpoints under the selected reviewed clock policy. Legal earning is
the native factor difference times the native period entitlement principal, once.
That entitlement can remain nonzero after an early principal payment. Replacing
it universally with current live principal would erase delayed or prepaid rights.
Each coupon retains its own source event, audit, cash and earning-origin identities.

The actual-path factor owner requires every indexed fixing, source availability
and selection cutoff no later than actual payment midnight. Availability and selection
must also be no later than each interest-segment start midnight; term reset cutoffs
must be no later than the period start. Intraday contingent rate ownership is unsupported in
this selected owner. Retained-payment balances inherit prior indexed information
cutoffs, so later fixed cash cannot reveal a future indexed balance. Already
published lookback fixings remain supported. The broader supplied indexed
arithmetic analysis keeps its separate contract.

`credit_path_income` runs inside the existing CTMC event loop. It retains the
actual occupied state, transition clock and shared draws. On an occupied span
`dt`, effective interest is

```text
E = G * expm1(log1p(reviewed_annual_effective_rate) * dt)
G_next = G + E - actual_cash + funded_draw
financial_basis = G_next - principal - recognized_coupon_rights
```

Original acquisition cost and yield reconcile only to the complete original
cash source. A changed current source cannot refit them. Explicit prospective
recognition-rate reviews and stopped-income reviews are separate from that
immutable original anchor. A zero yield alone does not select nonaccrual.

`commercial_coupon_accounting` retains legal coupon earning/cash and recognized
coupon earning/cash separately. Actual cash selects a native coupon event; it
cannot settle all outstanding coupons using one aggregate fraction. Signed
recognized lots are classified before aggregation. A legal/recognized difference
remains visible rather than disappearing into a basis or terminal cash plug.

Default transfers the actual gross principal, basis and coupon lots. Economic
LGD and modeled recovery amounts are distinct from reviewed allowance, chargeoff,
legal waiver and book disposal. Unpaid claims remain explicit. Exact zero may omit
a posting; tolerances never authorize erasing a tiny outstanding right.

Integral fee cash is collected while live. It does not introduce an earned legal
fee claim at default. Optional reviewed prepayment premiums use actual live
principal. Opening undrawn commitments enter default EAD at the actual occupied
origin's supplied CCF. Explicit performing draw tranches retain their own actual
funding date, acquisition cost, original effective yield, principal and coupon
entitlements; eligibility is supplied and undrawn overspending rejects.
Issuer-credit debt and credit-free references are separate selected
identities, rather than an implicit spread interpreted as both reference and credit.

`maturity_basis_policy` may explicitly select
`dispose_financial_basis_after_exact_legal_close-1` with a reviewed identity.
After all complete source cash, a native zero-cash exit requires principal and
legal interest to be exactly zero and disposes the remaining financial carrying
basis through the reviewed exit policy. Default claims never enter that exit.
Absence retains the existing explicit closing claims. At every source clock,
canonical legal balance is native entitlement principal times the cumulative
source factor minus the actual coupon cash collected. This removes accumulation
drift without inventing terminal cash or waiving a legal right. Legal earning
and recognized earning retain distinct deltas and gross coupon identities.

Optional `reviewed_credit_actions` supplies separately identified provisions,
chargeoffs and legal waivers through the shared reviewed accounting primitive.
Book principal, allowance and legal principal remain separate from modeled
principal, gross carry and economic loss. A chargeoff does not cancel modeled
recovery cash or turn modeled LGD into a second provision. Complete later recovery
and reviewed action clocks extend replay beyond the contractual reporting date.

## One journal

`commercial_ledger` admits all original/current source, credit, coupon inventory
and daily work before source generation. It retains each source once and gives
borrowed accounting groups to `commercial_intent_owner`. That owner reuses the
shared account mapper and signed-right classifier, then enters `Sim::raw_post`.
It does not run another credit generator or concatenate standalone journals.

Monetary participation is `allocated_face / native_unit_face * source_weight`,
once. Native unit face must equal actual opening source principal. Every path is
validated, including zero participation. Scenarios retain identical original
source/cost/yield and complete draw-stream/path identities. Physical intensities
remain separate from a Pricing measure and an OAS calibration.

Opening account cash/equity is posted once by the shared specification. Commercial
acquisition uses that cash. Exclusive placeholders reject generic cash overlays,
position policies and collateral ownership; actual pooled cash floors remain
enforced. Capital, liquidity and HTM eligibility require separate explicit policy.
Acquisition is attributed to actual day zero. The raw reporting horizon remains
immutable; native preparation derives a separate complete calendar through the
contractual tail plus the maximum reviewed recovery lag, bounded to 1,080 days.
Other generic positions, netting, pooled fees/taxes and policies reject in this
selected independently verified publication tier. Sharing Sim does not imply
arbitrary mixed-book adoption.

Subday actions retain native credit years and stable chronological order under
`actual_credit_clock_order_day_end_journal_no_retiming_source-1`. Journal days use
the enclosing day end; exact integer source dates preserve their native bits.
The daily journal does not claim to record the precise intraday legal timestamp.

`commercial_source_lineage` retains complete native atomic groups and transaction
numbers, source acquisition offsets and explicit native source classes.
`commercial_daily` and `commercial_closing` retain unweighted gross book
and coupon lots alongside path/allocation identities. Historical `exposures`
placeholder rows are not commercial credit exposures. Protocol 6 identifies this
explicit journal; historical protocols remain separately readable.
This raw route retains the commercial-loan class. Issuer-credit assets require
the explicit amortized-cost security candidate owner; they cannot enter a loan
placeholder. AFS/HTM marks and institution funding liabilities need separate
owners and policies.

[Commercial treasury and candidate contracts](commercial-treasury-candidate.md)
describe the separate exact-closing capital, daily FTP and purchased-journal gates.

## Admission and verification

The source components retain their own 2M work/32MiB limits. Joined preparation
admits at most 512 source requests, 1MiB raw JSON, 8M aggregate work and 128MiB
retained component capacity. The bounded complete header/journal output is at
most 32MiB. These budgets do not promise aggregate process RSS or full 60,000
position mixed graph/credit/solver/journal acceptance.

The standalone native dispatcher retains the original input clock and inherited
cancellation. Raw duplicate keys, incomplete scenario coverage, changed original
anchors, changed CRN, missing source weights and unsupported owners reject.
Private preparation and journal execution finish before emission. Write/flush
failures and final checkpoints remain failures.

Durable publication requires independent raw source, credit/carry, coupon book,
ordered journal and persisted partition replay, with the original complete-job
deadline, lease/token and current input/executable/artifact fences. The native
receipt does not set its own independent-verification flag. Synthetic checks do
not establish observed predictive accuracy, accounting approval or deployment.
The exact current implementation and validation status is recorded in
[the issue implementation ledger](implementation-plan.md).

## Primary references and policy limits

The [ARRC technical appendices](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2020/ARRC-Syndicated-Loan-Conventions-Technical-Appendices.pdf)
explain why simple/compounded interest and observation shifts need distinct
weights. The selected contract still supplies its own conventions.
[IFRIC's June 2021 discussion](https://www.ifrs.org/news-and-events/updates/ifric/2021/ifric-update-june-2021/)
distinguishes market-rate resets and other changes in estimated cashflows. This
supports a separate reviewed prospective rate rather than overwriting an original
quote/yield anchor. The [FFIEC 051 glossary](https://www.ffiec.gov/pdf/FFIEC_forms/FFIEC051_202412_i.pdf)
separates nonaccrual, cash recovery and collectibility judgments. Accordingly,
stopped recognition does not infer principal chargeoff or extinguish legal rights.
These references guide required policy fields; they do not approve supplied
research policies or certify an accounting methodology.
