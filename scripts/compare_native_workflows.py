"""Compare retained financial outputs from two complete workflow benchmarks.

Each benchmark independently replays its persisted journal before publishing this
compact evidence. This comparison does not retain or compare every journal row.
"""
import argparse
import json
import math
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('optimized', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    before, after = (json.loads(p.read_text(encoding='utf-8')) for p in (args.baseline, args.optimized))
    ignored = {'timings_ms', 'solver', 'work', 'token', 'cashflow_capacity_bytes'}
    def financial(value):
        if isinstance(value, dict):
            return {k: financial(v) for k, v in value.items() if k not in ignored}
        if isinstance(value, list):
            return [financial(v) for v in value]
        return value
    count, largest = 0, 0.
    def compare(a, b, path):
        nonlocal count, largest
        if isinstance(a, bool) or a is None or isinstance(a, str):
            assert a == b, (path, a, b)
        elif isinstance(a, (int, float)):
            assert isinstance(b, (int, float)) and not isinstance(b, bool), path
            assert math.isfinite(a) and math.isfinite(b), path
            difference = abs(a-b)
            assert difference <= 1e-5 + 1e-7*abs(a), (path, a, b)
            count += 1
            largest = max(largest, difference)
        elif isinstance(a, list):
            assert isinstance(b, list) and len(a) == len(b), path
            for i, (x, y) in enumerate(zip(a, b)):
                compare(x, y, f'{path}[{i}]')
        else:
            assert isinstance(a, dict) and isinstance(b, dict) and a.keys() == b.keys(), path
            for key in a:
                compare(a[key], b[key], f'{path}.{key}')
    for key in ('positions', 'paths', 'horizon', 'threads'):
        assert before['configuration'][key] == after['configuration'][key], key
    assert before['rows'] == after['rows']
    assert before['validation']['journal_replayed'] and after['validation']['journal_replayed']
    for key in ('workflow', 'summary', 'validation'):
        compare(financial(before[key]), financial(after[key]), key)
    report = dict(scope=__doc__.strip(), baseline=str(args.baseline), optimized=str(args.optimized),
        numeric_count=count, max_absolute_difference=largest, rtol=1e-7, atol=1e-5,
        parity=True, identical_row_counts=True, excluded_implementation_fields=sorted(ignored),
        speedup=before['seconds']/after['seconds'],
        native_rss_reduction_pct=100*(1-after['native_peak_rss_mib']/before['native_peak_rss_mib']),
        tree_rss_reduction_pct=100*(1-after['peak_tree_rss_mib']/before['peak_tree_rss_mib']),
        baseline_binary_sha256=before['binary_sha256'], optimized_binary_sha256=after['binary_sha256'])
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False)+'\n', encoding='utf-8')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
