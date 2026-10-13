# Native commercial conditional exercise

An explicitly selected `commercial-native-exercise-1` policy in a commercial
product graph contains reviewed identities, permitted actual dates, call/put clean
factors and fees, and simultaneous-right priority. It contains no supplied
principal, coupon money, continuation values, membership partitions or decisions.
Before- and after-scheduled-cash decisions explicitly select their native phase.

Rust constructs native no-option Pricing contractual/credit paths, then forms
information sets from complete observed contract/rate and realized credit/event
prefixes. Future index rates remain hidden until both their fixing date and
selection timestamp. Path identifiers, future default draws and future outcomes
do not identify an information set. Initially observable contract/state data must
agree across the Pricing distribution. Each weighted path has one native credit
identity, and all positive weights sum to one.

For a decision date `t`, continuation is the conditional weighted mean of cash
strictly after `t` through the next decision, discounted at each actual event time,
plus the next decision value. Discounting uses the supplied collateral curve and
one immutable original OAS; a paired parallel bump enters separately. After-cash
redemption equals actual live principal times the contractual clean factor, plus
legal accrued interest and the explicit fee. The existing issuer/holder priority
determines the order of call minimization and put maximization. Exact ties retain
continuation or the preceding selected right. Defaulted or exited paths do not
exercise. A zero-principal contract with unpaid or future rights requires a
reviewed `settle_earned_cancel_future-1` termination owner. The native stop settles
actual earned rights and cancels future contractual cash without inventing principal.

The first reached decision becomes a private native stop. The same native credit
loop applies it at the selected phase and retires future source cash. Before-cash
decisions precede repayment, maturity and draw funding; credit events already
reached at that clock remain observable. A caller cannot deserialize or restore
a native stop. Default before
exercise retains the full default/recovery tail. Separately supplied Physical
paths use the Pricing policy only when their observed prefix has verified
coverage; uncovered states reject rather than guessing an action.

This conditional expectation is exact for an explicit finite path distribution.
It does not establish convergence for continuous stochastic market/credit models,
observed issuer exercise behavior or statistical accuracy. The distinction follows
the conditional-expectation requirement in
[Longstaff and Schwartz (2001)](https://escholarship.org/uc/item/43n1k4jb): a future
realized path payoff cannot be substituted for information available at exercise.
The finite-distribution implementation does not claim to fit their regression.

The graph keeps original OAS through current scenarios and paired risk bumps.
Acquired exercise coefficients additionally need the original stopped Physical
cash distribution and original cost/yield reconciliation. An analytical Pricing
receipt cannot supply that accounting anchor. Acquired units now retain and
verify the separately supplied complete original stopped Physical distribution
at the original purchase yield/cost before current scenario preparation.
Legacy threshold options and the supplied-DAG `conditional-exercise-1`
contract retain their existing meanings. This additive selection does not
reinterpret a saved contract or complete [issue #15](implementation-plan.md).

The separate [generated exercise contract](commercial-generated-exercise.md)
fits continuation on native observable joint market/credit training paths and
evaluates a retained policy on fresh paths. The finite-distribution contract keeps
its original exact conditional expectation; it does not approximate a continuous
state law by assigning each generated path its own future-aware information set.
