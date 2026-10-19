"""Independent reference for saved-derivative-risk-1 paired curve risk.

Values every bumped swap with the derivative verifier's own swap valuation and every hedged
item with the hedge reference's benchmark valuation. Shares no code with the native graph.
"""
import copy
import math

from .derivative_verification import _require, _swap, _tau
from .hedge_accounting_reference import _unit_clean

SCHEMA = 'saved-derivative-risk-1'


def _weight(knots, k, t):
    n = len(knots)
    if n == 1:
        return 1.
    if (k == 0 and t <= knots[0]) or (k == n - 1 and t >= knots[-1]):
        return 1.
    if k > 0 and knots[k - 1] < t <= knots[k]:
        return (t - knots[k - 1]) / (knots[k] - knots[k - 1])
    if k + 1 < n and knots[k] < t < knots[k + 1]:
        return (knots[k + 1] - t) / (knots[k + 1] - knots[k])
    return 0.


def _scenarios(bumps):
    out = [None, ('parallel', -1.), ('parallel', 1.)]
    for k in range(len(bumps['key_rate_ordinals'])):
        out += [('key', k, -1.), ('key', k, 1.)]
    return out


def _shift(bumps, scenario, t):
    if scenario is None:
        return 0.
    h = bumps['bump_bp'] * 1e-4
    if scenario[0] == 'parallel':
        return scenario[1] * h
    return scenario[2] * h * _weight(bumps['key_rate_ordinals'], scenario[1], t)


def _curve(curve, bumps, scenario):
    c = copy.deepcopy(curve)
    for p in c['points']:
        p['discount_factor'] *= math.exp(-_shift(bumps, scenario, p['ordinal']) * (p['ordinal'] - curve['asof_ordinal']) / 365.)
    return c


def _projection(curve, asof, bumps, scenario):
    return {p['ordinal']: p['discount_factor'] * math.exp(-_shift(bumps, scenario, p['ordinal']) * (p['ordinal'] - asof) / 365.)
            for p in curve['points']}


def _bumped_swap(swap, request, scenario):
    bumps = request['bumps']
    named = request['projection_policy'] == 'named_projection_curves'
    _require(request['projection_policy'] in {'named_projection_curves', 'additive_projected_observations'}
             and bool(request.get('projection_curves')) == named, 'invalid projection policy')
    curves = {c['index_id']: c for c in request.get('projection_curves', [])}
    s = copy.deepcopy(swap)
    s['discount_curve'] = _curve(swap['discount_curve'], bumps, scenario)
    for p in s['floating_periods']:
        index = p['coupon']['index']['index_id']
        if named:
            _require(index in curves, 'projected index has no named projection curve')
            points = _projection(curves[index], swap['asof_ordinal'], bumps, scenario)
        for o in p['coupon']['observations']:
            if o['source']['kind'] != 'conditional_projection':
                continue
            if named:
                a, b = o['interest_start_ordinal'], o['interest_end_ordinal']
                _require(a in points and b in points, 'projection curve lacks an exact observation date')
                o['rate'] = (points[a] / points[b] - 1.) / _tau(a, b, curves[index]['day_count'])
            else:
                o['rate'] += _shift(bumps, scenario, o['fixing_ordinal'])
    return s


def _risk(values, bumps):
    h = bumps['bump_bp']
    d = lambda down, up: (down - up) / (2. * h)
    return dict(clean_value=values[0], dv01=d(values[1], values[2]),
                key_rate_dv01=[d(values[3 + 2 * k], values[4 + 2 * k]) for k in range(len(bumps['key_rate_ordinals']))])


