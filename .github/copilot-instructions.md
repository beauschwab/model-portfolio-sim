# GitHub Copilot Instructions

## Production execution contract (0.29.4; supersedes historical defaults)

Rust is the required production financial runtime. Python calculation is deprecated;
it is retained only as an independent test/reference oracle, not an API/UI backend.
New and omitted `compute_backend` settings resolve to `rust`. Historical explicit
Python snapshots remain readable, but must be switched to Rust and their libraries/
sessions rebuilt before production execution. Never silently change saved economics
or fall back when native libraries are missing.

Implement new built-in pricing, graph/coordinator, simulation, capital/FTP and solver
financial logic in Rust. HiGHS stays C++; Rust owns LP construction, invocation and
allocation validation. Production pricing/what-if must enter the native graph with
no custom discount-backend override. Do not reintroduce Python/NumPy/Numba execution
choices in application requests or controls. Keep Python oracles explicit in tests
so parity cannot accidentally compare Rust with itself. Defaults/ownership tests
must run outside the reference test context and forbid Python financial callbacks.

The existing FastAPI/worker, persistence/artifact transport and independent verifier
still use Python; this change does not claim a Rust HTTP/storage service. Keep quant
logic out of adapters. See `docs/rust-production-contract.md` at the repository root
for deployment, compatibility and validation rules. Preserve fixed OAS, CRN, final
output tolerances, immutable publication and the coefficient-only interactive path.

This repository is Rates Workbench, a monorepo with a Rust financial runtime (C++ HiGHS), a Python
FastAPI API, and Vite/React trader dashboard.

Follow the repository `AGENTS.md` hierarchy first. The root file defines the
cross-layer contract; `packages/portfolio-risk/AGENTS.md` and its nested files are
authoritative before changing engine code, tests, or skills.

Use these local commands:

- `bun run setup` installs Python environments with `uv` and frontend packages with `bun`.
- `bun run dev:api` runs FastAPI on port 8000.
- `bun run dev:web` runs Vite on port 5173 and proxies `/api` to port 8000.
- `bun run test:py` runs the engine test suite.
- `cd apps/web && bun run build` runs the production web build.

The Makefile mirrors these commands for systems where `make` is available.

Implement production quant logic in the native Rust crates. `packages/portfolio-risk`
contains adapters and independent references. The API in `apps/api` adapts engine
outputs and uses its existing repository facade backed by SQLite/PostgreSQL revisions
and immutable Parquet artifacts; do not add a second mutable source of truth. The web app
in `apps/web` talks to the API through `src/lib/api.ts` and should preserve the
existing Supabase-dark, zinc/emerald design language.

Preserve engine invariants when they surface through API or UI behavior:
scenario runs keep base OAS fixed, common random numbers matter for risk, and
prepay assumption changes should report restart-required semantics rather than
silently mutating frozen numba constants.
### Native product graphs and modeled replay (0.29.7 follow-through)

Explicit consumer, card, mortgage, deposit, term/CD and whole-book funding graphs retain separate supplied
physical/pricing histories, paired pricing bumps, full cash/rights/recovery tails
and immutable original base OAS. Their metrics disclose contractual recognition
and cash; effective-yield income, empirical fitting and saved-book adoption need
their own policies and gates. Graph admission exposes validated aggregate work
before cohort generation; component limits never imply an aggregate RSS guarantee.

`credit-path-ledger-1` selects stream protocol 5 and independently recomputes
raw event-time paths, reviewed accounting, ordered postings and closing balances.
Expected protocol 4 and historical monthly/dated protocols remain separately
readable. Reviewed realized credit journals can feed native closing capital;
realized paths are not expected-flow FTP inputs. Signed accrued liabilities retain
gross presentation and economic loss is never deducted twice.

`graph-dated-ledger-1` and `driven-credit-ledger-1` run source preparation and
daily replay in one native `portfolio-model-ledger` process. Durable modeled jobs
retain raw requests and prepared sources through independent persisted verification,
source/artifact hashes, deadlines, cancellation and lease/token publication.
Consumer capture admits reviewed performing par-note simple interest; unsupported
credit/fees reject. Frozen credit covariates retain chronological training/holdout
separation and explicit analytical/adoption eligibility. Supplied projected drivers,
LGD, CCF and recovery policies are not inferred empirical evidence.

