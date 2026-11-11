# Complete-origin acquired commercial loan and fee holdings

`commercial-candidate-holding-journal-1` selects continuation of existing finite
native commercial candidates, including separately acquired service-fee rights.
It uses the same raw graph, acquired coefficients and candidate daily Sim as
`commercial-candidate-journal-1`. Its exact request fields are:

```text
schema: commercial-candidate-holding-journal-1
cut_ordinal: actual calendar date ordinal
completed_candidate: complete original commercial-candidate-journal-1 request
revised_candidate: complete revised commercial-candidate-journal-1 request
```

The raw example is
[`commercial-candidate-holding-journal-1.json`](../../examples/models/commercial-candidate-holding-journal-1.json).
Its current fee collection moves from day 40 to day 42, and its final acquired
fee cost release moves from day 40 to day 44, after a day 15 cut. Original Pricing
fee cash, allocation, quote and loan yield/OAS remain identical. Its one-jump
bound and zero hazards are explicit synthetic inputs, not calibrated behavior.

## Original authority and observed history

The two requests retain the exact same units, allocations, mapping, calendar,
contract identities, run identities, collateral curves, opening authority and
supplied accounting reviews. Every original graph Pricing and Physical proof,
original loan quote/cost/effective yield/OAS, separate fee quote/spread/cost
allocation and original fee history is immutable. Current scenario/path coverage
and path weights remain identical. This route cannot introduce an acquisition,
replace an original contract, add a fee or change its classification.

For each current loan history, the existing native complete-origin continuation
rules check original source and acquisition authority, already begun periods,
available observations, original credit stream, intervals already begun and
their accounting policies. Only the permitted complete future assumptions can
change. A source whose purchase has not occurred by the cut remains completely
unchanged in this tier; its existing future acquisition is still replayed once.

For each current fee history, contracts, entitlement, review identities, exit
disposition policies, complete tail and coincident timing policy remain
unchanged. Service/cash/refund events and reviewed acquired-cost releases on or
before the cut must be exactly identical, including order and review/source
metadata. Only events/releases after the cut may be revised. Both full sources
must still satisfy complete rights and acquired-cost conservation.

These raw checks precede source generation. Both candidates then run through
their original native graph/conditional stopping/acquisition and daily Sim.
Every dated source/group, ordered posting, daily snapshot and dated report
through the cut must be exactly equal between those runs. Matching only final
cut balances is insufficient. Zero allocations retain and compare the complete
unweighted loan and fee snapshots. In particular, revised future economics
cannot retroactively change an earlier exercise decision or its actual cash.

## Opening and future journal

The native window owner derives its opening from the verified full journal
through the cut. Loan and fee receivables/payables, deferred/prepaid stocks,
separate acquired fee assets/liabilities and other non-P&L balances retain
their signed gross ownership. Prior P&L is reported separately and rolled into
opening equity. The window contains no second opening acquisition.

Only complete-origin source postings after the cut enter the prospective
journal, preserving source event clocks, order and original transaction
lineage. Remaining fee cash, refunds, recoveries and acquired-cost releases can
extend beyond both loan payoff and the requested report date. No terminal
settlement is invented. Complete fee daily/closing snapshots accompany the
loan snapshots. Every non-P&L final stock must reproduce the complete native
journal's final stock bits.

The service-fee and acquired-cost release policy remains the explicit supplied
policy described in [commercial-fee-adoption.md](commercial-fee-adoption.md).
This route does not infer that purchased fees are integral loan interest or
approve an accounting classification/release method. Loan NII and the existing
external-NII strategy objective retain their original meanings.

## Admission and independent publication

The envelope is bounded to 2 MiB and the combined result to 32 MiB. Each materialized
candidate receipt and the window have a stricter 8 MiB component tier. Native
candidate stream writes enforce that smaller tier before decoding; structured
child receipts and the window are sized before constructing the final result
Value. The original exact raw request is rechecked against those same bounds.
The ordinary finite candidate route retains its existing 32 MiB output cap.

Exact native candidate admission runs for both requests before either graph or
Sim is generated. The unchanged combined 32-million-work/128 MiB retention gate
charges both full financial preparations, raw-prefix validation, both retained
decoded results, window output and their scans. The existing ledger 8M/32 MiB
component bounds still apply. The final bounded encoder/transport is a separate
component, and these capacities do not promise an aggregate process RSS cap.
Larger individual finite candidates can be valid in their original route and
still be explicitly refused by the stricter dual holding tier.

`commercial_candidate_holding_reference.verify` independently replays both raw
graph/unit acquisitions and complete candidate journals, checks original/raw
prefix identity and exact ordered actual prefixes, and reconstructs the cut
opening, prior P&L, future journal and closing gross balances. The ordinary
model-analysis publication path persists and restores these tables before the
same independent replay. Raw input, source/executable identity, original
deadline, immutable artifacts and final cancellation/lease/attempt fences
remain publication requirements.

This is complete-origin prospective replay. It does not establish a verified
prior-publication opening, a Sim started at the cut, arbitrary mixed backbooks,
secondary conveyed fee rights, drawn-tranche fees, generated fee exercise or
observed predictive accuracy. Those boundaries remain explicit false flags;
full issue #15/#25 evidence and scale gates remain required. Deployment requires
matching rebuilt native artifacts and restarted workers; retained libraries
and sessions need explicit compatible rebuilds.

The separate API [source-ID route](commercial-candidate-prior-opening.md) can
establish modeled prior-publication evidence at the exact already-published cut.
The native route above still reports its original false provenance flags; the
API retains independently verified source/object evidence separately. Observed
bank reconciliation and cut-started mixed adoption remain required.
