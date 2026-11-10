# Observable commercial indexed coupon ownership

This explicit source convention prepares actual commercial credit/carry and
reviewed coupon accounting. It is selected inside the existing commercial
request; the finite product graph and acquired source owners consume that same
compiler. It is not a separate pricing calculator. Original acquisition cost,
effective yield, original OAS, source cash identity and actual settlement remain
separate. The complete #15/#25 acceptance criteria still apply.

## Convention and source selection

Select both `factor_policy.overnight =
"observable_index_prefix_simple_margin-1"` and
`factor_policy.observation_mapping =
"selected_observation_weight_over_interest_daycount-1"`, with the existing exact
credit ACT/365 elapsed clock, reviewed partial-day and native endpoint policies.
The review identity is mandatory. Historical `pre_observation_accumulator_partial_weight-1`
and `explicit_equal_calendar_span-1` selections keep their old economics and
start-midnight availability guard. No default or saved model is replaced.

Every observed source must still have been published at the retained as-of
snapshot cutoff. Future observed fixings remain prohibited. Existing conditional
projections retain the rule that their observation selection uses no later day's
information. The additive `conditional_published_projection` variant instead
retains `model`, `path`, `measure`, `state_information_unix_seconds` and
`publication_unix_seconds`: conditioning state must be available by observation
start midnight; modeled publication must be at or after observation end. This
variant is overnight-only, requires matching path/measure, and is never an
observed fixing. It requires the new observable factor selection.

Availability for recognition is the maximum of actual/modeled publication,
selection cutoff, fixing-value-date midnight, and any term reset cutoff. The
compiler inserts these exact relative-second clocks into existing credit
intervals, retaining the original hazard, reviewed recognition rate, and CRN
stream. An information clock does not draw a new random number or refit yield.
Actual coupon cash still refuses settlement before its fixings/publication and
selection are available. No advance-estimate/true-up cash policy is inferred.

## Quantified factors and day weights

For observation i, let q_i be the native bounded all-in selected rate, m the
contract margin, d_i(t) elapsed interest day count through t, w_i(t) its elapsed
observation weight, and K_i(t) one only when its information clock has arrived.
The observable quantified factors are:

- Daily simple: F(t) = sum_i [K_i(t)(q_i-m)w_i(t) + m d_i(t)].
- Daily compounded, simple margin: F(t) = product_i
  [1 + K_i(t)(q_i-m)w_i(t)] - 1 + m sum_i d_i(t).
- Daily compounded, compounded margin: F(t) = product_i
  [1 + K_i(t)q_i w_i(t)] - 1.

The implementation accumulates products with log1p/expm1 in contractual order
and retains positive-factor validation. A simple spread already known by
contract accrues separately; a compounded spread remains coupled to its
observation. Unknown indices supply no guessed number. Their contingent legal
obligation is **not** declared waived or fully settled.

Unshifted weights use actual interest days. Supplied shifted weights use actual
observation dates. Equal calendar spans retain their exact original day mapping;
unequal spans explicitly allocate a selected observation's known weight in
proportion to elapsed interest day count. This is a reviewed partial-observation
convention, not proration of an unknown full-period coupon. All complete endpoints
retain the full native observation weight. ACT/360, ACT/365F and ACT/ACT ISDA
remain explicit; no holiday calendar, lookback date or time-zone conversion is
invented. Supplied lookbacks and lockouts select/reuse explicit observations;
the numeric lookback count alone does not certify the supplied calendar mapping.

The canonical coupon factor and incremental earning use the same source owner.
At an availability clock its newly known past service is caught up under that
clock's current credit/recognition state. Reviewed nonaccrual can therefore stop
recognized income while quantified legal rights continue. Positive and negative
rights retain their existing gross ownership. Original purchase EIR remains
unchanged; an information update does not infer a new accounting yield.

## Contingent terminal rights

Without an explicit terminal review, default/prepayment/exercise with served but
unavailable index observations still refuses. The source never treats an unknown
rate as a legal waiver, final zero coupon, or already available future fixing.

The additive `factor_policy.terminal_obligations` selects a reviewed continuation:

```json
{
  "review": {"id": "approved-model-policy", "revision": "1"},
  "live_exit_recognition": "recognize_known_served_right_at_information-1",
  "live_exit_settlement": "retain_original_coupon_settlement_after_information-1",
  "default_recognition": "new_known_default_right_is_legal_only-1",
  "default_recovery": "original_signed_interest_fraction_and_tails_after_information-1"
}
```

These are explicit model policies requiring review, not inferred legal or
accounting-standard conclusions. The native terminal owner freezes the served
interest interval at the actual exit/default clock, original coupon entitlement,
source/audit identity and factor recipe. It does not assign a monetary value to
an unavailable observation at that clock. `pending_coupon_obligation_indices`
retain the sealed current-source segment catalog references in native carry and
coupon-book snapshots. The source receipt retains those catalog dates,
publication/selection clocks, conventions and identities; the path retains the
actual service cutoff. Together they are independently reconstructible
contingent-right evidence, separate from quantified legal/recognized balances.
The indices retire only as the source information becomes available.

At each later information clock, the source recomputes the frozen service factor
with only observations then known. Differences become native
`pending_coupon_fixing` groups. Thus a partial served observation is determined
from its actual rate and weight when available; subsequent unserved observations
contribute nothing. Compounding cross terms are released when their necessary
rates become known. There is no pro rata allocation of the final full-period
coupon and no second acquisition of principal or original effective yield.

