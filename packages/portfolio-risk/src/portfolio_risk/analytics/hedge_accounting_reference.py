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
             saved_book_adoption=False, portfolio_layer_supported=True, net_investment_supported=False,
             asu_2025_09_similar_risk_supported=True, asu_2025_09_selectable_rate_supported=True,
             asu_2025_09_swap_written_option_supported=True, seasoned_openings_supported=True)
SIMILAR_METHODS = ('instrument_highly_effective_each_risk', 'hedged_risks_similar_to_each_other')


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


def _position(t):
    return (t['terminal'] if t['kind'] == 'expired_european' else t['request'])['position_id']


def _swap_of(node, position):
    for t in node['trades']:
        if t['kind'] == 'swap' and t['request']['position_id'] == position: return t['request']
        if t['kind'] in ('european_swaption', 'expired_european'):
            pid = t.get('request', t.get('terminal', {})).get('position_id')
            _require(pid != position, 'option hedging instruments require their own reviewed lane')
    raise ValueError('hedging instrument is not a mapped raw swap')


def _year(start, end, convention):
    return 0. if start == end else _tau(start, end, convention)


def _unit_clean(item, rate, dfs, t, before, term_end=None):
    active = (lambda pay: pay >= t) if before else (lambda pay: pay > t)
    term_end = item['principal_ordinal'] if term_end is None else term_end
    dirty = accrued = 0.
    for p in item['periods']:
        if p['end_ordinal'] > term_end: continue
        tau = _tau(p['start_ordinal'], p['end_ordinal'], p['day_count'])
        if active(p['payment_ordinal']):
            _require(p['payment_ordinal'] in dfs, 'missing exact hedged item discount point')
            dirty += rate * tau * dfs[p['payment_ordinal']]
            if p['end_ordinal'] <= t: accrued += rate * tau
        if p['start_ordinal'] <= t < p['end_ordinal']: accrued += rate * _year(p['start_ordinal'], t, p['day_count'])
    if active(term_end):
        _require(term_end in dfs, 'missing exact principal discount point')
        dirty += dfs[term_end]
    return _money(dirty - accrued)


def _unit_clean_amortizing(item, rate, dfs, t, before, schedule, hedged):
    """Amortizing portfolio layer: coupons on the amount outstanding each period, principal as it amortizes."""
    active = (lambda pay: pay >= t) if before else (lambda pay: pay > t)
    first = schedule[0]['amount']; by_end = {x['period_end_ordinal']: x['amount'] for x in schedule}
    last_end = schedule[-1]['period_end_ordinal']
    dirty = accrued = 0.
    for p in item['periods']:
        if p['end_ordinal'] > last_end: continue
        f = by_end.get(p['end_ordinal'], first) / hedged
        tau = _tau(p['start_ordinal'], p['end_ordinal'], p['day_count'])
        if active(p['payment_ordinal']):
            _require(p['payment_ordinal'] in dfs, 'missing exact hedged item discount point')
            dirty += f * rate * tau * dfs[p['payment_ordinal']]
            if p['end_ordinal'] <= t: accrued += f * rate * tau
        if p['start_ordinal'] <= t < p['end_ordinal']: accrued += f * rate * _year(p['start_ordinal'], t, p['day_count'])
    for k, x in enumerate(schedule):
        repaid = (x['amount'] - (schedule[k + 1]['amount'] if k + 1 < len(schedule) else 0.)) / hedged
        if repaid != 0. and active(x['period_end_ordinal']):
            _require(x['period_end_ordinal'] in dfs, 'missing exact layer amortization discount point')
            dirty += repaid * dfs[x['period_end_ordinal']]
    return _money(dirty - accrued)


def _layer_amount(rel, item, at):
    """Layer amount outstanding at `at` before breach scaling; zero from the layer end."""
    end = rel.get('hedged_term_end_ordinal') or item['principal_ordinal']
    if at >= end: return 0.
    schedule = rel.get('layer_amortization')
    if schedule is None: return rel['hedged_notional']
    return next((x['amount'] for x in schedule if at < x['period_end_ordinal']), 0.)


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
        _require(asof - 36600 <= p['start_ordinal'] < p['end_ordinal'] <= p['payment_ordinal'] <= asof + 36600 and (prior is None or prior == p['start_ordinal'])
                 and p['period_id'] not in ids, label + ' periods must be chronological, contiguous and unique')
        ids.add(p['period_id']); prior = p['end_ordinal']


def _accrual_free(periods, t):
    """True when t lies strictly inside no period (starts, ends and forward dates are accrual-free)."""
    return not any(p['start_ordinal'] < t < p['end_ordinal'] for p in periods)


def _same_periods(contract, exposure):
    return len(contract) == len(exposure) and all(
        c['start_ordinal'] == e['start_ordinal'] and c['end_ordinal'] == e['end_ordinal'] and c['payment_ordinal'] == e['payment_ordinal']
        and c['day_count'] == e['day_count'] and c['fee'] == 0. for c, e in zip(contract, exposure))


