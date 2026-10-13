# Commercial #15 native integration and validation

Local engine version: **0.29.10**. Issue
[#15](https://github.com/beauschwab/model-portfolio-sim/issues/15) remains open.
This review records a supported implementation stage, preserving every original
implementation and acceptance gate. It does not establish deployment, empirical
accuracy, accounting approval or complete tracker #25 acceptance.

## Implemented behavior

Commercial native coupon factors now enter the shared event-time credit/carry
loop. Actual entitlement, legal coupon rights and recognized coupon lots remain
separate. Explicit original purchase cost/yield, Physical and Pricing sources,
issuer/borrower identity, integral fee cash, reviewed nonaccrual, contingent default
funding, prepayment premium and complete recovery tails retain their owners.
No Python production financial callback or calculation fallback is added.

`commercial-product-graph-1` retains original base OAS and paired common draws.
Its native continuation policy constructs observed-prefix information sets and
replays actual stopped cash. Future rate observations/default outcomes cannot
select a current action. The supplied finite distribution is explicit; continuous
joint market/credit generation and convergence are separate gates.

`commercial-units-1` derives native acquired nonoptional coefficients from
authoritative Physical interest postings and paired Pricing risk, with complete
actual cash-tail funding. `strategy-commercial-1` constructs HiGHS problems,
replays source amounts and retains coefficients for constraint-only evaluation.
Face, purchase cost, category and refreshed capital dependencies remain distinct.

`commercial-credit-ledger-1` compiles complete Physical sources into the existing
daily Sim and journal, stream protocol 6. It retains actual day-zero acquisition,
gross source lineage, daily/closing coupon books and a full contractual/recovery
calendar separately from the original reporting date. It admits exclusive
commercial owners and cash accounts; arbitrary mixed owners remain unsupported.

Durable commercial graph/unit analyses and daily journals independently verify
raw sources and persisted reports. Final publication checks the accepted deadline,
source/executable identity, every verified object and queued authority inside the
SQL lock, including current cancellation, lease/token, revision and attempt.
Rehashed financial tampering and changes after verification fail publication.

Detailed mathematics, schemas and limits are in
[commercial graphs/strategy](../models/commercial-graph-strategy.md),
[native exercise](../models/commercial-exercise.md) and
[commercial credit journals](../models/commercial-credit-ledger.md).

## Fresh validation

Evidence directory: `artifacts/issue15-installed-adoption-validation/`.
The official product, ledger and decision/workflow build wrappers completed;
the frozen product and decision wrappers were rerun after final Rust edits.
Eight executable/library SHA-256 identities are retained in `artifacts.json`.
The current dependency/input freeze has 803 files and aggregate SHA-256
`edeaa0f32f1356e2dc4aac4c82cb79257fc327e2baf073299dae0ae536bc04d8`.
This freeze includes pre-existing dependency work and is not a commit change set.

| Gate | Fresh result | Evidence |
|---|---|---|
| Shared model-core all-target Rust tests | 377 passed | `core-tests.log` |
| Product all-target Rust tests | 454 passed | `risk-tests.log` |
| Ledger all-target Rust tests | 196 passed | `ledger-tests.log` |
| Decision all-target Rust tests | 128 passed | `decision-tests-final.log` |
| Core/product/ledger strict all-target/all-feature Clippy | Passed | `portfolio-*-clippy-final.log` |
| Decision strict all-target/all-feature Clippy | Passed | `decision-clippy-final.log` |
| Decision documentation compilation | Passed; zero executable doc tests | `decision-doc-tests-final.log` |
| Official commercial source/stream/reference gates | 42 passed | `commercial-engine.txt` |
| Official commercial graph/unit gates | 19 passed | `engine-repair-tests.log` also includes the repaired journal case |
| Official commercial durable API gates | 42 passed, one dependency warning | `commercial-api.txt` |
| Full engine regression | 2,452 passed, two skipped in 431.76s | `engine-tests-final.log` |
| Full API regression | 1,009 passed, one dependency warning in 1,049.52s | `api-tests.log` |
| Architecture and generated HTTP/native ABI contracts | Passed; 110 existing reviewed cycle edges | `commercial-architecture.txt` and fresh parent checks |
| Operator skill mirrors and packaged Markdown | Identical | Fresh byte comparisons and archive validation |

The first full engine run retained 2,451 passes, two skips and one failure after
collecting the earlier oversized multiaccount fixture. The same binaries pass the
corrected case plus all 19 graph/unit cases. No admission ceiling or financial
tolerance changed. The final full rerun passes with frozen fixtures. An initial API run
from the repository root failed import-only collection; the full run uses the
documented API working directory. These failed logs remain retained.

Earlier Decision documentation compilation failed while Rust source was changing
during a release test build; current all-target tests and fresh documentation
compilation pass after the source freeze. The recovery-tail Rust control initially
exceeded the aggregate budget. Its replacement proves an actual default on day 25,
cash recovery after reporting day 35 and complete replay through day 55. The
supported multiaccount and multipath controls remain separate admitted requests;
their larger combined request explicitly rejects before source generation and
leaves its output directory empty.

## Remaining completion gates

| Original scope | Current boundary requiring further work |
|---|---|
| Commercial draws and complete credit accounting | Performing discretionary draw tranches, seasoned source prefixes, nonintegral/legal fee claims and reviewed allowance/chargeoff/legal-waiver integration |
| Complete contractual closure | Daily IEEE accumulation can leave a legal-right residual; exact reviewed maturity disposal rejects it. Canonical cumulative entitlement reconciliation remains required |
| Conditional call/put and acquired units | Before-cash and rights-only termination owners; original stopped Physical acquisition cash/yield anchors; continuous generated joint market/credit convergence and valid boundary cases |
| Graph/library/accounting adoption | Complete saved-book, cohort and mixed candidate/library adoption; reviewed closing capital/FTP integration |
| Integrated acceptance | Original issue #10/#11/#12 dependencies and all original #15/#25 acceptance criteria, including supplied observed histories/policies and complete mixed scale evidence |

No empirical history, reviewed legal/accounting/regulatory policy or deployment
is inferred from a synthetic fixture. New explicit selections require compatible
native artifacts and library/session rebuilds. Production API/workers have not
been restarted, and these changes remain local and uncommitted.