For a live prepayment/exercise, the selected review recognizes the newly
quantified served right at information time. Actual `pending_coupon_cash` remains
on its **original contractual coupon settlement date**, even when publication
is earlier. Principal retirement does not erase that right or accelerate its
cash. This specific convention does not implement an arbitrary alternative
terminal true-up date; such a policy requires an explicit additional convention.

For default, newly known rights remain legal-only: no default effective-interest
income is resumed. Cash uses the original default-origin **signed interest
recovery fraction and complete recovery tail weights**, separately from principal
LGD. Each pending component settles no earlier than both its information clock
and its original default recovery date. Unrecovered signed rights remain in the
closing gross coupon book. This is the explicitly selected recovery convention;
it is not an inference from a principal-loss assumption.

The continuation merges fixings and pending cash with the original native
recovery timeline in chronological order. Original same-clock recovery groups
keep their phase before new fixing groups, and pending cash follows its fixing.
The shared coupon book produces balanced intents for the existing daily Sim;
actual pending cash also enters native path economics and acquired cash
partitioning. Finite exercise selection must include the conditional value of
these stopped cash tails within the observable information group; a future
per-path fixing must never be included in a decision key.

A sealed `PendingCouponObligation` descriptor exposes served interval, fractional
day, source/audit and information clocks without a public restoration constructor
or caller monetary override. `NativeEarnedFactors` supplies bounded factor
recipes and catalog indices; the core multiplies native entitlement and applies
reviewed recovery policies. Source/point/query/continuation work and memo capacity
are admitted before projection. Historical no-terminal owners preserve their
previous selected behavior and omit all new fields.

Each released fragment has a stable source-owned coupon identity
`pending-coupon:{native_audit_index}:{information_unix_seconds}` and retains
its original contractual event/audit/cash lineage. With `reviewed_credit_actions`,
a defaulted fragment creates separate book and legal cash-only claims when its
amount first becomes known, with no booked interest or restarted EIR. Existing
allowance targets and principal/basis chargeoffs remain retained across those
publications. A legal waiver can name the fragment's
`commercial-interest:{path}:{coupon}:{origin}` identity after publication; it
cannot waive an unknown numeric claim before publication. The selected waiver
policy retains separately supplied voluntary recovery cash. That cash settles
its own once-only recovery identity through the explicitly selected reviewed
excess-cash income/allowance policy. Earlier claims are never replaced by a later
unreviewed source snapshot. Signed coupons retain the same gross claim ownership;
a signed recovery that would make a reviewed allowance negative still rejects.

Remaining boundaries include observation-specific alternative legal termination,
arbitrary reviewed true-up dates, contingent retained-payment **principal** when
a prior unknown coupon determines a subsequent opening balance, and generated
joint-law calibration/adoption outside the explicit supplied-path owners. Such
retained-payment sources reject before service starts with unknowable principal;
no future principal or fixing is guessed. The standalone source-income capture,
term graph, funding and derivative accounting routes reject delayed modeled
publication until an appropriate observable owner is selected.

## Verification and publication

Native tests cover simple/compound margins, prefix invariance when a future rate
changes, exact information clocks, nonaccrual, preserved pending memos, terminal refusal
without review, and explicit default/prepayment/exercise continuation. Signed
rights and separated late recovery dates are checked as well. Reviewed allowance,
chargeoff, signed fragment waiver, publication-before-review admission, once-only
recovery and native/independent book replay are also covered.
Independent Python reconstruction begins at retained raw sources and recomputes
source factors, credit intervals, original quote, coupon groups and graph output;
hand controls additionally use direct compounding products. It never dispatches
native financial callbacks. Publication continues to require source/executable
identity, original deadline/cancellation/lease fences, ordered persisted replay
and unchanged monetary tolerances. Component admission remains unchanged;
additional information clocks, frozen factor work, continuation rows and memo
capacity are charged before generation. Ordered IEEE lot-reduction residuals
remain evidence; no epsilon retirement or zeroing plug is introduced.

Tests and synthetic supplied projections do not demonstrate observed predictive
accuracy, legal/accounting policy approval, complete saved/cohort/strategy
adoption, full joint mixed-book scale acceptance or deployment. Matching native
artifacts and explicit library/session rebuilding remain required.

## Primary convention references

The [ARRC syndicated business loan conventions](https://www.newyorkfed.org/medialibrary/Microsites/arrc/files/2020/ARRC_SOFR_Synd_Loan_Conventions.pdf)
distinguish daily simple and compounded in-arrears rates from rates fixed for a
whole period in advance, discuss cumulative/noncumulative compounding, and prefer
business-day lookback without observation shift for the described loan market.
They identify mismatched observation/interest day weights as a prepayment and
secondary-transfer concern. Those voluntary recommendations do not choose this
repository's accounting or terminal legal policy.

The [New York Fed SOFR page](https://www.newyorkfed.org/markets/reference-rates/sofr)
states that SOFR is published on business days at approximately 8 a.m. ET. Actual
UTC publication timestamps, holidays and time zones must be supplied; the tests'
8 a.m. UTC timestamps are synthetic controls, not actual SOFR observations.
The [ARRC business loans archive](https://www.newyorkfed.org/arrc/publications/business-loans)
links the original technical appendices and worked lookback/compounding examples.
These sources were checked on 2026-10-15.
