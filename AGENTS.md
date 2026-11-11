# AGENTS.md — rates-workbench

## Current execution contract

- Read [the production policy](docs/rust-production-contract.md); it overrides historical backend defaults. Dated migration notes are [archived](docs/changelog/agents-history.md), not execution instructions.
- Rust owns built-in pricing, calibration, paths/CRN, graph/cache/coordinator, simulation, income/KPIs, capital/FTP, unit preparation and solver financial logic. HiGHS stays C++; Rust constructs, invokes and validates
  its problem.
- Python owns API/storage/transport, audit coordination and explicit independent reference/verification. Keep financial calculations out of application adapters; no Python/NumPy/Numba production execution options.
- New/omitted backends select Rust. Historical Python inputs/results stay readable unchanged; execution requires an explicit Rust selection and compatible library/session rebuild. Missing/mismatched artifacts fail
  without fallback.
- Production pricing/what-if enters the native graph without a custom discount override. Custom Python model extensions remain an explicit reference path, separate from built-in production.

## Layers and working rules

- `packages/portfolio-risk`: read its [AGENTS.md](packages/portfolio-risk/AGENTS.md), then the [source](packages/portfolio-risk/src/portfolio_risk/AGENTS.md) and [test](packages/portfolio-risk/tests/AGENTS.md)
  instructions before kernel/model edits. They own assumptions, invariants and extension recipes.
- `apps/api`: FastAPI transport over engine drivers; adapters in `app/store.py` format Polars tables. Read [API instructions](apps/api/AGENTS.md) before persistence, publication or worker changes.
- Long runs are durable jobs in the separate `app.worker` process, one at a time per workspace; kernels saturate cores. Preserve queued revision, backend, source and executable identity.
- `apps/web`: Vite/React talks only through `src/lib/api.ts`; primitives are in `components/ui.tsx`. Read [DESIGN.md](DESIGN.md) before styling; Aperture tokens live in `src/styles/aperture.css` and `tailwind.config.js`.
- Preserve functions over module dictionaries, revisioned SQLite/PostgreSQL via `app.persistence`, and immutable local/S3 Parquet artifacts. Never add a second mutable source of truth.
- New products follow the engine recipe, then a `run_*` adapter, book tab/KRD key and `unitlib.TEMPLATES` entry when available in Strategy Lab.
- Maintain the [architecture](docs/architecture.md), [interface/dependency contracts](docs/contracts/README.md), engine operator skill and mirrored references when behavior changes; preserve compatibility aliases and
  generated contracts.

## Numerical and interactive invariants

- Solve original base OAS once; hold it fixed across risk/stress/scenario/decision bumps. Share CRN on both sides of differences; never reseed or refit the original purchase OAS/yield/cost from revised expectations.
- Preserve final-output tolerances, reduction/posting order and historical output contracts. No fast-math, simplified fallback, live-mass normalization or second probability weighting of monetary state.
- Kernel/reference MODEL-BLOCK pairs stay synchronized. Numba freezes constants/LUTs per process; prepay edits require a fresh process/rebuild and must report `RESTART_REQUIRED` rather than silently no-op.
- `POST /run` with `kind="unitlib"` prices/prepares tensors and base KPIs. Synchronous `/strategy/eval` stays sub-ms and coefficient-only: no engine/pricer calls. Reject absent coefficients or mismatched grids/backend.
- Constraint-only edits reuse coefficients without pricing. Dirty edits selectively retain exactly equal sources; shared identity/grid/count changes require complete preparation. Retained capital limits need refreshed
  dependencies (`CAPITAL_REFRESH_REQUIRED`).
- Conditional forecasts are income/runoff only, never OAS/EVE/derivative valuation. Fit/select on chronological training data only; holdout affects diagnostics. Stable observation ties and sorted forecast driver names
  remain deterministic.
- Cache keys retain complete collision-safe input/parent identity, model/backend identity and live cashflow ownership. Validate price/notional even on hits; eviction is safe recomputation, not changed economics. Budgets
  include retained identities.
- Perform native aggregate work/byte admission before generation and reserve extra groups prospectively. Chunked cashflows/schedules must not become multi-million-row JSON. Component/cache limits never imply an aggregate
  RSS or full-scale guarantee.

## Publication, execution and evidence

- Publish immutable raw requests, hashes, sources, executable/bundle identity and Parquet outputs only after independent raw-source and ordered persisted-journal replay, including a fresh artifact/source reread.
  Producers cannot self-certify independent verification.
- Preserve original scalar deadlines and cancellation through preparation, replay, persistence and final publication, including checks after progress callbacks/rename. Cooperative native controls, bounded IO and process
  watchdogs are separate requirements.
- Recheck workspace revision, attempt/request/token/lease, cancellation, source authority, executable and consumed artifact bytes under final scoped SQL publication. Failure publishes no manifest; cleanup removes only
  fresh owned unpublished output.