def _admit(raw):
    _require(raw['schema'] == SCHEMA and raw['model_version'] == SCHEMA, 'explicit hedge-accounting-1 selection required')
    policy = raw['policy']
    _require(policy['rule_version'] == RULE, 'unsupported rule matrix')
    adoption = policy['adoption']; asu = adoption['asu_2025_09']; elections = asu['elections']
    _require(elections['dual_hedge_basis'] is False, 'unimplemented election')
    layer_policy = policy.get('portfolio_layer')
    _require((adoption['asu_2022_01_portfolio_layer'] is True) == (layer_policy is not None), 'portfolio-layer election and reviewed breach policy')
    _require(layer_policy is None or set(layer_policy) == {'identity', 'breach_order'} and layer_policy['breach_order'] in ('last_designated_first', 'pro_rata'), 'portfolio-layer policy')
    _require((asu['adopted'] is True) == (asu['adoption_ordinal'] is not None), 'adoption date selection')

    def elected(flag, inception, label):
        # Prospective application (815-20-65-7): only relationships designated at or after adoption.
        _require(asu['adopted'] is True and flag is True and inception >= asu['adoption_ordinal'], label + ' requires the adopted election')
    band = policy['assessment']
    _require(band['method'] == 'cumulative_dollar_offset' and 0. < band['lower_bound'] < band['upper_bound'], 'assessment band')
    timing = policy.get('forecast_timing')
    _require(timing is None or type(timing.get('additional_delay_days')) is int and 1 <= timing['additional_delay_days'] <= 366, 'forecast timing policy')
    d = raw['derivative']
    _require(raw['entity'] == d['entity'] and raw['currency'] == d['currency'] and raw['monetary_unit'] == d['monetary_unit'] == 'currency_units', 'entity/currency/unit')
    _require(d['derivative']['operation']['kind'] == 'netting', 'netting lifecycle required')
    csa = d['derivative']['operation']['request']; path = csa['paths'][0]
    _require(len(csa['paths']) == 1 and path['default'] is None, 'single Physical path without default required')
    nodes = {n['ordinal']: i for i, n in enumerate(path['nodes'])}
    asof = csa['asof_ordinal']
    items = {}; exposures = {}
    for item in raw['hedged_items']:
        _require(item['kind'] in ('fixed_rate_instrument', 'closed_portfolio') and item['direction'] in ('asset', 'liability') and item['item_id'] not in items, 'hedged item')
        portfolio = item.get('portfolio')
        _require((item['kind'] == 'closed_portfolio') == (portfolio is not None), 'portfolio block belongs to closed portfolios')
        if portfolio is not None:
            _require(adoption['asu_2022_01_portfolio_layer'] is True and item['direction'] == 'asset', 'closed portfolio requires the adopted method on assets')
            ids = [m['asset_id'] for m in portfolio['members']]
            _require(0 < len(ids) <= 4096 and len(set(ids)) == len(ids) and all(type(m['prepayable']) is bool for m in portfolio['members']), 'closed portfolio membership')
            _require(len(portfolio['balances']) <= 8192 and len(portfolio['projections']) <= 8192, 'portfolio observation count')
            prior = None
            for b in portfolio['balances']:
                v = _money(b['outstanding'])
                _require(0. <= v <= item['notional'] + 1e-9 and (prior is None or b['ordinal'] > prior[0] and v <= prior[1] + 1e-9), 'closed portfolio balances')
                prior = (b['ordinal'], v)
            prior = None
            for x in portfolio['projections']:
                v = _money(x['anticipated'])
                _require(0. <= v <= item['notional'] + 1e-9 and x['term_end_ordinal'] > x['ordinal'] and (prior is None or (x['ordinal'], x['term_end_ordinal']) > prior), 'portfolio projections')
                prior = (x['ordinal'], x['term_end_ordinal'])
        _require(0. < _money(item['notional']), 'item notional'); _money(item['contractual_rate'])
        _periods(item['periods'], asof, 'hedged item')
        _require(item['principal_ordinal'] >= item['periods'][-1]['payment_ordinal'], 'principal date')
        items[item['item_id']] = item
    for x in raw['forecast_exposures']:
        _require(x['direction'] in ('receipt', 'payment') and x['exposure_id'] not in exposures and 0. < _money(x['notional']), 'forecast exposure')
        _periods(x['periods'], asof, 'forecast exposure'); exposures[x['exposure_id']] = x
        cyr = x.get('choose_your_rate')
        if cyr is not None:
            options = cyr['documented_index_ids']
            _require(2 <= len(options) <= 16 and len(set(options)) == len(options) and x['index_id'] in options
                     and x['direction'] == 'payment' and cyr['debt_classification'] == 'liability', 'choose-your-rate exposure')
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
        option = rel.get('written_option_position_id')
        if option is not None:
            # 815-20-25-88(a)-(c): written cash-settled swaption of matching notional combined with the swap.
            elected(elections['swap_written_option_exception'], inception, 'swap/written-option combination')
            _require(option != inst and reviews[option]['designation'] == 'undesignated_economic_hedge'
                     and (rel['designation'] != 'cash_flow_hedge' or reviews[option]['coupon_recognition'] == 'settle_derivative_carrying'), 'written option review')
            proportion[option] = proportion.get(option, 0.) + p; _require(proportion[option] <= 1. + 1e-12, 'option over-designated')
            for node in path['nodes']:
                trade = next((t for t in node['trades'] if _position(t) == option), None)
                _require(trade is not None and trade['kind'] in ('european_swaption', 'expired_european'), 'written option on every node')
                if trade['kind'] == 'expired_european':
                    _require(node['ordinal'] >= trade['terminal']['expiry_ordinal'], 'expired written option after expiry')
                    u = trade['delivered_underlying']
                    _require(u is None or _near(u['notional'], swap['notional']) and reviews[option]['delivery_review'] is not None, 'delivered written option terms')
                o = trade.get('request', trade.get('terminal'))
                _require(o['side'] == 'short' and all(_near(s['underlying']['notional'], swap['notional']) for s in o['states']), 'written option terms')
        opening = rel.get('opening'); seasoned = inception < asof
        _require(seasoned == (opening is not None), 'opening block exactly for relationships designated before the opening date')
        if opening is not None:
            for key in ('hedged_value', 'cumulative_instrument_change', 'cumulative_hedged_change', 'basis_balance', 'oci_balance'): _money(opening[key])
            _require(rel.get('group') is None and rel.get('rate_hypotheticals') is None and rel['predecessor_relationship_id'] is None, 'seasoned opening combinations')
            scale = opening.get('scale'); scale = 1. if scale is None else _money(scale); _require(0. < scale <= 1., 'seasoned opening scale')
            fb = opening.get('frozen_basis') or []; fo = opening.get('frozen_oci') or []
            for t_ in fb:
                _money(t_['amount']); _money(t_['amortized'])
                _require(asof - 36600 <= t_['start_ordinal'] < asof and t_['amortized'] * t_['amount'] >= 0. and abs(t_['amortized']) <= abs(t_['amount']) + 1e-12, 'frozen basis tranche')
            for t_ in fo:
                _money(t_['amount']); _money(t_['released'])
                _require(t_['released'] * t_['amount'] >= 0. and abs(t_['released']) <= abs(t_['amount']) + 1e-12, 'frozen OCI tranche')
                pays = [e['payment_ordinal'] for e in t_['entries']]
                _require(all(p_ >= asof for p_ in pays) and pays == sorted(set(pays)) and all(e['weight'] >= 0. for e in t_['entries']), 'frozen OCI entries')
                _require(t_['amount'] == 0. or _near(math.fsum(e['weight'] for e in t_['entries']), 1. - t_['released'] / t_['amount']), 'frozen OCI weights')
            _require((opening['oci_balance'] == 0. and not fo) if rel['designation'] == 'fair_value_hedge' else (opening['basis_balance'] == 0. and not fb), 'seasoned opening balance kind')
        else: _require(inception in nodes, 'inception node')
        components = [swap]
        for extra_id in rel.get('additional_instrument_position_ids') or []:
            _require(extra_id != inst and extra_id != rel.get('written_option_position_id') and all(s['position_id'] != extra_id for s in components), 'distinct additional instruments')
            extra = _swap_of(path['nodes'][0], extra_id)
            _require(reviews[extra_id]['designation'] == 'undesignated_economic_hedge'
                     and (rel['designation'] != 'cash_flow_hedge' or reviews[extra_id]['coupon_recognition'] == 'settle_derivative_carrying'), 'additional instrument review')
            proportion[extra_id] = proportion.get(extra_id, 0.) + p; _require(proportion[extra_id] <= 1. + 1e-12, 'additional instrument over-designated')
            components.append(extra)
        for s in components:
            last_end = max(q['end_ordinal'] for q in s['fixed_periods'] + s['floating_periods'])
            _require(_accrual_free(s['fixed_periods'], inception) and _accrual_free(s['floating_periods'], inception) and inception < last_end, 'inception boundary')
        _require(rel['hedged_risk'] == 'benchmark_interest_rate', 'hedged risk')
        pred = rel['predecessor_relationship_id']
        if pred is not None:
            _require(pred in compiled and raw['relationships'][compiled[pred]['index']]['hedging_instrument_position_id'] == inst, 'predecessor')
        entry = dict(index=index, item=None, dir=1., members=[], matched=False, similar=None, cyr=False)
        last = path['nodes'][-1]['ordinal']

        def hedged_of(x):
            _require(_accrual_free(x['periods'], inception) and any(q['start_ordinal'] >= inception for q in x['periods']), 'exposure inception boundary')
            hedged = [i for i, q in enumerate(x['periods']) if q['start_ordinal'] >= inception]; _require(hedged, 'hedged periods')
            first = path['nodes'][0]['ordinal']
            for i in hedged: _require(not first <= x['periods'][i]['payment_ordinal'] <= last or x['periods'][i]['payment_ordinal'] in nodes, 'hedged period node')
            return hedged

        def hyp_nodes(h, x, index_id):
            nonlocal work
            side = 'payer' if x['direction'] == 'payment' else 'receiver'
            hyp = {}
            for hn in h['nodes']:
                q = hn['request']; o = hn['ordinal']
                _require(o in nodes and o >= inception and o not in hyp and q['asof_ordinal'] == o and q['payment_timing'] == 'after_scheduled_payments'
                         and path['nodes'][nodes[o]]['payment_timing'] == 'after_scheduled_payments' and q['scenario_id'] == path['path_id']
                         and q['currency'] == raw['currency'] and q['monetary_unit'] == raw['monetary_unit'] and _near(q['notional'], x['notional'])
                         and q['side'] == side and _same_periods(q['floating_periods'], x['periods'])
                         and all(f['coupon']['kind'] == 'indexed' and f['coupon']['index']['index_id'] == index_id for f in q['floating_periods']), 'hypothetical mirror')
                hyp[o] = q; work += len(q['fixed_periods']) + sum(1 + len(f['coupon']['observations']) for f in q['floating_periods'])
            _require(all(n['ordinal'] in hyp for n in path['nodes'][nodes[max(inception, path['nodes'][0]['ordinal'])]:]), 'hypothetical coverage')
            return hyp

        if rel['designation'] == 'fair_value_hedge':
            _require(rel['forecast_exposure_id'] is None and rel['hypothetical'] is None and rel['matched_critical_terms_evidence'] is None
                     and rel.get('group') is None and rel.get('rate_hypotheticals') is None, 'FVH inputs')
            item = items[rel['hedged_item_id']]; hedged = rel['hedged_notional']; _money(rel['benchmark_rate'])
            _require(0. < hedged <= item['notional'] + 1e-12, 'hedged notional')
            hedged_total[item['item_id']] = hedged_total.get(item['item_id'], 0.) + hedged
            _require(hedged_total[item['item_id']] <= item['notional'] + 1e-9, 'item over-hedged')
            _require(_accrual_free(item['periods'], inception) and any(q['start_ordinal'] >= inception for q in item['periods']), 'item inception boundary')
            end = rel.get('hedged_term_end_ordinal')
            _require(end is None or inception < end < item['principal_ordinal'] and any(q['end_ordinal'] == end for q in item['periods']), 'partial-term end')
            layer = item['kind'] == 'closed_portfolio'; schedule = rel.get('layer_amortization')
            _require(not layer or opening is None and rel.get('written_option_position_id') is None, 'layers admit no seasoned opening or written option')
            _require(layer or schedule is None, 'layer amortization applies to closed-portfolio layers')
            if schedule is not None:
                last = end or item['principal_ordinal']
                expected = [q['end_ordinal'] for q in item['periods'] if inception < q['end_ordinal'] <= last]
                amounts = [_money(x['amount']) for x in schedule]
                _require([x['period_end_ordinal'] for x in schedule] == expected and amounts and amounts[0] == hedged
                         and all(a_ > 0. for a_ in amounts) and all(b_ <= a_ + 1e-12 for a_, b_ in zip(amounts, amounts[1:])), 'layer amortization')
            entry.update(item=item, dir=1. if item['direction'] == 'asset' else -1.)
            if opening is not None: _require(_near(opening['basis_balance'] + math.fsum(t_['amount'] for t_ in opening.get('frozen_basis') or []), -entry['dir'] * opening['cumulative_hedged_change']), 'seasoned basis plus frozen equals signed cumulative hedged change')
        else:
            _require(rel['designation'] == 'cash_flow_hedge' and rel['hedged_item_id'] is None and rel['hedged_notional'] is None and rel['benchmark_rate'] is None
                     and rel.get('hedged_term_end_ordinal') is None, 'CFH inputs')
            _require(reviews[inst]['coupon_recognition'] == 'settle_derivative_carrying', 'CFH coupon policy')
            group = rel.get('group')
            if group is not None:
                # ASU 2025-09 Issue 1: a documented similar-risk group with one hypothetical per member.
                elected(elections['similar_risk_grouping'], inception, 'similar-risk grouping')
                _require(rel['forecast_exposure_id'] is None and rel['hypothetical'] is None and rel['matched_critical_terms_evidence'] is None
                         and rel.get('rate_hypotheticals') is None and group['similar_risk_method'] in SIMILAR_METHODS, 'group inputs')
                ids_ = group['member_exposure_ids']
                _require(2 <= len(ids_) <= 16 and len(set(ids_)) == len(ids_) and [h['exposure_id'] for h in group['hypotheticals']] == ids_, 'group members')
                for xid, mh in zip(ids_, group['hypotheticals']):
                    x = exposures[xid]; _require(x.get('choose_your_rate') is None, 'group member cannot be choose-your-rate')
                    entry['members'].append(dict(exposure=x, hedged_periods=hedged_of(x), hyps={x['index_id']: (hyp_nodes(mh['hypothetical'], x, x['index_id']), mh['hypothetical']['evidence'])}))
                entry.update(similar=group['similar_risk_method'])
            else:
                x = exposures[rel['forecast_exposure_id']]; hedged = hedged_of(x)
                if opening is not None:
                    pays = {x['periods'][k]['payment_ordinal'] for k in hedged}
                    _require(all(e['payment_ordinal'] in pays for t_ in opening.get('frozen_oci') or [] for e in t_['entries']), 'frozen OCI entries name hedged payment dates')
                side = 'payer' if x['direction'] == 'payment' else 'receiver'
                hyps = {}
                if rel['hypothetical'] is not None:
                    _require(rel['matched_critical_terms_evidence'] is None, 'hypothetical or matched')
                    hyps[x['index_id']] = (hyp_nodes(rel['hypothetical'], x, x['index_id']), rel['hypothetical']['evidence'])
                else:
                    _require(rel['matched_critical_terms_evidence'] is not None, 'hypothetical or matched')
                    _require(_near(p * swap['notional'], x['notional']) and swap['side'] == side and _same_periods(swap['floating_periods'], x['periods'])
                             and all(q['coupon']['kind'] == 'indexed' and q['coupon']['index']['index_id'] == x['index_id'] for q in swap['floating_periods']), 'matched critical terms')
                    entry.update(matched=True)
                    _require(rel.get('written_option_position_id') is None and not rel.get('additional_instrument_position_ids'), 'combination cannot claim matched critical terms')
                cyr = x.get('choose_your_rate'); rate_hyps = rel.get('rate_hypotheticals')
                if cyr is not None or rate_hyps is not None:
                    # ASU 2025-09 Issue 2: one hypothetical per documented alternative, terms as of inception.
                    _require(cyr is not None and rate_hyps is not None and not entry['matched'], 'choose-your-rate inputs')
                    elected(elections['selectable_rate_debt'], inception, 'choose-your-rate debt')
                    expected = [i for i in cyr['documented_index_ids'] if i != x['index_id']]
                    _require(sorted(h['index_id'] for h in rate_hyps) == sorted(expected), 'rate hypotheticals per documented alternative')
                    for rh in rate_hyps: hyps[rh['index_id']] = (hyp_nodes(rh['hypothetical'], x, rh['index_id']), rh['hypothetical']['evidence'])
                    entry.update(cyr=True)
                entry['members'].append(dict(exposure=x, hedged_periods=hedged, hyps=hyps))
        compiled[rid] = entry
    _require(work <= 1_000_000, 'hypothetical work')
    prior = None; ids = set()
    for ev in raw['events']:
        clock = (ev['ordinal'], ev['sequence'])
        _require(ev['event_id'] not in ids and (prior is None or prior < clock), 'event order'); ids.add(ev['event_id']); prior = clock
        rel = raw['relationships'][compiled[ev['relationship_id']]['index']]; c = compiled[ev['relationship_id']]
        _require(ev['ordinal'] in nodes and ev['ordinal'] >= rel['inception_ordinal'], 'event node')
        swaps = [_swap_of(path['nodes'][0], rel['hedging_instrument_position_id'])] + [_swap_of(path['nodes'][0], i_) for i_ in rel.get('additional_instrument_position_ids') or []]
        extra = {k: ev.get(k) for k in ('proportion', 'reason', 'status')}
        _require(ev.get('index_id') is None or ev['kind'] == 'rate_election', 'index on a non-election event')
        _require(ev.get('effective_ordinal') is None or ev['kind'] == 'assess' and c['similar'] is not None, 'specific last-similar date on a non-group assessment')

        def assessment_node():
            final = max(q['payment_ordinal'] for s in swaps for q in s['fixed_periods'] + s['floating_periods'])
            free = lambda periods: _accrual_free(periods, ev['ordinal'])
            boundary = free(c['item']['periods']) if c['item'] else all(free(m['exposure']['periods']) for m in c['members'])
            _require(rel['inception_ordinal'] < ev['ordinal'] < final and path['nodes'][nodes[ev['ordinal']]]['payment_timing'] == 'after_scheduled_payments'
                     and all(free(s['fixed_periods']) and free(s['floating_periods']) for s in swaps) and boundary, 'assessment date')

        if ev['kind'] == 'assess':
            _require(all(v is None for v in extra.values()), 'assessment fields')
            assessment_node()
            eff = ev.get('effective_ordinal')
            _require(eff is None or eff in nodes and rel['inception_ordinal'] <= eff < ev['ordinal'], 'specific last-similar date')
        elif ev['kind'] == 'rate_election':
            _require(c['cyr'] and all(v is None for v in extra.values()) and ev.get('index_id') in c['members'][0]['exposure']['choose_your_rate']['documented_index_ids'], 'rate election fields')
            assessment_node()
        elif ev['kind'] == 'discontinue':
            _require(extra['reason'] in ('dedesignation', 'hedged_item_sold', 'instrument_terminated', 'hedged_item_matured') and extra['status'] is None, 'discontinuation fields')
            _require(extra['proportion'] is None or 0. < extra['proportion'] <= 1., 'discontinued proportion')
            if extra['reason'] == 'hedged_item_matured':
                # Settlement of the item on its principal date derecognizes the remaining basis, as a sale.
                _require(rel['designation'] == 'fair_value_hedge' and rel.get('hedged_term_end_ordinal') is None
                         and extra['proportion'] in (None, 1.) and c['item']['principal_ordinal'] == ev['ordinal'], 'hedged item maturity')
        else:
            _require(ev['kind'] == 'forecast_status' and rel['designation'] == 'cash_flow_hedge'
                     and extra['status'] in ('no_longer_probable', 'probable_not_to_occur', 'delayed', 'period_probable_not_to_occur')
                     and extra['proportion'] is None and extra['reason'] is None, 'forecast status fields')
            located = next((m_['exposure']['periods'][q_] for m_ in c['members'] for q_ in m_['hedged_periods'] if m_['exposure']['periods'][q_]['period_id'] == ev.get('period_id')), None)
            if extra['status'] in ('no_longer_probable', 'probable_not_to_occur'):
                _require(ev.get('period_id') is None and ev.get('new_payment_ordinal') is None, 'relationship-level forecast status fields')
            elif extra['status'] == 'delayed':
                _require(timing is not None and located is not None and type(ev.get('new_payment_ordinal')) is int, 'delayed forecast fields')
                later = ev['new_payment_ordinal']; original = located['payment_ordinal']
                _require(ev['ordinal'] < original < later <= original + timing['additional_delay_days'], 'delay outside the documented timing window')
                _require(later > last or later in nodes, 'delayed payment node')
            else:
                _require(located is not None and ev.get('new_payment_ordinal') is None, 'period non-occurrence fields')
    return compiled, csa, path, nodes