Model analyses use lossless canonical JSON cells for nested variants. Reviewed
derivative and realized-credit accounting sources are independently checked before
and after persistence. These analytical receipts do not imply mixed-book journal
adoption. Every analysis and dated-library publication checks final executable,
source and deadline fences. Interactive allocation remains coefficient-only.

`product-units-1` retains supplied actual-purchase performing consumer/card/
mortgage and deposit-runoff histories, reviewed original effective yield, verified
locked purchase OAS/quote and replayed physical age prefixes. Full-tail funding is
scenario-specific; income is effective interest rather than gross coupon cash.
Unsupported draws, credit/nonaccrual, migration, fees and modifications reject.
`strategy-product-1` constructs native HiGHS problems from these coefficients,
replays source cash/amounts and retains face, purchase-cost, category and explicit
capital dependencies. Constraint-only actor updates and evaluation use retained
coefficients. Dirty unit edits selectively retain exactly equal native sources;
shared identity/grid/count changes require complete preparation. Refreshed capital
dependencies remain required. Native and independent raw-source replay gates both
product reports and persisted libraries, including their exact adapter cells.
This stage does not establish mixed candidate journal acceptance.
The explicit `product-candidate-journal-1` route posts standalone performing
candidate acquisition and original-yield recognition through complete physical
cash and rights tails. Independent raw-source, ordered journal and persisted
replay gate its durable analysis. Supplied opening provenance is not verified
opening evidence. Existing mixed owners, credit/default, fees, taxes and
collateral overlays reject. Native flags retain the separate mixed acceptance
boundary. Unit face must reconcile to source opening principal before reuse or
pricing; final source/artifact/ownership/deadline fences follow progress reporting.

`deposit-graph-dated-ledger-1` admits reviewed par deposit runoff and daily simple
interest expense with complete rights, raw source and independent persisted replay.
Migrations, new inflows, negative rates and non-par yield policy are unsupported
in that capture tier. It does not reinterpret the broader analytical deposit graph.
The separate `deposit-graph-income-ledger-1` route retains explicit purchase quote,
original effective yield and carrying-value anchors, complete unchanged expected
physical cash and daily liability basis amortization. Changed expectations,
seasoned purchases, migrations/inflows and negative-rate accounting reject.
Independent raw-source and persisted journal replay remain publication gates.
Final publication retains the original scalar deadline through persistence and
checks source/artifacts even after rename; failed checks remove only fresh owned
unpublished output.

Full issue 25 acceptance remains required: observed behavioral/credit validation,
product accounting/FTP, saved-book/candidate/cohort/strategy adoption and complete
60,000-position graph/credit/solver/journal acceptance. Rebuild product, ledger and
dependent decision/workflow artifacts before additive-schema execution; restart
deployed API/workers and explicitly rebuild saved libraries/sessions.

## Performing mixed dated acquisition stage (local 0.29.7)

The explicit `product-mixed-dated-1` route combines a reviewed performing dated
backbook and performing product candidates in one chronological native journal.
Actual opening cash/equity are posted once; acquisitions precede recognition and
settlement on their actual dates. Original candidate EIR/OAS/quote and separately
supplied physical expectations remain retained. Backbook recognition and opening
provenance are supplied reviewed inputs. Complete contractual tails are replayed
separately from the requested reporting date; no terminal payoff is invented.
Independent raw-source and ordered persisted-journal verification gate publication.
The verified complete-tail closing can supply native closing capital with an exact
closing-date identity. Report-date capital and modeled product FTP remain separate.

Candidate gross interest rights retain separate physical-path ownership. Uniform
negative coupons classify receivables/deferred income independently of principal
direction and original EIR. Sign-changing contractual coupons require separate
gross obligation ownership and reject at this journal boundary. Zero participation
retains the complete unweighted source policy. The legacy dated route is unchanged.

