# Acquisition-aware performing product candidates

`product-candidate-journal-1` is a standalone native expected journal source for
selected `product-units-1` consumer installment, revolving card, fixed mortgage
and deposit candidates. It consumes the actual immutable native preparation,
explicit allocation face, reviewed original effective yield and purchase quote.
It does not compose an existing mixed book or establish that supplied opening
provenance has been independently verified.

The canonical raw request is [product-candidate-journal-1.json](../../examples/models/product-candidate-journal-1.json).
The standalone `portfolio-strategy` envelope is:

```json
{"op":"analyze_product_candidates","schema":"product-candidate-journal-1","execution_timeout_ms":60000,"request":"replace with the complete typed raw request"}
```

The executable returns `{ "ok": result }` only after complete native evaluation
and output admission. A rejected or cancelled evaluation emits an error and no
successful candidate result. No database, Parquet artifact or manifest is written
by this component.

## Source and accounting boundary

Raw specification accounts must reconcile opening cash and equity. Every mapped
candidate is an explicit zero-balance placeholder with zero accrued interest,
basis and allowance. The mapping retains unit, position, account, source and
reviewed policy identities. All supplied physical scenarios and paths are
retained, including zero-weight paths and zero allocations. Pricing histories
remain in the raw/native preparation receipt; pricing cash does not post.

Acquisition follows the supplied opening anchor at the exact purchase date.
The posting uses actual quote times allocated face, with principal equal to face
and book adjustment equal to cost less face. Each physical path receives its
explicit probability weight exactly once. Deposit issuance reverses the bank
cash/principal direction; it does not become an asset purchase.

Daily recognition uses reviewed original annual effective yield and the native
dated-income recurrence. Contract interest is distributed over explicit calendar
recognition intervals under ACT360 or ACT365 fixed only. Recognition posts signed
accrued rights, book amortization and effective-interest earnings; contractual
cash then settles those rights and principal on the exact supplied event date.
Early settlement can produce deferred income or prepaid expense rights. No
terminal principal, interest or book plug is permitted. All complete cash and
recognition tails remain, including dates after the reporting end.

Contractual interest direction is derived from the complete unweighted physical
earned and cash source, including zero-allocation and zero-weight paths. Uniform
positive or negative coupons are admitted; paths whose contractual direction
changes require separate gross obligation ownership and reject at this journal
boundary. Principal cash direction and accounting effective yield are separate.
A negative-coupon deposit accrues interest receivable before cash receipt and
deferred income after early receipt. A positive-coupon deposit instead accrues
interest payable and prepaid expense after early payment. Classification changes
neither the signed monetary right nor earnings, cash, principal or basis. Negative
original effective yield alone does not make a contractual coupon negative.

The exact supported policy is `reviewed_original_eir-1`, acquisition phase
`after_verified_opening_before_recognition-1`, and `retain_complete-1` tails.
Generic fees, taxes, distributions, default/credit, mark, collateral and existing
mixed-source owners reject. The component does not infer a hedge designation,
accounting standard, observed cash settlement, calibrated behavior, regulatory
capital, replacement funding or backbook scenario KPI. The underlying product
preparation retains its performing-only and supplied-history limitations.

## Journal and result contract

Postings use debit-positive signed values. Journal order is scenario preparation
order, calendar date, physical source order, then acquisition, recognition and
settlement. Principal and basis trial balances retain position and path keys;
cash, equity and effective-interest earnings are pooled by account. Signed
interest rights retain receivable, payable, deferred-income or prepaid-expense
codes. Every event is balanced and every retained daily position is reconciled
to its original-yield source and its path-keyed GL balance. Aggregate account
cash and report income are independently checked against selected source replay.

The result includes the complete raw request, native normalized specification,
immutable preparation, native source index and raw daily source requests/results,
all journals, daily position/account states, report-date and complete closing
states, and closing trial balances. `verified_opening_identity` is supplied
provenance only. `independent_persisted_verification` and
`dynamic_mixed_acceptance` remain false in the native result.

Publication requires an independent verifier to reconstruct raw product
economics, original-yield recognition, acquisition, ordered journals and closing
GL, then reread the persisted artifact and verify the same receipt. That gate is
separate from native producer reconciliation and from future mixed-book daily
adoption.

## Admission and synthetic evidence

Before daily source/journal generation, native admission sums all physical paths,
complete days, account/scenario states, journal lines and retained source receipt
bytes. Limits are 4095 days per purchase tail, 250,000 daily source/state/account
rows, 2,000,000 conservative posting lines, 8,000,000 work units, a 32 MiB raw
request and 128 MiB conservative retained/output capacity. Completed serialized
output is checked again. Cooperative cancellation runs during admission,
source generation, daily replay and serialization. Capacity admission measures
the actual escaped owner, policy, run, path, yield-anchor and complete cash and
recognition-origin identifiers. It charges those lengths against conservative
per-path record counts, including repeated daily lineage and closing records;
account records and full raw/prepared receipts are additional. Every capacity
and work multiplication/addition is checked before daily vectors are generated.

The reused raw specification also requires a 30 to 1080 day reporting horizon,
at most 50 accounts and at least one explicit nonbaseline scenario. The native
baseline and every supplied scenario/reverse severity need an exact source
mapping; a smaller product source may still close before the reporting end.

The example has two independently supplied scenarios and two loans allocated
20 and 80 currency units of face. Let `cost = 20*quote1 + 80*quote2`. Each
scenario buys 100 face, receives 104.45 complete contractual cash, closes cash at
`1000-cost+104.45`, and recognizes `104.45-cost` effective interest. Neither
scenario changes these values through cross-scenario netting. This is a synthetic
hand fixture, not empirical acceptance or a scale benchmark.