def _state(rel, c):
    return dict(status='pending', scale=1., proportion=rel['designated_proportion'], unit=None, inst=None, cum_d=0., cum_e=0., basis=0.,
                frozen_basis=[], oci=0., frozen_oci=[], basis_adjustments=0., basis_amortized=0., basis_derecognized=0., oci_recognized=0.,
                oci_reclassified=0., last_clean=None, last_hyp=None, last_assessed=rel['inception_ordinal'], oci_at={}, reclass_log=[],
                anticipated_breach=0., actual_breach=0.,
                members=[dict(selected=m['exposure']['index_id'], hyp=None, cum_e=0., last_hyp=None, delays={}, excluded=set(), deferred=[]) for m in c['members']])


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


def _locate(c, period_id):
    for m, member in enumerate(c['members']):
        for q in member['hedged_periods']:
            p = member['exposure']['periods'][q]
            if p['period_id'] == period_id: return m, q, p['payment_ordinal']
    raise ValueError('period is not a hedged exposure period of the relationship')


def _in_band(band, ratio):
    return ratio is not None and band['lower_bound'] <= ratio <= band['upper_bound']


def _ratio(d, e):
    return None if e == 0. else d / e


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
    rels = raw['relationships']; comp = [compiled[r['relationship_id']] for r in rels]; states = [_state(r, c) for r, c in zip(rels, comp)]
    lifecycle = []; assessments = []; daily = []
    cursor = 0; event_cursor = 0; intents = derivative['intents']
    opening_equity_adjustment = 0.; opening_oci = 0.; opening_reclassified = 0.; asof = path['nodes'][0]['ordinal']
    opening_values = {t['position_id']: t['clean_value'] for t in raw['derivative']['opening']['trades']}
    for i, rel in enumerate(rels):
        o = rel.get('opening')
        if o is None: continue
        st, c = states[i], comp[i]; rid = rel['relationship_id']; inst = rel['hedging_instrument_position_id']
        instrument = opening_values[inst] + (opening_values[rel['written_option_position_id']] if rel.get('written_option_position_id') else 0.)
        instrument = _money(instrument + math.fsum(opening_values[i_] for i_ in rel.get('additional_instrument_position_ids') or []))
        if c['matched']: _require(_near(o['hedged_value'], instrument), 'matched opening hedged value')
        st.update(status='qualified', inst=instrument, cum_d=o['cumulative_instrument_change'], cum_e=o['cumulative_hedged_change'], last_assessed=asof)
        st['scale'] = 1. if o.get('scale') is None else o['scale']; st['proportion'] = _money(rel['designated_proportion'] * st['scale'])
        sources = [deepcopy(hedge_policy), deepcopy(rel['documentation']), deepcopy(o['evidence'])]
        if rel['designation'] == 'fair_value_hedge':
            item = c['item']; st.update(unit=o['hedged_value'], basis=o['basis_balance'], basis_adjustments=o['basis_balance'])
            for t_ in o.get('frozen_basis') or []:
                st['frozen_basis'].append(dict(amount=t_['amount'], start=t_['start_ordinal'], amortized=t_['amortized'],
                                               end=rel.get('hedged_term_end_ordinal') or c['item']['principal_ordinal']))
                st['basis_adjustments'] = _money(st['basis_adjustments'] + t_['amount']); st['basis_amortized'] = _money(st['basis_amortized'] + t_['amortized'])
            retained = _money(o['basis_balance'] + math.fsum(t_['amount'] - t_['amortized'] for t_ in o.get('frozen_basis') or []))
            journal.emit('hedge_opening', asof, 'financial', inst, item['item_id'], None, rid, hedge_policy, sources,
                         [_posting('hedged_item_basis_adjustment', c['dir'] * retained, item['item_id'], None, rid),
                          _posting('opening_equity', -c['dir'] * retained, item['item_id'], None, rid)])
            opening_equity_adjustment = _money(opening_equity_adjustment - c['dir'] * retained)
        else:
            x = c['members'][0]['exposure']; st['members'][0].update(hyp=o['hedged_value'], cum_e=o['cumulative_hedged_change'])
            st.update(oci=o['oci_balance'], oci_recognized=o['oci_balance'])
            for t_ in o.get('frozen_oci') or []:
                st['frozen_oci'].append(dict(amount=t_['amount'], released=t_['released'],
                                             entries=[dict(pay=e['payment_ordinal'], member=0, weight=e['weight'], released=False) for e in t_['entries']]))
                st['oci_recognized'] = _money(st['oci_recognized'] + t_['amount']); st['oci_reclassified'] = _money(st['oci_reclassified'] + t_['released'])
                opening_reclassified = _money(opening_reclassified + t_['released'])
            retained = _money(o['oci_balance'] + math.fsum(t_['amount'] - t_['released'] for t_ in o.get('frozen_oci') or []))
            journal.emit('hedge_opening', asof, 'financial', inst, x['exposure_id'], None, rid, hedge_policy, sources,
                         [_posting('cash_flow_hedge_oci', -retained, x['exposure_id'], None, rid), _posting('opening_equity', retained, x['exposure_id'], None, rid)])
            opening_equity_adjustment = _money(opening_equity_adjustment + retained); opening_oci = _money(opening_oci + retained)
        lifecycle.append(_record(asof, rel, st, kind='opening', review=deepcopy(o['evidence']), status_before='pending', status_after='qualified',
                                 proportion_before=0., proportion_after=st['proportion'],
                                 frozen_basis=math.fsum(t_['amount'] - t_['amortized'] for t_ in o.get('frozen_basis') or []),
                                 frozen_oci=math.fsum(t_['amount'] - t_['released'] for t_ in o.get('frozen_oci') or [])))

    def instrument_value(k, position):
        for x in derivative['daily'][k]['trades']:
            if x['mapped_position_id'] == position: return x['clean_carrying']
        raise ValueError('instrument missing from derivative closing')

    def designated_value(k, rel):
        value = instrument_value(k, rel['hedging_instrument_position_id']); option = rel.get('written_option_position_id')
        value = value if option is None else _money(value + instrument_value(k, option))
        for i_ in rel.get('additional_instrument_position_ids') or []: value = _money(value + instrument_value(k, i_))
        return value

    def two(account_a, value, owner, rid, account_b):
        return [_posting(account_a, value, owner, None, rid), _posting(account_b, -value, owner, None, rid)]

    def release_frozen(i, t, sources):
        """Release frozen OCI entries due at or before t per member; the last entry clears exactly."""
        rel, st, c = rels[i], states[i], comp[i]; total = 0.
        for tranche in st['frozen_oci']:
            last = max(e['pay'] for e in tranche['entries']); per_member = [0.] * len(c['members']); final = None
            for e in tranche['entries']:
                if not e['released'] and e['pay'] <= t:
                    per_member[e['member']] = _money(per_member[e['member']] + tranche['amount'] * e['weight']); e['released'] = True
                    if e['pay'] == last: final = e['member']
            if final is not None:
                per_member[final] = _money(tranche['amount'] - tranche['released'] - math.fsum(v for j, v in enumerate(per_member) if j != final))
            for m, release in enumerate(per_member):
                if release == 0.: continue
                x = c['members'][m]['exposure']
                journal.emit('frozen_oci_release', t, 'financial', rel['hedging_instrument_position_id'], x['exposure_id'], None, rel['relationship_id'],
                             hedge_policy, sources(m), two('cash_flow_hedge_oci', release, x['exposure_id'], rel['relationship_id'], 'hedged_forecast_reclassification'))
                tranche['released'] = _money(tranche['released'] + release); st['oci_reclassified'] = _money(st['oci_reclassified'] + release)
                total = _money(total + release)
        return total

    def catch_up(i, t, kind, effective, review):
        """Instrument changes recognized in OCI after the effective node return to earnings."""
        rel, st = rels[i], states[i]; _require(effective in st['oci_at'], 'effective date outside the retained OCI history')
        g = _money(st['oci_recognized'] - st['oci_at'][effective]); inst = rel['hedging_instrument_position_id']
        if g != 0.:
            journal.emit(kind, t, 'financial', inst, inst, None, rel['relationship_id'], hedge_policy,
                         [deepcopy(hedge_policy), deepcopy(rel['documentation']), deepcopy(review)],
                         two('cash_flow_hedge_oci', g, inst, rel['relationship_id'], 'mark_gain_loss'))
            st['oci'] = _money(st['oci'] - g); st['oci_recognized'] = _money(st['oci_recognized'] - g)
        return g

    def discontinue(i, t, q, reason, review, record, cutoff=None):
        cutoff = t if cutoff is None else cutoff
        rel, st, c = rels[i], states[i], comp[i]
        _require(st['status'] == 'qualified', 'discontinuation requires a qualified relationship')
        sources = [deepcopy(hedge_policy), deepcopy(rel['documentation']), deepcopy(review)]
        inst = rel['hedging_instrument_position_id']; rid = rel['relationship_id']
        if rel['designation'] == 'fair_value_hedge':
            item = c['item']; frozen = _money(q * st['basis']); st['basis'] = _money(st['basis'] - frozen)
            if reason in ('hedged_item_sold', 'hedged_item_matured'):
                journal.emit('hedged_item_basis_derecognition', t, 'financial', inst, item['item_id'], None, rid, hedge_policy, sources,
                             two('hedged_item_basis_adjustment', -c['dir'] * frozen, item['item_id'], rid, 'hedged_item_basis_derecognition'))
                st['basis_derecognized'] = _money(st['basis_derecognized'] + frozen); record['derecognized_basis'] = frozen
            elif frozen != 0.:
                st['frozen_basis'].append(dict(amount=frozen, start=t, amortized=0., end=rel.get('hedged_term_end_ordinal') or item['principal_ordinal'])); record['frozen_basis'] = frozen
        else:
            frozen = _money(q * st['oci']); st['oci'] = _money(st['oci'] - frozen)
            remaining = [(st['members'][m].get('delays', {}).get(k, x['periods'][k]['payment_ordinal']), m, x['notional'] * _tau(x['periods'][k]['start_ordinal'], x['periods'][k]['end_ordinal'], x['periods'][k]['day_count']))
                         for m, member in enumerate(c['members']) for x in [member['exposure']] for k in member['hedged_periods']
                         if x['periods'][k]['payment_ordinal'] > cutoff and k not in st['members'][m]['excluded']]
            total = math.fsum(w for _, _, w in remaining)
            if not remaining or total <= 0.:
                if frozen != 0.:
                    x = c['members'][0]['exposure']
                    journal.emit('frozen_oci_release', t, 'financial', inst, x['exposure_id'], None, rid, hedge_policy, sources,
                                 two('cash_flow_hedge_oci', frozen, x['exposure_id'], rid, 'hedged_forecast_reclassification'))
                    st['oci_reclassified'] = _money(st['oci_reclassified'] + frozen); record['released_oci'] = frozen
            elif frozen != 0.:
                st['frozen_oci'].append(dict(amount=frozen, released=0., entries=[dict(pay=pay, member=m, weight=w / total, released=False) for pay, m, w in remaining]))
                record['frozen_oci'] = frozen
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
                        if rels[i]['hedging_instrument_position_id'] != mapped and rels[i].get('written_option_position_id') != mapped and mapped not in (rels[i].get('additional_instrument_position_ids') or []): continue
                        value = _money(m * share)
                        postings.append(_posting('cash_flow_hedge_oci', value, p['position_id'], p['bucket'], rels[i]['relationship_id']))
                        states[i]['oci'] = _money(states[i]['oci'] - value); states[i]['oci_recognized'] = _money(states[i]['oci_recognized'] - value)
                        retained -= share; relationship = rels[i]['relationship_id']
                    postings.append(_posting('mark_gain_loss', _money(m * retained), p['position_id'], p['bucket']))
                else:
                    postings.append(_posting(p['account'], p['debit_amount'], p['position_id'], p['bucket']))
            journal.emit(di['event_id'].split(':')[0], t, di['ledger'], di['mapped_position_id'], di['contract_position_id'], di['bucket'], relationship,
                         di['policy'], di['sources'], postings)
        for st in states: st['oci_at'][t] = st['oci_recognized']
        for i, rel in enumerate(rels):
            st, c = states[i], comp[i]; inst = rel['hedging_instrument_position_id']; rid = rel['relationship_id']
            base = lambda extra: [deepcopy(hedge_policy), deepcopy(rel['documentation'])] + [deepcopy(e) for e in extra]
            if t >= rel['inception_ordinal']:
                value = designated_value(k, rel)
                if st['status'] == 'qualified': st['cum_d'] = _money(st['cum_d'] + st['proportion'] * (value - st['inst']))
                st['inst'] = value
            if rel['designation'] == 'fair_value_hedge':
                item = c['item']; hedged = rel['hedged_notional']; rate = rel['benchmark_rate']
                if t >= rel['inception_ordinal']:
                    swap = _swap_of(node, inst)
                    dfs = _discount(swap['discount_curve'], t, raw['currency'], swap['valuation_cutoff_unix_seconds'])
                    term_end = rel.get('hedged_term_end_ordinal')
                    schedule = rel.get('layer_amortization')
                    if schedule is None:
                        before = _unit_clean(item, rate, dfs, t, True, term_end); unit = _unit_clean(item, rate, dfs, t, False, term_end)
                    else:
                        before = _unit_clean_amortizing(item, rate, dfs, t, True, schedule, hedged); unit = _unit_clean_amortizing(item, rate, dfs, t, False, schedule, hedged)
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
                    term_end = tranche['end']
                    if term_end <= tranche['start']: target = tranche['amount']
                    else: target = _money(tranche['amount'] * min(1., (t - tranche['start']) / (term_end - tranche['start'])))
                    delta = _money(target - tranche['amortized'])
                    if delta != 0.:
                        journal.emit('hedged_item_basis_amortization', t, 'financial', inst, item['item_id'], None, rid, hedge_policy,
                                     base([item['source'], item['carrying_source']]),
                                     two('hedged_item_basis_adjustment', -c['dir'] * delta, item['item_id'], rid, 'hedged_item_basis_amortization'))
                        tranche['amortized'] = target; st['basis_amortized'] = _money(st['basis_amortized'] + delta)
            else:
                if t >= rel['inception_ordinal']:
                    total_hyp = 0.
                    for m, member in enumerate(c['members']):
                        x = member['exposure']; ms = st['members'][m]
                        if c['matched']:
                            physical = math.fsum(r['physical_amount'] for r in derivative['cash_rights'] if r['position_id'] == inst and r['payment_ordinal'] == t)
                            h, settlement, evidence = instrument_value(k, inst), _money(rel['designated_proportion'] * physical), rel['matched_critical_terms_evidence']
                        else:
                            nodes_, evidence = member['hyps'][ms['selected']]; h, settlement = _hypothetical(nodes_[t], t)
                            if t == rel['inception_ordinal']:
                                # Every documented hypothetical has terms as of inception (815-30-35-37M): at market there.
                                for alt_nodes, _ in member['hyps'].values(): _require(_near(_hypothetical(alt_nodes[t], t)[0], 0.), 'hypothetical must be at market at inception')
                        if st['status'] == 'qualified':
                            delta = _money(st['scale'] * (h - ms['hyp'])); ms['cum_e'] = _money(ms['cum_e'] + delta); st['cum_e'] = _money(st['cum_e'] + delta)
                            due = next((q for q in member['hedged_periods'] if x['periods'][q]['payment_ordinal'] == t and q not in ms['excluded']), None)
                            if due is not None and due in ms['delays']:
                                ms['deferred'].append((ms['delays'][due], _money(st['scale'] * settlement)))
                            elif due is not None:
                                reclass = _money(st['scale'] * settlement)
                                if reclass != 0.:
                                    journal.emit('cash_flow_hedge_reclassification', t, 'financial', inst, x['exposure_id'], None, rid, hedge_policy,
                                                 base([x['source'], x['probability_evidence'], evidence]),
                                                 two('cash_flow_hedge_oci', reclass, x['exposure_id'], rid, 'hedged_forecast_reclassification'))
                                    st['oci'] = _money(st['oci'] - reclass); st['oci_reclassified'] = _money(st['oci_reclassified'] + reclass)
                                    st['reclass_log'].append((t, m, reclass))
                        ready = [amt for when, amt in ms['deferred'] if when == t]; ms['deferred'] = [d for d in ms['deferred'] if d[0] != t]
                        for reclass in ready:
                            if reclass == 0.: continue
                            journal.emit('cash_flow_hedge_reclassification', t, 'financial', inst, x['exposure_id'], None, rid, hedge_policy,
                                         base([x['source'], x['probability_evidence'], evidence]),
                                         two('cash_flow_hedge_oci', reclass, x['exposure_id'], rid, 'hedged_forecast_reclassification'))
                            st['oci'] = _money(st['oci'] - reclass); st['oci_reclassified'] = _money(st['oci_reclassified'] + reclass)
                            st['reclass_log'].append((t, m, reclass))
                        ms['hyp'] = h; ms['last_hyp'] = _money(st['scale'] * h); total_hyp = _money(total_hyp + ms['last_hyp'])
                    st['last_hyp'] = total_hyp
                release_frozen(i, t, lambda m: base([c['members'][m]['exposure']['source'], c['members'][m]['exposure']['probability_evidence']]))
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
            inst = rel['hedging_instrument_position_id']; rid = rel['relationship_id']
            if ev['kind'] == 'assess':
                record['kind'] = 'assessment'
                _require(st['status'] == 'qualified', 'assessment on a relationship that is not qualified')
                d, e = st['cum_d'], st['cum_e']; _require(not (d == 0. and e == 0.), 'nothing to assess')
                ratio = _ratio(d, e); qualified = _in_band(band, ratio)
                assessment = dict(event_id=ev['event_id'], relationship_id=rid, ordinal=t, method=band['method'], lower_bound=band['lower_bound'],
                                  upper_bound=band['upper_bound'], cumulative_instrument_change=d, cumulative_hedged_change=e, ratio=ratio, qualified=qualified)
                if c['cyr']: assessment['index_id'] = st['members'][0]['selected']
                similar = True
                if c['similar'] is not None:
                    notionals = [m['exposure']['notional'] for m in c['members']]; group_notional = math.fsum(notionals)
                    changes = [ms['cum_e'] for ms in st['members']]; units = [ch / n for ch, n in zip(changes, notionals)]
                    members = []
                    for m, member in enumerate(c['members']):
                        if c['similar'] == 'instrument_highly_effective_each_risk':
                            member_ratio = _ratio(_money(d * notionals[m] / group_notional), changes[m]); ok = _in_band(band, member_ratio)
                        else:
                            ok = all(_in_band(band, _ratio(units[m], units[j])) for j in range(len(units)) if j != m); member_ratio = _ratio(units[m], units[0])
                        members.append(dict(exposure_id=member['exposure']['exposure_id'], notional=notionals[m], cumulative_hedged_change=changes[m],
                                            ratio=member_ratio, similar=ok))
                    similar = all(x['similar'] for x in members)
                    assessment['similar_risk'] = dict(method=c['similar'], similar=similar, members=members)
                assessments.append(assessment)
                if not similar:
                    # 815-20-55-23D: dedesignate as of the last date all risks were assessed similar (or the specific date).
                    effective = ev.get('effective_ordinal'); effective = st['last_assessed'] if effective is None else effective
                    _require(effective >= st['last_assessed'], 'specific last-similar date before the last similar assessment')
                    g = catch_up(i, t, 'similar_risk_earnings_catch_up', effective, ev['review'])
                    reverse = [x for x in st['reclass_log'] if x[0] > effective]; st['reclass_log'] = [x for x in st['reclass_log'] if x[0] <= effective]
                    reversed_ = 0.
                    if reverse:
                        per_member = [0.] * len(c['members'])
                        for _, m, amt in reverse: per_member[m] = _money(per_member[m] + amt)
                        postings = []
                        for m, amt in enumerate(per_member):
                            x = c['members'][m]['exposure']
                            postings += two('cash_flow_hedge_oci', -amt, x['exposure_id'], rid, 'hedged_forecast_reclassification'); reversed_ = _money(reversed_ + amt)
                        journal.emit('similar_risk_reclassification_reversal', t, 'financial', inst, inst, None, rid, hedge_policy,
                                     [deepcopy(hedge_policy), deepcopy(rel['documentation']), deepcopy(ev['review'])], postings)
                        st['oci'] = _money(st['oci'] + reversed_); st['oci_reclassified'] = _money(st['oci_reclassified'] - reversed_)
                    discontinue(i, t, 1., 'similar_risk_failed', ev['review'], record, cutoff=effective)
                    released = release_frozen(i, t, lambda m: [deepcopy(hedge_policy), deepcopy(rel['documentation']), deepcopy(ev['review'])])
                    record.update(effective_ordinal=effective, earnings_catch_up=g, reversed_reclassification=reversed_, released_oci=_money(record['released_oci'] + released))
                elif not qualified: discontinue(i, t, 1., 'failed_assessment', ev['review'], record)
                else: record['reason'] = 'qualified'; st['last_assessed'] = t
            elif ev['kind'] == 'rate_election':
                # 815-30-35-37L/M: final retrospective assessment on the prior rate, then the new hypothetical governs.
                record['kind'] = 'rate_election'
                _require(st['status'] == 'qualified', 'rate election requires a qualified relationship')
                ms = st['members'][0]; prior_index = ms['selected']; new_index = ev['index_id']
                _require(new_index != prior_index, 'rate election must change the selected index')
                d, e = st['cum_d'], st['cum_e']; _require(not (d == 0. and e == 0.), 'nothing to assess')
                ratio = _ratio(d, e); qualified = _in_band(band, ratio)
                assessments.append(dict(event_id=ev['event_id'], relationship_id=rid, ordinal=t, method=band['method'], lower_bound=band['lower_bound'],
                                        upper_bound=band['upper_bound'], cumulative_instrument_change=d, cumulative_hedged_change=e, ratio=ratio,
                                        qualified=qualified, index_id=prior_index))
                if not qualified:
                    effective = st['last_assessed']; g = catch_up(i, t, 'rate_election_earnings_catch_up', effective, ev['review'])
                    record.update(effective_ordinal=effective, earnings_catch_up=g, reason='ineffective_period_in_earnings')
                else: record['reason'] = 'qualified'
                nodes_, _ = c['members'][0]['hyps'][new_index]; h, _ = _hypothetical(nodes_[t], t); scaled = _money(st['scale'] * h)
                ms.update(selected=new_index, hyp=h, cum_e=scaled, last_hyp=scaled)
                st['cum_e'] = math.fsum(x['cum_e'] for x in st['members']); st['last_hyp'] = scaled; st['last_assessed'] = t
                record['index_id'] = new_index
            elif ev['kind'] == 'discontinue':
                record['kind'] = 'discontinuation'
                discontinue(i, t, ev.get('proportion') or 1., ev['reason'], ev['review'], record)
            else:
                record['kind'] = 'forecast_status'
                _require(st['status'] != 'pending', 'forecast status before inception')
                if ev['status'] == 'no_longer_probable':
                    if st['status'] == 'qualified': discontinue(i, t, 1., 'forecast_no_longer_probable', ev['review'], record)
                    else: record['reason'] = 'forecast_no_longer_probable'
                elif ev['status'] == 'probable_not_to_occur':
                    x = c['members'][0]['exposure']; release = st['oci']
                    for tranche in st['frozen_oci']:
                        release = _money(release + tranche['amount'] - tranche['released']); tranche['released'] = tranche['amount']
                        for e in tranche['entries']: e['released'] = True
                    st['oci'] = 0.
                    if release != 0.:
                        journal.emit('forecast_not_occurring_release', t, 'financial', rel['hedging_instrument_position_id'], x['exposure_id'], None,
                                     rel['relationship_id'], hedge_policy, [deepcopy(hedge_policy), deepcopy(rel['documentation']), deepcopy(ev['review'])],
                                     two('cash_flow_hedge_oci', release, x['exposure_id'], rel['relationship_id'], 'hedged_forecast_reclassification'))
                        st['oci_reclassified'] = _money(st['oci_reclassified'] + release)
                    st['scale'] = 0.; st['proportion'] = 0.; st['status'] = 'discontinued'
                    record.update(reason='forecast_probable_not_to_occur', released_oci=release, status_after='discontinued', proportion_after=0.)
                elif ev['status'] == 'delayed':
                    m, q, original = _locate(c, ev['period_id']); ms = st['members'][m]
                    _require(q not in ms['excluded'] and q not in ms['delays'], 'delayed period excluded or already delayed')
                    ms['delays'][q] = ev['new_payment_ordinal']
                    for tranche in st['frozen_oci']:
                        for e in tranche['entries']:
                            if not e['released'] and e['member'] == m and e['pay'] == original: e['pay'] = ev['new_payment_ordinal']
                    record.update(reason='forecast_delayed', period_id=ev['period_id'], new_payment_ordinal=ev['new_payment_ordinal'])
                else:
                    m, q, original = _locate(c, ev['period_id']); ms = st['members'][m]
                    effective = ms['delays'].get(q, original)
                    _require(q not in ms['excluded'] and original > t, 'period already excluded or already affected earnings')
                    weights = [(m2, k, x2['notional'] * _tau(x2['periods'][k]['start_ordinal'], x2['periods'][k]['end_ordinal'], x2['periods'][k]['day_count']))
                               for m2, member2 in enumerate(c['members']) for x2 in [member2['exposure']] for k in member2['hedged_periods']
                               if x2['periods'][k]['payment_ordinal'] > t and k not in st['members'][m2]['excluded']]
                    total = math.fsum(w for _, _, w in weights); own = next((w for m2, k, w in weights if m2 == m and k == q), 0.); only = len(weights) == 1
                    share = st['oci'] if only or total <= 0. else _money(st['oci'] * own / total); release = share
                    for tranche in st['frozen_oci']:
                        taken = 0.
                        for e in tranche['entries']:
                            if not e['released'] and e['member'] == m and e['pay'] == effective: taken = _money(taken + tranche['amount'] * e['weight']); e['released'] = True
                        if all(e['released'] for e in tranche['entries']): taken = _money(tranche['amount'] - tranche['released'])
                        tranche['released'] = _money(tranche['released'] + taken); release = _money(release + taken)
                    st['oci'] = _money(st['oci'] - share); x = c['members'][m]['exposure']
                    if release != 0.:
                        journal.emit('forecast_not_occurring_release', t, 'financial', rel['hedging_instrument_position_id'], x['exposure_id'], None,
                                     rel['relationship_id'], hedge_policy, [deepcopy(hedge_policy), deepcopy(rel['documentation']), deepcopy(ev['review'])],
                                     two('cash_flow_hedge_oci', release, x['exposure_id'], rel['relationship_id'], 'hedged_forecast_reclassification'))
                        st['oci_reclassified'] = _money(st['oci_reclassified'] + release)
                    ms['excluded'].add(q)
                    if only and st['status'] == 'qualified': st['scale'] = 0.; st['proportion'] = 0.; st['status'] = 'discontinued'
                    record.update(reason='forecast_period_probable_not_to_occur', period_id=ev['period_id'], released_oci=release, status_after=st['status'], proportion_after=st['proportion'])
            lifecycle.append(record)
        _portfolio_layers(raw, t, rels, comp, states, compiled, journal, lifecycle, hedge_policy)
        closing = derivative['daily'][k]
        for b in closing['financial_trial_balance']:
            mine = journal.gl.get(b['account'], 0.)
            if b['account'] == 'mark_gain_loss':
                mine += journal.gl.get('cash_flow_hedge_oci', 0.) + opening_oci + opening_reclassified - math.fsum(s['oci_reclassified'] for s in states)
            elif b['account'] == 'opening_equity': mine -= opening_equity_adjustment
            _require(_near(mine, b['debit_balance']), 'derivative owner balance ' + b['account'])
        for b in closing['custody_trial_balance']: _require(_near(journal.memo.get(b['account'], 0.), b['debit_balance']), 'custody balance')
        _require(_near(math.fsum(journal.gl.values()), 0.) and _near(math.fsum(journal.memo.values()), 0.), 'trial balance')
        rows = []
        for i, rel in enumerate(rels):
            st = states[i]
            value = _money(st['proportion'] * designated_value(k, rel)) if t >= rel['inception_ordinal'] else 0.
            row = dict(relationship_id=rel['relationship_id'], status=st['status'], designated_proportion=st['proportion'],
                       instrument_designated_value=value, hedged_item_clean_value=st['last_clean'], hypothetical_value=st['last_hyp'],
                       cumulative_instrument_change=st['cum_d'], cumulative_hedged_change=st['cum_e'], basis_adjustment_balance=st['basis'],
                       frozen_basis_balance=math.fsum(x['amount'] - x['amortized'] for x in st['frozen_basis']), oci_balance=st['oci'],
                       frozen_oci_balance=math.fsum(x['amount'] - x['released'] for x in st['frozen_oci']))
            c = comp[i]
            if c['similar'] is not None or c['cyr']:
                row['members'] = [dict(exposure_id=m['exposure']['exposure_id'], index_id=ms['selected'], hypothetical_value=0. if ms['last_hyp'] is None else ms['last_hyp'],
                                       cumulative_hedged_change=ms['cum_e']) for m, ms in zip(c['members'], st['members'])]
            rows.append(row)
        daily.append(dict(ordinal=t, relationships=rows, financial_trial_balance=journal.balances(journal.gl), custody_trial_balance=journal.balances(journal.memo)))
    _require(cursor == len(intents) and event_cursor == len(raw['events']), 'intents or events beyond the tail')
    last = path['nodes'][-1]['ordinal']; relationships = []; closing = []
    for i, rel in enumerate(rels):
        st, c = states[i], comp[i]
        pending = [m['exposure']['periods'][q]['period_id'] for m, ms in zip(c['members'], st['members']) for q in m['hedged_periods']
                   if q not in ms['excluded'] and ms['delays'].get(q, m['exposure']['periods'][q]['payment_ordinal']) > last]
        frozen_basis = math.fsum(x['amount'] - x['amortized'] for x in st['frozen_basis']); frozen_oci = math.fsum(x['amount'] - x['released'] for x in st['frozen_oci'])
        relationships.append(dict(relationship_id=rel['relationship_id'], predecessor_relationship_id=rel['predecessor_relationship_id'], designation=rel['designation'],
                                  hedging_instrument_position_id=rel['hedging_instrument_position_id'], hedged_item_id=rel['hedged_item_id'],
                                  forecast_exposure_id=rel['forecast_exposure_id'], inception_ordinal=rel['inception_ordinal'], status=st['status'],
                                  designated_proportion=st['proportion'], basis_adjustment_balance=st['basis'], frozen_basis_balance=frozen_basis,
                                  oci_balance=st['oci'], frozen_oci_balance=frozen_oci, pending_reclassification_periods=pending))
        if c['similar'] is not None: relationships[-1]['member_exposure_ids'] = [m['exposure']['exposure_id'] for m in c['members']]
        if c['cyr']: relationships[-1]['selected_index_id'] = st['members'][0]['selected']
        if rel.get('written_option_position_id') is not None: relationships[-1]['written_option_position_id'] = rel['written_option_position_id']
        if rel.get('opening') is not None: relationships[-1]['opening_evidence'] = deepcopy(rel['opening']['evidence'])
        if rel.get('hedged_term_end_ordinal') is not None: relationships[-1]['hedged_term_end_ordinal'] = rel['hedged_term_end_ordinal']
        if rel.get('additional_instrument_position_ids'): relationships[-1]['additional_instrument_position_ids'] = list(rel['additional_instrument_position_ids'])
        if c.get('item') is not None and c['item']['kind'] == 'closed_portfolio':
            relationships[-1].update(layer_amount=_money(rel['hedged_notional'] * st['scale']), anticipated_breach_amount=st['anticipated_breach'],
                                     actual_breach_amount=st['actual_breach'], basis_excluded_from_credit_loss_measurement=True)
        closing.append(dict(relationship_id=rel['relationship_id'], basis_adjustments=st['basis_adjustments'], basis_amortized=st['basis_amortized'],
                            basis_derecognized=st['basis_derecognized'], basis_closing=_money(st['basis'] + frozen_basis), oci_recognized=st['oci_recognized'],
                            oci_reclassified=st['oci_reclassified'], oci_closing=_money(st['oci'] + frozen_oci)))
        _require(_near(st['basis_adjustments'] - st['basis_amortized'] - st['basis_derecognized'], st['basis'] + frozen_basis)
                 and _near(st['oci_recognized'] - st['oci_reclassified'], st['oci'] + frozen_oci), 'roll-forward')
    check()
    return dict(schema=SCHEMA, model_version=SCHEMA, rule_version=RULE, run=deepcopy(raw['run']), entity=raw['entity'], currency=raw['currency'],
                monetary_unit=raw['monetary_unit'], policy=deepcopy(raw['policy']), derivative=derivative, relationships=relationships,
                lifecycle=lifecycle, assessments=assessments, daily=daily, intents=journal.intents, closing=closing, **FLAGS)