- Prior source deadlines are provenance; a new job keeps its own original budget. Caller balances, object descriptors or verification flags do not establish authorized opening evidence. Managed raw-input references
  survive restart/GC.
- Preserve full gross cash/rights/recovery/commitment tails, even at zero allocation, before applying path/allocation weights once. Principal zero/net carrying zero does not close gross rights; never invent terminal
  payoffs or closing plugs.
- Keep actual recognition, accrual, settlement and integer/fractional clocks distinct; preserve milliseconds without float round trips. Acquisitions/opening cash/equity and atomic posting groups post once, not again
  through evidence receipts.
- Keep economic loss, reviewed allowance/provision/chargeoff/recovery, book basis and legal rights separate; no implicit allowance or double capital deduction. Separate Physical/Pricing histories, projected drivers and
  observed fixings.
- Explicitly select additive schemas/models; historical economics/ABI/protocols remain separately readable. Match native product, ledger and decision/workflow artifacts, restart API/workers and explicitly rebuild
  selected libraries/sessions before use.
- [Build/deploy policy](docs/rust-production-contract.md) supplies commands (`make build-native` / `make build`). Never hot-replace libraries in live processes. Local/repository checks do not deploy or prove provider,
  empirical, regulatory or accounting acceptance.
- Synthetic fixtures and supplied laws/policies are not observed predictive evidence. Keep every remaining [model implementation gate](docs/models/implementation-plan.md) under #15/#25 and separate #35 hedge gates;
  verify current receipts before claiming adoption/scale.

## Product-specific contracts (read linked schemas before changing a lane)

- [Shared models/native schemas](docs/models/native-contracts.md): supplied measure/calendar/assumption/policy identities are required. Dated/floating cashflows use actual accrual/reset/settlement conventions and
  complete available fixings; do not invent future SOFR or infer reference-calendar certification.
- `annuity` remains equal principal. Explicit fixed corporate `level_payment` uses the actual accrual schedule, rejects floating/sinking/negative-amortization/truncated grids, and never infers a seasoned original payment
  from remaining balance/term.
- [Credit operator](docs/models/credit-operator.md) keeps probability, principal, undrawn, signed accrual and basis vectors separate; [credit accounting](docs/models/credit-accounting.md) requires reviewed physical daily
  steps and complete recovery claims. No output implies CECL/IFRS9 compliance.
- Exact-observation hazard fitting uses elapsed origin exposure/known transitions; unobserved origins need assumptions/rejection. Interval-censored/event-time analysis has separate contracts; LGD/EAD/borrower/macro
  inputs and empirical holdouts are not inferred.
- [Dated replay](docs/models/dated-ledger.md): exclusive dated owners bypass legacy default/interest/flight/scaling/maturity. Require complete scenario/reverse coverage and recognition segments; reject
  overlapping/Pricing sources and unsupported owners/opening anchors. Keep legacy monthly and protocols 3/4/5 separate.
- [Product workbench](docs/models/product-analysis-workbench.md): additive analyses, graph receipts, dated units and daily mixed journal adoption are distinct. Full source/persisted replay and final
  executable/source/deadline fences apply to every analysis/library, including exact nested canonical JSON cells.
- Actual-purchase aged/product units retain original EIR, OAS/quote and complete Physical age prefixes. Funding uses quoted purchase cost and full scenario-specific principal/coupon tails; face and purchase-cost caps
  differ. Dated incremental allocations have no implicit monthly backbook KPI baseline.
- [Performing mixed dated](docs/models/product-mixed-dated.md): opening cash/equity once, actual-date acquisitions before recognition/settlement, complete contractual tails beyond report date. Uniform negative coupon
  rights are independent of principal/EIR direction; sign-changing coupons need separate gross ownership and reject here.
- [Consumer credit](docs/models/consumer-credit-accounting.md): acquired non-PCD coupon/default/allowance policy is explicit; gross recovery/cost postings differ. Unsupported fees/nonaccrual/modifications/PCD/draws
  reject. Interim weights must explicitly be zero in this mixed slice; closing capital uses separately supplied eligible policies.
- [Card sources](docs/models/card-credit-accounting.md) and [Card2](docs/models/card-credit-accounting-v2.md): explicit raw tags select their own independent verifier; statement cycles do not repeat after source end.
  Keep billed/unbilled interest, signed basis, memo commitments and reviewed collections separate; default claims earn no continuing EIR.
- Card2 gross/basis/commitment tails must close unweighted; native/independent receipts plus exact full-tail date separately authorize closing capital. `capital_ftp_eligible` stays false; broader
  fee/grace/draw/nonaccrual/PCD policies require their own owners.
