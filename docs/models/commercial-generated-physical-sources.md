# Generated Physical source catalogs and reviewed actions

`commercial-generated-policy-1` may explicitly select
`physical_path_inputs` with schema
`commercial-generated-physical-path-inputs-1`. Omission retains the previous
request serialization, Pricing/Physical generation and admission formulas.
Every built-in financial calculation remains native Rust. The independent
Python modules reconstruct raw sources, seeded paths, original acquisition
proofs, ordered coupon books and persisted journals for verification.

## Immutable input and ownership

The selected object has an `identity`, `original_physical`, and an ordered
`current_physical` list. A current entry contains `scenario_id` and `inputs`.
Every role input contains these two collections:

* `source_recipes`: immutable `{identity, performing_draw_tranches}` catalogs.
  Each draw retains its complete existing source, original cost/EIR, legal
  contract, review, scheduled draw date, eligible origins and full cash tail.
* `paths`: complete ordered `{path_key, review, source_recipe_index,
  reviewed_credit_actions?}` selections. Keys must be exactly 1 through the
  evaluation sample count. Each path review identity is unique in its role;
  every catalog entry is used. Empty actions are retained by the explicit path
  coverage. Actions name their actual path key; a template key is never
  silently changed to a generated key.

Shared catalogs avoid repeating a complete draw source hundreds of times in the
raw request. They do not authorize prepared cash, monetary opening checkpoints,
caller stops, implicit yields or an alternative mutable source of truth.
The retained raw request binds every catalog, review and path selection. The
ordinary materialized source still contains its complete native
`performing_draw_tranches` and `reviewed_credit_actions` owners.

Pricing training and every Pricing evaluation/bump law declare their own common
contractual draw catalog in `source.performing_draw_tranches`. Physical laws and
every selected recipe must have the same ordered contractual feature inventory:
draw review/date/eligibility, face, acquisition, principal plan, coupon event
identities and contractual shapes. Historical observations remain unchanged.
Future `conditional_projection` rates may vary under the separately supplied
law; their values and source lineage do not become contractual decision keys.
The regression basis, policy, driver observation conventions, time grid and
loading inventory must match across applied Pricing/Physical roles.

`CouponLoading` may supply `draw_tranche_id` to identify a draw review ID;
omission keeps its original main-contract target. Every future indexed
observation in every source requires an exact native loading and observable
midnight grid point. Generated `conditional_published_projection` observations
remain unsupported by this generation tier and reject. The separately selected
observable supplied-path coupon owner remains available; this extension does
not infer a generated daily fixing/publication law.

## Actual draws, original acquisition and credit

The existing native credit/accounting owner executes a scheduled performing draw
only in its supplied eligible live origin. Prior default, prepayment or native
exercise prevents a later acquisition. Actual nominal principal, funded cash,
remaining undrawn commitment, original draw cost and original draw effective
yield remain distinct. The source retains all surviving coupon and recovery
rights after principal retirement. CCF/default funding remains the original
explicit credit policy and is not inferred from the scheduled draw catalog.

Original Physical acquisition is generated from each path's declared original
sources. Native original and current source equality is established at that
origin. Each draw's original cost and EIR must reconcile to its conditional
original stopped cash distribution. For draw `j`, with original acquisition
clock `t_j`, original EIR `y_j`, path weight `w_i`, acquisition indicator `I_ij`
and source-owned post-acquisition cash `C_ijk`, the retained proof checks

```
activation_j = sum_i(w_i * I_ij) > 0
quote_j = sum_i(w_i * I_ij * sum_k(C_ijk * exp(-log1p(y_j)*(t_ijk-t_j))))
          / activation_j
quote_j == supplied_original_cost_j   (existing quote tolerance)
```

The main acquired cash partition is checked separately against its retained
original quote and EIR. Native cash partitioning prevents a second acquisition
or a face/probability weighting from entering either original quote. No original
EIR is fitted from a future conditional payment.

Current inputs must bind the same original raw tranche source/acquisition as
the corresponding original sample. After native generation, each current draw
receives the compiled original source from that same sealed original path.
Both main and draw proof bindings include `path_key`; an original draw source
from another path cannot substitute even if it exists elsewhere in the proof.
The public shared original proof remains the same authority and representation.

## Observable policy state and reviewed books

