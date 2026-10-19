"""Independent reference for net-investment-hedge-1.

Recomputes translation, forward and debt net-investment hedges, the ASU 2025-09 dual hedge
(basis from the independent hedge-accounting reference) and disposal from the raw request, then
compares every table and posting with the native result. Shares no code with the native owner.
"""
import math

from .hedge_accounting_reference import replay as hedge_replay

SCHEMA = 'net-investment-hedge-1'


class NetInvestmentVerificationError(ArithmeticError):
    pass


def _require(condition, message):
    if not condition:
        raise NetInvestmentVerificationError(message)


def _ratio(actual, hypothetical):
    return None if hypothetical == 0. else actual / hypothetical


def _in_band(policy, ratio):
    return ratio is None or policy['assessment_lower_bound'] <= ratio <= policy['assessment_upper_bound']


def _point(points, t):
    for p in points:
        if p['ordinal'] == t:
            return p['discount_factor']
    raise NetInvestmentVerificationError('missing exact discount point')


def _forward_value(inst, d):
    sign = 1. if inst['side'] == 'sell_foreign' else -1.
    t = d['ordinal']
    if t < inst['trade_ordinal'] or t > inst['maturity_ordinal']:
        return 0.
    if t == inst['maturity_ordinal']:
        return sign * inst['notional'] * (inst['forward_rate'] - d['spot'])
    p_rc = _point(d['reporting_discount'], inst['maturity_ordinal'])
    p_fc = _point(d['foreign_discount'], inst['maturity_ordinal'])
    return sign * inst['notional'] * (inst['forward_rate'] - d['spot'] * p_fc / p_rc) * p_rc


def _dual_basis(raw, rel):
    d = rel['dual_fair_value_hedge']
    h = d['request']
    _require(h['currency'] == raw['foreign_currency'] and h['entity'] == raw['entity'], 'dual hedge currency or entity differs')
    expected = hedge_replay(h)
    basis, discontinued = {}, None
    for day in expected['daily']:
        for x in day['relationships']:
            if x['relationship_id'] == d['relationship_id']:
                basis[day['ordinal']] = x['basis_adjustment_balance'] + x['frozen_basis_balance']
                if x['status'] == 'discontinued' and discontinued is None:
                    discontinued = day['ordinal']
    return basis, discontinued


class _Ledger:
    def __init__(self):
        self.lines, self.balances, self.seq = [], {}, 0

    def post(self, t, event, lines, position=None, relationship=None):
        lines = [(a, v) for a, v in lines if v != 0.]
        if not lines:
            return
        _require(abs(math.fsum(v for _, v in lines)) <= 1e-9, f'unbalanced {event}')
        self.seq += 1
        tx = f'nih:{t}:{self.seq:06d}:{event}'
        for a, v in lines:
            self.balances[a] = self.balances.get(a, 0.) + v
            self.lines.append(dict(transaction_id=tx, ordinal=t, event=event, account=a, position_id=position,
                                   relationship_id=relationship, debit=max(v, 0.), credit=max(-v, 0.)))


