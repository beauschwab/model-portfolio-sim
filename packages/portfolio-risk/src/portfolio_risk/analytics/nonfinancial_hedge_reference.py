"""Independent reference for nonfinancial-hedge-1.

Recomputes swap legs, the hypothetical derivative on the hedged component, the cumulative dollar
offset, OCI by delivery and every reclassification from the raw request, then compares all tables
and the ordered journal with the native result. Shares no code with the native owner.
"""
import math

SCHEMA = 'nonfinancial-hedge-1'


class NonfinancialVerificationError(ArithmeticError):
    pass


def _require(condition, message):
    if not condition:
        raise NonfinancialVerificationError(message)


def _price(d, index, delivery):
    for c in d['indices']:
        if c['index_id'] == index:
            for f in c['forwards']:
                if f['delivery_ordinal'] == delivery:
                    return f['price']
    raise NonfinancialVerificationError('missing forward price')


def _df(d, t):
    for p in d['discount']:
        if p['ordinal'] == t:
            return p['discount_factor']
    raise NonfinancialVerificationError('missing discount point')


def _leg(d, index, sign, quantity, fixed, settle):
    if d['ordinal'] > settle:
        return 0.
    return sign * quantity * (_price(d, index, settle) - fixed) * (1. if d['ordinal'] == settle else _df(d, settle))


class _Ledger:
    def __init__(self):
        self.lines, self.balances, self.seq = [], {}, 0

    def post(self, t, event, tags, lines):
        lines = [(a, v) for a, v in lines if v != 0.]
        if not lines:
            return
        _require(abs(math.fsum(v for _, v in lines)) <= 1e-9, f'unbalanced {event}')
        self.seq += 1
        tx = f'nfh:{t}:{self.seq:06d}:{event}'
        for a, v in lines:
            self.balances[a] = self.balances.get(a, 0.) + v
            self.lines.append(dict(transaction_id=tx, ordinal=t, event=event, account=a, position_id=tags[0],
                                   relationship_id=tags[1], delivery_id=tags[2], debit=max(v, 0.), credit=max(-v, 0.)))


