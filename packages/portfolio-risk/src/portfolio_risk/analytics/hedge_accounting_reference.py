"""Independent replay of hedge-accounting-1; never a production calculator.

The derivative source is rebuilt with the existing raw derivative reference. The
hedged-item benchmark component, hypothetical values, relationship lifecycle,
assessments, re-owned journal, daily balances and roll-forwards are rebuilt here
from the raw request and docs/models/hedge-accounting.md alone. No native call.
"""
from copy import deepcopy
import math
from .derivative_verification import _derive, _discount, _swap, _tau, normalize_intents, verify_derivative_accounting
from .modeled_json_identity import exact_json_identity

SCHEMA = 'hedge-accounting-1'
RULE = 'asc815-asu2017-12'
ACCOUNTS = ('cash', 'derivative_asset', 'derivative_liability', 'unpaid_receivable', 'unpaid_payable', 'posted_vm_asset',
            'received_vm_liability', 'posted_im_asset', 'opening_equity', 'mark_gain_loss', 'contractual_income',
            'collateral_interest_income', 'counterparty_loss_expense', 'posted_collateral_return_loss_expense',
            'memo_custodian_asset', 'memo_custodian_obligation', 'cash_flow_hedge_oci', 'hedged_item_basis_adjustment',
            'hedged_item_basis_gain_loss', 'hedged_item_basis_amortization', 'hedged_item_basis_derecognition',
            'hedged_forecast_reclassification')
FLAGS = dict(accounting_standard_approval=False, empirical_validation=False, persisted_journal_verified=False,
             saved_book_adoption=False, portfolio_layer_supported=False, net_investment_supported=False)


def _require(condition, message):
    if not condition: raise ValueError(message)


def _near(a, b):
    return abs(a - b) <= max(1e-9, 64 * 2.220446049250313e-16 * max(abs(a), abs(b)))


def _money(v):
    _require(type(v) in (int, float) and math.isfinite(v) and abs(v) <= 1e15, 'finite monetary value required')
    return float(v)


def _typed(raw):
    """Mirror only the DTO's f64 fields as floats; every other scalar keeps its original category."""
    raw = deepcopy(raw)
    def money(row, key):
        if row.get(key) is not None: row[key] = _money(row[key])
    for key in ('lower_bound', 'upper_bound'): money(raw['policy']['assessment'], key)
    for item in raw['hedged_items']:
        for key in ('notional', 'contractual_rate'): money(item, key)
    for x in raw['forecast_exposures']: money(x, 'notional')
    for rel in raw['relationships']:
        for key in ('designated_proportion', 'hedged_notional', 'benchmark_rate'): money(rel, key)
    for ev in raw['events']: money(ev, 'proportion')
    return raw


def _swap_of(node, position):
    for t in node['trades']:
        if t['kind'] == 'swap' and t['request']['position_id'] == position: return t['request']
        if t['kind'] in ('european_swaption', 'expired_european'):
            pid = t.get('request', t.get('terminal', {})).get('position_id')
            _require(pid != position, 'option hedging instruments require their own reviewed lane')
    raise ValueError('hedging instrument is not a mapped raw swap')


def _year(start, end, convention):
    return 0. if start == end else _tau(start, end, convention)


def _unit_clean(item, rate, dfs, t, before):
    active = (lambda pay: pay >= t) if before else (lambda pay: pay > t)
    dirty = accrued = 0.
    for p in item['periods']:
        tau = _tau(p['start_ordinal'], p['end_ordinal'], p['day_count'])
        if active(p['payment_ordinal']):
            _require(p['payment_ordinal'] in dfs, 'missing exact hedged item discount point')
            dirty += rate * tau * dfs[p['payment_ordinal']]
            if p['end_ordinal'] <= t: accrued += rate * tau
        if p['start_ordinal'] <= t < p['end_ordinal']: accrued += rate * _year(p['start_ordinal'], t, p['day_count'])
    if active(item['principal_ordinal']):
        _require(item['principal_ordinal'] in dfs, 'missing exact principal discount point')
        dirty += dfs[item['principal_ordinal']]
    return _money(dirty - accrued)


def _hypothetical(req, t):
    value, _ = _swap(req)
    before = deepcopy(req); before['payment_timing'] = 'before_scheduled_payments'
    for p in req['floating_periods']:
        if p['payment_ordinal'] == t:
            for o in p['coupon']['observations']:
                s = o['source']
                _require(s['kind'] == 'observed' or s['kind'] == 'conditional_projection' and s['measure'] == 'physical',
                         'matured hypothetical period requires observed or Physical fixings')
    _, cash = _swap(before, due_date=t)
    return value, _money(math.fsum(c['amount'] for c in cash))


