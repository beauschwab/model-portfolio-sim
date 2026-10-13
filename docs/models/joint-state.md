# Joint market, behavioral and credit state

`joint-state-1` is a bounded Rust coordinator for a product that already has
explicit market, behavioral and credit drivers. It provides one event-time
state path per position and preserves the caller's common-random-number (CRN)
identity across competing exits. The Python adapter
`analytics.model_contracts.evaluate_joint_state` only serializes the request;
it does not calculate probabilities or provide a fallback.

## Inputs and identity

The request carries `source_id`, `source_revision`, `assumption_id`,
`common_random_numbers_id`, and an explicit `measure` (`contractual`,
`pricing`, or `physical`). The measure is a label. The coordinator never
converts physical probabilities to a pricing measure, solves an OAS, or
derives a market curve from a cashflow stream.

Each period is an actual-date interval with a supplied market discount factor,
three annualized competing hazards, a recovery fraction, and a recovery lag.
Each path supplies a unique position and path key, principal, and exactly one
caller-owned pair of uniforms per period. The same pair can therefore be sent
to a sibling product graph while preserving scenario parity.

## Competing event math

For an interval of length \(\Delta t\), let

\[
  h = h_b + h_c + h_p,
  \qquad p_{exit} = 1 - e^{-h\Delta t}.
\]

The behavioral-exit, credit-default, and prepayment probabilities are,
respectively,

\[
  p_b = p_{exit}h_b/h,\quad
  p_c = p_{exit}h_c/h,\quad
  p_p = p_{exit}h_p/h,
\]

with all three equal to zero when \(h=0\). The waiting uniform selects the
first event in that stable order: behavioral exit, credit default, prepayment,
then continuation. The destination uniform is retained in the input for CRN
identity and future state-specific destinations; this bounded coordinator does
not invent a destination transition.

An exit consumes the live principal once. Credit default records recovery
`principal × recovery_fraction` and economic loss as the residual. The market
discount factor is reported on every event and is used only for the supplied
live-balance diagnostic. Recovery lag is retained on every event as an explicit
policy input; actual recovery-date cashflow and journal posting require a
downstream accounting owner.

## Bounds and validation

Rust rejects unknown fields, non-contiguous actual-date periods, duplicate
position/path identities, out-of-range hazards, invalid discount factors,
negative principal, missing CRN pairs, and non-finite values. The route admits
at most 4,096 periods, 10,000 paths, and 2,000,000 path-period work units.
These are component limits and do not constitute an aggregate process RSS
guarantee. Native request/output size and thread limits remain those in
`native-contracts.md`.

## Adoption boundary

This contract supplies a reusable joint state frame; it is not empirical model
validation, interval-censored fitting, LGD/EAD calibration, or a CECL/IFRS 9
allowance calculation. It does not automatically alter saved-book economics,
the daily ledger, FTP/capital, a Strategy Lab library, a cohort audit, or a
complete mixed-book graph. Those integrations must retain fixed base OAS,
shared CRN, independent Python reference checks, persisted source/journal
verification, and their issue-specific observed-data and scale gates.

This separation follows current interagency model-risk guidance: validation
must assess conceptual soundness, limitations, ongoing monitoring and outcomes
analysis, and extending a model beyond its original use requires analysis of
the new use and its limitations. See [SR 26-2](https://www.federalreserve.gov/supervisionreg/srletters/SR2602a1.pdf).
The contract therefore treats supplied hazards and synthetic CRN paths as
implementation fixtures until observed outcomes, chronological holdouts and
product-use monitoring are available.

The current tests prove schema/domain validation, exact competing-hazard math,
CRN identity preservation, and public native dispatch. Synthetic fixtures do
not establish predictive accuracy. Production use requires rebuilding the
native product and dependent workflow artifacts and explicitly rebuilding any
saved library or session that elects this route.
