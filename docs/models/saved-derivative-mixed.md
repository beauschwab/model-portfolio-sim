# Saved derivatives in the shared dated journal

The explicit `saved-derivative-mixed-1` successor generates reviewed physical
derivative sources in Rust and executes their financial intents in the existing
dated daily `Sim`. It is currently isolated under `artifacts/derivative-mixed/`.
It is not installed public/default adoption or completion of issues #12/#20/#25.

## Inputs and ownership

The request contains `schema`, a complete `dated-ledger-1` backbook request, and
`owners`. Each owner names an actual ledger account and supplies one `runs` entry
for every configured ledger scenario, including baseline. Each entry names the
ledger scenario and supplies a raw `saved-derivative-source-1` request. Callers
cannot supply precomputed derivative intents, cash overrides or financial reports.

The saved-source contract validates original trade order, unique position and
legal ownership, calendar/source identities, complete opening anchors and explicit
reviewed legacy-row upgrades. All runs retain exactly the same saved book and
reviewed opening. Physical histories can differ between scenarios. Cash, entity,
currency, units and opening date must match the ledger account. A legal netting
bucket cannot be split between derivative owners of the same ledger account.
Legacy netting overlays and reverse-severity runs require their own complete joint
ownership contract and are rejected in this successor.

Risk owns all swap, European option, supplied-fixing, exposure, CSA, MPOR, default
closeout and physical derivative accounting calculations. The coordinator calls
that existing owner and transports its typed result. Ledger owns chronological
execution, GL reduction, daily account state and journal-derived closing reports.
Ledger does not import Risk or implement a second derivative valuation model.

## Opening and chronological replay

For each account, supplied actual opening cash and equity are posted once in the
shared opening transaction. Generated gross derivative assets, liabilities,
posted VM/IM and received VM extend that transaction. Independent native opening
stock admission incorporates those explicit gross anchors into the existing
balance-sheet equation; it does not alter the supplied cash/equity or insert a
balancing amount. Generated source stocks must reconcile to those admitted facts
in every scenario before replay.

The derivative source retains a shadow of its original cash/equity only to verify
its own source accounting. That shadow is never posted as another account opening.
Multiple derivative books may therefore share actual account cash without
duplicating it. Financial source intents post through the same `Sim::raw_post`
used by dated owners, preserving source line order and actual transaction dates.
Source P&L changes shared equity once. Each financial line retains its book,
source event, intent index, posting index, date and collision-safe instrument ID.

Received IM custody remains a balanced memo journal. Its opening and movements
retain source lineage but never enter cash, financial GL, assets, liabilities or
equity. Unpaid receivables/payables, derivative assets/liabilities and collateral
stocks retain separate gross ownership, including after physical option delivery.

Daily source reconciliation compares replayed categories with the complete native
source daily trial balances. Duplicate closing categories reject. At the source's
full tail, every individual gross derivative/rights/collateral and custody owner
must be closed and every intent consumed. If the shared dated backbook continues
longer, the closed derivative state is held; no additional mark, cash, settlement
or synthetic terminal payoff is generated.

## Admission and control

Original JSON is limited to 32 MiB and parsed with the shared duplicate-key
rejecting Core parser before Value conversion. Typed raw source mappings and
complete scenario coverage precede financial projection. The coordinator reserves
each selected source's existing 250,000 native financial work ceiling against
the eight-million raw joined source/backbook admission.

After generation, the owner separately admits actual source lines and complete
mixed-tail daily scans within eight million work units. It charges vector
capacities, retained source string capacities and conservative per-line shadow/GL
key allowances against 128 MiB. Retained native source wire data have a separate
64 MiB ceiling. Native NDJSON and final output each have a 32 MiB encoded limit.
These are distinct component/transport budgets, not an aggregate process RSS cap.
At most 64 selected source-run pairs and 512 dated backbook positions are admitted.

A required native Control spans input, projection, compilation, replay and output.
Checks also run during bounded stream writes. Independent tests use a 70-second
process watchdog for the 60-second native scope, since cooperative checks cannot
interrupt blocking I/O. Public durable adoption must preserve the original queued
deadline and final source/artifact/token/lease/publication fences through storage.

## Independent evidence and remaining integration

The independent checker reconstructs every raw derivative financial source with
the existing independent reference. It verifies exact saved identities, once-only
shared cash/equity opening, gross opening stocks, source financial/memo lineage,
ordered source transactions, actual pooled postings, closing GL and every daily
gross balance. The existing independent dated-source verifier separately checks
calendar recognition, scheduled settlement, rights, source provenance and closing
statements. No native financial callback is used to construct expected values.

Its current independently verified backbook is fully dated with unseasoned par
opening anchors and no discretionary policies, fees, tax, dividends or legacy
financial owners. Other backbook owners need their own independent combined
verification before they can obtain publication eligibility. This checker is a
test/analysis tool, never a production financial fallback.

Contained process checks cover swaps, receiver positions, signed coupons and
collateral interest, MPOR/default/closeout, cash and physical European exercise,
delayed settlement, collateral lag, supplied overnight conventions, distinct
physical scenarios and multiple derivative books sharing one opening cash pool.
Adversarial checks alter balanced postings, source policy, opening ownership,
ordered lineage, transaction identity, daily gross liabilities, dated recognition,
rights and completion. Native checks cover cancellation and admitted opening /
generated source binding. Current hand cases give combined closing cash 200.80
for the base swap plus dated loan and 200.42 for the reviewed MPOR case.

The frozen component source predecessor remains separately recorded in
`artifacts/saved-derivative-source/validation.json`; early failed mixed fixture
executions and missing historical test-fixture failures remain preserved.
Neither predecessor nor current contained evidence proves installed adoption,
empirical predictive accuracy, accounting-standard approval or complete scale.

Still required: public saved-book/native graph and paired risk integration,
cohort/library/candidate/strategy adoption, combined credit and behavioral owners,
capital/FTP policies, durable persisted publication, observed holdouts and the
complete 60,000-position graph/credit/solver/journal gate. The unchanged original
issue acceptance criteria remain authoritative in `implementation-plan.md`.
