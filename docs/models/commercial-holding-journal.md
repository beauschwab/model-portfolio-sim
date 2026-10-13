# Complete-origin commercial holding journals

Local 0.29.14 adds explicitly selected `commercial-holding-journal-1`.
`analytics.model_contracts.journal_commercial_holding` transports raw requests to
`portfolio-strategy`; `/models/runs` executes the same native contract as a durable
job. Python independently verifies raw economics, journal postings and persisted
tables. The existing monthly and commercial ledger schemas remain unchanged.

The request supplies `origin_ledger`, `revised_sources` and `cut_ordinal`.
The complete original ledger request supplies accounts, allocations, actual-run
path weights and complete acquisition sources. Each revised source pairs with
the corresponding original source through `commercial-holding-continuation-1`.
Both raw and financial completed prefixes must agree; original quotes, yields,
gross rights, draw anchors, reviewed credit state and credit streams remain retained.
Future changes require the existing complete-origin source conventions. No money
checkpoint or prior-publication identifier is accepted by this contract.

Rust transfers the sealed revised owners directly into one existing daily Sim.
The credit clock runs from original acquisition. The Sim executes actual opening,
acquisition, ordered recognition and settlement through the complete contractual
and recovery tail. It neither truncates that tail at the report date nor inserts
a terminal payoff. The result retains the complete source receipt, complete
ordinary daily reports and protocol-6 journal for independent verification.

The prospective journal is derived from that full native journal. Let B(g,i)
be the ordered sum of postings through the cut for GL account g and instrument i.
Non-P&L stocks retain B(g,i). Prior signed P&L is reduced in stable GL/instrument
order and added to the account's opening equity. One balanced opening transaction
per nonempty account is posted on the cut date. The window then contains only
original transactions strictly after the cut. Their values, dates, instrument,
account, event and order are retained; `origin_transaction_number` records their
full-journal lineage. Future transaction numbers are local to the window.

`window_tables` contains `journal`, `opening`, `retained_prior_pnl`,
`trial_balance`, `commercial_daily` and `commercial_closing`. The daily/closing
tables retain native gross legal and recognized rights and reviewed credit state
from the cut onward. Daily rows on the cut describe the opening state; journal
rows on that day are only the derived opening. Prior acquisition and earnings
are not reposted. Every non-P&L/non-opening-equity closing stock must reproduce
the complete-origin native GL with identical f64 bits. Independent verification
recomputes the full raw source and full daily journal before deriving the window,
then checks opening, ordered future transactions and published closing balances.
Existing final-output tolerances remain unchanged.

The example retains a day-15 unpaid coupon right on a synthetic100 loan with
5% original EIR. The stress source changes only its future default hazard. The
same original waiting draw survives the cut; original cost and yield remain
unchanged. Other controls cover stopped recognition, signed rights after principal
settlement, reviewed default/chargeoff/waiver/recovery, zero/half allocation and
a complete funded draw with its original tranche yield. They are numerical
controls, not observed predictive validation.

The joined preflight charges complete prefix owners/comparisons, sealed openings,
one full daily Sim, raw transport and retained full/window buffers before any
generator. It uses the existing joined32M-work/128-MiB tier. Inner continuation
and ordinary all-run ledger8M/128-MiB caps and per-source daily8M/32-MiB caps
remain unchanged. Raw input is bounded to2MiB and result encoding to32MiB.
A complete12-day zero-hazard draw control uses an explicit one-jump guard;
the larger draw control rejects at the unchanged ordinary cap. No reservation
is reduced to admit an unsupported contract. Component capacity bounds do not
constitute aggregate process RSS guarantees.

The original native deadline covers decoding, all sources, daily replay, window
derivation, bounded encoding and output writes. Durable jobs retain the original
queue deadline through independent raw and persisted verification, lossless
Parquet cells, source/artifact/executable hashes and final lease/token/revision/
attempt publication fences. Rehashed opening, prefix, prior-P&L, journal-order,
closing and adoption-evidence mutations reject publication.

`complete_origin_daily_sim_adoption` is true. `prior_publication_verified`,
`cut_started_mixed_sim_adoption`, `secondary_acquisition_adoption` and
`empirical_validation` remain false. This derives a window from one complete
origin run; it does not run a new mixed Sim starting at a previously published
opening, prove external bank reconciliation, prepare seasoned strategy/cohort
coefficients or allocate secondary conveyed rights. Those gates, nonintegral
fees, daily in-arrears indexed earning, generated draws/reviews and streaming,
observed histories/reviewed policies and complete60000-position mixed graph/
credit/solver/journal acceptance remain required under issues15/25.

Rebuild matching product, ledger and decision/workflow artifacts and restart
deployed API/workers before additive execution. Existing saved economics require
explicit selected-session/library rebuilding. Local validation does not deploy
artifacts, restart workers or close full issue15/25.

Matching official local builds pass1,220 native tests and strict checks,210
commercial engine and135 commercial API checks,2,598 full engine checks with two
existing skips and1,100 full API checks. Contract, source/artifact, independent
hand-control and four historical whole-output identity gates pass. See
[the accepted local review](../reviews/2026-10-12-commercial-holding-journal.md)
for exact source, artifact, log and snapshot identities.