This performing slice rejects credit/default, fees, taxes, marks, collateral,
netting, legacy mixed owners and reverse-severity journals. Component bounds are
512 positions, eight million aggregate work units and 128 MiB retained capacity;
they are not aggregate RSS or complete 60,000-position acceptance. Joint credit,
behavioral generation, empirical holdouts and all issue 25 acceptance gates remain
required. See `docs/models/product-mixed-dated.md` and the implementation ledger.
Rebuild native artifacts and explicitly rebuild selected saved libraries/sessions;
local validation does not deploy changes or restart production workers.

## Acquired consumer-credit event ownership (local 0.29.7)

`consumer-credit-accounting-1` derives one supplied physical auto/personal path,
original effective-yield prefix and reviewed default/allowance/chargeoff intents
in Rust. Gross principal recovery and collection cost have separate cash and
expense postings. Acquisition cost, original yield, source and policy identities
are explicit. The component neither infers allowance from economic loss nor
claims closure of retained unpaid rights.

`product-mixed-consumer-credit-1` executes those exclusive product-event owners
with performing dated backbook/candidates in the same daily Sim, cash/equity state
and protocol-3 GL. Purchase cash and explicitly reviewed initial allowance are
posted once. Allocation and physical path weights are applied once; complete
unweighted source/claim tails are required even for zero participation. Default
principal, basis and gross signed accrued rights have distinct GL ownership.
Independent raw-source and ordered persisted-journal verification are publication
gates; the producer cannot self-certify persisted verification.

The original installment coupon lane is nonnegative and acquired/non-PCD, with
supplied allowance and recovery policies. Fees, nonaccrual, modifications, PCD,
draws and empirical/joint stochastic adoption require their own reviewed models.
Consumer interim regulatory/liquidity weights must explicitly be zero; related
account constraints reject. Complete-tail closing capital uses verified GL and
separately supplied eligible capital/weight policies, not inferred default RWA.
Report-date capital, modeled product FTP, saved-book integration and the complete
60,000-position joint graph/credit/solver/journal gate remain open.

The mixed slice shares the existing 512-position, eight-million-work and 128 MiB
conservative retained limits, with complete raw/final JSON bounded to 32 MiB.
It reserves consumer generation/replay before performing capture; component limits
are not aggregate process RSS evidence. Card accounting has a separate explicit
source route; its shared mixed-owner release gates remain required. See
`docs/models/consumer-credit-accounting.md`, `product-mixed-consumer-credit.md`
and the complete issue 25 implementation ledger. Rebuild native artifacts and
explicitly rebuild selected saved libraries/sessions before execution. Local
validation does not deploy changes or restart production workers.

## Explicit acquired card and tagged credit owners (local 0.29.7)

`card-credit-accounting-1` is an explicitly selected acquired non-PCD physical
card source. Native card economics own principal, billed/unbilled interest,
statements, default and gross recovery/cost. Original effective yield and reviewed
allowance/default/basis/chargeoff are retained separately. Principal zero does not
close unpaid interest or commitment rights; reviewed complete closure is required.
The admitted source is fee-free/non-grace/no-draw, and contingent undrawn balances
are memo rights rather than inferred CCF. Shared principal-plus-basis guards and
all final tolerances remain unchanged. The separate richer interest-basis proof
is not selected implicitly by this public schema.

`product-mixed-product-credit-1` uses tagged raw installment/card requests and
the existing daily Sim, pooled cash/equity, account mapper and one ordered GL.
The old consumer schemas, bare native sources, table names and output remain
unchanged. Purchases and explicitly reviewed initial allowance post once;
allocation/physical weights apply once, and zero participation retains complete
unweighted source/claim checks. Statement dates are indexed and not repeated after
source end. Independent raw source/ordered journal replay and a fresh persisted
reread remain publication gates, with original deadlines and final source/artifact
and durable ownership fences. Rebuild matching native artifacts before execution.

