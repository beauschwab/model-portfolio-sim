# Saved hedge book: 60,000-position acceptance tier (2026-10-18)

Issue #51, for #20 and #35 stage 8. This follows the baseline in
[2026-10-18-hedge-book-scale-baseline.md](2026-10-18-hedge-book-scale-baseline.md). It records a
measured, independently verified publication of a 60,000-position saved hedge journal.

## The tier

`saved-derivative-mixed-1` and `saved-hedge-mixed-1` accept an optional `scale_tier:
"acceptance_60000"`. Without it every bound, output byte and verification check is unchanged. With
it, these limits apply:

| Limit | Default | Tier |
|---|---|---|
| Dated positions | 512 | 60,000 (the dated ledger's own large tier) |
| Joined work | 8 million | 90 million |
| Raw request | 32 MiB | 512 MiB |
| Native output | one buffered 32 MiB envelope | header, then the shared journal streamed in 1 MiB chunks, up to 16 GiB |
| Retained partitions | 128 MiB | 16 GiB |

**Input identity.** The native process hashes the exact envelope bytes it read from stdin. The
header and source receipt carry that SHA-256 instead of echoing the raw request twice. The Python
runtime checks it against the hash of the bytes it sent, which the manifest's input descriptor also
records. Re-encoded JSON is never hashed, because Rust and Python format some floats differently.

**Streaming.** The daily Sim runs on a scoped thread. It hands bounded chunks through a channel to
the caller's output, with cancellation and deadlines on both threads. A failed write stops the Sim.
If the run fails mid-stream, no completion line is written and no manifest is published.

## Verifier changes found at scale

1. **Quadratic opening rights.** `opening_dated_rights` rebuilt the full plan index and scanned
   every event in the book for each position. It now builds one index per pass, keeping the same
   order and duplicate checks. 2,000 loans dropped from 142 s to 11 s.
2. **Aggregate rounding.** Account-level aggregates (daily gross stocks, reconciliation residual,
   path metrics, event ledger and summary totals) are running sums of up to n journal rows in the
   native order. Their rounding grows with n: the measured difference was about 1e-10 per loan.
   Under the tier only, these checks use the floating-summation bound
   `max(1e-12, n × 1.12e-16)` relative to the summed magnitude (Higham's γ_n). Individual postings,
   instrument balances, source amounts and every default-tier comparison keep the fixed tolerances.
   The bound is scoped to one verification run.

## Results

Windows 11, 32 GB RAM, release artifacts, decision binary
`def33aea9071ea4628d4740b9d8218ab59cadd7d0fcff44a5d4595d5fe9e400b`. Each input comes from
`python scripts/make_scaled_hedge_book.py N OUT --tier`. Each run goes through
`saved_derivative_runtime.run`, which covers native execution, independent preparation and
persisted-journal replay, and publication.

| Loans | End to end | Journal rows | Partitions | Peak RSS, verifier / native | Result |
|---|---|---|---|---|---|
| 600 | 3.4 s | 82,178 | 15 | 306 / 126 MB | verified, published |
| 2,000 | 13.3 s | 272,578 | 22 | 474 / 257 MB | verified, published |
| 5,000 | 34.3 s | 680,578 | 39 | 699 / 520 MB | verified, published |
| 60,000 | 436.8 s | 8,160,578 | 324 | 2,514 / 5,069 MB | verified, published |

The input at 60,000 loans is 145 MB. The run finishes inside the 30-minute job deadline.

## Scope

This is the journal half of the combined gate. It covers the dated backbook, the saved derivative
owner and the formal fair-value and cash-flow hedges in one shared daily GL. The positions are
synthetic copies of one reviewed loan; they are not an observed bank tape.

Three things are not exercised: graph pricing at 60,000 positions, a credit owner, and the HiGHS
solver over a 60,000-position unit library. #51 keeps those, and the Rust service and verifier work
in #46 and #47.