- [Mortgage source](docs/models/mortgage-credit-accounting.md): original/current Physical whole-loan contracts retain cost/EIR, complete claims and exact compiled dependency receipts. Economic cash retires claims;
  chargeoff does not waive them. Preserve default-accrued valuation guards and bounded interval-end conventions.
- [Mortgage mixed](docs/models/mortgage-mixed-accounting.md): economic memos differ from monetary P/A/basis; enforce exact gross admission without netting/epsilon and independently recompute the single ordered daily
  GL/pending phases. Analytical acceptance is separate from saved/cohort/library/capital/FTP/empirical acceptance.
- [Raw instructions](docs/models/raw-instruction-validation.md): reject keys outside the selected instruction kind before financial/feasibility handling, including future boundaries. Do not silently drop
  selector/settlement fields; retain acquired-mortgage nested duplicate/enum-alias rejection without claiming byte-duplicate rejection in older outer decoders.
- [Market indexed adapter](docs/models/market-indexed-adapter.md) and [books](docs/models/market-indexed-books.md): retain supplied index/calendar/curve parents and original OAS, exact typed identities including signed
  zero. Per-call cancellation/deadline controls cannot be retained as graph-creation deadlines; multi-call operations need an outer deadline and missing capabilities fail with rebuild required.
- [Deposit revised income](docs/models/deposit-revised-income.md): retained original cost/EIR/tails, one reviewed day-zero catch-up and gross FIFO earning/cash origins. Original raw input and declared normalization stay
  separate; older unchanged-expectation routes retain their economics and exclusions.
- [Mortgage closing capital](docs/models/mortgage-closing-capital.md): new authorized job independently binds/replays the completed source and exact closing date with typed binary64 digest. Signed balances stay
  unchanged; regulatory inputs, negative-exposure policy and monetary multiplier are explicit; interim/FTP permission is separate.
- [Commercial models](docs/models/commercial-generated-exercise.md): complete supplied Physical/Pricing histories, original cost/EIR/OAS, shared credit draws, legal versus recognized coupon lots, nonaccrual/integral fees
  and default funding/recovery policies remain separate. Credit-free references reject hazards/commitments.
- [Generated OAS](docs/models/commercial-generated-oas.md): retain distinct Pricing training/evaluation samples; refit continuation only on training each trial. Final stopped native cash must meet unchanged quote
  tolerance; ambiguous/unbracketed/nonmonotone/gap quotes reject. Reuse accepted original policy/cash in graph/units; bumps never regenerate/refit it.
- [Holding continuation](docs/models/commercial-continuation.md) and [journals](docs/models/commercial-holding-journal.md): derive sealed opening/future postings from complete original history; no caller checkpoint,
  credit restart or second acquisition. Preserve original clocks, completed prefixes, gross coupon/draw/default rights and identical closing non-P&L stock bits.
- [Prior openings](docs/models/commercial-prior-opening.md): resolve authorized completed publications and independently bind exact cut opening/completed postings; verified modeled evidence is separate from observed bank
  reconciliation. [Treasury/candidates](docs/models/commercial-treasury-candidate.md) retains its own source and adoption gates.
- [Separate fees](docs/models/nonintegral-fees.md) and [loan/fee journal](docs/models/commercial-fee-journal.md): signed receivable/payable/prepaid/deferred rights stay separate from loan cost/EIR/credit. Reviewed native
  exit policies own waiver/refund/cancellation/recovery and fee LGD; fractional event clocks cannot round backward.
- Earned refunds require permitted earned AND collected service; reverse cash/income once without creating new earning/collection capacity or using prepaid/waived rights. Retain gross history/full tails/zero allocations;
  reserve refund work before generation. Fee pricing/units/capital/FTP/secondary integration is not implied.
- MBS `prepay_mult` is finite [0,10], null/absent=1, zero stops prepayment; scale turnover+refi before the existing CPR cap. Include it in cashflow identities and every native/reference mortgage path; new-origination
  templates use 1 and custom Python prepay rejects non-unit values.
- Deposit `deposit-terminal-1`: default `payout` preserves historical par residual; `truncate` excludes residual liability; `runoff_tail` uses final attrition/paid/short rates and a 1e-4 monthly denominator floor, lumped
  at the final A column. In-horizon runoff/NII stays unchanged; horizon sensitivity uses one base OAS.
- [Capital/FTP](docs/capital-and-ftp.md): thirteen ratios keep minimum/buffer/management headroom distinct. Missing requirements/denominators never pass; eligibility/RWA/GSIB/average assets/advanced risks remain
  supplied. Prepared unit limits are coefficient feasibility, separate from daily journal acceptance.
- FTP keeps repricing/reference and behavioral funding/liquidity tenors distinct, explicit option/contingent costs, lineage and annualized RAROC. Entity/currency/scenario/period transfers offset treasury without changing
  external NII, OAS or capital; solver objective remains worst-case external NII. Linear interpolation/flat tails are disclosed.