def replay(raw):
    _require(raw['schema'] == SCHEMA and raw['model_version'] == SCHEMA, 'unsupported schema')
    dates, fo, policy = raw['dates'], raw['foreign_operation'], raw['policy']
    duals = {r['relationship_id']: _dual_basis(raw, r) for r in raw['relationships'] if r.get('dual_fair_value_hedge')}
    if duals:
        _require(policy.get('dual_hedge_adoption') is not None, 'dual hedge needs adoption')
    led = _Ledger()
    first, s0 = dates[0]['ordinal'], dates[0]['spot']
    na = fo['opening_net_assets']
    carrying = na * s0
    cta_t = fo['opening_translation_cta']
    led.post(first, 'opening_net_investment', [('net_investment_foreign_operation', carrying), ('cta_translation', -cta_t),
                                               ('opening_equity', cta_t - carrying)])
    for inst in raw['instruments']:
        if inst['kind'] == 'foreign_debt':
            v = inst['notional'] * s0
            led.post(first, 'opening_foreign_debt', [('foreign_debt', -v), ('opening_equity', v)], inst['position_id'])
    translation = [dict(ordinal=first, spot=s0, net_assets=na, carrying=carrying, translation_adjustment=0., translation_cta=cta_t)]
    rel_of = {r['instrument_position_id']: r for r in raw['relationships']}
    st = {r['relationship_id']: dict(active=False, discontinued=None, recognized=0., released=0., actual=0., hyp=0., amort=0.)
          for r in raw['relationships']}
    values, periods, instruments = {}, [], []
    hedge_cta = 0.

    def designate(rel, k, inst, value):
        s = st[rel['relationship_id']]
        s['active'] = True
        if inst['kind'] == 'fx_forward':
            sign = 1. if inst['side'] == 'sell_foreign' else -1.
            s['amort'] = sign * inst['notional'] * (inst['forward_rate'] - dates[k]['spot']) - value

    for k, d in enumerate(dates):
        t = d['ordinal']
        if k:
            flows_rc = flows_fc = 0.
            for m in fo['movements']:
                if m['ordinal'] != t:
                    continue
                fc = -m['amount'] if m['kind'] == 'distribution' else m['amount']
                rc = fc * m['rate']
                other = 'pnl:foreign_operation_income' if m['kind'] == 'income' else 'cash'
                led.post(t, 'foreign_operation_flow', [('net_investment_foreign_operation', rc), (other, -rc)])
                flows_rc += rc
                flows_fc += fc
            na += flows_fc
            _require(na >= 0., 'negative net assets')
            closing = na * d['spot']
            adj = closing - carrying - flows_rc
            led.post(t, 'translation', [('net_investment_foreign_operation', adj), ('cta_translation', -adj)])
            carrying = closing
            cta_t += adj
            translation.append(dict(ordinal=t, spot=d['spot'], net_assets=na, carrying=carrying, translation_adjustment=adj,
                                    translation_cta=cta_t))
        opening_na = na if k == 0 else translation[k - 1]['net_assets']
        designated = sum(r['hedged_net_investment'] for r in raw['relationships'] if st[r['relationship_id']]['active'])
        scale = opening_na / designated if designated > opening_na and designated > 0. else 1.
        for inst in raw['instruments']:
            pid = inst['position_id']
            rel = rel_of.get(pid)
            s = st[rel['relationship_id']] if rel else None
            if inst['kind'] == 'fx_forward':
                if t < inst['trade_ordinal'] or t > inst['maturity_ordinal']:
                    continue
                v = _forward_value(inst, d)
                if t == inst['trade_ordinal']:
                    led.post(t, 'fx_forward_trade', [('fx_forward', v), ('cash', -v)], pid)
                    values[pid] = v
                    instruments.append(dict(ordinal=t, position_id=pid, carrying=v, change=0.))
                    if rel and rel['designation_ordinal'] == t:
                        designate(rel, k, inst, v)
                    continue
                change = v - values[pid]
                sign = 1. if inst['side'] == 'sell_foreign' else -1.
                lines, rid = [('fx_forward', change)], None
                if s and s['active']:
                    rid = rel['relationship_id']
                    hedged = rel['hedged_net_investment'] * scale
                    phi = min(1., hedged / inst['notional'])
                    included = sign * inst['notional'] * (dates[k - 1]['spot'] - d['spot']) if rel['method'] == 'spot' else change
                    excluded = change - included
                    amort = (s['amort'] * (t - dates[k - 1]['ordinal']) / (inst['maturity_ordinal'] - rel['designation_ordinal'])
                             if rel.get('excluded_component') == 'amortization' else 0.)
                    if rel.get('excluded_component') == 'mark_to_market':
                        ex_pnl, ex_cta = phi * excluded, 0.
                    elif rel.get('excluded_component') == 'amortization':
                        ex_pnl, ex_cta = phi * amort, phi * (excluded - amort)
                    else:
                        ex_pnl = ex_cta = 0.
                    cta = phi * included + ex_cta
                    excess = (1. - phi) * (included + excluded)
                    s['actual'] += phi * included
                    s['hyp'] += hedged / inst['notional'] * included
                    ratio = _ratio(s['actual'], s['hyp'])
                    passed = _in_band(policy, ratio)
                    if passed:
                        lines += [('cta_net_investment_hedge', -cta), ('pnl:fx_forward_excluded_component', -ex_pnl),
                                  ('pnl:net_investment_hedge_excess', -excess)]
                        s['recognized'] += cta
                        hedge_cta += cta
                    else:
                        lines.append(('pnl:fx_forward_mark', -change))
                        s['active'], s['discontinued'] = False, t
                    periods.append(dict(ordinal=t, relationship_id=rid, status='qualified' if passed else 'failed_assessment',
                                        hedged_effective=hedged, designated_fraction=phi, included_change=included,
                                        excluded_change=excluded, cta=cta if passed else 0., excluded_earnings=ex_pnl if passed else 0.,
                                        excess_earnings=excess if passed else change, basis_remeasurement=0.,
                                        cumulative_ratio=ratio, assessment_passed=passed))
                else:
                    lines.append(('pnl:fx_forward_mark', -change))
                led.post(t, 'fx_forward_mark', lines, pid, rid)
                values[pid] = v
                instruments.append(dict(ordinal=t, position_id=pid, carrying=v, change=change))
                if t == inst['maturity_ordinal']:
                    led.post(t, 'fx_forward_settlement', [('cash', v), ('fx_forward', -v)], pid)
                    values[pid] = 0.
                    if s and s['active']:
                        s['active'], s['discontinued'] = False, t
                elif rel and rel['designation_ordinal'] == t:
                    designate(rel, k, inst, v)
            else:
                n = inst['notional']
                if k == 0:
                    values[pid] = -n * d['spot']
                    instruments.append(dict(ordinal=t, position_id=pid, carrying=values[pid], change=0.))
                    if rel and rel['designation_ordinal'] == t:
                        designate(rel, k, inst, 0.)
                    continue
                prev_t = dates[k - 1]['ordinal']
                if prev_t >= inst['maturity_ordinal']:
                    continue
                ds = d['spot'] - dates[k - 1]['spot']
                gain = -n * ds
                dual = rel is not None and rel['relationship_id'] in duals
                basis_prev = basis_gain = 0.
                fvh_active = False
                if dual:
                    basis, stop = duals[rel['relationship_id']]
                    _require(prev_t in basis, 'dual basis missing on a reporting date')
                    basis_prev = basis[prev_t]
                    basis_gain = -basis_prev * ds
                    fvh_active = stop is None or stop > prev_t
                lines = [('foreign_debt', gain)] + ([('foreign_debt_basis_translation', basis_gain)] if dual else [])
                rid = None
                if s and s['active']:
                    rid = rel['relationship_id']
                    hedged = rel['hedged_net_investment'] * scale
                    joined = dual and not fvh_active
                    amount_fc = n + basis_prev if joined else n
                    _require(amount_fc > 0., 'nonpositive hedging debt')
                    phi = min(1., hedged / amount_fc)
                    included = gain + basis_gain if joined else gain
                    cta, excess = phi * included, (1. - phi) * included
                    s['actual'] += phi * included
                    s['hyp'] += hedged / amount_fc * included
                    ratio = _ratio(s['actual'], s['hyp'])
                    passed = _in_band(policy, ratio)
                    basis_pnl = basis_gain if dual and (not joined or not passed) else 0.
                    if passed:
                        lines += [('cta_net_investment_hedge', -cta), ('pnl:net_investment_hedge_excess', -excess)]
                        s['recognized'] += cta
                        hedge_cta += cta
                    else:
                        lines.append(('pnl:fx_transaction', -gain))
                        s['active'], s['discontinued'] = False, t
                    if basis_pnl != 0.:
                        lines.append(('pnl:fx_basis_remeasurement', -basis_pnl))
                    periods.append(dict(ordinal=t, relationship_id=rid, status='qualified' if passed else 'failed_assessment',
                                        hedged_effective=hedged, designated_fraction=phi, included_change=included,
                                        excluded_change=0., cta=cta if passed else 0., excluded_earnings=0.,
                                        excess_earnings=excess if passed else gain, basis_remeasurement=basis_pnl,
                                        cumulative_ratio=ratio, assessment_passed=passed))
                else:
                    lines.append(('pnl:fx_transaction', -gain))
                    if dual:
                        lines.append(('pnl:fx_basis_remeasurement', -basis_gain))
                led.post(t, 'foreign_debt_remeasurement', lines, pid, rid)
                values[pid] += gain
                instruments.append(dict(ordinal=t, position_id=pid, carrying=values[pid], change=gain))
                if t == inst['maturity_ordinal']:
                    v = values[pid]
                    led.post(t, 'foreign_debt_repayment', [('foreign_debt', -v), ('cash', v)], pid)
                    values[pid] = 0.
                    if s and s['active']:
                        s['active'], s['discontinued'] = False, t
                elif rel and rel['designation_ordinal'] == t:
                    designate(rel, k, inst, 0.)
        for e in raw['events']:
            if e['ordinal'] == t and e['kind'] == 'dedesignate':
                s = st[e['relationship_id']]
                _require(s['active'], 'dedesignation of an inactive relationship')
                s['active'], s['discontinued'] = False, t
        for e in raw['events']:
            if e['ordinal'] != t or e['kind'] != 'disposal':
                continue
            f, proceeds = e['fraction'], e['proceeds']
            _require(na > 0., 'disposal after full sale')
            rel_t, rel_h, sold = f * cta_t, f * hedge_cta, f * carrying
            led.post(t, 'cta_reclassification', [('cta_translation', rel_t), ('cta_net_investment_hedge', rel_h),
                                                 ('pnl:cta_reclassification', -rel_t - rel_h)])
            led.post(t, 'disposal', [('cash', proceeds), ('net_investment_foreign_operation', -sold),
                                     ('pnl:disposal_gain_loss', sold - proceeds)])
            for r in raw['relationships']:
                s = st[r['relationship_id']]
                if s['recognized'] - s['released'] != 0.:
                    s['released'] += f * (s['recognized'] - s['released'])
                if f == 1. and s['active']:
                    s['active'], s['discontinued'] = False, t
            cta_t -= rel_t
            hedge_cta -= rel_h
            na *= 1. - f
            carrying -= sold
            translation[-1].update(net_assets=na, carrying=carrying, translation_cta=cta_t)
    relationships = [dict(relationship_id=r['relationship_id'],
                          status='qualified' if st[r['relationship_id']]['active'] else 'discontinued' if st[r['relationship_id']]['discontinued'] is not None else 'pending',
                          discontinued_ordinal=st[r['relationship_id']]['discontinued'],
                          hedge_cta_recognized=st[r['relationship_id']]['recognized'],
                          hedge_cta_released=st[r['relationship_id']]['released'],
                          dual_hedge_applied=bool(r.get('dual_fair_value_hedge'))) for r in raw['relationships']]
    closing = [dict(account=a, debit_balance=v) for a, v in sorted(led.balances.items())]
    _require(abs(math.fsum(b['debit_balance'] for b in closing)) <= 1e-9, 'trial balance does not balance')
    return dict(journal=led.lines, translation=translation, periods=periods, instruments=instruments,
                relationships=relationships, closing=closing, dual_hedge_applied=bool(duals))


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
    for key in ('translation', 'periods', 'instruments', 'relationships', 'closing', 'journal'):
        _rows(result[key], expected[key], key)
    _require(result['schema'] == SCHEMA and result['dual_hedge_applied'] == expected['dual_hedge_applied']
             and result['accounting_standard_approval'] is False and result['debt_interest_included'] is False
             and result['capital_ftp_eligible'] is False, 'net investment scope flags differ')
    return dict(verified=True, translation_recomputed=True, hedge_cta_recomputed=True, ordered_journal_recomputed=True,
                dual_hedge_basis_recomputed=bool(expected['dual_hedge_applied']), accounting_standard_approval=False)
