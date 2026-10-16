# Reviewed earned-fee refunds: local acceptance

The work started from clean `main` at `93ef58a99689ee0dd28ffc10e9bd0ac3dada1e20` after fetching
and fast-forward checking `origin/main`. The earlier commercial implementation is
present on main. This 0.29.18 follow-through completes reviewed immediate refunds
of earned AND collected service cash in the separate fee owner and native loan/fee
journal. **Issue #15 and tracker #25 remain incomplete.**

## Behavior and limits

The optional `earned_refunds_allowed` permission defaults to false. A reviewed
`refund_earned` event increases cumulative refunded earning F and decreases net
service cash K. It reverses fee income and cash, retaining historical gross
earning E. Both remaining earned service and actual collected cash bound the
refund; prepaid cash and waived recoveries cannot grant that right. Remaining
service capacity is unchanged. Negative fees mirror signed postings. Loan
original cost, effective yield, carrying value and sampled credit paths do not
change. Supplied refund reviews do not supply native loan exit decisions.

Each native fee exit policy may explicitly select `refund_earned_fraction` within
[0,1]. It refunds the remaining earned/collected right at that same sealed native
default, prepayment or reviewed maturity clock. Prior supplied refunds reduce the
remaining right. One additional group per fee/path is reserved before generation
when selected. Historical zero/omitted settings retain their original admission
and complete outputs. Protocol 7, ABI 10 and older separately readable schemas
retain their identities; additive fields/actions require compatible artifacts.

Rust owns every production financial calculation. Python independently replays
raw money, ordered shared journal, gross rights, pooled reports and persisted
output. The original source/executable/deadline and final SQL publication fences
remain intact. No numerical tolerance was relaxed and no fast-math was added.

## Accepted evidence

Native tests: **1244** across core
393, product 507, ledger
197 and decision 147.
All four crates pass strict all-target/all-feature Clippy. Decision documentation
compiles. Targeted Rust files pass rustfmt checking and the Git diff has no
whitespace errors.

| Installed gate | Result |
|---|---|
| Fee engine | 96 passed in 11.40s |
| Fee durable API | 24 passed in 15.69s |
| Full engine | 2777 passed, 2 skipped in 600.14s (0:10:00) |
| Full API | 1196 passed, 1 skipped, 1 warning in 1292.96s (0:21:32) |
| MCP | 40 passed in 4.95s |
| Real MCP/API/worker | 2 passed, 1 warning in 7.03s |

The existing reference-context engine skips and optional MCP module skip remain
disclosed; the dedicated real MCP integration executes separately without skips.
Architecture, ten checker tests, generated native/HTTP/MCP contracts and packaged
operator Markdown byte checks pass. Four complete historical native outputs are
byte-identical. Both historical fee results match complete canonical JSON; explicit
zero/false settings preserve complete fee economics and admission. Eight installed
artifact hashes are unchanged before and after full regression.

The source freeze records 1747 files and aggregate SHA256
`6119509b6b9a899a231ae6b2415578d155e3eb69cdd42b1637b1d5f670434da9`. The immutable local receipt is
`artifacts/issue15-earned-refund-validation/receipt.json`; its source/input bodies,
patch, commands and logs are retained in `validated-source.zip` alongside it.
An early run against the not-yet-rebuilt product executable rejected the new
permission field in two tests. That failed preliminary run is not accepted evidence;
the final source-pinned builds and full installed runs above pass.

## Remaining issue gates

Fee Pricing graphs, loan-versus-fee acquisition allocation, units, capital/FTP and
exercise/candidate/holding/draw/secondary integration remain required. Observable
in-arrears earning, generated draws/reviews and bounded streaming/convergence,
seasoned clean/dirty rights and complete saved/cohort/mixed adoption remain open.
Observed issuer/commercial histories, chronological holdouts and reviewed policy
acceptance were not supplied by this implementation. Original dependency gates
and the combined 60,000-position graph/credit/solver/journal acceptance are not
established by these component suites. See
[the complete remaining plan](../models/commercial-completion-plan.md).

No production deployment, service restart, database rewrite, observed predictive
validation or accounting/regulatory approval is claimed. Saved execution requires
matching native artifacts and explicit compatible library/session rebuilding.
