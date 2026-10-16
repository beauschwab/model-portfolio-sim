# Separate fee rights in funding preparation

`modeled-product-ftp-source-channels-3` is an additive native funding component.
The historical source1/source2 contracts retain their original channel inventory,
policy domains, work calculations and serialized result shape. The fee component
does not itself establish a source-backed commercial treasury publication.

## Reviewed channel ownership

The common Rust preparer accepts all eighteen existing loan/credit channels plus
six separate, nonnegative gross amounts:

| Channel | Source-owned stock |
|---|---|
| `fee_receivable` | Earned service money receivable |
| `fee_payable` | Earned service money payable |
| `deferred_fee_income` | Collected service consideration not yet earned |
| `prepaid_fee_expense` | Paid service consideration not yet expensed |
| `acquired_fee_asset` | Remaining separately allocated positive fee cost |
| `acquired_fee_liability` | Remaining separately allocated negative fee cost |

Every channel requires an ordered supplied coefficient in {-1, 0, 1} and a review
identity. Loan/borrower/economic memo and allowance channels remain excluded from
funding. Fee channels are not automatically eligible, assigned a sign, or netted
away. The supplied policy must decide which stocks participate and avoid double
counting overlapping legal rights and acquired cost. Financial carrying-value
allocation is distinct from actual cash funding demand and regulatory exposure.

For each actual date, the preparer reconciles every channel to its complete source
origins and computes the supplied linear funding allocation. Every cash event
has either one reconciled funding assignment or an explicit reviewed exclusion.
Noncash changes require their own reviewed identities. Opening/trapezoid daily
averaging, reference and liquidity tenors, negative-funding treatment and complete
gross termination remain explicit policies. Allocation and Physical probability
apply once. Zero allocation still verifies the unweighted rights and tail.

## Income and cost

Four additional earnings channels retain service revenue, acquired cost release,
fee writeoff and fee recovery. They are separate from original effective-interest
income, reviewed loan basis realization and credit loss. Cash collection does
not imply new service income. A cost release is a reviewed supplied schedule,
not an automatically inferred accounting method.

For example, service income 5 and acquired fee cost release 3 give fee profit 2.
At a Physical participation of 0.25, the contribution is 0.5; original loan
effective-interest income remains unchanged. Gross rights, cash assignment and
cost stocks reconcile separately rather than being substituted for that profit.

## Bounds, validation and adoption

The selected fee tier keeps the 64-day, 2-million work and 1-MiB component bounds.
It charges the complete 24-channel inventory in its work calculation. Unknown or
missing policies, nonfinite amounts, unassigned cash and incomplete gross rights
refuse even when allocation is zero. These limits are not an aggregate RSS cap.

`portfolio-model-core/tests/fee_funding.rs` contains controlled signed-channel,
income-separation, explicit-selection and zero-allocation/full-tail checks. The
independent `modeled_product_funding_reference.py` supports the new inventory;
it performs no production financial execution. Full native source/candidate,
ordered journal, treasury and persisted-publication acceptance must be recorded
separately. The component's source reconstruction, journal/FTP adoption,
accounting approval and empirical eligibility flags remain false.

## Research basis and policy boundary

