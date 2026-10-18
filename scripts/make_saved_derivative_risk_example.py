"""Write the saved-derivative-risk-1 example from the hedge-accounting-1 example.

Positions are the complete Pricing swaps at the as-of node of the saved derivative source,
with their legal netting sets. The fair-value relationship carries its hedged item and
benchmark rate; the cash-flow relationship has instrument risk only.
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def build():
    hedge = json.loads((ROOT / 'examples/models/hedge-accounting-1.json').read_text())
    csa = hedge['derivative']['derivative']['operation']['request']
    netting = {m['position_id']: m['netting_set_id'] for m in csa['mapping']}
    node = csa['paths'][0]['nodes'][0]
    positions = [dict(position_id=t['request']['position_id'], netting_set_id=netting[t['request']['position_id']],
                      swap=t['request']) for t in node['trades'] if t['kind'] == 'swap']
    items = {i['item_id']: i for i in hedge['hedged_items']}
    relationships = []
    for rel in hedge['relationships']:
        entry = dict(relationship_id=rel['relationship_id'],
                     instrument_position_ids=[rel['hedging_instrument_position_id']])
        if rel['hedged_item_id']:
            entry['hedged_item'] = dict(item=items[rel['hedged_item_id']], benchmark_rate=rel['benchmark_rate'],
                                        hedged_notional=rel['hedged_notional'])
        relationships.append(entry)
    points = [p['ordinal'] for p in positions[0]['swap']['discount_curve']['points']]
    return dict(schema='saved-derivative-risk-1',
                source=dict(id='saved-derivative-source-example', revision='r1'),
                positions=positions, relationships=relationships,
                bumps=dict(identity=dict(id='paired-curve-bumps', revision='r1'), bump_bp=1.0,
                           key_rate_ordinals=points[1:]),
                projection_policy='named_projection_curves',
                projection_curves=projection_curves(positions))


def projection_curves(positions):
    """One named curve per index whose simple forwards reproduce every supplied fixing."""
    curves = {}
    for p in positions:
        asof = p['swap']['asof_ordinal']
        for period in p['swap']['floating_periods']:
            coupon = period['coupon']
            _require(period['day_count'] == 'act360', 'example builder expects ACT/360 periods')
            points = curves.setdefault(coupon['index']['index_id'], {asof: 1.0})
            for o in coupon['observations']:
                start, end = o['interest_start_ordinal'], o['interest_end_ordinal']
                _require(start in points, 'example builder expects contiguous observations')
                value = points[start] / (1. + o['rate'] * (end - start) / 360.)
                _require(end not in points or abs(points[end] - value) < 1e-15, 'inconsistent fixings for one index')
                points[end] = value
    return [dict(index_id=index, source=dict(id=f'{index}-projection', revision='r1'), day_count='act360',
                 points=[dict(ordinal=t, discount_factor=points[t]) for t in sorted(points)])
            for index, points in sorted(curves.items())]


def _require(condition, message):
    if not condition:
        raise ValueError(message)


if __name__ == '__main__':
    body = json.dumps(build(), indent=1, sort_keys=True) + '\n'
    for path in ('examples/models/saved-derivative-risk-1.json',
                 'packages/portfolio-risk-native/tests/fixtures/saved-derivative-risk.json'):
        (ROOT / path).write_text(body, encoding='utf-8', newline='\n')
