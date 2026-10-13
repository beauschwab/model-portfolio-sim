# Native supplied product graph cohort audit

`graph-cohort-audit-1` compares an explicitly supplied representative with an
ordered set of explicitly supplied individual members. Rust invokes the existing
consumer installment, revolving card, mortgage and deposit native product graphs.
It never derives a representative state by averaging member contracts, calibrates
OAS, fits behavioral models, or authorizes cohort publication. A passing diagnostic
demonstrates the requested numerical comparison under the supplied histories;
synthetic histories do not demonstrate observed predictive accuracy.

The complete synthetic example is
[`examples/models/graph-cohort-audit-1.json`](../../examples/models/graph-cohort-audit-1.json).
It compares a two-unit representative deposit with two one-unit members, each with
1000 principal, a supplied .365 annual paid rate and a two-day maturity. The resulting
representative and member-sum liability PVs are 2004, bank cash PVs are -2004, and
full physical earned interest is 4 currency units. The one-day reporting period
contains 2 of that interest; the remaining 2 and the maturity settlement remain in
the full tail. This is an intentionally synthetic mechanics fixture.

## Contract and ownership

`Request` contains mandatory `schema`, `source_revision`, `monetary_unit` and
`cohorts`. `monetary_unit` must equal `currency_units`: amounts are in the native
graph's explicit currency, not implicitly USD or a scaled thousands convention.
Each cohort contains `cohort_id`, one `representative`, ordered `members`, and
`tolerances`. A row has `row_id`, a positive `amount_multiplier`, and a tagged
`graph` with `kind` and the entire raw native `request`. Kinds are
`consumer_installment`, `card`, `mortgage`, and `deposit`. All fields use strict
versioned serde contracts; unknown fields are rejected.

Each row identifies exactly one native position and requires `oas_target.kind`
`locked`, with its original supplied OAS and provenance. Members may have different
locked original OAS values; no representative OAS is imposed on them. The audit
compares amounts, not OAS labels or an average of member OAS. Core monetary/state
quantities remain owned by the selected native product graph. The multiplier
scales only the final signed reported amounts; it does not alter the contract's
state, default intensity, migration mechanics, rate or path probability. Member
amounts are summed in their supplied order without normalization.

Within a cohort the product, currency, as-of date, reporting endpoints, base
scenario, curve/bump contexts, ordered scenarios and applicable terminal/cash
policies must match exactly. The model and paired-history source/revision/randomness
domain must match, as must the ordered path IDs and weights in every scenario and
physical/pricing/down/up role. Consumer subtype and mortgage agency/whole-loan
ownership kind must match. Contracts and supplied behavioral histories can differ
between rows: comparing their effect is the purpose of the diagnostic. Each
component's native admission additionally verifies physical versus pricing measure,
calendar/currency/run ownership, complete source obligations and paired bumps.
The audit does not infer physical hazards from pricing histories or transform P to Q.

## Metrics, signs and tolerances

All native monetary metrics are compared, including PV, the supplied-history
finite-difference DV01 and every reported physical-period/tail field. Names are
deterministic JSON field paths, with zero-based array ordinals, for example
`physical_periods/0/interest_expense` or
`gross_physical_contract_interest/0`. ID fields are retained in source identity
and are excluded from monetary reductions. Every metric label must have exactly
one tolerance; missing, unknown or duplicate labels fail. A single label policy
applies to that metric across the explicitly matched scenarios.

The audit additionally sums every physical-source cash and recognition event,
including opening-date cash, beyond-reporting tails and recoveries after principal
has ended. These metrics are `source_full_cash/principal`,
`source_full_cash/interest`, `source_full_cash/fees`, and
`source_full_gross_interest`. Consumer/card outputs also report
`source_full_default/principal` and `source_full_default/principal_loss`.
These supplementary metrics retain complete physical source coverage where the
component's ordinary period reports end before the source tail. They do not create
extra cashflows or an additional economic loss charge. Mortgage-specific default,
agency advance/buyout and recovery metrics are retained from its complete native
reports. Deposit principal/interest are bank-perspective signed cash; liability PV
and bank cash PV retain their opposite signs.

For representative amount `R` and original-order sum of weighted members `M`,
the signed error is `R-M`, the absolute error is `abs(R-M)`, and a metric passes
when `abs(R-M) <= absolute_amount + relative*abs(M)`. `relative_error` is
`abs(R-M)/abs(M)` when `M != 0` and null at zero; a nonfinite ratio fails admission.
The tolerance is absolute-plus-relative, not an implicit OR between separate
tests. Negative coupons, signed bank flows and zero member sums keep their signs;
no absolute value is applied to source amounts. Tolerances must be supplied,
finite, nonnegative; relative tolerance is at most 1. Absolute amounts and all
reduced monetary quantities obey the shared 1e15 currency-unit bound.

## Bounds, work and retained evidence

At most 4096 total rows, 256 rows including the representative in a cohort, 10000
tolerance labels per cohort, 32 MiB serialized raw input, and 32 million admitted
work units are accepted. All component domain validators and admitted-cost helpers
run before the first financial generation. Aggregate work charges serialized
input bytes, each component's native generation/reporting cost, and conservative
scenario/tolerance reductions. Component ceilings alone do not imply aggregate
acceptance. A fixture uses structurally admitted but dynamically invalid behavior
and excessive aggregate work to prove the aggregate gate precedes simulation.

Complete native results, raw sources, immutable locked OAS anchors, scaled row
amounts, cohort errors and stable row order remain in `ResultData`. Retained JSON
size is checked incrementally and at completion with a conservative factor of four
within a 128 MiB limit. These are admission budgets, not measured process-wide RSS
limits. Existing cooperative cancellation/deadline checkpoints apply during input
serialization, preflight, rows, native graph work and output serialization. There
are no additional dependencies, normalized survival factors, reseeding operations
or Python financial callbacks.

The result always declares `diagnostic_only=true`, `empirical_validation=false`
and `cohort_publication_allowed=false`, even when every metric passes. Durable
publication, independent persisted comparison, selection/rebuilding of cohorts,
observed predictive validation and 60000-position aggregate scale acceptance are
separate integration gates. This module changes neither legacy product selection
nor existing cohort economics.

## Validation

Dedicated native tests cover independent two-day cash/accrual/PV/bump hand math,
heterogeneous pricing histories with unchanged physical histories, negative
coupons, original per-member OAS retention, exact tolerance and pairing identities,
aggregate admission before generation, cancellation, deterministic replay and all
four native product dispatches. A separate default hand fixture reconciles 2000
defaulted principal, 1100 loss and 900 recovery cash after the reporting endpoint;
a EUR fixture verifies that currency units are not relabeled USD. Product pricing
equations and their independent
validation fixtures remain in the respective product graph/model modules.
