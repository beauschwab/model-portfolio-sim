# Combined 60,000-position acceptance (2026-10-18)

Issue #51, for the scale gates in #20 and #35 (stage 8) and #25. Each stage ran end to end at
60,000 positions on the production native route, with its independent verification gate and
immutable publication. Platform: Windows 11, Intel Core i7-14700, 32 GB RAM, release artifacts
built from this branch.

## Results

| Stage | Route | Positions | End to end | Rows | Peak RSS (tree or verifier / native) | Gate |
|---|---|---|---|---|---|---|
| Graph, unit library, HiGHS solver, accounting and daily journal | owned workflow (`run_owned_workflow`) | 60,007 | 354 s | 10,497,266 journal; 3,360,392 exposures | 1,275 MB tree / 632 MB native | journal replayed, 364,834 GL keys |
| Credit | `credit-ledger-1`, `acceptance_60000` tier | 60,000 in baseline and stress runs | 1,328 s | 4,920,004 journal; 3,600,000 states; 7,080,000 claims | 4,457 / 2,384 MB | journal replayed, credit source reconciled |
| Saved derivatives and formal hedges | `saved-hedge-mixed-1`, `acceptance_60000` tier | 60,000 | 437 s | 8,160,578 journal | 2,514 / 5,069 MB | independent preparation and persisted journal replay |

Every run finished inside the 30-minute job deadline.

Commands:

```
uv run --project apps/api --with psutil python scripts/benchmark_native_workflow.py --positions 60000 --paths 8 --threads 4 --memory-mib 24000 --output docs/reviews/2026-10-18-native-workflow-60000.json
python scripts/make_scaled_credit_book.py 60000 credit-60000.json --tier
python scripts/make_scaled_hedge_book.py 60000 hedge-60000.json --tier
```

The two generated books run through `run_credit_ledger(..., large_book=True)` and
`saved_derivative_runtime.run` respectively. The graph-stage report is
`2026-10-18-native-workflow-60000.json`, and the hedge stage is recorded in
`2026-10-18-hedge-book-scale-acceptance.md`.

## Changes needed to reach 60,000 credit positions

- **Reconciliation at book scale.** The native daily credit check allowed only 1e-8 absolute or 128 ULP relative. It compares account totals summed in two orders. With identical loans it refused a correct 2,000-position book from rounding alone, well below the route's own admission. The native check and the independent persisted check now also allow the floating summation bound `n × ε × Σ|terms|` for those aggregates. Postings, credit states and claims keep their fixed tolerances.
- **Opt-in `acceptance_60000` tier on `credit-ledger-1`.** It admits 124,000 run-position contracts (60,000 positions in baseline and stress runs), 1,000,000 retained claims per run, and input up to 512 MB through the ledger binary, FFI validator and Python validator. Without the tier the historical bounds are unchanged. The 90-million work cap is unchanged.

## What this is and is not

Each stage is the production route for its owners, at full size, with its own verification
gate. The three stages are separate native processes over books of the same size. They are not one
daily Sim holding credit, derivative and graph-priced products together; no such joint owner
exists in the repository.

The books are synthetic. The graph stage uses the 60,000-contract fixture from
`benchmark_balance_sheet.py`, and the credit and hedge stages replicate one reviewed contract. This
is position-count acceptance, not empirical or regulatory validation.