def _portfolio_layers(raw, t, rels, comp, states, compiled, journal, lifecycle, hedge_policy):
    """ASU 2022-01 breach monitoring after the node's reviewed events: actual, then anticipated."""
    policy = raw['policy'].get('portfolio_layer')
    for item in raw['hedged_items']:
        portfolio = item.get('portfolio')
        if portfolio is None: continue
        layers = [i for i, c in enumerate(comp) if c.get('item') is item and states[i]['status'] == 'qualified']
        if not layers: continue
        layers.sort(key=lambda i: (rels[i]['inception_ordinal'], i))
        current = lambda i, at: _layer_amount(rels[i], item, at) * states[i]['scale']
        actual = next((b['outstanding'] for b in portfolio['balances'] if b['ordinal'] == t), None)
        _require(actual is not None, 'closed portfolio balance missing at a hedged node')
        projections = [x for x in portfolio['projections'] if x['ordinal'] == t]
        required_projection = any(rels[i]['inception_ordinal'] == t or any(ev['ordinal'] == t and ev['kind'] == 'assess'
                                  and compiled[ev['relationship_id']]['index'] == i for ev in raw['events']) for i in layers)

        def allocate(affected, excess, at):
            if policy['breach_order'] == 'last_designated_first':
                out = []; rest = excess
                for i in reversed(affected):
                    cur = current(i, at); take = min(rest, cur)
                    if take > 0.: out.append((i, take, take / cur)); rest -= take
                    if rest <= 0.: break
                return out
            total = math.fsum(current(i, at) for i in affected)
            return [(i, current(i, at) * excess / total, excess / total) for i in affected if current(i, at) > 0.]

        def apply(i, take, q, actual_breach):
            rel, st, c = rels[i], states[i], comp[i]; q = min(q, 1.)
            record = _record(t, rel, st, kind='portfolio_layer_breach', review=deepcopy(policy['identity']),
                             reason='actual_breach' if actual_breach else 'anticipated_breach', breach_amount=take)
            frozen = _money(q * st['basis']); st['basis'] = _money(st['basis'] - frozen)
            if actual_breach:
                journal.emit('portfolio_layer_breach_basis', t, 'financial', rel['hedging_instrument_position_id'], item['item_id'], None, rel['relationship_id'],
                             hedge_policy, [deepcopy(hedge_policy), deepcopy(rel['documentation']), deepcopy(policy['identity']), deepcopy(portfolio['balance_source'])],
                             [_posting('hedged_item_basis_adjustment', -c['dir'] * frozen, item['item_id'], None, rel['relationship_id']),
                              _posting('hedged_item_basis_derecognition', c['dir'] * frozen, item['item_id'], None, rel['relationship_id'])])
                st['basis_derecognized'] = _money(st['basis_derecognized'] + frozen); st['actual_breach'] = _money(st['actual_breach'] + take)
                record['derecognized_basis'] = frozen
            else:
                if frozen != 0.: st['frozen_basis'].append(dict(amount=frozen, start=t, amortized=0., end=item['principal_ordinal']))
                st['anticipated_breach'] = _money(st['anticipated_breach'] + take); record['frozen_basis'] = frozen
            st['scale'] *= 1. - q; st['proportion'] = _money(rel['designated_proportion'] * st['scale'])
            if q >= 1. - 1e-12: st['scale'] = 0.; st['proportion'] = 0.; st['status'] = 'discontinued'
            record.update(status_after=st['status'], proportion_after=st['proportion']); lifecycle.append(record)

        required = math.fsum(current(i, t) for i in layers)
        if actual < required - 1e-9:
            _require(not any(rels[i]['inception_ordinal'] == t for i in layers), 'layer exceeds the closed portfolio at designation')
            for i, take, q in allocate(layers, required - actual, t): apply(i, take, q, True)
        layers = [i for i in layers if states[i]['status'] == 'qualified']
        ends = sorted({e for e in ((rels[i].get('hedged_term_end_ordinal') or item['principal_ordinal']) for i in layers) if e > t})
        have = {x['term_end_ordinal']: x for x in projections}
        _require(not required_projection or all(e in have for e in ends), 'portfolio projections at designation and assessment')
        if projections:
            _require(set(have) <= set(ends), 'projection names no active layer end')
            for e in ends:
                _require(e in have, 'portfolio projections must cover every active layer end')
                affected = [i for i in layers if states[i]['status'] == 'qualified' and (rels[i].get('hedged_term_end_ordinal') or item['principal_ordinal']) >= e]
                need = math.fsum(current(i, e - 1) for i in affected)
                if have[e]['anticipated'] < need - 1e-9:
                    _require(not any(rels[i]['inception_ordinal'] == t for i in affected), 'layer must be expected to remain at designation')
                    for i, take, q in allocate(affected, need - have[e]['anticipated'], e - 1): apply(i, take, q, False)


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
