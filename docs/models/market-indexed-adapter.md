# Supplied curves and conditional indexed-product analysis

`market-indexed-adapter-1` is an explicit analytical source. Rust converts supplied
named index and collateral curves into conditional term-index observations, then
calls the existing `term_enhancements::indexed::build_path` and
`derivative_contract::swaps::value_swap`. These are actual product owners; the
transport does not calculate cashflows or discount factors. This schema does not
enter the saved-book incremental pricing graph.

The outer lifecycle request contains `schema`, `threads`, and the typed `request`.
The canonical example is `examples/models/market-indexed-adapter-1.json`, whose
`request` member is the typed raw request. The adapter retains that complete request
as `cache_parent`, including both complete original/current supplied markets, all
source/revision/currency/date fields, named roles, curve nodes/tails, contract/reset
dates, margin, index tenor, calendar and review identities. A result from this source
is unsuitable for a coefficient library, accounting publication, capital or FTP.

For actual-date year fraction `tau`, the simple projected index is
`F = expm1(log(P_index(start)) - log(P_index(end))) / tau`.
Supplied pseudo-discount factors use the existing native log-linear interpolation
and explicit `reject`, `flat_zero`, or `flat_forward` tail. Curve time is ACT/365
from as-of; contractual fixed and indexed accrual may independently use ACT/360 or
ACT/365. A parallel index shift changes the index factors by `exp(-shift*time)`.
Collateral discounting and its bumps use the separately named collateral curve.
Both may include negative rates within admitted coupon/factor domains.

The clean swap is constant notional, with no principal exchange. Payer fixed cash
is `N*(F+margin)*tau_index - N*fixed_rate*tau_fixed`; receiver signs reverse.
`swap_result.clean_value` uses pure collateral discounting. The separately labeled
`spread_adjusted_swap_pv` applies the retained original reviewed OAS to those actual
dated cashflows. This OAS is supplied, never solved or refitted on current/bumped
curves. The indexed illustration returns principal at maturity; its principal cash
is deliberately separate from the swap and must not be added to swap value.

Eight deterministic cases retain complete product DTOs, native cash/result receipts
and source identities: original, current, and down/up collateral, index and parallel
bumps. Their contract/calendar/reset/OAS parents are identical. There are no random
draws; this deterministic paired case is not a proof of stochastic shared CRN or
joint market dynamics. The original/current comparison is not quote calibration.

Only same-as-of Pricing sources are admitted. Every source must be available before
its declared information time, and information must be at/before each exact reset
cutoff on the supplied reset calendar date. Past resets, missing named index/role,
currency/as-of mismatch, missing complete maturity tail, unsupported coupon factors
and unadmitted curve tails fail before product generation. Generated observations
are explicitly `conditional_projection`, never observed fixings. Calendar identities
and caller dates are retained; business-day mapping is supplied and unverified.
Availability is explicitly supplied Unix seconds and may precede 1970, down to
the beginning of calendar year 1. Information and actual product selection cutoffs
retain their existing 1970..9999 UTC domains; availability cannot follow information.
Publication/observation lookup, overnight reset conventions, actual observed fixes,
joint drift/numeraire dynamics, smile, exercise and behavioral/credit paths remain
separate work. See [floating-rate.md](floating-rate.md),
[term-enhancements.md](term-enhancements.md), and
[derivative-contracts.md](derivative-contracts.md) for the product foundations.

Preflight admits at most 1 MiB serialized typed raw adapter input, 512 periods,
16 curves per original/current market, and 4096 nodes per curve. Let `I` be the
adapter serialized byte length, `B` the bridge byte length, `M` the total original
and current curve node count, and `R=8*periods`. Adapter work is `4*M+320*R`, capped
at 2 million; retained/output capacity is `8*R*(I+16384)+4*I+65536`, capped at 32 MiB.
The nested bridge additionally admits `4*M+64*R` work and
`R*(B+4096)+B+16384` bytes capped at 8 MiB. These conservative repeated-metadata
bounds use actual serde escaped byte lengths and checked arithmetic before output
vectors. Parsing, transport, source hashing and separate caches are additional
bounded stages; these figures are not an aggregate process RSS claim.

The new curve boundary accepts tail enum strings only. It rejects singleton-object
aliases, extra fields and duplicate typed fields in direct lifecycle JSON bytes.
An earlier caller parsing JSON into a dictionary/`Value` loses duplicate occurrences;
the native boundary cannot reconstruct that lost information. Historical shared
curve parsers, saved schemas and their economics are unchanged.

`market_indexed_reference.verify` independently reconstructs raw named-curve
projections, generated DTOs, complete native product cash/PV and retained source,
review, clock, admission and flag fields. It uses unchanged final tolerances of
`1e-12` relative and `1e-9` absolute for financial numbers, exact nonfinancial
identity/date/policy comparison, and stable log-ratio/expm1 for near-zero forwards.
Python is an independent verifier only. Durable ModelJobs require this verification
before formatting and again after immutable Parquet reload, with original deadlines,
lease/cancellation checks and queued/executed/current source/executable fences before
publishing a manifest. Retained raw/source/OAS parents and descriptor source fields
use exact recursive typed identity, including float signed zero; financial tolerance applies only to calculated
outputs. Neither identity alone nor producer results certify economics.

When a Rust caller supplies an active compute Control, this new analytical branch
captures it before Rayon dispatch and scopes it on the worker through calculation
and result conversion. Legacy branches retain their existing behavior. The standalone
`portfolio-lifecycle` CLI does not create a Control from this envelope; durable CLI
deadline/cancellation is enforced by the process watchdog and publication checks.
This patch introduces no broad envelope or process-control ownership change.

Result flags `production_adoption`, `observed_fixings`, `physical_accounting`,
`joint_dynamics`, `calendar_mapping_verified`, `accounting_adoption`,
`capital_ftp_eligible`, and `empirical_validation` are all false. Source verification
does not certify observed data, an accounting standard, a persisted journal, an
incremental graph cache, held-out calibration, or 60,000-position acceptance.

The source changes are applied locally and require a matching native rebuild.
Isolated copied-target/native and independent-reference tests are distinct from the still-required installed default
lifecycle, full regression, durable API, Parquet-tamper and browser execution gates.