def _periods(periods, asof, label):
    _require(1 <= len(periods) <= 4096, label + ' period count')
    prior = None; ids = set()
    for p in periods:
        _require(set(p) == {'period_id', 'start_ordinal', 'end_ordinal', 'payment_ordinal', 'day_count'}, label + ' period fields')
        _require(asof <= p['start_ordinal'] < p['end_ordinal'] <= p['payment_ordinal'] and (prior is None or prior == p['start_ordinal'])
                 and p['period_id'] not in ids, label + ' periods must be chronological, contiguous and unique')
        ids.add(p['period_id']); prior = p['end_ordinal']


def _same_periods(contract, exposure):
    return len(contract) == len(exposure) and all(
        c['start_ordinal'] == e['start_ordinal'] and c['end_ordinal'] == e['end_ordinal'] and c['payment_ordinal'] == e['payment_ordinal']
        and c['day_count'] == e['day_count'] and c['fee'] == 0. for c, e in zip(contract, exposure))


def _admit(raw):
    _require(raw['schema'] == SCHEMA and raw['model_version'] == SCHEMA, 'explicit hedge-accounting-1 selection required')
    policy = raw['policy']
    _require(policy['rule_version'] == RULE, 'unsupported rule matrix')
    adoption = policy['adoption']; asu = adoption['asu_2025_09']
    _require(adoption['asu_2022_01_portfolio_layer'] is False and not any(asu['elections'].values()), 'unimplemented election')
    _require((asu['adopted'] is True) == (asu['adoption_ordinal'] is not None), 'adoption date selection')
    band = policy['assessment']
    _require(band['method'] == 'cumulative_dollar_offset' and 0. < band['lower_bound'] < band['upper_bound'], 'assessment band')
    d = raw['derivative']
    _require(raw['entity'] == d['entity'] and raw['currency'] == d['currency'] and raw['monetary_unit'] == d['monetary_unit'] == 'currency_units', 'entity/currency/unit')
    _require(d['derivative']['operation']['kind'] == 'netting', 'netting lifecycle required')
    csa = d['derivative']['operation']['request']; path = csa['paths'][0]
    _require(len(csa['paths']) == 1 and path['default'] is None, 'single Physical path without default required')
    nodes = {n['ordinal']: i for i, n in enumerate(path['nodes'])}
    asof = csa['asof_ordinal']
    items = {}; exposures = {}
    for item in raw['hedged_items']:
        _require(item['kind'] == 'fixed_rate_instrument' and item['direction'] in ('asset', 'liability') and item['item_id'] not in items, 'hedged item')
        _require(0. < _money(item['notional']), 'item notional'); _money(item['contractual_rate'])
        _periods(item['periods'], asof, 'hedged item')
        _require(item['principal_ordinal'] >= item['periods'][-1]['payment_ordinal'], 'principal date')
        items[item['item_id']] = item
    for x in raw['forecast_exposures']:
        _require(x['direction'] in ('receipt', 'payment') and x['exposure_id'] not in exposures and 0. < _money(x['notional']), 'forecast exposure')
        _periods(x['periods'], asof, 'forecast exposure'); exposures[x['exposure_id']] = x
    _require(1 <= len(raw['relationships']) <= 64 and len(raw['events']) <= 1024, 'cardinality')
    reviews = {t['position_id']: t for t in d['policy']['trades']}
    compiled = {}; proportion = {}; hedged_total = {}; work = 0
    for index, rel in enumerate(raw['relationships']):
        rid = rel['relationship_id']; _require(rid not in compiled, 'duplicate relationship')
        p = rel['designated_proportion']; _require(0. < p <= 1., 'designated proportion')
        inst = rel['hedging_instrument_position_id']; swap = _swap_of(path['nodes'][0], inst)
        _require(reviews[inst]['designation'] == 'undesignated_economic_hedge', 'raw instrument review')
        proportion[inst] = proportion.get(inst, 0.) + p; _require(proportion[inst] <= 1. + 1e-12, 'instrument over-designated')
        inception = rel['inception_ordinal']
        _require(inception in nodes and inception >= asof, 'inception node')
        _require(any(q['start_ordinal'] == inception for q in swap['fixed_periods']) and any(q['start_ordinal'] == inception for q in swap['floating_periods']), 'inception boundary')
        _require(rel['hedged_risk'] == 'benchmark_interest_rate', 'hedged risk')
        pred = rel['predecessor_relationship_id']
        if pred is not None:
            _require(pred in compiled and raw['relationships'][compiled[pred]['index']]['hedging_instrument_position_id'] == inst, 'predecessor')
        entry = dict(index=index, item=None, exposure=None, dir=1., hedged_periods=[], hyp={}, matched=False)
        if rel['designation'] == 'fair_value_hedge':
            _require(rel['forecast_exposure_id'] is None and rel['hypothetical'] is None and rel['matched_critical_terms_evidence'] is None, 'FVH inputs')
            item = items[rel['hedged_item_id']]; hedged = rel['hedged_notional']; _money(rel['benchmark_rate'])
            _require(0. < hedged <= item['notional'] + 1e-12, 'hedged notional')
            hedged_total[item['item_id']] = hedged_total.get(item['item_id'], 0.) + hedged
            _require(hedged_total[item['item_id']] <= item['notional'] + 1e-9, 'item over-hedged')
            _require(any(q['start_ordinal'] == inception for q in item['periods']), 'item inception boundary')
            entry.update(item=item, dir=1. if item['direction'] == 'asset' else -1.)
        else:
            _require(rel['designation'] == 'cash_flow_hedge' and rel['hedged_item_id'] is None and rel['hedged_notional'] is None and rel['benchmark_rate'] is None, 'CFH inputs')
            _require(reviews[inst]['coupon_recognition'] == 'settle_derivative_carrying', 'CFH coupon policy')
            x = exposures[rel['forecast_exposure_id']]
            _require(any(q['start_ordinal'] == inception for q in x['periods']), 'exposure inception boundary')
            hedged = [i for i, q in enumerate(x['periods']) if q['start_ordinal'] >= inception]; _require(hedged, 'hedged periods')
            last = path['nodes'][-1]['ordinal']
            for i in hedged: _require(x['periods'][i]['payment_ordinal'] > last or x['periods'][i]['payment_ordinal'] in nodes, 'hedged period node')
            side = 'payer' if x['direction'] == 'payment' else 'receiver'
            index_ok = lambda ps: all(q['coupon']['kind'] == 'indexed' and q['coupon']['index']['index_id'] == x['index_id'] for q in ps)
            if rel['hypothetical'] is not None:
                _require(rel['matched_critical_terms_evidence'] is None, 'hypothetical or matched')
                hyp = {}
                for hn in rel['hypothetical']['nodes']:
                    q = hn['request']; o = hn['ordinal']
                    _require(o in nodes and o >= inception and o not in hyp and q['asof_ordinal'] == o and q['payment_timing'] == 'after_scheduled_payments'
                             and path['nodes'][nodes[o]]['payment_timing'] == 'after_scheduled_payments' and q['scenario_id'] == path['path_id']
                             and q['currency'] == raw['currency'] and q['monetary_unit'] == raw['monetary_unit'] and _near(q['notional'], x['notional'])
                             and q['side'] == side and _same_periods(q['floating_periods'], x['periods']) and index_ok(q['floating_periods']), 'hypothetical mirror')
                    hyp[o] = q; work += len(q['fixed_periods']) + sum(1 + len(f['coupon']['observations']) for f in q['floating_periods'])
                _require(all(n['ordinal'] in hyp for n in path['nodes'][nodes[inception]:]), 'hypothetical coverage')
                entry.update(hyp=hyp)
            else:
                _require(rel['matched_critical_terms_evidence'] is not None, 'hypothetical or matched')
                _require(_near(p * swap['notional'], x['notional']) and swap['side'] == side and _same_periods(swap['floating_periods'], x['periods'])
                         and index_ok(swap['floating_periods']), 'matched critical terms')
                entry.update(matched=True)
            entry.update(exposure=x, hedged_periods=hedged)
        compiled[rid] = entry
    _require(work <= 1_000_000, 'hypothetical work')
    prior = None; ids = set()
    for ev in raw['events']:
        clock = (ev['ordinal'], ev['sequence'])
        _require(ev['event_id'] not in ids and (prior is None or prior < clock), 'event order'); ids.add(ev['event_id']); prior = clock
        rel = raw['relationships'][compiled[ev['relationship_id']]['index']]; c = compiled[ev['relationship_id']]
        _require(ev['ordinal'] in nodes and ev['ordinal'] >= rel['inception_ordinal'], 'event node')
        swap = _swap_of(path['nodes'][0], rel['hedging_instrument_position_id'])
        extra = {k: ev.get(k) for k in ('proportion', 'reason', 'status')}
        if ev['kind'] == 'assess':
            _require(all(v is None for v in extra.values()), 'assessment fields')
            final = max(q['payment_ordinal'] for q in swap['fixed_periods'] + swap['floating_periods'])
            periods = (c['item'] or c['exposure'])['periods']
            _require(rel['inception_ordinal'] < ev['ordinal'] < final and path['nodes'][nodes[ev['ordinal']]]['payment_timing'] == 'after_scheduled_payments'
                     and any(q['end_ordinal'] == ev['ordinal'] for q in swap['fixed_periods']) and any(q['end_ordinal'] == ev['ordinal'] for q in swap['floating_periods'])
                     and any(q['end_ordinal'] == ev['ordinal'] for q in periods), 'assessment date')
        elif ev['kind'] == 'discontinue':
            _require(extra['reason'] in ('dedesignation', 'hedged_item_sold', 'instrument_terminated') and extra['status'] is None, 'discontinuation fields')
            _require(extra['proportion'] is None or 0. < extra['proportion'] <= 1., 'discontinued proportion')
        else:
            _require(ev['kind'] == 'forecast_status' and rel['designation'] == 'cash_flow_hedge' and extra['status'] in ('no_longer_probable', 'probable_not_to_occur')
                     and extra['proportion'] is None and extra['reason'] is None, 'forecast status fields')
    return compiled, csa, path, nodes


