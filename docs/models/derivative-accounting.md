# Explicit derivative accounting intents

`portfolio_risk_native::derivative_accounting::evaluate` consumes an immutable
`DerivativeAccountingRequest` and returns a `DerivativeAccountingResult`.
The schema and model version are both `derivative-accounting-1`. It is an additive
component: the saved `derivative-contracts-1` and `derivative-contracts-2` valuation
contracts retain their own admission and economics. MPOR requests still require
an explicit V2 selection; enabling accounting does not upgrade a saved derivative.

This component prepares balanced journal **intents for one modeled Physical
scenario**. It does not claim observed settlements, a complete financial reporting
standard, persistence, saved-book adoption or verified publication. The immutable
request must accompany the result when a coordinator publishes it.

## Supported reviewed policies

The caller supplies an accounting policy identity/revision and one reviewed
designation per mapped position. `undesignated_trading` and
`undesignated_economic_hedge` are supported with
`clean_value_changes_in_earnings`. Formal cash flow, fair value and net investment
hedge designations are rejected: there is no effectiveness assessment, hedged-item
accounting, OCI recycling or designation history in this component.

Coupon presentation is explicit:

* `settle_derivative_carrying` transfers the signed contractual right from
  derivative carrying into cash or an unpaid receivable/payable.
* `contractual_income_at_due_with_mark_offset` records signed income at the due
  date and independently marks the remaining derivative. Its mark offset prevents
  counting the same coupon in income and derivative value. This policy is due-date
  presentation, not a daily earned-interest accrual or effective-yield policy.

`reviewed_modeled_closeout_shortfall_in_earnings` recognizes the native reviewed
counterparty closeout loss and posted-collateral return loss separately.
`segregated_custodian_memo_only` records received IM principal as equal custody
asset/obligation memo balances. Received IM is not entity income, cash or a source
of funding; native custodian interest is retained as a third-party diagnostic.
Posted IM remains a financial asset, and its actual native return/remuneration
affects entity cash. Signed VM becomes received VM liability or posted VM asset;
crossing zero uses the gross accounts and never silently nets presentation.

XVA and modeled funding costs are `report_only`. An optional raw Pricing
derivative request is independently evaluated for XVA diagnostics; it must share
the exact opening clean value, CRN, entity/currency and legal trade mapping.
Its CVA/FVA never change the Physical journal or automatically adjust carrying.
Actual Physical default recovery/loss is recognized through the closeout waterfall
once. A supplied spread cost diagnostic is not an observed financing transaction.