- Treasury bridges require verified journal closing and exact date/unit/source hashes. Monthly expected-flow FTP uses average principal and principal-weighted life with explicit residual tail; reset tenors/costs/capital
  are assumptions. Realized credit/conditional forecasts are not expected-flow FTP sources; reviewed credit deductions apply once.
- Liquidity applicability is `internal_limit` with positive floor, `not_applicable` or `unconfigured` without floor; legacy floor infers internal limit. Research proxies reject `regulatory`; undefined ratios are
  unavailable and nonapplicable rules never pass. Transfer approvals delay dispatch; intercompany balances eliminate per currency daily.
- HTM `asc320-htm-v1` uses reviewed sale reasons/materiality and the 90-day near-maturity rule; excess unsupported sales reclassify remaining HTM to AFS with balanced AOCI postings. Without policy report not_assessed;
  category LP constraints, auditor judgment and tainting periods are separate.
- [Hedge accounting](docs/models/hedge-accounting.md): explicitly reviewed Physical derivative/item/forecast sources, append-only relationships, hypothetical effectiveness, OCI/basis recognition and one ordered journal.
  Legacy `fvh`/`cfh` labels do not designate a hedge; marks/coupons/collateral/closeout post once and retained opening balances offset equity once.
- Apply the documented ASC 815 rule matrix and reviewed ASU 2025-09/2022-01 adoption prospectively; preserve per-member/option hypotheticals, dated elections/dedesignation, assessment bounds and portfolio-layer breach
  ordering/basis treatment. Unsupported instruments/default paths reject under their selected lane.
- Verified saved derivative/hedge mixed closings feed capital only through receipt-gated gross-GL bridges and explicit AOCI eligibility (default excluded). Exact report-node interim selection, unpaid rights/negative
  basis refusals and separate FTP/credit permissions remain required.
- [Swap units](docs/models/swap-strategy-units.md) are opt-in/off-balance-sheet NII/DV01/research RWA only; exclude from balance/funding/liquidity/asset caps and ALL_ASSET/ALL_LIAB. Allocation never designates a hedge.
  [Saved risk](docs/models/saved-derivative-risk.md) uses explicit named curves or additive shifts, paired bumps and immutable observed fixings.
- [Net investment](docs/models/net-investment-hedge.md): closing-spot translation, supplied flow rates, only hedged CTA released on disposal; dual hedge retains debt basis from its embedded run and spot earnings during
  fair-value hedge. [Nonfinancial](docs/models/nonfinancial-hedge.md): component evidence/adoption and hypothetical index effectiveness are explicit; release OCI at delivery/consumption or probable nonoccurrence.
- Saved derivative/hedge `acceptance_60000` is opt-in with hash-bound envelope and streamed journal. Only that tier permits the documented account aggregate summation bound `max(1e-12,n*1.12e-16)` relative to magnitude;
  defaults/historical bytes and other tolerances stay unchanged. See [scale evidence](docs/reviews/2026-10-18-hedge-book-scale-acceptance.md).
- Cohort audits reprice each selected record in <=256 Rust batches with fixed OAS/shared CRN, base/+/-200bp PV/DV01/NII and monthly cashflows; explicit dollar/relative tolerances apply. Splits require rebuild/audit. Tape
  intake stays <=100,000 rows/32 MiB; refresh reconciles changes without inferring payments or posting journals; preserve namespace/cohort IDs and compatible historical yields.

## Required checks and navigation

- Run `make check-architecture` for boundary/contract changes: static boundaries, script tests and reproducible native/HTTP/MCP contracts. CI also enforces this file's 150-line maximum, including blank lines.
- Run affected numerical/publication/ownership gates from the nested test instructions; kernel edits require the full engine suite. Explicit Python oracles must stay independent. Fresh-process default/ownership tests run
  outside reference context and forbid Python financial callbacks.
- Root commands: `make install`, `dev-api`, `dev-worker`, `dev-web`, `build-native`, `build`. Test tiers: `smoke` (under five minutes, after `build-native`), `test-changed` (suites mapped from changed paths in
  `scripts/select_tests.py`), `test-py`/`test` (full); CI selects by path on PRs and runs everything nightly. Each tier establishes its stated scope only; retain PostgreSQL, immutable storage and Iceberg retry/idempotency contracts.
- Keep detailed schema/adoption rules in the linked current model documents and [implementation plan](docs/models/implementation-plan.md); do not append version logs here or to operator skill/reference entrypoints.
- Historical text is preserved unchanged in [the root archive](docs/changelog/agents-history.md) and [per-source archives](docs/changelog/agents-history-manifest.json). The [review
  checklist](docs/reviews/issue-67-agent-contract.md) maps former sections to current rules; archive relative paths refer to their original source location.