The tagged route retains conservative joined 512-position/eight-million-work/
128-MiB capacity admission and 32-MiB raw/final JSON bounds. Those component
controls do not establish aggregate RSS or 60,000-position joint acceptance.
Closing capital uses the verified complete-tail date and separately supplied
eligible inputs; interim default regulatory weights and modeled product FTP are
not inferred. Wider fee/grace/draw/nonaccrual/PCD, empirical/joint stochastic,
saved-book/cohort/solver and complete issue 25 gates remain required. See
`docs/models/card-credit-accounting.md`, `product-event-owner.md` and
`product-mixed-product-credit.md`; the validation report distinguishes debug,
release, durable and browser evidence. Local changes do not deploy or restart
production workers, and saved libraries require explicit compatible rebuilds.

## Explicit card interest-basis ownership (local 0.29.7)

The additive `card-credit-accounting-2` source retains whole-carrying original
effective yield, gross billed/unbilled interest, signed live/default interest
basis, reviewed actual interest collection and complete claim/commitment tails.
Acquisition and full principal settlement keep their original anchors. Original
EIR and reviewed basis realization are distinct P&L, and default claims receive
no continuing EIR. Atomic event groups are the complete posting stream; shared
intents and income receipts are evidence and must never post a second time.

`product-mixed-product-credit-1` accepts the explicit raw `card_v2` tag in the
same dated Sim, cash/equity and ordered GL. Complete unweighted gross rights,
signed basis and commitments must close before allocation or physical path
weight applies, including zero participation; zero net carrying alone is
insufficient. Supplemental daily-right and atomic/acquisition lineage tables
retain native source identities. The durable independent verifier is selected
from the raw tag, and persisted summaries must match the verified source.
Publication retains original deadline, cancellation, ownership and artifact
fences. Older Card1 and consumer sources retain their original economics/bytes.

The bounded Card2 lane admits 64 actual days, 64 steps/cycles, 16 interest
collections, 256 conservative events, 1 MiB raw, 16 MiB component capacity and 2 million
work. Joined mixed 128 MiB/eight-million-work/512-position admission remains.
These are component ceilings, not aggregate RSS or complete 60,000-position
joint graph/credit/solver/journal acceptance. Fees, grace, new draws, default
before principal zero, nonaccrual, modifications, PCD and wider policies reject
until their own ownership is implemented. No empirical or accounting approval
is inferred. Card2 mixed `capital_ftp_eligible` remains false. Separate
`closing_capital_eligible:true` permission requires matching native and independent
persisted receipts, complete unweighted gross/basis/commitment tails, ordered
journal replay and the exact full-tail date. Closing capital still requires
explicit supplied policies/weights; interim capital and modeled FTP remain open.

See `docs/models/card-credit-accounting-v2.md`, `product-event-owner.md`,
`product-rights-reference.md` and `product-mixed-product-credit.md`. Validation
reports distinguish component, DEBUG, official release, durable and browser
evidence. Rebuild libraries/sessions explicitly; local work does not deploy or
restart production workers, or complete wider issue 25 acceptance gates.

## Explicit acquired-mortgage analytical source (local 0.29.7)

`mortgage-credit-accounting-1` explicitly selects paired Physical original/current
whole-loan contracts, reviewed original purchase cost/effective yield and complete
original/current claim tails. The existing Rust mortgage loop provides borrowed
interval/boundary/cash receipts; its ordinary None path preserves historical
output bytes. The coupled owner transports monetary P/A/G and original-EIR reward
without multiplying probability into money again. Changed credit, prepayment,
recovery or supplied ARM observations cannot refit original cost/yield/carry.
Actual cash drives economic claim retirement; book reversal/chargeoff does not
waive an economic right, and missing gross tails reject without a closing plug.

The compiled generator receipt binds nine length-prefixed source/dependency files.
Its preinstrumentation hash is provenance only. Immutable raw and executed input,
current artifact/source, original/current receipt and persisted-source checks
gate analytical publication, with original deadlines/cancellation/worker fences.
The independent Python reference is publication/validation support only; adapters
transport raw inputs and all financial production calculation stays in Rust.