These are reviewed model policies rather than an inference of accounting rules.
[IAS 32](https://www.ifrs.org/issued-standards/list-of-standards/ias-32-financial-instruments-presentation/)
sets requirements for financial offsetting; a supplied enforceable netting flag is
not by itself a financial statement offsetting conclusion.
[IFRS 9](https://www.ifrs.org/issued-standards/list-of-standards/ifrs-9-financial-instruments/)
requires designation and qualification for hedge accounting. The
[March 2019 IFRIC discussion](https://www.ifrs.org/news-and-events/updates/ifric/2019/ifric-update-march-2019/)
also distinguishes physical settlement from reversing previously recognized
fair-value results. The component therefore retains prior mark earnings and
requires an explicit delivery-recognition review rather than erasing them.

## Input and early admission

The raw `derivative` operation must be a native `netting` lifecycle. Standalone
swap/option snapshots lack the daily path and cannot be silently expanded.
Admission requires exactly one Physical path with probability exactly one,
`modeled_physical_scenario`, actual `currency_units`, matching entity/currency,
complete reviewed trade coverage and every calendar-day raw node from opening
through the full contractual/default/collateral tail. Missing daily marks are
rejected; no interpolation, extrapolated coupon source or report-horizon tail
truncation is introduced. Each native node retains its exact valuation cutoff,
curve/source identities, payment timing and legal mapping.

Opening anchors explicitly provide settlement cash, every mapped clean carrying
value, posted VM, received VM, posted IM, received IM custody and an equity credit
balance. Financial opening debits must reconcile to that equity anchor, and
carrying/collateral must reconcile to native opening marks/CSA stocks. Cash may be
signed; its financing or overdraft treatment is not inferred. Nonzero opening
collateral cannot be allocated across independently reviewed buckets implicitly.

Normal coupon cash is derived from raw contractual legs. Floating cash requires
observed fixings or a declared Physical path with information available by the
node cutoff. `physical_index_paths` supplies immutable observations keyed by
mapped position and contractual period, using the same reviewed reset/calendar
conventions as the raw leg. A Pricing projection cannot become Physical cash by
changing an output label. Unknown, duplicate, unused or late fixing inputs fail.
Publication/reset cutoffs and rate bounds are validated by shared native indexed
arithmetic; calendar/source mapping remains supplied rather than independently
verified against a fixing provider.

MPOR uses V2's native retained obligations and identity/date-only settlement
instructions. Physical MPOR observations belong to the MPOR policy in the raw
derivative request; normal accounting observations belong to the accounting
request. Raw cash/exposure overrides are not admitted. A cash European election
uses its selected native contractual settlement price; physical delivery changes
the carrying owner to the admitted delivered swap and retains future leg cash.
Short delivery uses the explicitly reversed underlying contract required by the
native legal mapping. No intrinsic cash is added to physical delivery.

The initial tier admits 256 trades, 4,096 daily nodes and 250,000 combined native
work units. Before schedule allocation it charges raw Physical and optional
Pricing paths, observations, native MPOR work and each independent due-right
repricing, with saturating work arithmetic. Requests are limited to 32 MiB;
journal output to 250,000 intents. Finite monetary values and cumulative account
balances retain the native monetary domain. These component bounds do not claim
an aggregate process RSS limit or acceptance of the full 60,000-position workflow.

## Replay and output contract

Each dated signed-debit event balances within its financial or custody ledger.
Positive amounts are debits and negative amounts credits; generic typed account
codes require a reviewed adapter mapping before persistence. Every intent retains
run/scenario/entity/currency, Physical measure, policy, source identities, mapped
position, current contractual owner and legal bucket. Zero events do not create
empty postings. The returned native result still retains zero/raw marks.

Daily replay first retains native collateral flows, reconstructs the clean mark
including any same-day contractual due right, recognizes or retains signed cash
rights, and marks the remaining derivative. Before/after scheduled payment
snapshots partition the same due cash explicitly. An option delivery transfers
carrying between owners with balanced book postings and no settlement cash.
MPOR settlements clear individual retained rights by identity; aggregate native
movement is independently reconciled instead of being posted a second time.
Closeout removes future carrying, unpaid claims and collateral once, recognizing
native entity cash and both types of shortfall. Subsequent marks/rights remain
extinguished.

`cash_rights` reports Physical amount separately from the Pricing amount excluded
from clean carrying, selected fixing/model sources, recognition sources, current
contractual owner, due date, settlement date and closeout inclusion. `native`
retains raw clean trade marks, reviewed mapping, collateral/call records, default
waterfall, MPOR policy, obligations and exercises. `daily` reconciles independent
gross rights/cash/collateral state to the replayed financial and custody trial
balances. The full tail ends with no derivative or unpaid rights and exactly the
native residual collateral stocks; unsupported residual rights fail publication
preparation rather than being dropped.

## Independent fixtures and example

The native tests independently sum every intent and replay its signed postings
from explicit opening balances. Hand fixtures cover payer/receiver coupons,
negative actual rates, coupon-income offsets, before/after marks, delayed/outage
VM calls, custody and posted IM, negative collateral remuneration, long/short
physical delivery, cash European settlement, retained/settled signed MPOR rights,
default recovery and posted-return losses, report-only nonzero CVA, finite/source/
policy/measure/calendar/work rejection and unknown cash overrides. The existing
derivative valuation fixtures continue to run unchanged.

[`derivative-accounting-1.json`](../../examples/models/derivative-accounting-1.json)
is synthetic: N=360, ten ACT/360 days, fixed 5%, observed floating 10%, DF=1.
Fixed cash is -0.50 and floating cash +1.00, so net settlement is +0.50.
Opening cash 100 and derivative asset 0.50 reconcile to equity 100.50.
The twelve daily snapshots end with cash 100.50, zero derivative/VM/IM and zero
net mark earnings; VM receipts and returns cancel. This is a component fixture,
not a market calibration or production book.

Remaining #20 gates include raw saved-book mapping, durable daily-ledger adoption,
independent persisted replay/publication verification, reviewed account mapping,
actual transaction ingestion, journal-to-capital/FTP reconciliation, production
scale/deadline acceptance, empirical option/XVA validation and legally supported
closeout/hedge-accounting scope. This component does not close those gates.

## Closing capital feed (`derivative-treasury-1`)

`portfolio_risk_native::derivative_treasury::evaluate` (protocol
`derivative-treasury-1`, `analytics.model_contracts.prepare_derivative_treasury`)
re-executes a standalone reviewed `derivative-accounting-1` request natively and
feeds its balances to the existing treasury bridge (`treasury/bridge.rs`,
`treasury-1`) under supplied ledger policies. It is the no-hedge counterpart of
`hedge-treasury-1`: every mark stays in earnings, so no cash-flow-hedge AOCI,
basis adjustment or reclassification row exists and the reviewed
`aoci_eligibility` policy block is optional (absent, book AOCI is zero, eligible
AOCI equals included AOCI and the extended AOCI audit keys are omitted, exactly
as the native bridge emits them).

Request: `schema`, the complete `derivative` request, `account`, a reviewed
`mapping` identity, the `treasury` request with `as_of` and empty supplied
capital/positions/curves/deltas/units, 1..=16 `policies` naming the derivative
scenario, account and the closing date as capital period, a positive finite
`amount_multiplier`, and an optional `interim_report` flag. Balances are
accumulated per `(account, instrument)` from Financial-ledger intents with
ordinal at or before the reporting node plus the opening anchors (cash, margin,
opening equity, gross opening trade values), reconciled against that node's
`financial_trial_balance`, and mapped: `cash`, `derivative_value` (gross asset
and liability per instrument), `posted_margin`, `received_margin`,
`opening_equity` and the `pnl:derivative_*` earnings GLs. Custodian memo
accounts map to nothing; near-zero rows are omitted; unpaid rights at the
reporting date refuse. `treasury.as_of` must equal the complete-tail closing
date, or with `interim_report` a derivative path node strictly before it.

Result: the reporting node (`closing_ordinal`, `closing_date`), rows, the
embedded derivative result, the bridge `capital` output (thirteen ratios, no
FTP positions or limits) and explicit flags: `closing_capital_selected`,
`interim_capital`, `ftp_selected=false`, `hedge_accounting_applied=false`,
and false accounting-standard, regulatory, empirical and persisted-verification
claims. `derivative_treasury_reference.verify` independently re-derives the
derivative journal with the existing raw verifier, rebuilds the rows through
the selected node, recomputes the capital audit and ratios, and compares every
field; its receipt reports `derivative_closing_capital_recomputed`. The durable
`/models/runs` route, Model Lab panel and example `derivative-treasury-1.json`
reuse the existing fences. Mixed saved-derivative journals, FTP and regulatory
approval remain open.

## Independent raw-source verification

`portfolio_risk.analytics.derivative_verification.verify_derivative_accounting(raw_request,
result)` runs after native calculation. It returns an immutable
`derivative-accounting-verification-1` receipt only after the complete result
passes; unsupported conventions or mismatches raise `DerivativeVerificationError`
and return no receipt. The module has no native callbacks, application pricer
imports, backend selection or financial execution route.

The verifier reconstructs actual-date year fractions with Python's independent
calendar, fixed and indexed cash from supplied raw observations, exact discount
PV, conditional European intrinsic/cash carry, explicit delivery and election,
Physical MPOR obligations, signed lagged/failed margin calls, custody/principal
returns and the reviewed closeout waterfall. It then independently constructs
reviewed recognition events, sums signed postings and reconciles all daily gross
rights, cash, collateral and trial balances. Producer clean marks and reported
cash are checked against raw arithmetic rather than accepted as economic truth.
Optional raw Pricing XVA references are separately reconstructed and remain
report-only. All supported accounting policies, day counts, coupon/margin
conventions, legal buckets and European/MPOR modes are checked; there is no
approval for a partially supported result.

Financial comparisons preserve the existing 1e-12 relative / 1e-9 absolute
tolerance. Identity, revision, date, cutoff, measure, enum, event order and field
coverage comparisons are exact. Because both implementations post any nonzero
change, a last-ulp difference in a near-zero carrying value (an at-market swap
after a settlement, or exactly offsetting legs) decides whether a mark intent
exists at all; the verifier therefore treats an intent whose postings all lie
within the 1e-9 absolute tolerance of zero as a zero event on both sides and
renumbers the remaining intents in order before comparing
(`normalize_intents`). Daily balances are still compared in full. The canonical raw-request SHA-256 in the receipt
binds the reviewed source; checked day/journal/right/path counts and the optional
Pricing diagnostic check are reported. Unknown output fields and duplicate,
omitted or balanced-but-economically-wrong entries fail.

The test suite invokes fresh native artifacts for independent source cases and
tampering cases, including amounts, dates, sources, policy reviews, default,
exercise/delivery, margin and diagnostic CVA. A fresh subprocess forbids Python
financial callbacks and disables the independent oracle during native calculation,
then enables the verifier afterward. Verification failure is tested before any
publication write. No fixture is a market calibration or accounting qualification.

Persist variant-bearing nested data as lossless canonical JSON cells (or an
equivalent immutable JSON payload) and decode it before re-verification.
`RateSource` and MPOR settlement instructions have heterogeneous tagged fields;
automatic Polars struct union can add undeclared null fields. Those additions are
intentionally rejected rather than silently normalized into a different source
shape. Every JSON serialization must reject nonfinite values and retain exact
integer date/cutoff types.

The receipt explicitly reports `persisted_journal_verified=false`. Verifying the
native model JSON, and later re-verifying a decoded persisted model-result cell,
is **analysis-source verification**. It is not independent persisted mixed/saved
ledger replay, account-mapping approval, immutable ledger publication or adoption
into daily capital/FTP. Those gates require the corresponding coordinator and
persisted financial journal verification, and remain separate.
