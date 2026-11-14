# SVB reconstruction tie-out recheck

**Result: the reconstruction reproduces its 2026-10-01 record exactly on the current engine. The three residuals it reported are unchanged, because nothing that would close them has been added to the reconstruction.**

This is a rerun of the existing aggregate experiment, not new validation. It adds no model, fits nothing and changes no reference fact.

## What was rerun

`scripts/reconstruct_svb.py` on engine 0.29.20 (Linux x86-64, freshly built native runtimes), against the committed `examples/svb-2022/reference.json` (same SHA-256 as the original record), written to an ignored folder so the dated record is untouched:

```bash
make build-native
uv run --project apps/api python scripts/reconstruct_svb.py --output artifacts/svb-reconstruction/rerun/svb-reconstruction.json
```

| Check | Original record (0.29.5, Windows) | Rerun (0.29.20, Linux) |
|---|---|---|
| Explicit reconstruction checks | 18 of 18 | 18 of 18 |
| Numeric results compared with the committed JSON | | 214 of 214 identical, to the last bit |
| Journals independently replayed | 10 | 10 |
| `tests/test_ledger_svb_policies.py` | | 20 passed |

The only difference from the committed JSON is the optional `downloaded_source_sha256`, which is recorded only when the downloaded 10-K is present locally. Run metadata (timestamps, platform, artifact paths, engine version) is excluded from the comparison.

## What ties and what does not

Unchanged from the [original comparison](2026-10-01-svb-reconstruction.md); amounts in USD billions.

| Item | Model | Disclosed or observed | Residual | Status |
|---|---:|---:|---:|---|
| Opening assets, liabilities, equity, cash | 211.793 / 195.498 / 16.295 / 13.803 | same | 0 | Ties (input check) |
| CET1, Tier 1, total capital, TCE/TA | 12.0542% / 15.4047% / 16.1756% / 5.6219% | 12.05% / 15.40% / 16.18% / 5.62% | under 0.5 bp | Ties to disclosed rounding (supplied capital and RWA) |
| March 9 cash after the $21bn sale and $42bn withdrawals | -7.197 | about -0.958 | -6.239 | Does not tie |
| After-tax sale loss at 26–28% | 1.469–1.510 | 1.800 | -0.290 to -0.331 | Approximate |
| AFS OCI against reported AOCI | -2.533 (gross) | -1.911 (total) | -0.622 | Not bridged |
| First-day outflow from the behavioural model | 0.75–63.9 across severities | 42 | not applicable | Not predictive; severity is an input |

## Open gaps

The residuals map to the issues that are still open:

| Residual | Issue |
|---|---|
| March cash: December consolidated opening, no January–March roll-forward | #26 |
| March cash: intraday requests, collateral and facility settlement | #27 |
| Run speed: calibrated confidence-driven deposit runs | #28 |
| Sale loss: security-level repricing and the actual sold lots | #29 |
| AOCI and CET1: deferred-tax and loss-realization bridge | #30 |

Three follow-ups have since been implemented and closed in the engine: HTM sale and reclassification consequences (#31), legal-entity transfer restrictions (#32) and source-backed LCR/NSFR applicability (#33). **The reconstruction script does not yet use any of them.** Its specifications set no HTM policy, model one consolidated entity and state that LCR and NSFR are not reproduced. Wiring them in would change what the experiment claims and needs reviewed SVB-specific inputs (sale reasons, entity structure, the Fed's retrospective LCR components), so it is left as a separate decision.

## Limits of this recheck

Bit-identical reproduction shows the daily ledger, capital arithmetic and journal replay used here did not change behaviour between 0.29.5 and 0.29.20 for these ten specifications. It says nothing new about historical accuracy, run prediction or regulatory acceptance.