def _state(rel):
    return dict(status='pending', scale=1., proportion=rel['designated_proportion'], unit=None, hyp=None, inst=None, cum_d=0., cum_e=0., basis=0.,
                frozen_basis=[], oci=0., frozen_oci=[], basis_adjustments=0., basis_amortized=0., basis_derecognized=0., oci_recognized=0.,
                oci_reclassified=0., last_clean=None, last_hyp=None)


class _Journal:
    def __init__(self, raw, scenario):
        self.raw = raw; self.scenario = scenario; self.intents = []
        o = raw['derivative']['opening']
        self.gl = {'cash': _money(o['cash']), 'posted_vm_asset': _money(o['posted_vm_asset']), 'received_vm_liability': -_money(o['received_vm_liability']),
                   'posted_im_asset': _money(o['posted_im_asset']), 'opening_equity': -_money(o['equity_credit_balance'])}
        for t in o['trades']:
            self.gl['derivative_asset'] = self.gl.get('derivative_asset', 0.) + max(t['clean_value'], 0.)
            self.gl['derivative_liability'] = self.gl.get('derivative_liability', 0.) - max(-t['clean_value'], 0.)
        self.memo = {'memo_custodian_asset': _money(o['received_im_custody']), 'memo_custodian_obligation': -_money(o['received_im_custody'])}

    def emit(self, kind, date, ledger, mapped, owner, bucket, relationship, policy, sources, postings):
        postings = [p for p in postings if p['debit_amount'] != 0.]
        if not postings: return
        _require(abs(math.fsum(p['debit_amount'] for p in postings)) <= max(1e-9, 64 * 2.220446049250313e-16 * max(abs(p['debit_amount']) for p in postings)), 'unbalanced hedge intent ' + kind)
        balances = self.gl if ledger == 'financial' else self.memo
        for p in postings: balances[p['account']] = _money(balances.get(p['account'], 0.) + p['debit_amount'])
        r = self.raw
        self.intents.append(dict(event_id=f'{kind}:{len(self.intents)}', ordinal=date, ledger=ledger, run=deepcopy(r['run']), scenario_id=self.scenario,
                                 entity=r['entity'], currency=r['currency'], policy=deepcopy(policy), sources=deepcopy(sources), mapped_position_id=mapped,
                                 contract_position_id=owner, bucket=bucket, relationship_id=relationship, measure='physical',
                                 basis=r['derivative']['basis'], postings=postings))

    def balances(self, which):
        return [dict(account=a, debit_balance=float(which[a])) for a in ACCOUNTS if a in which]


