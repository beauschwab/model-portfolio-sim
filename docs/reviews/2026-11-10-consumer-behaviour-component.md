# Consumer panel mapping and shared-path component

This checkpoint implements the specification, panel transformation, native fit and
shared credit-path portions of #91. It does not close that issue.

`consumer-behaviour-spec-1` retains explicit product state/exit policy, feature
names, domains, frozen transforms, categorical coding, availability lags, term
levels, training support policy and omitted-driver assumptions. Exact events use
the existing exact likelihood; snapshots use the interval-censored estimator.
The specification hash remains distinct from the coefficient hash. Both are
needed when policy changes leave observed state mapping unchanged.

Projection changes only declared zero hazard slots in the complete Physical
credit-path source. Original path keys/draws and monetary/recovery policies
remain intact. A joint response budget and actual serialization checks bound
the standalone native coordinator. It is not registered as a durable Model Lab
analysis before independent fit/publication verification exists.

Local evidence in `artifacts/consumer-behaviour-2026-11-10/`:

- 15 component tests pass: all three products, hand hazards, snapshot likelihood,
  chronological holdout invariance, domains/availability, categorical coding,
  support/terms, policy identity, complete recoveries and cancellation.
- The synthetic monthly fixture contains 528 actual-calendar account intervals,
  with 264 training and 264 held-out intervals. It is generated data, not an
  anonymized observed bank panel or predictive-accuracy evidence.
- Four native coordinator/protocol tests pass, including strict nested duplicate
  keys, original entry deadline, cancellation and rejection before fitting.
- The core all-target suite passed 496 tests plus its raw-parser allocation
  harness. The final response-admission refactor subsequently passed the 15
  focused core and four native tests.
- Native all-target/all-feature clippy, formatting, architecture boundaries,
  the root agent contract and regenerated MCP references pass.

Remaining: daily installment/card action projection with explicit fractional-to-
daily clock ownership and a zero-balance card closure owner; independent native
fit/publication verification; matching installed artifacts; full regression;
reviewed policies and observed validation where required by the broader tracker.
No graph, saved-book, journal, capital/FTP, empirical or production acceptance is
conferred. See [the contract](../models/consumer-behaviour-spec.md).