def replay(raw):
    _require(raw['schema'] == SCHEMA and raw['model_version'] == SCHEMA, 'unsupported schema')
    policy = raw['policy']
    exposures = {e['exposure_id']: e for e in raw['exposures']}
    swaps = {s['position_id']: s for s in raw['instruments']}
    for rel in raw['relationships']:
        comp = exposures[rel['exposure_id']]['component']
        if comp['kind'] != 'contractually_specified':
            a = policy.get('asu_2025_09_adoption')
            _require(a is not None and a['adoption_ordinal'] <= rel['designation_ordinal'], 'component needs ASU 2025-09 adoption')
    rel_of_leg = {(r['instrument_position_id'], m['leg_id']): (r, m['delivery_id']) for r in raw['relationships'] for m in r['legs']}
    led = _Ledger()
    legs, legs_out, periods = {}, [], []
    deliveries = {(e['exposure_id'], d['delivery_id']): dict(recognized=0., reclassified=0., consumed=0., status='probable', designated=False)
                  for e in raw['exposures'] for d in e['deliveries']}
    rels = {r['relationship_id']: dict(active=False, completed=False, discontinued=None, ci=0., ch=0., fixed={}, value={})
            for r in raw['relationships']}
    dates = raw['dates']
    for k, d in enumerate(dates):
        t = d['ordinal']
        by_rel = {}
        for s in raw['instruments']:
            sign = 1. if s['side'] == 'receive_index' else -1.
            for leg in s['legs']:
                if t < s['trade_ordinal'] or (k and dates[k - 1]['ordinal'] >= leg['settlement_ordinal']):
                    continue
                key = (s['position_id'], leg['leg_id'])
                v = _leg(d, s['index_id'], sign, leg['quantity'], s['fixed_price'], leg['settlement_ordinal'])
                tags = (s['position_id'], None, None)
                if t == s['trade_ordinal']:
                    led.post(t, 'commodity_swap_trade', tags, [('commodity_swap', v), ('cash', -v)])
                    rel = rel_of_leg.get(key)
                    designated = bool(rel and rels[rel[0]['relationship_id']]['active'])
                    legs[key] = dict(value=v, designated=designated)
                    legs_out.append(dict(ordinal=t, position_id=s['position_id'], leg_id=leg['leg_id'], value=v, change=0., designated=designated))
                    continue
                st = legs[key]
                change = v - st['value']
                st['value'] = v
                mapped = rel_of_leg.get(key)
                designated = st['designated'] and mapped is not None and rels[mapped[0]['relationship_id']]['active']
                if designated:
                    entry = by_rel.setdefault(mapped[0]['relationship_id'], [0., []])
                    entry[0] += change
                    entry[1].append((mapped[1], change))
                else:
                    led.post(t, 'commodity_swap_mark', tags, [('commodity_swap', change), ('pnl:commodity_swap_mark', -change)])
                legs_out.append(dict(ordinal=t, position_id=s['position_id'], leg_id=leg['leg_id'], value=v, change=change, designated=designated))
        for rel in raw['relationships']:
            st = rels[rel['relationship_id']]
            if not st['active'] or rel['designation_ordinal'] == t:
                continue
            e = exposures[rel['exposure_id']]
            sign = 1. if e['direction'] == 'purchase' else -1.
            swap = swaps[rel['instrument_position_id']]
            hyp = 0.
            for m in rel['legs']:
                dl = next(x for x in e['deliveries'] if x['delivery_id'] == m['delivery_id'])
                leg = next(x for x in swap['legs'] if x['leg_id'] == m['leg_id'])
                if dates[k - 1]['ordinal'] >= dl['ordinal'] or not deliveries[(e['exposure_id'], m['delivery_id'])]['designated']:
                    continue
                v = _leg(d, e['component']['index_id'], sign, leg['quantity'], st['fixed'][m['delivery_id']], dl['ordinal'])
                hyp += v - st['value'][m['delivery_id']]
                st['value'][m['delivery_id']] = v
            inst, parts = by_rel.pop(rel['relationship_id'], [0., []])
            st['ci'] += inst
            st['ch'] += hyp
            ratio = None if st['ch'] == 0. else st['ci'] / st['ch']
            passed = ratio is None or policy['assessment_lower_bound'] <= ratio <= policy['assessment_upper_bound']
            tags = (rel['instrument_position_id'], rel['relationship_id'], None)
            if passed:
                for delivery, change in parts:
                    led.post(t, 'cash_flow_hedge_oci', (tags[0], tags[1], delivery), [('commodity_swap', change), ('cash_flow_hedge_oci', -change)])
                    deliveries[(e['exposure_id'], delivery)]['recognized'] += change
            else:
                led.post(t, 'commodity_swap_mark', tags, [('commodity_swap', inst), ('pnl:commodity_swap_mark', -inst)])
                st['active'], st['discontinued'] = False, t
            periods.append(dict(ordinal=t, relationship_id=rel['relationship_id'], instrument_change=inst, hypothetical_change=hyp,
                                cumulative_instrument_change=st['ci'], cumulative_hypothetical_change=st['ch'],
                                cumulative_ratio=ratio, assessment_passed=passed, oci=inst if passed else 0., earnings=0. if passed else inst))
        for rel in raw['relationships']:
            if rel['designation_ordinal'] != t:
                continue
            e = exposures[rel['exposure_id']]
            st = rels[rel['relationship_id']]
            st['active'] = True
            for m in rel['legs']:
                dl = next(x for x in e['deliveries'] if x['delivery_id'] == m['delivery_id'])
                st['fixed'][m['delivery_id']] = _price(d, e['component']['index_id'], dl['ordinal'])
                st['value'][m['delivery_id']] = 0.
                deliveries[(e['exposure_id'], m['delivery_id'])]['designated'] = True
                if (rel['instrument_position_id'], m['leg_id']) in legs:
                    legs[(rel['instrument_position_id'], m['leg_id'])]['designated'] = True
        _require(not by_rel, 'designated swap change without assessment')
        for s in raw['instruments']:
            for leg in s['legs']:
                key = (s['position_id'], leg['leg_id'])
                if leg['settlement_ordinal'] == t and key in legs:
                    v = legs[key]['value']
                    led.post(t, 'commodity_swap_settlement', (s['position_id'], None, None), [('cash', v), ('commodity_swap', -v)])
                    legs[key]['value'] = 0.
        for e in raw['exposures']:
            for dl in e['deliveries']:
                if dl['ordinal'] != t:
                    continue
                ds = deliveries[(e['exposure_id'], dl['delivery_id'])]
                if ds['status'] == 'probable_not_to_occur':
                    continue
                ds['status'] = 'occurred'
                if e['direction'] == 'sale':
                    release = ds['recognized'] - ds['reclassified']
                    led.post(t, 'sale_reclassification', (None, None, dl['delivery_id']),
                             [('cash_flow_hedge_oci', release), ('pnl:revenue_hedge_reclassification', -release)])
                    ds['reclassified'] = ds['recognized']
        for ev in raw['events']:
            if ev['ordinal'] != t:
                continue
            if ev['kind'] == 'consumption':
                ds = deliveries[(ev['exposure_id'], ev['delivery_id'])]
                f = ev['fraction']
                _require(ds['status'] == 'occurred' and ds['consumed'] + f <= 1. + 1e-12, 'consumption exceeds delivery')
                release = (ds['recognized'] - ds['reclassified']) * f / (1. - ds['consumed'])
                led.post(t, 'consumption_reclassification', (None, None, ev['delivery_id']),
                         [('cash_flow_hedge_oci', release), ('pnl:cost_of_sales_hedge_reclassification', -release)])
                ds['reclassified'] += release
                ds['consumed'] = min(ds['consumed'] + f, 1.)
            elif ev['kind'] == 'forecast_status':
                ds = deliveries[(ev['exposure_id'], ev['delivery_id'])]
                ds['designated'] = False
                for rel in raw['relationships']:
                    if rel['exposure_id'] != ev['exposure_id']:
                        continue
                    for m in rel['legs']:
                        key = (rel['instrument_position_id'], m['leg_id'])
                        if m['delivery_id'] == ev['delivery_id'] and key in legs:
                            legs[key]['designated'] = False
                if ev['status'] == 'probable_not_to_occur':
                    release = ds['recognized'] - ds['reclassified']
                    led.post(t, 'missed_forecast_reclassification', (None, None, ev['delivery_id']),
                             [('cash_flow_hedge_oci', release), ('pnl:missed_forecast_reclassification', -release)])
                    ds['reclassified'] = ds['recognized']
                    ds['status'] = 'probable_not_to_occur'
                else:
                    ds['status'] = 'not_probable'
            else:
                st = rels[ev['relationship_id']]
                _require(st['active'], 'dedesignation of inactive relationship')
                st['active'], st['discontinued'] = False, t
        for rel in raw['relationships']:
            st = rels[rel['relationship_id']]
            e = exposures[rel['exposure_id']]
            done = all(not deliveries[(e['exposure_id'], m['delivery_id'])]['designated'] or
                       any(x['delivery_id'] == m['delivery_id'] and x['ordinal'] <= t for x in e['deliveries'])
                       for m in rel['legs'])
            if st['active'] and done and rel['designation_ordinal'] < t:
                st['active'], st['completed'] = False, True
    delivery_rows = [dict(exposure_id=x, delivery_id=y, status=d['status'], oci_recognized=d['recognized'],
                          oci_reclassified=d['reclassified'], oci_balance=d['recognized'] - d['reclassified'],
                          consumed_fraction=d['consumed']) for (x, y), d in sorted(deliveries.items())]
    kinds = {'contractually_specified', 'clearly_and_closely_related', 'subcomponent'}
    relationships = []
    for rel in raw['relationships']:
        st = rels[rel['relationship_id']]
        kind = exposures[rel['exposure_id']]['component']['kind']
        _require(kind in kinds, 'unknown component kind')
        relationships.append(dict(relationship_id=rel['relationship_id'],
                                  status='qualified' if st['active'] else 'completed' if st['completed'] else 'discontinued' if st['discontinued'] is not None else 'pending',
                                  discontinued_ordinal=st['discontinued'], component_kind=kind,
                                  asu_2025_09_component=kind != 'contractually_specified'))
    closing = [dict(account=a, debit_balance=v) for a, v in sorted(led.balances.items())]
    _require(abs(math.fsum(b['debit_balance'] for b in closing)) <= 1e-9, 'trial balance does not balance')
    return dict(journal=led.lines, legs=legs_out, periods=periods, deliveries=delivery_rows, relationships=relationships, closing=closing)