def _posting(account, value, owner=None, bucket=None, relationship=None):
    return dict(account=account, position_id=owner, bucket=bucket, relationship_id=relationship, debit_amount=float(value))


def _record(t, rel, st, **fields):
    base = dict(ordinal=t, sequence=None, relationship_id=rel['relationship_id'], kind='', event_id=None, review=None, reason=None,
                status_before=st['status'], status_after=st['status'], proportion_before=st['proportion'], proportion_after=st['proportion'],
                frozen_basis=0., frozen_oci=0., released_oci=0., derecognized_basis=0.)
    base.update(fields); return base


def replay(raw, check=lambda: False):
    check()
    raw = _typed(raw)
    compiled, csa, path, node_index = _admit(raw)
    check()
    derivative = _derive(raw['derivative'])
    check()
    _require(len(derivative['daily']) == len(path['nodes']), 'derivative daily coverage')
    journal = _Journal(raw, path['path_id'])
    band = raw['policy']['assessment']; hedge_policy = raw['policy']['identity']
    rels = raw['relationships']; states = [_state(r) for r in rels]; comp = [compiled[r['relationship_id']] for r in rels]
    lifecycle = []; assessments = []; daily = []
    cursor = 0; event_cursor = 0; intents = derivative['intents']

    def instrument_value(k, position):
        for x in derivative['daily'][k]['trades']:
            if x['mapped_position_id'] == position: return x['clean_carrying']
        raise ValueError('instrument missing from derivative closing')

    def two(account_a, value, owner, rid, account_b):
        return [_posting(account_a, value, owner, None, rid), _posting(account_b, -value, owner, None, rid)]

    def discontinue(i, t, q, reason, review, record):
        rel, st, c = rels[i], states[i], comp[i]
        _require(st['status'] == 'qualified', 'discontinuation requires a qualified relationship')
        sources = [deepcopy(hedge_policy), deepcopy(rel['documentation']), deepcopy(review)]
        inst = rel['hedging_instrument_position_id']; rid = rel['relationship_id']
        if rel['designation'] == 'fair_value_hedge':
            item = c['item']; frozen = _money(q * st['basis']); st['basis'] = _money(st['basis'] - frozen)
            if reason == 'hedged_item_sold':
                journal.emit('hedged_item_basis_derecognition', t, 'financial', inst, item['item_id'], None, rid, hedge_policy, sources,
                             two('hedged_item_basis_adjustment', -c['dir'] * frozen, item['item_id'], rid, 'hedged_item_basis_derecognition'))
                st['basis_derecognized'] = _money(st['basis_derecognized'] + frozen); record['derecognized_basis'] = frozen
            elif frozen != 0.:
                st['frozen_basis'].append(dict(amount=frozen, start=t, amortized=0.)); record['frozen_basis'] = frozen
        else:
            x = c['exposure']; frozen = _money(q * st['oci']); st['oci'] = _money(st['oci'] - frozen)
            remaining = [(x['periods'][k]['payment_ordinal'], _tau(x['periods'][k]['start_ordinal'], x['periods'][k]['end_ordinal'], x['periods'][k]['day_count']))
                         for k in c['hedged_periods'] if x['periods'][k]['payment_ordinal'] > t]
            total = math.fsum(w for _, w in remaining)
            if not remaining or total <= 0.:
                if frozen != 0.:
                    journal.emit('frozen_oci_release', t, 'financial', inst, x['exposure_id'], None, rid, hedge_policy, sources,
                                 two('cash_flow_hedge_oci', frozen, x['exposure_id'], rid, 'hedged_forecast_reclassification'))
                    st['oci_reclassified'] = _money(st['oci_reclassified'] + frozen); record['released_oci'] = frozen
            elif frozen != 0.:
                st['frozen_oci'].append(dict(amount=frozen, released=0., weights=[(pay, w / total) for pay, w in remaining])); record['frozen_oci'] = frozen
        st['scale'] *= 1. - q; st['proportion'] = _money(rel['designated_proportion'] * st['scale'])
        if q >= 1.: st['scale'] = 0.; st['proportion'] = 0.; st['status'] = 'discontinued'
        record.update(reason=reason, status_after=st['status'], proportion_after=st['proportion'])

    for k, node in enumerate(path['nodes']):
        check(); t = node['ordinal']
        shares = [(i, states[i]['proportion']) for i, r in enumerate(rels) if r['designation'] == 'cash_flow_hedge' and states[i]['status'] == 'qualified']
        _require(cursor >= len(intents) or intents[cursor]['ordinal'] >= t, 'derivative intents out of node order')
        while cursor < len(intents) and intents[cursor]['ordinal'] == t:
            di = intents[cursor]; cursor += 1
            mark = di['event_id'].startswith('clean_mark:'); postings = []; relationship = None
            for p in di['postings']:
                if mark and p['account'] == 'mark_gain_loss':
                    m = _money(p['debit_amount']); mapped = di['mapped_position_id']; retained = 1.
                    for i, share in shares:
                        if rels[i]['hedging_instrument_position_id'] != mapped: continue
                        value = _money(m * share)
                        postings.append(_posting('cash_flow_hedge_oci', value, p['position_id'], p['bucket'], rels[i]['relationship_id']))
                        states[i]['oci'] = _money(states[i]['oci'] - value); states[i]['oci_recognized'] = _money(states[i]['oci_recognized'] - value)
                        retained -= share; relationship = rels[i]['relationship_id']
                    postings.append(_posting('mark_gain_loss', _money(m * retained), p['position_id'], p['bucket']))
                else:
                    postings.append(_posting(p['account'], p['debit_amount'], p['position_id'], p['bucket']))
            journal.emit(di['event_id'].split(':')[0], t, di['ledger'], di['mapped_position_id'], di['contract_position_id'], di['bucket'], relationship,
                         di['policy'], di['sources'], postings)
        for i, rel in enumerate(rels):
            st, c = states[i], comp[i]; inst = rel['hedging_instrument_position_id']; rid = rel['relationship_id']
            base = lambda extra: [deepcopy(hedge_policy), deepcopy(rel['documentation'])] + [deepcopy(e) for e in extra]
            if t >= rel['inception_ordinal']:
                value = instrument_value(k, inst)
                if st['status'] == 'qualified': st['cum_d'] = _money(st['cum_d'] + st['proportion'] * (value - st['inst']))
                st['inst'] = value
            if rel['designation'] == 'fair_value_hedge':
                item = c['item']; hedged = rel['hedged_notional']; rate = rel['benchmark_rate']
                if t >= rel['inception_ordinal']:
                    swap = _swap_of(node, inst)
                    dfs = _discount(swap['discount_curve'], t, raw['currency'], swap['valuation_cutoff_unix_seconds'])
                    before = _unit_clean(item, rate, dfs, t, True); unit = _unit_clean(item, rate, dfs, t, False)
                    if st['status'] == 'qualified':
                        d_c = _money(hedged * st['scale'] * (before - st['unit']))
                        if d_c != 0.:
                            journal.emit('hedged_item_basis', t, 'financial', inst, item['item_id'], None, rid, hedge_policy,
                                         base([item['source'], item['carrying_source']]),
                                         two('hedged_item_basis_adjustment', c['dir'] * d_c, item['item_id'], rid, 'hedged_item_basis_gain_loss'))
                            st['basis'] = _money(st['basis'] + d_c); st['basis_adjustments'] = _money(st['basis_adjustments'] + d_c)
                            st['cum_e'] = _money(st['cum_e'] - c['dir'] * d_c)
                    st['unit'] = unit; st['last_clean'] = _money(hedged * st['scale'] * unit)
                for tranche in st['frozen_basis']:
                    if item['principal_ordinal'] <= tranche['start']: target = tranche['amount']
                    else: target = _money(tranche['amount'] * min(1., (t - tranche['start']) / (item['principal_ordinal'] - tranche['start'])))
                    delta = _money(target - tranche['amortized'])
                    if delta != 0.:
                        journal.emit('hedged_item_basis_amortization', t, 'financial', inst, item['item_id'], None, rid, hedge_policy,
                                     base([item['source'], item['carrying_source']]),
                                     two('hedged_item_basis_adjustment', -c['dir'] * delta, item['item_id'], rid, 'hedged_item_basis_amortization'))
                        tranche['amortized'] = target; st['basis_amortized'] = _money(st['basis_amortized'] + delta)
            else:
                x = c['exposure']
                if t >= rel['inception_ordinal']:
                    if c['matched']:
                        physical = math.fsum(r['physical_amount'] for r in derivative['cash_rights'] if r['position_id'] == inst and r['payment_ordinal'] == t)
                        h, settlement, evidence = instrument_value(k, inst), _money(rel['designated_proportion'] * physical), rel['matched_critical_terms_evidence']
                    else:
                        h, settlement = _hypothetical(c['hyp'][t], t); evidence = rel['hypothetical']['evidence']
                        if t == rel['inception_ordinal']: _require(_near(h, 0.), 'hypothetical must be at market at inception')
                    if st['status'] == 'qualified':
                        st['cum_e'] = _money(st['cum_e'] + st['scale'] * (h - st['hyp']))
                        if any(x['periods'][q]['payment_ordinal'] == t for q in c['hedged_periods']):
                            reclass = _money(st['scale'] * settlement)
                            if reclass != 0.:
                                journal.emit('cash_flow_hedge_reclassification', t, 'financial', inst, x['exposure_id'], None, rid, hedge_policy,
                                             base([x['source'], x['probability_evidence'], evidence]),
                                             two('cash_flow_hedge_oci', reclass, x['exposure_id'], rid, 'hedged_forecast_reclassification'))
                                st['oci'] = _money(st['oci'] - reclass); st['oci_reclassified'] = _money(st['oci_reclassified'] + reclass)
                    st['hyp'] = h; st['last_hyp'] = _money(st['scale'] * h)
                for tranche in st['frozen_oci']:
                    last = max(pay for pay, _ in tranche['weights']); release = 0.
                    for pay, w in tranche['weights']:
                        if pay == t: release = _money(tranche['amount'] - tranche['released']) if last == t else _money(tranche['amount'] * w)
                    if release != 0.:
                        journal.emit('frozen_oci_release', t, 'financial', inst, x['exposure_id'], None, rid, hedge_policy,
                                     base([x['source'], x['probability_evidence']]),
                                     two('cash_flow_hedge_oci', release, x['exposure_id'], rid, 'hedged_forecast_reclassification'))
                        tranche['released'] = _money(tranche['released'] + release); st['oci_reclassified'] = _money(st['oci_reclassified'] + release)
        for i, rel in enumerate(rels):
            if rel['inception_ordinal'] != t: continue
            if rel['predecessor_relationship_id'] is not None:
                prior = states[compiled[rel['predecessor_relationship_id']]['index']]
                _require(prior['status'] == 'discontinued' and prior['proportion'] == 0., 'predecessor must be fully discontinued')
            if comp[i]['matched']: _require(_near(instrument_value(k, rel['hedging_instrument_position_id']), 0.), 'matched terms require zero inception value')
            st = states[i]; _require(st['status'] == 'pending', 'activated twice'); st['status'] = 'qualified'
            lifecycle.append(_record(t, rel, st, kind='inception', review=deepcopy(rel['documentation']), status_before='pending', status_after='qualified',
                                     proportion_before=0., proportion_after=st['proportion']))
        while event_cursor < len(raw['events']) and raw['events'][event_cursor]['ordinal'] == t:
            ev = raw['events'][event_cursor]; event_cursor += 1
            i = compiled[ev['relationship_id']]['index']; rel = rels[i]; st = states[i]; c = comp[i]
            record = _record(t, rel, st, sequence=ev['sequence'], event_id=ev['event_id'], review=deepcopy(ev['review']))
            if ev['kind'] == 'assess':
                record['kind'] = 'assessment'
                _require(st['status'] == 'qualified', 'assessment on a relationship that is not qualified')
                d, e = st['cum_d'], st['cum_e']; _require(not (d == 0. and e == 0.), 'nothing to assess')
                ratio = None if e == 0. else d / e
                qualified = ratio is not None and band['lower_bound'] <= ratio <= band['upper_bound']
                assessments.append(dict(event_id=ev['event_id'], relationship_id=rel['relationship_id'], ordinal=t, method=band['method'],
                                        lower_bound=band['lower_bound'], upper_bound=band['upper_bound'], cumulative_instrument_change=d,
                                        cumulative_hedged_change=e, ratio=ratio, qualified=qualified))
                if qualified: record['reason'] = 'qualified'
                else: discontinue(i, t, 1., 'failed_assessment', ev['review'], record)
            elif ev['kind'] == 'discontinue':
                record['kind'] = 'discontinuation'
                discontinue(i, t, ev.get('proportion') or 1., ev['reason'], ev['review'], record)
            else:
                record['kind'] = 'forecast_status'
                _require(st['status'] != 'pending', 'forecast status before inception')
                if ev['status'] == 'no_longer_probable':
                    if st['status'] == 'qualified': discontinue(i, t, 1., 'forecast_no_longer_probable', ev['review'], record)
                    else: record['reason'] = 'forecast_no_longer_probable'
                else:
                    x = c['exposure']; release = st['oci']
                    for tranche in st['frozen_oci']:
                        release = _money(release + tranche['amount'] - tranche['released']); tranche['released'] = tranche['amount']
                    st['oci'] = 0.
                    if release != 0.:
                        journal.emit('forecast_not_occurring_release', t, 'financial', rel['hedging_instrument_position_id'], x['exposure_id'], None,
                                     rel['relationship_id'], hedge_policy, [deepcopy(hedge_policy), deepcopy(rel['documentation']), deepcopy(ev['review'])],
                                     two('cash_flow_hedge_oci', release, x['exposure_id'], rel['relationship_id'], 'hedged_forecast_reclassification'))
                        st['oci_reclassified'] = _money(st['oci_reclassified'] + release)
                    st['scale'] = 0.; st['proportion'] = 0.; st['status'] = 'discontinued'
                    record.update(reason='forecast_probable_not_to_occur', released_oci=release, status_after='discontinued', proportion_after=0.)
            lifecycle.append(record)
        closing = derivative['daily'][k]
        for b in closing['financial_trial_balance']:
            mine = journal.gl.get(b['account'], 0.)
            if b['account'] == 'mark_gain_loss':
                mine += journal.gl.get('cash_flow_hedge_oci', 0.) - math.fsum(s['oci_reclassified'] for s in states)
            _require(_near(mine, b['debit_balance']), 'derivative owner balance ' + b['account'])
        for b in closing['custody_trial_balance']: _require(_near(journal.memo.get(b['account'], 0.), b['debit_balance']), 'custody balance')
        _require(_near(math.fsum(journal.gl.values()), 0.) and _near(math.fsum(journal.memo.values()), 0.), 'trial balance')
        rows = []
        for i, rel in enumerate(rels):
            st = states[i]
            value = _money(st['proportion'] * instrument_value(k, rel['hedging_instrument_position_id'])) if t >= rel['inception_ordinal'] else 0.
            rows.append(dict(relationship_id=rel['relationship_id'], status=st['status'], designated_proportion=st['proportion'],
                             instrument_designated_value=value, hedged_item_clean_value=st['last_clean'], hypothetical_value=st['last_hyp'],
                             cumulative_instrument_change=st['cum_d'], cumulative_hedged_change=st['cum_e'], basis_adjustment_balance=st['basis'],
                             frozen_basis_balance=math.fsum(x['amount'] - x['amortized'] for x in st['frozen_basis']), oci_balance=st['oci'],
                             frozen_oci_balance=math.fsum(x['amount'] - x['released'] for x in st['frozen_oci'])))
        daily.append(dict(ordinal=t, relationships=rows, financial_trial_balance=journal.balances(journal.gl), custody_trial_balance=journal.balances(journal.memo)))
    _require(cursor == len(intents) and event_cursor == len(raw['events']), 'intents or events beyond the tail')
    last = path['nodes'][-1]['ordinal']; relationships = []; closing = []
    for i, rel in enumerate(rels):
        st, c = states[i], comp[i]
        pending = [c['exposure']['periods'][q]['period_id'] for q in c['hedged_periods'] if c['exposure']['periods'][q]['payment_ordinal'] > last] if c['exposure'] else []
        frozen_basis = math.fsum(x['amount'] - x['amortized'] for x in st['frozen_basis']); frozen_oci = math.fsum(x['amount'] - x['released'] for x in st['frozen_oci'])
        relationships.append(dict(relationship_id=rel['relationship_id'], predecessor_relationship_id=rel['predecessor_relationship_id'], designation=rel['designation'],
                                  hedging_instrument_position_id=rel['hedging_instrument_position_id'], hedged_item_id=rel['hedged_item_id'],
                                  forecast_exposure_id=rel['forecast_exposure_id'], inception_ordinal=rel['inception_ordinal'], status=st['status'],
                                  designated_proportion=st['proportion'], basis_adjustment_balance=st['basis'], frozen_basis_balance=frozen_basis,
                                  oci_balance=st['oci'], frozen_oci_balance=frozen_oci, pending_reclassification_periods=pending))
        closing.append(dict(relationship_id=rel['relationship_id'], basis_adjustments=st['basis_adjustments'], basis_amortized=st['basis_amortized'],
                            basis_derecognized=st['basis_derecognized'], basis_closing=_money(st['basis'] + frozen_basis), oci_recognized=st['oci_recognized'],
                            oci_reclassified=st['oci_reclassified'], oci_closing=_money(st['oci'] + frozen_oci)))
        _require(_near(st['basis_adjustments'] - st['basis_amortized'] - st['basis_derecognized'], st['basis'] + frozen_basis)
                 and _near(st['oci_recognized'] - st['oci_reclassified'], st['oci'] + frozen_oci), 'roll-forward')
    check()
    return dict(schema=SCHEMA, model_version=SCHEMA, rule_version=RULE, run=deepcopy(raw['run']), entity=raw['entity'], currency=raw['currency'],
                monetary_unit=raw['monetary_unit'], policy=deepcopy(raw['policy']), derivative=derivative, relationships=relationships,
                lifecycle=lifecycle, assessments=assessments, daily=daily, intents=journal.intents, closing=closing, **FLAGS)