def reference(request):
    _require(request['schema'] == SCHEMA, 'unsupported schema')
    bumps = request['bumps']
    cases = _scenarios(bumps)
    if request['projection_policy'] == 'named_projection_curves':
        for p in request['positions']:
            base = _bumped_swap(p['swap'], request, None)
            for a, b in zip(p['swap']['floating_periods'], base['floating_periods']):
                for x, y in zip(a['coupon']['observations'], b['coupon']['observations']):
                    _require(math.isclose(x['rate'], y['rate'], rel_tol=1e-12, abs_tol=1e-9),
                             'named projection curve does not reproduce a supplied projected fixing')
    values = {p['position_id']: [_swap(_bumped_swap(p['swap'], request, s))[0] for s in cases] for p in request['positions']}
    owners = {i: r['relationship_id'] for r in request.get('relationships', []) for i in r['instrument_position_ids']}
    positions = [dict(position_id=p['position_id'], netting_set_id=p['netting_set_id'],
                      relationship_id=owners.get(p['position_id']), **_risk(values[p['position_id']], bumps))
                 for p in request['positions']]
    total = [math.fsum(values[p['position_id']][i] for p in request['positions']) for i in range(len(cases))]
    sets = sorted({p['netting_set_id'] for p in request['positions']})
    netting = [dict(netting_set_id=s, **_risk([math.fsum(values[p['position_id']][i] for p in request['positions']
                                                         if p['netting_set_id'] == s) for i in range(len(cases))], bumps))
               for s in sets]
    by_id = {p['position_id']: p['swap'] for p in request['positions']}
    relationships = []
    for rel in request.get('relationships', []):
        instrument = _risk([math.fsum(values[i][k] for i in rel['instrument_position_ids']) for k in range(len(cases))], bumps)
        item = None
        x = rel.get('hedged_item')
        if x:
            anchor = by_id[rel['instrument_position_ids'][0]]
            sign = 1. if x['item']['direction'] == 'asset' else -1.
            end = x.get('hedged_term_end_ordinal') or x['item']['principal_ordinal']
            row = []
            for s in cases:
                curve = _curve(anchor['discount_curve'], bumps, s)
                dfs = {p['ordinal']: p['discount_factor'] for p in curve['points']}
                unit = _unit_clean(x['item'], x['benchmark_rate'], dfs, anchor['asof_ordinal'],
                                   anchor['payment_timing'] == 'before_scheduled_payments', end)
                row.append(sign * x['hedged_notional'] * unit)
            item = _risk(row, bumps)
        net = instrument['dv01'] + (item['dv01'] if item else 0.)
        net_key = [a + (item['key_rate_dv01'][i] if item else 0.) for i, a in enumerate(instrument['key_rate_dv01'])]
        ratio = -instrument['dv01'] / item['dv01'] if item and item['dv01'] != 0. else None
        relationships.append(dict(relationship_id=rel['relationship_id'], instrument=instrument, hedged_item=item,
                                  net_dv01=net, net_key_rate_dv01=net_key, dv01_offset_ratio=ratio))
    return dict(positions=positions, netting_sets=netting, relationships=relationships, total=_risk(total, bumps))


def _close(a, b, where):
    if a is None or b is None:
        _require(a is None and b is None, f'{where}: presence differs')
        return
    if isinstance(a, list):
        _require(len(a) == len(b), f'{where}: length differs')
        for i, (x, y) in enumerate(zip(a, b)):
            _close(x, y, f'{where}[{i}]')
        return
    _require(math.isclose(a, b, rel_tol=1e-12, abs_tol=1e-9), f'{where}: {a} != {b}')


def _same_risk(a, b, where):
    for k in ('clean_value', 'dv01', 'key_rate_dv01'):
        _close(a[k], b[k], f'{where}.{k}')


def verify(request, result):
    """Raise unless the native result matches the independent reference."""
    expected = reference(request)
    _require(result['schema'] == SCHEMA and result['source'] == request['source']
             and result['observed_fixings_bumped'] is False and result['accounting_designation_output'] is False
             and result['pairing_policy'] == 'deterministic_paired_curve_bumps-1:' + request['projection_policy'],
             'saved derivative risk identity or scope flags differ')
    for key in ('positions', 'netting_sets', 'relationships'):
        _require(len(result[key]) == len(expected[key]), f'{key} count differs')
    for a, b in zip(result['positions'], expected['positions']):
        _require((a['position_id'], a['netting_set_id'], a['relationship_id']) ==
                 (b['position_id'], b['netting_set_id'], b['relationship_id']), 'position identity differs')
        _same_risk(a, b, a['position_id'])
    for a, b in zip(result['netting_sets'], expected['netting_sets']):
        _require(a['netting_set_id'] == b['netting_set_id'], 'netting set order differs')
        _same_risk(a, b, a['netting_set_id'])
    for a, b in zip(result['relationships'], expected['relationships']):
        _require(a['relationship_id'] == b['relationship_id'], 'relationship order differs')
        _same_risk(a['instrument'], b['instrument'], a['relationship_id'])
        _require((a['hedged_item'] is None) == (b['hedged_item'] is None), 'hedged item presence differs')
        if b['hedged_item']:
            _same_risk(a['hedged_item'], b['hedged_item'], a['relationship_id'] + '.item')
        _close(a['net_dv01'], b['net_dv01'], 'net_dv01')
        _close(a['net_key_rate_dv01'], b['net_key_rate_dv01'], 'net_key_rate_dv01')
        _close(a['dv01_offset_ratio'], b['dv01_offset_ratio'], 'offset ratio')
    _same_risk(result['total'], expected['total'], 'total')
    return expected
