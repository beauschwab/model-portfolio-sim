# Commercial graphs, coefficients and allocation

The additive `commercial-product-graph-1`, `commercial-units-1` and
`strategy-commercial-1` contracts explicitly select the commercial source and
credit owners. They do not reinterpret historical corporate `annuity`, floating
quarter-rate proxies or threshold exercise. Rebuild native artifacts and selected
libraries/sessions when adopting them. Their numerical assumptions remain
separate from observed calibration and accounting or regulatory approval.

## Raw graph and original valuation anchor

Each graph identifies its input/model/grid revisions, CRN, currency, actual as-of,
reporting dates, original market scenario and complete current scenarios. Each
position identifies commercial lending, issuer-credit debt or a credit-free
reference. Credit-free references reject hazards and contingent commitments.
The selected commercial owner uses lender/holder receipt-side contracts and
asset units. Issuer-credit debt describes the borrower's credit, not an adopted
funding-liability owner for the institution's own issued debt. Currency labels
must agree; the contract does not infer FX conversion.

The original Pricing histories, original signed cash quote and explicit OAS target
establish one immutable original OAS. Current histories separately supply Physical,
Pricing, Pricing-down and Pricing-up sources and their weights. Native validation
requires compatible original contract/acquisition anchors, complete source tails,
path keys and shared draw-stream identities. A Pricing source does not generate a
Physical accounting policy or a Physical probability distribution.

For cash component total `C` at its native event year `u`, native valuation sums
the supplied Pricing path weights times `C`, discounted using the collateral
curve and `exp(-original_oas * elapsed_years)`. Reporting marks include only cash
strictly after the report boundary, using the curve ratio from that boundary.
Principal, coupon, fee and actual recovery cash remain separate source components.
Economic loss is not another cash payment. Subday defaults and recoveries retain
their actual valuation times. No complete-tail event is moved onto a report date.

The original quote either verifies an explicit OAS or selects bounded native OAS
calibration. Current scenario and down/up valuations retain that same OAS. With
parallel bump size `b` basis points, `DV01 = (PV_down - PV_up) / (2*b)` in currency
per basis point. Paired sources retain shared draws; an option policy can change
under a bump while its original OAS remains unchanged. The graph represents the
explicit supplied distributions; it does not fit their credit or market laws.

Optional [native conditional exercise](commercial-exercise.md) constructs its own
information sets and stopping decisions from native sources. Bellman value must
reconcile to independently generated actual stopped cash. Original and current
policies retain separate receipts. Physical replay requires covered observed
prefixes. Defaults before exercise retain complete recovery claims.

Admission precedes financial generation: at most 32 positions, 13 scenarios,
64 histories per role, 256 aggregate source requests, 32 million aggregate work
units and 128 MiB retained component capacity. The complete result has a 32 MiB
encoded bound. These limits do not promise aggregate process RSS or combined
60,000-position acceptance.

## Generated policy graphs

A position may explicitly select `generated_policy` with a
`commercial-generated-policy-1` request and
`admission_tier="bounded_generated_policy_replay-1"`. This selection is exclusive
with supplied finite histories and the finite exercise owner. The current tier
admits one position, within four billion conservative work units and 128 MiB
estimated retained capacity. Ordinary graph/source caps remain unchanged.

The request supplies original Pricing training/evaluation and original Physical
laws, plus complete current Physical and Pricing laws. Each current scenario has
base/down/up Pricing laws with the same training/evaluation stream identities.
Pricing training alone fits continuation; each separately supplied Physical law
applies that corresponding Pricing policy. Physical outcomes cannot refit it.
The stochastic assumptions and limits are documented in
[generated exercise](commercial-generated-exercise.md).

This tier requires a locked original OAS. Its bits must agree with the original
training request, whose generated Pricing cash independently replays the original
quote. General Monte Carlo OAS calibration is not implemented. Original Physical
acquisition generation uses the supplied original contract, independent of later
current coupons or scenarios. Its complete stopped weighted cash reconciles to
original cost and EIR; current assumptions cannot replace this proof.