def _compare(a, b, path='result'):
    if type(a) is dict:
        _require(type(b) is dict and set(a) == set(b), 'hedge result fields differ at ' + path)
        for k in a: _compare(a[k], b[k], path + '.' + k)
    elif type(a) is list:
        _require(type(b) is list and len(a) == len(b), 'hedge result rows differ at ' + path)
        for i, (x, y) in enumerate(zip(a, b, strict=True)): _compare(x, y, f'{path}[{i}]')
    elif type(a) is float:
        _require(type(b) in (int, float) and type(b) is not bool and math.isfinite(b) and math.isclose(a, b, rel_tol=1e-12, abs_tol=1e-9), 'hedge amount differs at ' + path)
    else:
        _require(exact_json_identity(a, b), 'hedge identity/state differs at ' + path)


def verify(raw, result, check=lambda: False):
    expected = replay(raw, check)
    _require(type(result) is dict and set(result) == set(expected) | {'disclosure'}, 'hedge result field set differs')
    _require(type(result.get('disclosure')) is str and result['disclosure'].strip() != '', 'disclosure required')
    # The embedded derivative evidence is checked by the existing raw derivative reference.
    _require(verify_derivative_accounting(raw['derivative'], result['derivative'])['verified'] is True, 'embedded derivative source differs')
    actual = {k: v for k, v in result.items() if k not in ('disclosure', 'derivative')}
    expected = {k: v for k, v in expected.items() if k != 'derivative'}
    # Rounding-level intents are zero events; both journals are compared after the same normalization.
    for side in (expected, actual):
        side['intents'] = normalize_intents(side['intents'])
    _compare(expected, actual)
    check()
    return dict(verified=True, formal_hedge_relationships_replayed=True, derivative_source_recomputed=True, assessments_recomputed=True,
                ordered_hedge_intents_replayed=True, **FLAGS)