This bounded analytical lane admits 16 intervals per original/current source,
1 MiB raw, 64 MiB retained output and nine million joined work units including
compiled-source hashing. It requires zero acquired accrued rights/remittance lag,
explicit non-PCD policies, physical whole-loan ownership and complete live/claim
closure. Agency/float/allowance/nonaccrual EIR/additional recovery-cost policies
require separate ownership. Existing default-accrued whole-loan valuation guards
remain unchanged. Source defaults/payoffs use the native interval-end convention;
this source does not claim exact daily journal recognition or new P/Q calibration.

Analytical source acceptance is separate from daily mixed journals, cohort/library/
saved-book adoption, capital, FTP, empirical holdouts and full issue 25 acceptance.
The separate exact-day pending-exit component remains an artifact under reviewed
atomic/domain gates and is not an installed journal source. Read
`docs/models/mortgage-credit-accounting.md`, `docs/models/native-contracts.md` and
the implementation/validation ledgers. Select new schemas and rebuild matching
native artifacts/libraries/sessions explicitly; local builds do not deploy or
restart production workers.

## Strict additive financial instructions (local 0.29.7)

New Card1/Card2 termination, CSA/MPOR disposition, credit maturity and product
strategy selector fields reject keys outside their explicit instruction kind.
Source-default/retain/maturity/all-assets/all-liabilities unit variants own only
`kind`; they must not silently discard settlement, repayment or narrowing fields.
Shared enums, valid wire forms, financial formulas and tolerances are unchanged.
Independent strategy checks selector shape before financial preparation or either
feasible/infeasible handling. Independent credit checks every boundary, including
future dates, before financial operators or persisted rows. Value-first outer
decoders can erase duplicate keys; these field seams do not claim complete byte
duplicate rejection there. The new acquired mortgage owner separately preserves
and rejects nested duplicates and enum-object aliases in its original/current
contract and interest-policy inputs. Historical mortgage parsing is unchanged.

Read `docs/models/raw-instruction-validation.md` and the current validation ledger.
Component and synthetic input checks do not close full model/empirical/scale gates.
Rebuild matching native artifacts for new execution; local builds do not deploy
or restart production workers.

## Market indexed sources and acquired mortgage mixed replay (local 0.29.7)

`market-indexed-adapter-1` explicitly selects supplied named-index/collateral curves,
actual reset/accrual/payment dates and retained reviewed original OAS. Rust prepares
indexed cashflows and values swaps through shared native owners. Projected curves
are not observed fixings or physical scenarios. Immutable input/source/OAS clones
use exact typed identity, including signed zero; calculated output tolerances stay
unchanged. This analytical source does not adopt saved books, incremental caches,
joint dynamics, smile calibration, physical journals or capital/FTP. See
`docs/models/market-indexed-adapter.md`.

`mortgage-mixed-accounting-1` explicitly joins complete registered Physical acquired
mortgage sources and reviewed dated backbook owners in one ordered daily GL.
Borrower/economic claim memos remain separate from monetary P/A/basis state.
Original purchase cost and effective yield stay fixed. Every allocation retains
complete unweighted source tails, including zero allocations. The bounded lane
admits 64 days, 1 MiB raw input, 32 MiB stream and eight million joined work units;
exact binary64 gross sums reject state above 10,000 without netting or epsilon.
These limits are not aggregate RSS or full 60,000-position acceptance guarantees.

Independent persisted verification must recompute source, ordered mixed journal,
daily GL and pending memo phases, retain the actual producer completion, and fence
raw/source/artifact identity, original deadlines, cancellation and worker leases.
The new route reports `single_daily_gl_verified` and
`ordered_mixed_journal_recomputed`; legacy receipt markers cannot substitute.
Accounting-standard approval, empirical validation, saved-book/library adoption
and capital/FTP eligibility remain false. See `docs/models/mortgage-mixed-accounting.md`
and the full issue gates in `docs/models/implementation-plan.md`.

Select additive schemas explicitly and rebuild matching native artifacts/libraries/
sessions before execution. Local source/build validation does not deploy or restart
production workers. Historical routes retain their separate ownership contracts.

## Indexed graph control, deposit revisions and closing capital (local 0.29.8)

