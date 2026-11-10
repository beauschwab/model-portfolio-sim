# Supplied consumer macro scenarios and daily replay

These Rust components implement the supplied-conditional-path alternative in
#92. The standalone native protocol accepts both schemas through transport-only
`analyze_consumer_macro` and `project_consumer_macro_products` entrypoints. They
are not ModelLab publication routes and do not complete the joined NII/EVE/loss
acceptance gate.

`consumer-macro-scenarios-1` accepts Physical history, declared sources and four
supplied scenarios: baseline, parallel +200bp, parallel -200bp and severely
adverse. Every scenario retains the same ordered path keys and interval grid.
The short-rate coordinate must match the supplied conditional rate parent bit
for bit. Parallel rate shifts are exactly two percentage points. Macro responses
are supplied explicitly; no stress response, Gaussian dynamics, LMM calibration
or Physical-to-Pricing conversion is inferred.

## History and fitting

The driver vocabulary covers short rates, national/regional unemployment levels
and 12-month changes, used-vehicle index levels and changes, prime-policy and
card APR spreads, consumer rate changes, real disposable income growth and HPI
changes. Region, units, source/revision, raw domain, publication lag and frozen
innovation scale are required. Index levels must be positive. Rate levels and
growth rates use percent; spreads and rate changes use percentage points.

Each observed interval supplies complete contiguous start/end levels and release
dates. A driver vector is available only when every coordinate's publication and
declared lag permit it. Cross-split intervals and releases unavailable at their
split cutoff are explicitly excluded. At least two available intervals in each
chronological split are required. The training mean and unbiased sample covariance
are computed from `(end-start)/(scale*sqrt(ACT/365 years))`; this is a declared
innovation diagnostic, not an estimated transition law. Holdout moments and mean
errors are separate. The training identity includes only available training rows,
frozen definitions and training policy; changing holdout observations changes the
raw-source hash but not fitted training moments or identity.

The unreleased macro moment reducer keeps observation order and uses compensated
summation for means and signed covariance products. Large opposing products must
retain small residual covariance; the independent `math.fsum` comparison bound
is unchanged. Existing monetary reducers and released product calculations are
unaffected.

## Projection and daily ownership

Mapping to `consumer-behaviour-spec-1` requires exact coverage of selected macro
columns, matching metric/unit/lag, available borrower values and the same origin,
interval grid and actual raw credit draws. Borrower values remain unchanged.
Supplied projections retain their issue date and projected-data tag. Out-of-domain
values or hazards reject; there is no clipping. The raw macro source is rehashed
before mapping. A raw index level cannot masquerade as an index change.

`consumer-macro-products-1` binds all four scenarios to up to three explicit
single-path daily owners. Each uses the existing fitted-state adapter and its
single installment/card monetary owner. No loan balance, commitment, recovery,
interest or loss is posted by the shared CTMC. Complete daily cashflows and
recovery tails, fractional event clocks, collision refusals and card zero-balance
closure rules remain intact. The same immutable training request is fitted for
each case, with at most twelve existing bounded fits; coefficients never depend
on scenario projections. The original outer callback/deadline is passed through
every fit, credit-path and daily-product loop.

Before generating any daily case, all templates and exact compiled requests are
admitted. Aggregate prospective rows are capped at 100,000 and retained response
reservation at 96 MiB; raw input is capped at 8 MiB. Macro analysis separately
caps raw input at 4 MiB, 16 drivers, 4,096 history rows, 16 paths, 64 projection
intervals and four scenarios. These are work/serialization bounds, not an RSS or
production scale guarantee. Failure returns no partial result.

These two new macro routes count UTF-8 source bytes in both the process adapter
and independent verifier. Optional ASCII escaping does not consume the raw-source
budget. Historical routes retain their transport encoding; the public canonical
evidence hash keeps its existing encoding, and the executed-input hash binds the
actual UTF-8 bytes delivered to the native process.

## Evidence and remaining scope

The unreleased macro contract uses the explicit result `identity_policy`
`consumer-macro-typed-binary64-1`. The digest begins with that UTF-8 policy name,
then encodes values with tags: null `n`, booleans `t/f`, strings `s`, binary64 `d`,
signed integers `i`, unsigned integers above i64 max `u`, arrays `a` and objects
`o`. Lengths/counts and integer/float bits are eight-byte little-endian values;
strings are UTF-8; object keys are sorted and encoded as strings. Array order,
signed zero, integer precision and source coordinates remain exact. This avoids
using language-specific JSON float spelling as a cross-language identity. The
native hash walk retains the original callback. Existing released schemas and
their hashes are unchanged; matching rebuilt macro artifacts are required.

`consumer_macro_reference.verify` independently binds the complete typed raw
source and training-only identity, checks source/scenario domains and release
chronology, and reconstructs moments using Python `math.fsum`. Its statistical
comparison bound is `1e-12 + 1e-12 * abs(expected)`; monetary tolerances are
unchanged. Native fixture variants cover signed zero, integer path keys above
2^63 and Unicode identities. Corrupted source hashes, covariance, holdout means,
row order, exclusions and permission flags reject. This explicit verifier is
not an execution backend. Its receipt covers macro source/innovation diagnostics
only, not the fitted credit hazards, daily product cashflows or publication.

Synthetic controls independently check training covariance entries 4/3, 8/3 and
16/3, holdout isolation, late releases, actual draw mismatch, unknown units/lags,
grid and retained-source mutation, and cancellation. Daily controls replay all
three products under all four scenarios; zero macro coefficients preserve the
complete existing financial result exactly. A fitted nonzero macro coefficient
changes daily default timing while the fitted model and raw draws remain equal.

The protocol requires an original execution deadline, rejects duplicate keys
before calculation, and retains the caller control through final serialization.
The daily response has an explicit 96 MiB bound; historical controlled schemas
retain their 32 MiB bound. The Python process adapter preserves raw input hashes,
executable identity checks and watchdog cancellation without financial callbacks.
The process deadline starts at entry and includes JSON preparation, executable
hashing, input-file writing and final canonical evidence hashing; each boundary
is checked before process creation or returning a result.

Remaining #92 work includes complete independent fitted-hazard/daily-source
verification and a joined analysis with separately supplied Pricing
parents and retained original OAS, plus frozen-EIR income accounting. Gross
contract interest is not NII. Physical conditional forecasts must never drive
OAS or EVE. Saved books, durable publication, capital/FTP, deployment and observed
predictive acceptance remain separate. All three permission flags are false:
`adoption_eligible`, `empirical_accuracy_validated`, `pricing_measure_permission`.
