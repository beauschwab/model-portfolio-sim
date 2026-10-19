# Saved hedge book scale baseline (2026-10-18)

Measured evidence toward the 60,000-position combined acceptance that #35 stage 8, #20 and #51
require. This is a baseline for one route, `saved-hedge-mixed-1`, not that acceptance.

## Method

`scripts/make_scaled_hedge_book.py N` replicates the fixture's dated backbook loan N times. It
keeps the saved derivative source and the formal fair-value and cash-flow hedge overlays, and adds
opening equity so the opening stays balanced. Each run goes through the production runtime: the
native `portfolio-model-ledger` process, then independent preparation, ordered-journal and persisted
replay before the manifest is published. Platform: Windows 11, release artifacts built from this
branch.

## Results

| Backbook loans | Native process | End to end with verification | Admitted work | Journal rows | Result |
|---|---|---|---|---|---|
| 1 | | 0.8 s | | 714 | verified |
| 64 | 0.16 s | 1.8 s | 619,956 | 9,282 | verified |
| 128 | | 1.7 s | | 17,986 | verified |
| 512 | 0.61 s | 8.6 s | 679,092 | 70,210 | verified |
| 513 | | 0.1 s | | | refused: 512-position cap |

Native output grows about 55 KB per loan, and admitted work about 132 units per loan. Native
time is a small share; the independent replay dominates the end-to-end time.

## Defect found and fixed

Above about 100 loans the independent reference refused correct journals with "daily
reconciliation". The check compared the residual `assets - liabilities - equity` at a fixed 1e-9
absolute tolerance. That residual is a difference of gross sums in the tens of thousands, so its
rounding scales with those sums. Native and reference round differently in the last bits.

The reference now checks the daily residual, and the summary's maximum residual, at
`1e-9 + 1e-12 * max(|assets|, |liabilities|, |equity|)`. That is the existing 1e-12 relative
tolerance applied to the operands. Every financial amount keeps its fixed tolerances. A 200-loan
regression test covers this.

## Limits between 512 and 60,000 loans

Extrapolating linearly, 60,000 loans would need about 8.5 million work units, produce about
3.3 GB of native output and 8 million journal rows, and take about 17 minutes end to end. That fits
the 30-minute job deadline but is refused by these component limits:

1. Dated backbook positions: 512 (`CompiledDatedLedger` admission in both mixed routes and the
   ledger specification).
2. Joined work: 8 million (`portfolio-ledger-native` derivative owner and both mixed routes).
3. Raw request: 32 MiB at six layers. These are the model-ledger binary's stdin read, strict
   parsing in both routes, the public route constant, the Python runtime and the prepared-request
   header. That header echoes the immutable raw request, which would be about 145 MB at 60,000 loans.
4. Owner capacity: 128 MiB (derivative owner). This is not reached by backbook growth in this
   measurement.
5. Complete native output stream: 32 MiB in the Python runtime (`saved_derivative_runtime`), which
   reads the whole line-delimited JSON stream through the independent verifier. At 512 loans the
   stream is already 29.5 MB, so this route tops out near 550 loans. At 60,000 loans it would be
   about 3.3 GB of JSON lines.

Raising 1 and 2 is local. Raising 3 needs the raw request referenced by hash and streamed rather
than echoed, with the same immutable-identity guarantees. Raising 5 needs a binary or columnar
native stream that the verifier replays incrementally, rather than JSON lines bounded in memory. That transport and publication change is
the scope of #51 (with #46 and #47), so it is not made here. The combined acceptance also needs
graph, credit and solver activity in the same run, which this route does not exercise.