Compact sealed raw recipes regenerate genuine native Physical accounting owners
one at a time. Reports retain source bindings, native stops and book coefficients;
they do not retain every expanded source owner simultaneously. Capacity charges
all retained recipes and original proof plus the peak temporary source/fit owner.
Independent verification regenerates raw Gaussian/credit paths, regression,
stopped cash and accounting points. Component capacity is not an aggregate RSS
guarantee. Generated path-specific draws/reviewed actions and generated mixed
candidate journals remain unsupported and reject before generation.

## Actual-purchase coefficients

Each acquired unit supplies its exact unit/template identity, positive
face, actual purchase graph, accounting category, explicit capital-policy revision
and research regulatory weights. Source opening principal must equal unit face;
face cannot be a denominator invented to rescale a different contract. Original
Physical source cash, purchase cost and original effective yield must reconcile.
An exercised unit supplies complete separate original Physical histories. Native
stopping selects their actual original acquisition cash under the original
Pricing policy, and their weighted full cash reconciles to original purchase
cost and original effective yield. Current scenarios cannot replace that proof
or refit the original yield. Per-tranche acquisition cash remains distinct.

The tensor layout is `[scenario, unit, metric, reporting_interval]`. Coefficients
are expressed per unit of allocated face. Physical `InterestIncome` postings in
`(interval_start, interval_end]` supply NII; contractual coupon cash is not used as
a substitute. The authoritative state at interval start supplies principal.
That state retains live or default principal claims, so a supplied RWA weight is
not a regulatory determination of default exposure eligibility. Paired Pricing
marks supply DV01. HQLA, outflow, ASF, RSF and RWA coefficients use explicit
supplied weights; no eligibility, capital requirement or FTP cost is inferred.

The funding grid contains reporting dates and every actual Physical cash date
through contractual, prepayment, default and recovery tails. Acquisition cash is
the original purchase quote per face; subsequent cash is separately weighted once.
Funding demand is the negative cumulative bank cash. Exact native integer dates
retain their dates; subday cash is attributed to the enclosing day end, consistently
with the [daily journal](commercial-credit-ledger.md). Funding does not stop at the
last NII interval and does not assume future reinvestment. Output admits at most
one million funding cells and 250,000 cash rows, within the result byte bound.

## Native allocation and retained sessions

`optimize_commercial` constructs HiGHS inputs from native prepared units, explicit
scenario baselines, constraints, actual-date funding and typed face limits.
HiGHS remains C++; Rust constructs and validates its financial problem. The
objective remains worst-case external NII. Total face and quoted purchase cost
have separate limits. A capital limit needs the exact corresponding preparation
identity; changed financial inputs require refreshed capital dependencies.

After solving, Rust independently reduces retained source cash and coefficients
for the complete ordered allocation, including zero allocations. NII, funding and
purchase amounts must reconcile to the solver replay at unchanged tolerances.
The caller cannot deserialize replacement prepared coefficients.

`create_commercial` retains that native preparation in an actor. Evaluation and
constraint-only updates use it without pricing or credit/accounting generation.
The native actor's coefficient evaluation also avoids raw-source cash replay;
source and allocation replay remain creation/solve publication gates.
Dirty unit edits reuse exactly equal native sources; changed identity/grid/count
requires preparation. Staging, publication and abort retain version/token checks.
Each commercial actor call receives its own control; the creation deadline is
not retained as the session's lifetime deadline.

## Durable analysis and remaining adoption

`POST /models/runs` admits the graph and unit schemas to the durable worker. The
worker independently reconstructs raw contractual/credit/exercise sources and
coefficients before and after lossless Parquet persistence. The accepted queue
deadline, raw request, source/executable identity and all uploaded objects are
checked again with the SQL publication lock and current lease/token/attempt.
Cancellation, changed authority or changed artifacts cannot publish a successful
result. Existing interactive Strategy Lab libraries and saved books are not
automatically converted by an analytical result.

Explicit performing draws, reviewed credit actions, acquired exercise anchors,
durable commercial cohorts/libraries and standalone finite candidate/treasury
routes now have their own supported gates. See
[purchased journals and treasury](commercial-treasury-candidate.md).
Seasoned prefixes, nonintegral fee claims, generated path-specific draws/reviews,
general generated OAS calibration, complete mixed candidate/saved-book adoption,
broader product capital/FTP integration, general exercise convergence and empirical
holdouts remain separate required gates under
[issue #15 and tracker #25](implementation-plan.md). The native finite distribution
and synthetic fixtures do not establish observed accuracy.