These additive source contracts require matching official native artifacts and
fresh acceptance. Earlier full suites validate their recorded earlier bundle.
Historical numeric ABI 8 IDs/layouts and financial contracts remain unchanged.
Select new models explicitly; no historical saved economics are silently replaced.

Indexed graph/book owners retain immutable supplied contract/index/calendar/curve
parents and the original reviewed OAS. The raw-book opt-in has a 360-month component
grid cap with unchanged joined work/byte caps; larger path/grid combinations can
still refuse. Unsupported income, saved-book, capital and other product branches
fail explicitly. `graph-call-control-1` is an additive per-call execution capability,
not a financial model input: bounded one-shot native cancellation tokens, fresh
deadlines and captured-library cleanup reach the indexed native dispatcher, Rayon,
owner-lock wait and serialization. No graph retains a creation deadline or callback.
Absent runtime options preserve the old ABI/payload; missing requested capability
fails with rebuild required and no uncontrolled fallback. IO callbacks must be
bounded. A multi-call book operation still requires a caller-owned outer deadline;
worker lease/cancellation/publication fences and the process watchdog remain
separate requirements. Component token/cache/schedule budgets are not aggregate
RSS, hard real-time or 60000-position joint acceptance.

`deposit-revised-income-ledger-1` retains original source, purchase cost, effective
yield and contractual tails. Explicit reviewed revised Physical expectations post
one day-zero catch-up and keep individual gross FIFO earning/cash origins through
retirement. Original raw input and declared native normalization are separate;
actual completion, private independent source/ordered GL/persisted replay and
original deadline/cancel/code/lease/token fences remain mandatory. Old None output
and original-income routes remain separately readable and validated.

`mortgage-closing-capital-1` separately authorizes reviewed complete-tail closing
capital from a completed bounded research source. The new durable job owns a new
original deadline, immutable source/job/code binding and independently encoded
typed binary64 source digest. Rust rebuilds raw source and ordered GL; independent
source/capital replay and checksummed source/output rereads precede final scoped
SQL publication. Signed accounting balances are unchanged; supplied negative
exposure policy, regulatory inputs and monetary multiplier are explicit. Source
journal permission is unchanged. FTP, interim capital, accounting-standard,
empirical and regulatory approval remain false.

See `docs/models/native-contracts.md`, `market-indexed-books.md`,
`deposit-revised-income.md`, `mortgage-closing-capital.md` and complete remaining
issue gates in `docs/models/implementation-plan.md`. Local source/build validation
does not deploy or restart production workers, close empirical gates or complete
tracker 25.


## Complete-origin commercial holding windows (local 0.29.13)

`commercial-holding-continuation-1` replays complete original acquisition histories
and derives a sealed opening plus subsequent native posting indices. Original cost,
EIR, gross coupon lots, draw anchors, undrawn commitment and reviewed default state
remain retained. Completed raw/economic prefixes and CRN identities cannot change.
Future interval/projected assumptions require explicit retained source conventions.
No caller monetary checkpoint, credit restart or second acquisition is accepted.
Independent raw and persisted replay, original deadlines and final source/executable/
object/SQL publication fences remain required. Rebuild compatible native artifacts
and explicitly rebuild selected saved sessions/libraries before additive execution.

This is a holding-window foundation; verified prior-publication opening, cut-started
mixed daily adoption, seasoned graph/cohort/strategy coefficients, stopped acquisition
and secondary clean/dirty rights allocation remain open. Fees, daily in-arrears
coupons, generated streaming, empirical/policy evidence and complete mixed60000
acceptance remain required. See docs/models/commercial-continuation.md and the full
implementation ledger. Local checks do not imply deployment or full issue closure.


## Complete-origin commercial holding journals (local 0.29.14)

`commercial-holding-journal-1` transfers sealed complete-origin continuation
owners into one existing native daily Sim, retaining original credit clocks,
yields and complete cash/rights/recovery tails. The prospective journal derives
a balanced cut opening from that full journal, rolls prior signed P&L into
opening equity and retains ordered future postings with original transaction
lineage. Closing non-P&L stock bits must remain identical. Python independently
recomputes full raw sources, the complete daily journal and persisted window.
Original execution/publication deadlines and source/executable/object/SQL fences
remain mandatory. Inner admission caps and final tolerances remain unchanged.