The [FSI liquidity transfer pricing paper (2011)](https://www.bis.org/publications/fsi-paper-10-liquidity-transfer-pricing-guide-better-practice)
supports explicit treasury policy, maturity-sensitive funding costs and contingent
liquidity charges. It is a better-practice research paper, not a new regulatory
capital or liquidity rule. Here it motivates separating funding tenors and
supplied allocations from the recognition of service income.

[IFRS 9, B5.4.1–B5.4.3 (2022 issued text)](https://www.ifrs.org/content/dam/ifrs/publications/pdf-standards/english/2022/issued/part-a/ifrs-9-financial-instruments.pdf?bypass=on)
distinguishes fees integral to effective interest from other service fees. This
model requires that classification as reviewed input. Its separate fee owner and
supplied release schedule are implementation choices, not a finding that a
particular contract complies with IFRS, US GAAP or regulatory policy.

## Source-owned candidate capital and FTP

The additive `commercial-candidate-fee-treasury-1` route supplies
`raw_candidate`, optional reviewed `capital` and optional reviewed `ftp`. The
raw candidate must select the finite `commercial-candidate-journal-1` route and
separate fee ownership for every Physical path. The native coordinator executes
that candidate once, retains its sealed loan and acquired fee owners, and uses
their existing protocol7 daily Sim journal. It never accepts a prepared monetary
receipt or reruns a separate loan/fee source to prepare funding.

Native replay checks chronological transactions, balance and exact trial-balance
coverage. Funding reads absolute daily loan and individual fee stocks from that
same Sim; its commitment cursor reads the retained native carry points. Cash and
earnings come from the retained atomic groups. Loan acquisition cash and separate
fee acquisition cash each enter once. Service earning, acquired cost release,
writeoff and recovery remain separate from original loan EIR. Actual purchase
offsets and the complete fee-only cash tail survive loan principal maturity.
Physical probability and face participation apply once; zero participation still
requires and verifies the complete unweighted source and all reviewed policies.

Capital uses the exact complete closing date and the journal-replayed trial
balance. Only the explicitly recognized balanced legal memo pairs are removed.
Fee receivables, prepaid fee expenses and acquired fee assets require their own
reviewed GL weights. A loan instrument weight cannot supply those fee weights.
Each fee asset GL has a separate exposure group, preserving loan lineage without
implicitly netting fee and loan exposure. Fee payables, deferred fee income and
acquired fee liabilities retain separate gross liability presentation. These
accounting mappings confer no regulatory eligibility or methodology approval.

The selected route retains the existing 32M candidate work and 128 MiB combined
native reservation, with a separate 2M bridge admission and 2 MiB raw envelope.
All source tails and the outer calendar must fit 64 days; at most 32 Physical
paths and 128 origins per source3 state are admitted. The bridge reserves its
additional 64 MiB Value phase inside the combined reservation, independently of
the unchanged 32 MiB encoded
result limit. Raw and candidate Value copies and funding request/result copies
are charged before conversion using a conservative 20-times serialized byte
bound. Completed Values move into the result rather than being cloned again.
Capital/FTP report creation has an additional bound from its complete typed
inputs, checked before creation and against the actual result. JSON transport,
encoded output and component reservations remain separately bounded phases;
these contracts are not an aggregate process RSS guarantee.

Daily loan and fee rows use borrowed keyed indices; commitment points advance
monotonically. Bridge work charges native source-group/origin walks, index and
daily channel/origin work before financial generation. Overflow, missing review,
incomplete channel/weight inventory, excessive tails and incompatible source
ownership refuse. The source3 component's explicitly signed research funding
remains incompatible with the existing nonnegative Treasury position domain.
The bridge rejects that case; a compatible explicit channel allocation can still
retain and report negative fee rights without assigning them funding weight.

The independent `commercial_fee_candidate_treasury_reference` recomputes raw
graphs, original quote/OAS/EIR, fee acquisition/release and source cash; verifies
the complete ordered Sim journal; then independently constructs all 24 funding
channels, 11 income channels, capital and Treasury outputs. Persisted restoration
uses the same raw-source oracle. The synthetic example is
`examples/models/commercial-candidate-fee-treasury-1.json`: loan settlement on
day 35, separately acquired fee cost/right through day 40, and complete day 45 tail.
Three targeted native bridge controls, one nonzero gross fee capital/weight
control and 14 actual native/independent engine controls passed locally. Twelve
public durable API publication, reopening, rehashed monetary tamper and final
SQL-fence controls passed separately. Matching official artifact/full-suite
acceptance remains the parent checkpoint's responsibility.

This bounded finite candidate stage does not complete arbitrary mixed-book,
seasoned holding, generated fee stopping, observed calibration or the 60,000-position
joint graph/credit/solver/journal requirements. Opening identity, accounting,
regulatory, empirical and general mixed-adoption flags remain false.