def _close(a, b, where):
    if isinstance(a, float) or isinstance(b, float):
        _require(a is not None and b is not None and math.isclose(a, b, rel_tol=1e-12, abs_tol=1e-9), f'{where}: {a} != {b}')
    else:
        _require(a == b, f'{where}: {a!r} != {b!r}')


def _rows(native, expected, where):
    _require(len(native) == len(expected), f'{where} row count differs')
    for i, (a, b) in enumerate(zip(native, expected)):
        _require(set(a) == set(b), f'{where}[{i}] fields differ')
        for key in b:
            _close(a[key], b[key], f'{where}[{i}].{key}')


def verify(raw, result, check=lambda: False):
    """Raise unless the native result equals the independent replay."""
    expected = replay(raw)
    check()
    for key in ('legs', 'periods', 'deliveries', 'relationships', 'closing', 'journal'):
        _rows(result[key], expected[key], key)
    _require(result['schema'] == SCHEMA and result['accounting_standard_approval'] is False
             and result['purchase_and_inventory_cash_included'] is False and result['capital_ftp_eligible'] is False,
             'nonfinancial scope flags differ')
    return dict(verified=True, hypothetical_derivative_recomputed=True, oci_by_delivery_recomputed=True,
                ordered_journal_recomputed=True, accounting_standard_approval=False)
