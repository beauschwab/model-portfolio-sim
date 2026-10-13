# Deposit graph to dated ledger

`deposit-graph-dated-ledger-1` is an additive native modeled-source request for the
reviewed performing par runoff boundary. It contains a raw
`deposit-product-graph-1` graph, complete `owners`, `capture_policy`, explicit
`run_mapping`, `monetary_unit`, actual-date `recognition_policy`, and raw ledger
`specification`. See [the synthetic complete example](../../examples/models/deposit-graph-dated-ledger-1.json).

Rust `deposit_ledger::prepare(&Request)` returns a validated `DatedLedgerRequest`
and an immutable source receipt. `run(&Request, Box<dyn Write + Send>)` executes
the same prepared dated request through the existing streamed runner. The native
modeled CLI emits the preparation/source header before daily output. Python
transports that header and partitions; it supplies no financial transformations.

The source receipt schema is `deposit-graph-dated-ledger-source-1`. It retains the
complete typed raw request, graph source/metrics/base anchor, scheduled capture,
cash origin lineage and `native_par_deposit_opening_anchors-1` normalization policy.
Explicit source/account/kind/opening balance, accrued or book adjustment conflicts
reject. Only omitted compatible opening anchors are filled. Deposits have positive
liability face, payment ownership, and zero opening accrued/book adjustment under
the admitted par policy. Equity is not plugged and source anchors are not replaced.

Every declared graph run maps explicitly to a ledger scenario and severity;
baseline alone is the original base run at severity one. All reverse severities
need their own supplied histories. Missing/duplicate mappings reject before
execution. Position-level monthly runoff, floating adjustment, sale/pledge and
other competing ownership are rejected by dated admission. Global deposit-flight
settings may govern unrelated legacy positions; supplied histories exclusively
control dated-owned stress and no second flight haircut is applied. Full contractual tails
are retained separately from the reporting horizon. Detailed journals and
independently replayed reports cover baseline and ordinary severity-one scenarios;
reverse runs currently produce reverse summaries rather than persisted detailed
reverse journals. Complete source coverage does not change that reporting limit.

For the example, opening liability and bank cash are each 1,000. Paid rate 0.365
with ACT/365F earns one currency unit daily. After 30 reporting days, cash remains
1,000, principal liability remains 1,000 and earned interest payable/expense is 30.
The retained day-31 tail pays 31 interest and 1,000 principal. Extending the report
to day 31 produces cash -31 and zero liability/interest payable. The fixture uses
an explicit synthetic zero-runoff assumption and separately declared pricing
histories; these are not empirical coefficients or a forecast of customer behavior.

Publication requires two distinct gates. `deposit_schedule_reference` independently
reconstructs physical repricing/runoff/payment/terminal equations from the original
queued histories, signed captured cash, recognition origins, opening anchors and
prepared dated request. It sets `physical_deposit_source_recomputed` and
`accrual_origins_retained`, while explicitly leaving pricing graph and empirical
verification false. Existing independent dated replay then checks journal,
recognition/settlement rights, trial balances and reports. API publication repeats
raw/source identity and streaming partition replay after storage encoding, with
the original deadline/cancellation and current native/source identity fences.
Balanced journal replay alone is not an economic source oracle.

The separate independent `project_declared` helper supports signed interest and
an explicitly selected physical or pricing measure for supplied-history analysis
and product-unit verification. It still requires ACT/360 or ACT/365F and complete
zero tails. The ledger verifier calls the stricter `project` wrapper, so this
analysis helper does not silently expand the admitted accounting boundary.

Coordinator input cap is 32 MiB, retained preparation estimate is 128 MiB, and
existing dated execution limits are 60,000 positions/90 million daily work units.
The graph/capture's stricter work and segment limits also apply. No aggregate RSS
or 60,000-position combined acceptance is claimed. Migration/inflow accounting,
opening accrued/negative-rate ownership, effective-yield deposits, empirical
calibration and detailed reverse journals remain separate open requirements.