This is complete-origin daily replay with a derived journal window. Verified
prior-publication opening, a cut-started mixed Sim, seasoned graph/cohort/strategy
coefficients, secondary conveyed rights and empirical evidence remain separate.
Nonintegral fees, daily in-arrears indexed earning, generated draws/reviews and
streaming, complete saved/product adoption and mixed60000 acceptance remain
required under15/25. See docs/models/commercial-holding-journal.md. Rebuild all
matching native artifacts and explicitly rebuild selected libraries/sessions;
local validation does not deploy or close the full issues.


## Verified modeled commercial prior openings (local0.29.15)

`POST /models/holding-journals` resolves a completed native holding source in
the authorized repository scope, independently replays its raw source/full
persisted journal, and binds its exact published cut opening and every completed
posting to the new complete-origin native replay. No caller balances, source
object descriptors or verification flags grant authority. Separate execution
evidence records verified modeled publication; native producer flags remain
unchanged and observed bank reconciliation is not inferred.

Both holding schemas now use the shared commercial final SQL guard, including
original queue deadlines, source/output bytes, executable/bundle, workspace
revision, request/attempt/token/lease and cancellation. Prior source authority and
all consumed objects are rechecked under that lock. Historical source deadlines
remain provenance; new work obeys the new job's original budget. Integer native
clock reconstruction must preserve retained milliseconds without a float round
trip. Shared bounded source transport retains existing FTP capacity limits.

Cut-started mixed replay, seasoned coefficients and secondary conveyed rights,
fees/in-arrears earning/generated streaming, observed validation and full mixed
60000 acceptance remain required. See docs/models/commercial-prior-opening.md.
Matching deployment/restart and explicit selected-library/session rebuilding
remain required; local numerical/source gates do not close full15/25.


## Separate nonintegral fee ownership (local0.29.16)

`nonintegral-fee-accounting-1` replays reviewed physical fee earning, cash,
prepaid refunds, unpaid-right waivers, future-service cancellation and recovery
in Rust. Signed gross receivables/payables and deferred/prepaid balances retain
complete supplied tails separately from loan principal/G/EIR. Exact source,
service/review/clock and independent ordered/persisted replay gate the durable
analytical route and its original deadline/final SQL authority fences.

This additive component does not bind native credit events or enter the mixed
Sim, graph/units, acquisition fee allocation, capital/FTP or saved economics.
Those integrations, earned refunds/secondary rights, observed/reviewed evidence
and complete mixed60000 acceptance remain required under15/25. Historical
cash-income schemas and ABI/protocols remain readable unchanged. See
`docs/models/nonintegral-fees.md`; matching artifacts/restarts and explicit
selected-library/session rebuilds remain required before adoption.


## Native loan and separate fee journal (local0.29.17)

`commercial-fee-journal-1` binds reviewed original-tranche fee earning/cash and
separate exit dispositions to one already generated physical commercial loan
owner. Native default/prepayment and explicit reviewed maturity events determine
waiver/refund/cancellation/recovery; fee LGD is not borrowed from loan policies.
Separate signed gross rights enter the same existing daily Sim once, with complete
fee tails, unchanged loan cost/EIR/carry and retained unweighted zero allocations.
Protocol7 adds fee lineage/daily/closing snapshots; historical6 is unchanged.
Actual fractional clocks cannot round into the preceding day. Independent raw
source, ordered shared journal, gross/report and persisted replay plus original
deadline/executable/object/final SQL authority fences gate durable publication.

This bounded lane does not yet adopt exercise/candidate/holding fee sources,
draw/secondary fee rights, fee acquisition allocation, Pricing graphs/units or
capital/FTP. In-arrears earning, generated streaming, saved/cohort/mixed strategy,
observed/reviewed histories and full60000 combined acceptance remain required
under15/25. See docs/models/commercial-fee-journal.md and the complete ledger.
Rebuild compatible native artifacts, restart deployed workers and explicitly
rebuild selected saved sessions/libraries before use; local validation does not
deploy or close the full issues.
