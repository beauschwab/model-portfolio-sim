"""Scale the credit-ledger-1 example to N credit positions in one baseline run (synthetic input).

Every copy gets its own position, contract position, claim namespace and boundary event IDs in
every run (baseline and stress); opening equity grows with each loan's carrying amount so the
opening balance sheet balances. --tier selects acceptance_60000 (needed above 31,000 positions).
Usage: python scripts/make_scaled_credit_book.py N OUT.json [--tier]
"""
import copy
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def scaled(n, tier=False):
    raw = json.loads((ROOT / 'examples/models/credit-ledger-1.json').read_text())
    raw = raw.get('request', raw)
    spec = raw['specification']
    base_position = spec['positions'][0]
    positions = []
    for i in range(n):
        pid = base_position['id'] if i == 0 else f"{base_position['id']}-{i:06d}"
        positions.append(dict(base_position, id=pid))
    for run in raw['runs']:
        run['positions'] = [_copy(run['positions'][0], i, positions[i]['id']) for i in range(n)]
    spec['positions'] = positions
    account = spec['accounts'][0]
    account['equity'] += (n - 1) * (base_position['balance'] + base_position['book_adjustment'])
    if tier:
        raw['scale_tier'] = 'acceptance_60000'
    return raw


def _copy(base_run_position, i, pid):
    p = copy.deepcopy(base_run_position)
    p['contract']['position_id'] = pid
    p['contract']['claim_namespace'] = f"{base_run_position['contract']['claim_namespace']}-{i:06d}"
    for b in p['boundaries']:
        b['event_id'] = f"{pid}-{b['event_id'].split('-', 2)[-1]}" if i else b['event_id']
    return p


if __name__ == '__main__':
    n, out = int(sys.argv[1]), Path(sys.argv[2])
    out.write_text(json.dumps(scaled(n, '--tier' in sys.argv[3:]), separators=(',', ':')), encoding='utf-8')
    print(out, out.stat().st_size)
