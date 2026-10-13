# Joint product funding: modeling rationale and acceptance evidence

This companion to [modeled-product FTP](modeled-product-ftp.md) records design
requirements for issues #12, #14, #20 and #24. Source2 funding and joint derivative
ownership are isolated continuations under review. Their acceptance remains
separate from the installed CD/source2 source freeze and from complete issue #25.

## Research basis

The interagency FTP guidance separates ordinary funding from standby liquidity,
requires transparent aggregation and independent review, and illustrates separate
reset and liquidity tenors for floating exposures. It calls for documentation when
FTP and stress behavioral assumptions differ, and consideration of derivative
collateral rights and stressed outflows. Its methodological examples are
illustrative; they do not prescribe one formula. See principles 1–3 and the
non-trading and trading attachments in the
[Federal Reserve's interagency guidance](https://www.federalreserve.gov/frrs/guidance/interagency-guidance-on-funds-transfer-pricing-related-to-funding-and-contingent-liquidity-risks.htm).

The Basel liquidity principles call for granular dynamic cashflow analysis,
behavioral funding sensitivity, derivative exercise/maturity and collateral
demands, currency analysis and collateral availability by entity and location.
They discourage assuming automatic overnight rollover and require realistic
transfer timing and restrictions. See LQY 10.34–10.35, 10.45, 10.48,
10.96–10.102 and 10.131 in
[Liquidity risk management and supervision](https://www.bis.org/committees/bcbs/basel-consolidated-guidelines/module/lqy/10).

The FSI better-practice study supports central treasury oversight, marginal
funding costs matched to product maturity, blended treatment for amortizing or
non-maturing products and explicit contingent usage. It is a survey-based
author paper, not a certification of this implementation or a binding formula.
See [Liquidity transfer pricing: a guide to better practice](https://www.bis.org/publications/fsi-paper-10-liquidity-transfer-pricing-guide-better-practice).

## Application to the native model

The following are repository design conclusions drawn from those sources and
the production contract. Numerical and accounting choices still require explicit
reviewed policies; the sources do not supply missing customer histories,
original yield, legal netting permission, curves or regulatory weights.

| Requirement | Implementation and validation obligation |
|---|---|
| Separate reset and funding life | Retain rate-reset dates, complete physical principal and gross coupon tails, and explicit funding curve/policy identity. Test an exposure whose next reset precedes final runoff; do not replace both tenors with one scalar. |
| Exclusive economic ownership | Preserve entity, currency, canonical position and account lineage across selected source jobs. A second job ID cannot authorize a second allocation to the same economic owner. Test duplicate-owner refusal and distinct-owner acceptance. |
| Complete cash and gross rights | Retain original/current histories and each earned/cash origin through settlement. Check original-EIR recognition and revision catchup separately. Do not fund settled cash again or erase small signed residuals. |
| Joint cash availability | Execute deposit and derivative events in one chronological journal with one reviewed opening cash/equity source. Independently reconstruct ordered postings and closing balances; adding standalone closing reports cannot prove this requirement. |
| Derivative and collateral scope | Exercise the existing complete admitted standalone domain, including options/delivered legs, seasoned openings, collateral and closeout policies. Verify custody and legal mapping independently. A vanilla swap fixture alone cannot establish this coverage. |
| Explicit contingent liquidity | Retain supplied draw, flight, collateral and haircut policies with scenario and measure identity. Disclose differences between funding and stress assumptions. Expected funding calculations do not establish observed withdrawal prediction or intraday availability. |
| Conservatively bounded generation | Admit aggregate source preparation, full tails, parser/encoding staging, journal work and retained capacity before generation. Preserve the original deadline and cancellation through routing, parsing, child preparation, replay and publication. Component capacity limits do not guarantee total process memory. |
| Immutable publication | Verify raw source, model identity, reviewed policy, exact receipt counters and independently persisted postings before successful publication. Repeat ownership, artifact, deadline and private lease/token checks at final commit. Late failures must publish no successful result. |

## Evidence required before completion

The selected source2 tier's 365-day input-shape ceiling is distinct from admission
of every possible tuple and from general long-tail deposit coverage. Current
40-day multi-owner and 90-day single-owner examples establish those examples
only. Joined work and retained capacity remain charged without relaxing source
or journal ceilings. Broader portfolio capacity requires explicit implementation
and the original combined 60,000-position graph/credit/solver/journal gate.

Historical Card2/source1 byte controls protect those exact inputs and outputs;
they are not independent financial validation. New source2 validation requires
native financial execution, independent original-source reconstruction and
persisted journal/funding replay. Browser evidence must use actual durable
workers and private publication checks, including failed-publication cases.
Contained artifacts must be rebuilt and checked again against the final installed
source before default execution is accepted.

Observed deposit, borrower, repayment and credit histories with chronological
holdouts remain necessary to establish empirical accuracy. Supplied scenarios,
synthetic fixtures and numerical parity cannot substitute for that evidence.
The original [implementation plan](implementation-plan.md) and child issue
criteria remain the completion authority. No financial tolerance, fixed base OAS,
shared random-number or coefficient-only interactive contract changes here.

## Shared derivative replay work reservation under review

The proposed raw structural reservation must serve both standalone saved
derivative replay and joint deposit/derivative replay. Charging only the native
source's 250,000-unit ceiling does not reserve the mapped journal's complete-tail
scans. The correction is an isolated successor; earlier receipts remain evidence
for their original source, and corrected final-source gates remain required.

For each admitted Physical source, let `N` be its supplied daily nodes, `B` its
distinct legal buckets, `T` its opening trades, `U` the sum of all node trade
slots, `C` a conservative count of raw potential cash rights and `D` the complete
mixed replay days, including as-of. `C` includes fixed/floating schedules in
every node, option states and expiry/terminal events, and delivered underlyings.
Optional Pricing XVA inputs are reserved separately and create no physical GL.

The reviewed posting-line upper is

```text
L = (7 + 2T) + 10U + 20NB + 8B + 2T + 6C
replay_upper = L * (1 + 6D) + 16N
```

Every arithmetic operation must check overflow before financial generation.
The terms correspond to current native emission branches:

| Term | Structural argument |
|---|---|
| `7 + 2T` | Explicit cash/equity, VM/IM/custody opening lines and both gross carrying sides per opening trade. |
| `10U` | At most two three-line marks and one four-line physical delivery per node/trade slot. |
| `20NB` | Across a complete path, at most one generated VM call per node/bucket, each settled once; collateral interest has at most seven financial/custody lines per node/bucket and tail return at most six per bucket. Twenty is a conservative whole-path bound. |
| `8B + 2T` | At most one reviewed closeout per bucket, its six financial and two custody lines, and both gross carrying reversals per trade. |
| `6C` | At most three due lines, two later MPOR settlement lines and one default unpaid retirement line per potential right. |
| `L * (1 + 6D)` | Source/intents validation plus the existing complete mixed-day historical-line scans, including days after a shorter derivative source closes. |
| `16N` | All sixteen current financial/custody account kinds can remain in each derivative daily closing; zero-value/netted output cannot reduce admission. |

Multiple delayed VM calls may settle on a single day. Consequently the `20NB`
argument applies to the full path and cannot be interpreted as a per-day
movement-count limit. Actual delayed/unavailable settlement and catchup cases
must verify this property without dropping historical calls.

The joined admission charges the dated base once, each complete source
reservation, raw transport work and retained/staging capacity before projection.
Observed mapped work must compare against the dated-plus-derivative reservation.
A field containing complete mapped work must retain that disclosed meaning;
it cannot be compared to a derivative-only upper or used to double-charge the
dated base. Native and independent references must agree on raw cardinalities,
complete replay days, overflow refusal and exact source/receipt identity.

Required validation includes admitted option/delivery/default/CSA branches,
longer mixed tails, zero-generator aggregate refusal, unchanged historical
streams and refreshed original-source/persisted-journal gates for both routes.
This posting bound does not by itself establish aggregate process memory,
empirical accuracy or the required combined 60,000-position acceptance.
