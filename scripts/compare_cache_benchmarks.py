"""Check financial parity and report measured cache changes on the 60k fixture."""
from pathlib import Path
import json

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'docs/reviews'


def validate_tree(before, after, path=''):
    if isinstance(before, dict):
        assert before.keys() == after.keys(), path
        for key in before:
            validate_tree(before[key], after[key], f'{path}/{key}')
    elif isinstance(before, list):
        assert len(before) == len(after), path
        for i, (left, right) in enumerate(zip(before, after)):
            validate_tree(left, right, f'{path}/{i}')
    elif isinstance(before, (int, float)) and not isinstance(before, bool):
        np.testing.assert_allclose(after, before, rtol=1e-12, atol=1e-5, err_msg=path)
    else:
        assert before == after, path


def main():
    before = json.loads((DIRECTORY/'2026-09-28-balance-sheet-60000.json').read_text(encoding='utf-8'))
    after = json.loads((DIRECTORY/'2026-09-28-balance-sheet-60000-cache-after.json').read_text(encoding='utf-8'))
    ablation = json.loads((DIRECTORY/'2026-09-28-cache-ablation-60000.json').read_text(encoding='utf-8'))
    assert ablation['status'] == 'passed' and ablation['baseline_sources_sha256_verified']
    pair = {r['variant']:r for r in ablation['measurements']}
    assert before['status'] == after['status'] == 'passed'
    assert not after['sources_changed_during_run']
    for field in ('positions', 'paths', 'threads', 'key_rates'):
        assert before['configuration'][field] == after['configuration'][field]
    for field in ('fixture', 'native_identity', 'initial_book_totals', 'initial_kpis',
                  'initial_nii', 'initial_bases', 'initial_objective_$', 'key_rate_totals'):
        validate_tree(before[field], after[field], field)
    rows = []
    def row(label, left, right, unit):
        rows.append(f'| {label} | {left:,.3f} {unit} | {right:,.3f} {unit} | {(1-right/left)*100:+.1f}% |')
    def initial(p):
        return sum(p['stages'][k]['wall_seconds'] for k in
                   ('initialize_three_market_session', 'first_solve_and_independent_replay'))
    row('Three-market initialization and first solve', initial(before), initial(after), 's')
    row('Base pricing, DV01, income and KPIs', before['pricing_calls'][0]['wall_seconds'], after['pricing_calls'][0]['wall_seconds'], 's')
    for label, key in [('Fresh full Python rebuild and solve','full_python_rebuild_and_solve'),
                       ('Full ten-tenor key-rate analytics, base market','full_key_rate_analytics_base_market')]:
        row(label,before['stages'][key]['wall_seconds'],after['stages'][key]['wall_seconds'],'s')
    for label, key in [('Single mortgage edit, median','single_mbs_update'),('Single loan edit, median','single_loans_update'),
                       ('100 loan edits, median','hundred_loan_update'),('Constraint update, median','constraint_update')]:
        row(label,before[key]['median_ms'],after[key]['median_ms'],'ms')
    row('First mortgage edit',before['single_mbs_update']['samples_ms'][0],after['single_mbs_update']['samples_ms'][0],'ms')
    row('Peak resident process memory',before['final_memory']['peak_rss_process_bytes']/2**30,
        after['final_memory']['peak_rss_process_bytes']/2**30,'GiB')
    row('Peak process commit',before['final_memory']['peak_commit_process_bytes']/2**30,
        after['final_memory']['peak_commit_process_bytes']/2**30,'GiB')
    def work(p):
        return sum(node['computed'] for call in p['pricing_calls'] for key,node in call['graph'].items()
                   if key.startswith('cashflows:'))
    assert len(after['pricing_calls']) == 3
    assert work(after) == 660_000  # 3 base legs plus 4 legs for each shifted market.
    assert all(p['cache']['bytes'] <= 512*2**20 for p in after['pricing_calls'])
    text = f'''# Pricing cache improvements: 60,000-instrument validation

Implemented in portfolio-risk 0.21.1. Product models, discounting, fixed-OAS
scenario treatment, common random numbers, input fixture and the configured
512 MiB retained-result budget are unchanged.

## Changes

- Income reuses the request's live base/current-market cashflows even after LRU
  eviction. Different mortgage calibration and sensitivity path counts still
  produce separate cashflows; edited contracts retain their original accounting anchors.
- A bounded protected tier retains shared fits and market/deposit paths. Its
  maximum is the lesser of 64 MiB and one quarter of the existing byte budget,
  with at most 1024 nodes or one quarter of the entry budget. Both tiers share
  the original total limits. Ordinary nodes borrow unused space.
- Completed risk legs and book-local cashflow references are released promptly.
- Identical CD input objects reuse base cashflows even when Polars equality
  returns false for their object-valued call schedules. Persistent keys remain
  based on content, never object identity.

## Measurements

### Back-to-back comparison of the actual old and new implementations

The original cache and pricing source files were reconstructed in benchmark-local
modules and their SHA-256 hashes verified against the original 60k manifest.
No installed engine code was reverted. With the same 60,000-contract base market,
128 paths, four threads and empty 512 MiB caches, measured consecutively after a
small kernel warmup:

| Measure | Original implementation | Improved implementation | Reduction |
|---|---:|---:|---:|
| Base pricing, parallel DV01, NII and KPIs | {pair['before']['wall_seconds']:.3f} s | {pair['after']['wall_seconds']:.3f} s | {(1-pair['after']['wall_seconds']/pair['before']['wall_seconds'])*100:.1f}% |
| Process CPU time | {pair['before']['cpu_seconds']:.3f} s | {pair['after']['cpu_seconds']:.3f} s | {(1-pair['after']['cpu_seconds']/pair['before']['cpu_seconds'])*100:.1f}% |

All instrument prices, OAS, parallel DV01, position NII and aggregate NII passed
parity in this comparison. This is one before/after pair, not a latency distribution.

### Historical full-run comparison

Same workstation, 60,000 distinct core contracts plus 11 auxiliary positions,
128 paths, four pricing threads, three markets and 27 income months. The key-rate
sweep covers ten tenors in the base market. Positive change below means reduced
elapsed time or memory. Full phases have one measurement each; interactive
operations have the repetitions preserved in the raw files.

The original implementation itself now takes longer than the earlier recorded
31.565-second base pass. Thus the historical wall-time columns below are not a
controlled estimate of the cache change's effect. Use the back-to-back comparison
above for that purpose; keep these full-run figures for transparency, validation,
work counts and memory observations. Cache-independent operations, including
same-coefficient solver calls, also vary substantially between the historical
and current runs, so the wall-time drift is not specific to the pricing cache.

| Measurement | Before | After | Reduction |
|---|---:|---:|---:|
{chr(10).join(rows)}

Initial three-market cashflow computations fell from **{work(before):,} to
{work(after):,}** ({(1-work(after)/work(before))*100:.1f}% fewer). The base-market
pass now performs exactly 180,000 instrument cashflow calculations: one spot
and two parallel sensitivity legs per instrument. Income adds no repeated legs.
These work counts are stronger evidence than small timing differences on a
shared workstation. The retained-result budget was not increased.

An earlier after-change timing attempt overlapped an external test suite.
Only the benchmark process was interrupted; the external work was untouched.
That partial run is preserved as `2026-09-28-balance-sheet-60000-cache-contended`
and excluded from this comparison. The reported after-run began after that
test process exited. Neither run is a dedicated-host service-level guarantee.
The subsequent profiling checkpoint is also retained separately. It prompted
the source-verified old/new comparison rather than accepting a presumed speedup.

## Validation

- **103 engine tests passed**, including eight new eviction, budget, shared
  retention, failed-batch, unequal-path, edited-anchor and full-key-rate cases.
- **55 API tests passed** (three pre-existing framework deprecation warnings).
- Before/after book values, NII, full base KPI trees, all three initial scenario
  KPI trees, optimized objective and ten-tenor risk totals pass numerical parity
  at relative tolerance 1e-12 / absolute tolerance 1e-5.
- The fixture fingerprints and native library identity match exactly. Engine
  sources did not change during the completed after-run.
- Both runs independently replay native allocations and compare against a fresh
  full pricing rebuild and SciPy solve. These are synthetic-model checks.

## Scope and remaining work

The cache byte counter measures retained results, not a hard process-memory
ceiling. Temporary arrays, interpreters, native libraries and allocator overhead
remain additional. The protected-tier size is an initial bounded policy, not a
claim that 64 MiB is optimal for every path/scenario configuration. Nonlinear
stress, vega, distributed workers and concurrent-user capacity were not benchmarked.

No product kernel or Rust solver was rewritten. This change applies to existing
Python pricing/what-if requests and the hybrid Rust decision workflow alike.

## Evidence and reproduction

- [Before measurements](2026-09-28-balance-sheet-60000.json)
- [After measurements](2026-09-28-balance-sheet-60000-cache-after.json)
- [After execution log](2026-09-28-balance-sheet-60000-cache-after.log)
- [Engine tests](2026-09-28-cache-engine-tests.log)
- [API tests](2026-09-28-cache-api-tests.log)
- [Source-verified 60k comparison](2026-09-28-cache-ablation-60000.json)

```powershell
uv run --project apps/api python scripts/benchmark_balance_sheet.py --positions 60000 --paths 128 --threads 4 --key-rates --output docs/reviews/2026-09-28-balance-sheet-60000-cache-after.json
uv run --project apps/api python scripts/compare_cache_benchmarks.py
uv run --project apps/api python scripts/ablate_cache.py --positions 60000 --order before,after --output docs/reviews/2026-09-28-cache-ablation-60000.json
```
'''
    destination = DIRECTORY/'2026-09-28-cache-improvements.md'
    destination.write_text(text,encoding='utf-8')
    print('Before/after financial parity and exact work-count checks passed.')
    print('\n'.join(rows))
    print(destination)


if __name__ == '__main__':
    main()
