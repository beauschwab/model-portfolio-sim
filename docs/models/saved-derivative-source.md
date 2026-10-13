# Explicit saved derivative source

The isolated `saved-derivative-source-1` contract binds a frozen saved book to the
existing Rust derivative accounting owner. Its schema and model version must be
selected explicitly. It is not installed-default or full issue #20 acceptance.

The book retains identity/revision, original as-of, entity/currency/unit, complete
dated trades and original legacy hedge rows. Every enhanced trade matches its
opening Physical node and legal mapping exactly, including order, curve/source
revisions and binary64 signed zero. Distinct rows with equal economics retain
distinct position owners. Each legacy row has one explicit position binding and
an explicit reviewed contractual upgrade. Notional, payer/receiver side, fixed
rate, final fixed/floating accrual ends and every indexed floating margin retain
the original saved anchors. Missing, surplus, duplicated or reordered ownership
rejects before financial projection. Complete enhanced books can have no legacy
rows or upgrade review.

This is an explicitly reviewed contractual upgrade. The old compact swap record
does not contain reset, calendar, fixing, settlement or legal terms; those terms
must be supplied in full. The binding does not infer them or assert that new
contractual schedules reproduce the historical generated schedules.

After mapping, the unchanged native `derivative_accounting` owner prepares marks,
coupon rights, European exercise/delivery, VM/IM, closeout and ordered journal
intents. Physical settlement evidence remains separate from Pricing projections;
XVA/funding diagnostics do not become journal loss or cash. Reviewed undesignated
policies, full daily calendars, original opening and complete rights/collateral
tails remain mandatory. Formal hedge accounting retains its existing rejection.

The independent verifier reconstructs saved coverage/economic anchors and exact
raw source identity, then uses the independent derivative raw-contract/journal
reference. Financial outputs retain the existing 1e-12 relative/1e-9 absolute
comparison. Source, metadata, dates, order, policies and binary64 source signs
use exact checks rather than those financial tolerances. Integer JSON spelling
is retained in the original request, while only explicitly declared financial
f64 cells receive typed normalization in the executed DTO.

## Shared transport utility and admission

The additive Core `strict_json` utility rejects duplicate object keys at every
depth before a `Value` decoder can erase them. It retains integer/float categories
and floating signed zero; it does not retain textual whitespace or number spelling.
Historical Value-first parsers are unchanged. This utility cannot recover keys
that an upstream HTTP parser has already discarded.

`serialized_size` counts escaped typed JSON without constructing an extra Value.
`encode` stages a bounded serialization without first cloning financial results
into a second JSON tree. Both refuse the first write exceeding the selected cap.
The selected source route bounds original and typed requests and staged output to
32 MiB, saved/legacy trades to 256 and Physical paths to one. Existing underlying
native admission includes 250,000 combined work, complete daily marks and its
separate product limits. These are component/wire limits, not an aggregate RSS
guarantee. Native cooperative checkpoints surround decoding/admission/projection
and encoding/writes; a process watchdog is still required for blocking IO.

## Recorded component evidence

`artifacts/saved-derivative-source/validation.json` records eight native tests,
strict all-target Clippy and 59 actual native process/independent checks. Receipt
SHA-256: `f708700aae668a1b55cd9e4b30340d3098f350fe350046e7394b4cc2c0213dd5`.
All 822 installed frozen files and the 162 copied baseline files were checked;
only two copied library export files changed, with additive source/helper files.
Underlying native derivative financial code is unchanged in that component.

Cases cover hand-computed coupon cash, signed rates, overnight conventions,
receiver and equal-economic rows, enforceable/gross legal buckets, European cash
and physical settlement, lag/outage/segregated collateral, default/MPOR and XVA
diagnostics. Refusal checks cover duplicate keys, row/owner aliases, partial or
permuted coverage, signed-zero source changes, wrong policies/measures/calendars
and altered raw/financial/journal receipts. No component receipt establishes
observed predictive accuracy, accounting/regulatory approval, saved UI/default
adoption, graph/risk/strategy/capital/FTP or full 60,000-position acceptance.

The successor `artifacts/derivative-mixed/` is connecting these native sources to
the existing Ledger Sim through a borrowed native interface. That successor is
in progress and has its own source, financial and independent mixed-journal gates;
the component receipt above must not be reused as proof that those gates pass.
The complete remaining requirements are in
[saved-derivative-integration-plan.md](saved-derivative-integration-plan.md).