One canonical source inventory determines feature count and ordering in Q
training, Q evaluation and Physical application. Historical main-coupon feature
ordering is unchanged. Each draw appends its actual acquisition flag, native
acquired nominal principal divided by original main face, selected
observable coupon rates and separate canonical coupon entitlement/legal-right
amounts. Before actual acquisition those draw features are zero. An indexed
rate also remains zero until its exact fixing/selection clock. Scheduled future
activation, recipe index, path/review IDs, future realized source values and
future allowance/chargeoff/waiver actions never become decision features.
Accounting G/cost/EIR remain complete book evidence. G is not a separate pricing
feature: after a random coupon settles it can be an affine duplicate of the
observed fixing with roundoff amplification in a regression. The native nominal
anchor and separate legal coupon states avoid that redundant accounting
coordinate without changing SVD thresholds or monetary tolerances.

Only Pricing training outcomes fit the continuation regression. Every fresh
Pricing evaluation, bump and separately supplied Physical role applies its
retained Pricing fit. Physical action rows do not fit continuation or change
Pricing outcomes. This is the existing supplied Markov/LSM research policy;
synthetic controls do not establish state coverage, predictive accuracy or
convergence.

Reviewed actions are applied by the existing full coupon book owner on their
actual review dates. Allowance targets, principal/basis chargeoff and legal
waiver do not rewrite economic credit transitions, contractual cash, original
acquisition or complete recovery tails. An unknown default/coupon claim fails
when its action is replayed. Original-role actions are also validated through a
genuine full native owner after its original acquisition proof is compiled.
Signed legal/recognized claims retain gross ownership. Late observable coupon
fragments use the separate claim owner described in
[observable coupons](commercial-observable-coupons.md) when that supported raw
source is explicitly selected outside the generated fixing tier.

## Bounds, pass ownership and publication

Existing 128..512 sample, 64-feature, source, raw-input, 4-billion work and
128-MiB capacity limits remain. The selected Physical input catalog itself is
bounded to one MiB; wrapper and candidate raw limits remain unchanged. Raw
catalogs, expanded per-path recipes, one live full native owner, compact book
points, all stopped tails and shared original proof coexistence are charged
before financial generation. The consuming candidate journal additionally uses
the maximum selected per-path full owner, reviewed-action tail and stopped-source
capacity. A graph result does not grant journal admission. The journal's
existing eight-million work/32-MiB compact limits remain separate.

The source evaluation reservation follows actual native calls. A generated
Pricing role collects `N_train` source paths, fits retained training data, then
constructs one evaluation prefix and one accepted stopped tail per evaluation
path: `N_train + 2*N_eval` genuine owners. Prospective declared LSM payoffs use
the retained native principal/legal state and reviewed clean terms; they do not
materialize another source. Each current Physical path constructs a prefix and
its stopped full accounting owner. Each original Physical path constructs its
prefix, stopped source and original acquisition proof replay; a selected
original reviewed action adds a full book validation. Four passes per original
path cover those owners. All grid generation, feature/forecast scans,
regression work, every scenario/bump and raw recipe capacities remain charged.
Only the new explicitly selected catalog tier removes hypothetical duplicate
training/Physical fit passes. The omitted tier keeps its old reservation.

Original OAS calibration generates its training/evaluation samples once and
refits retained rows at each trial; the accepted stopped sample is replayed
once. Each additional trial retains its fitting/discounting reservation.
Calibration columns use the selected linear/quadratic/cubic basis and complete
draw feature bound. Existing no-draw quadratic reservations are unchanged.
Measured test counters verify true owner and source-generation counts against
the selected pass reservation, including the complete main/draw sources.

Native tests cover complete source/path coverage, once-only draw acquisition,
conditional own quote, cross-path anchor substitution, future feature masking,
path-specific reviewed action replay and owner/source pass counts. Independent
raw-source graph controls cover fixed and indexed generated draws and per-path
reviews. A selected per-path review after the contractual cash tail has a positive
complete persisted candidate replay, with ordered source/journal, gross closing
and tamper checks. The minimal two-scenario128-path drawn candidate with a day-1 draw repaid
on day2, no options, no indexed loading and no reviews still refuses before
generation: work4,800,566,464 exceeds the unchanged four-billion bound while
bytes124,155,560 stay below128MiB. Each source owner reserves four factor
constructions, each retaining the existing250,000-work generic floor;3,712
reserved genuine owner passes already charge3.712billion from those floors.
Carry/default/book/query maxima remain additionally charged. Analytical draw
support therefore does not complete candidate draw-journal adoption within this
tier. Further component-bound refinement or genuine source reuse with independent
controls remains required; caps and minimum path counts have not changed. The
combined draw/late-review fixture separately refuses under the same admission. The reviewed
owner still reserves the selected day-40 tail rather than the empty template. Source/executable identity, original
deadline, cancellation, lease/token and immutable publication fences still apply.
No local control is a deployment, observed calibration, accounting approval or
complete issue-15/25 mixed-book scale acceptance.
