"""Scale the saved-hedge-mixed-1 fixture to N backbook positions (synthetic acceptance input).

Each copy of the fixture's dated backbook loan gets its own position, owner, source coverage,
schedule events and accrual plans; the saved derivative and formal hedge overlays are unchanged.
Usage: python scripts/make_scaled_hedge_book.py N OUT.json [--tier]
--tier selects the explicit acceptance_60000 tier (required above 512 positions).
"""
import copy
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def scaled(n, tier=False):
    raw = json.loads((ROOT / 'packages/portfolio-risk/tests/fixtures/saved_hedge_mixed/base.json').read_text())
    dated = raw['dated']['dated']
    spec = raw['dated']['specification']
    base_owner = dated['owners'][0]
    base_id = base_owner['position_id']
    source = dated['sources'][0]
    base_events = [e for e in source['schedule']['events'] if e['position_id'] == base_id]
    base_cover = [c for c in source['coverage'] if c['position_id'] == base_id]
    base_plans = [p for p in dated['accrual_plans'] if p['position_id'] == base_id]
    base_position = next(p for p in spec['positions'] if p.get('id', p.get('position_id')) == base_id)
    key = 'id' if 'id' in base_position else 'position_id'
    owners, events, cover, plans, positions = [], [], [], [], []
    for i in range(n):
        pid = base_id if i == 0 else f'{base_id}-{i:06d}'
        owners.append(dict(base_owner, position_id=pid))
        events += [dict(e, position_id=pid) for e in base_events]
        cover += [dict(c, position_id=pid) for c in base_cover]
        plans += [dict(p, position_id=pid) for p in base_plans]
        positions.append(dict(copy.deepcopy(base_position), **{key: pid}))
    dated['owners'] = owners
    source['schedule']['events'] = sorted(events, key=lambda e: (e['payment_ordinal'], e['position_id']))
    source['coverage'] = cover
    dated['accrual_plans'] = plans
    spec['positions'] = positions
    # Every added loan is an asset funded by opening equity, so the opening stays balanced.
    account = next(a for a in spec['accounts'] if a['id'] == base_position['account'])
    account['equity'] += (n - 1) * base_position['balance']
    if tier:
        raw['scale_tier'] = 'acceptance_60000'
    return raw


if __name__ == '__main__':
    n, out = int(sys.argv[1]), Path(sys.argv[2])
    out.write_text(json.dumps(scaled(n, '--tier' in sys.argv[3:]), separators=(',', ':')), encoding='utf-8')
    print(out, out.stat().st_size)
