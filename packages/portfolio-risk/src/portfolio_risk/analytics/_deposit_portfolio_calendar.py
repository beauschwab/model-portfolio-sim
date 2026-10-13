from collections import defaultdict
from copy import deepcopy
from dataclasses import asdict
import math,json,unicodedata
from types import SimpleNamespace
from ._deposit_portfolio_models import Account,Position,Scenario,_decode
from . import _deposit_portfolio_income_reference as c
RIGHTS=('dated_interest_receivable','dated_interest_payable','dated_prepaid_expense','dated_deferred_income')
def _finite(x):
    if type(x) not in (float,int) or not math.isfinite(x):raise ArithmeticError('finite monetary input required')
    return float(x)
product=SimpleNamespace(_finite=_finite)
def _keys(value,names,path):c.keys(value,names)
def _equal(a,b,path):c.equal(a,b,path)
def _check(check):
    if check():
        raise InterruptedError('mixed source verification cancelled')

def _label(value, limit=256):
    if (not isinstance(value, str) or not value.strip() or len(value.encode()) > limit
            or any(unicodedata.category(c) == 'Cc' for c in value)):
        raise ArithmeticError('mixed bounded explicit source identity required')

def _selector(raw, runs):
    if raw == {'kind': 'all'}:
        return None
    _keys(raw, 'kind scenario_id', 'mixed/run_selector')
    if raw['kind'] != 'named' or raw['scenario_id'] not in runs:
        raise ArithmeticError('mixed unknown source execution selector')
    return raw['scenario_id']

def _normalize(raw):
    allowed = {'version', 'provenance', 'horizon_days', 'accounts', 'positions',
               'scenarios', 'reverse_severities', 'ruleset', 'netting_sets', 'policies', 'cashflows'}
    if not isinstance(raw, dict) or set(raw)-allowed or raw.get('version') != 'balance-stress-2':
        raise ArithmeticError('mixed explicit typed specification required')
    if any(raw.get(k, []) for k in ('netting_sets', 'policies', 'cashflows')):
        raise ArithmeticError('mixed unsupported legacy/credit/netting owners')
    result = dict(horizon_days=raw.get('horizon_days', 360),
        accounts=[asdict(_decode(Account, a)) for a in raw.get('accounts', [])],
        positions=[asdict(_decode(Position, p)) for p in raw.get('positions', [])],
        scenarios=[asdict(_decode(Scenario, s)) for s in raw.get('scenarios', [])],
        reverse_severities=deepcopy(raw.get('reverse_severities', [])),
        ruleset=raw.get('ruleset', 'research-weights-v2'), netting_sets=[], policies=[], cashflows=[])
    if (type(result['horizon_days']) is not int or not 30 <= result['horizon_days'] <= 1080
            or result['ruleset'] != 'research-weights-v2' or result['reverse_severities']
            or not result['accounts'] or not result['scenarios'] or len(result['positions']) > 512):
        raise ArithmeticError('mixed explicit calendar/ruleset/run/position admission differs')
    for field, identity in [('accounts', 'id'), ('positions', 'id'), ('scenarios', 'name')]:
        values = [r[identity] for r in result[field]]
        if len(values) != len(set(values)):
            raise ArithmeticError('mixed duplicate opening identity')
        for value in values:
            _label(value, 120)
    if any(s['name'] == 'baseline' for s in result['scenarios']):
        raise ArithmeticError('mixed reserved baseline scenario identity')
    if len(result['accounts']) > 50 or len(result['scenarios']) > 8 or len({(a['entity'],a['currency']) for a in result['accounts']}) != len(result['accounts']):
        raise ArithmeticError('mixed bounded unique entity/currency account admission')
    def ranges(row, names, lower, upper):
        for k in names.split():
            v = product._finite(row[k])
            if not lower <= v <= upper:
                raise ArithmeticError('mixed financial input domain: '+k)
    for a in result['accounts']:
        _label(a['entity']); _label(a['currency'])
        ranges(a, 'cash cash_floor capital_deductions annual_fees annual_costs annual_dividends', 0.,1e15)
        ranges(a, 'cet1_floor leverage_floor tax_rate htm_asset_limit', 0.,1.)
        ranges(a, 'lcr_floor nsfr_floor', 0.,10.); product._finite(a['equity'])
        if type(a['include_aoci']) is not bool:
            raise ArithmeticError('mixed explicit boolean capital policy required')
    ids = {a['id'] for a in result['accounts']}
    for p in result['positions']:
        if p['account'] not in ids or p['kind'] not in {'loan','security','deposit','funding','repo','reverse_repo'}:
            raise ArithmeticError('mixed unsupported position/account identity')
        if p['classification'] not in {'ac','htm'} or (p['kind'] != 'security' and p['classification'] != 'ac'):
            raise ArithmeticError('mixed performing accounting classification domain')
        ranges(p, 'balance commitment allowance opening_accrued', 0.,1e15)
        ranges(p, 'book_adjustment', -1e15,1e15)
        ranges(p, 'floating_beta asf_weight rsf_weight lcr_outflow_weight hqla_weight annual_pd lgd watch_fraction migration_rate draw_fraction monthly_runoff uninsured_fraction concentration operational_fraction digital_fraction rollover eligible_fraction encumbered_fraction haircut lcr_inflow_weight', 0.,1.)
        ranges(p, 'risk_weight watch_pd_multiplier duration', 0.,100.)
        ranges(p, 'rate', -1.,2.); ranges(p, 'opening_market_price', .000001,100.)
        if (p['balance']+p['book_adjustment'] < p['allowance'] or (p['balance']==0. and p['book_adjustment']!=0.)
                or p['hqla_level'] not in {'level1','level2a'} or type(p['payment_interval_days']) is not int or p['payment_interval_days']<1):
            raise ArithmeticError('mixed original gross carrying/recognition domain')
    for s in result['scenarios']:
        if type(s['start_day']) is not int or s['start_day']<1:
            raise ArithmeticError('mixed scenario activation day domain')
        ranges(s, 'rate_shift spread_shift', -.5,.5)
        ranges(s, 'deposit_flight lgd_add rollover_loss haircut_add', 0.,1.)
        ranges(s, 'pd_multiplier migration_multiplier draw_multiplier market_shock', 0.,20.)
    return result

def _rights(earned, paid, settled):
    value = earned-(paid if settled else 0.)
    return (0., 0., max(value, 0.), min(value, 0.)) if settled else (max(value, 0.), min(value, 0.), 0., 0.)

def _cumulative(segments, day, field):
    value = 0.
    for segment in segments:
        fraction = max(0, min(day, segment['end_ordinal'])-segment['start_ordinal']) / (segment['end_ordinal']-segment['start_ordinal'])
        value += segment[field]*fraction
    return product._finite(value)

def _backbook(raw, spec, check):
    """Admit complete explicit source/plan ownership without a native callback."""
    _keys(raw, 'schema monetary_unit recognition_policy specification dated', 'mixed/backbook')
    if raw['schema'] != 'dated-ledger-1' or raw['monetary_unit'] != 'currency_units':
        raise ArithmeticError('mixed dated physical monetary contract required')
    policy = raw['recognition_policy']
    _keys(policy, 'policy_id policy_revision recognition_model fees', 'mixed/backbook/policy')
    _label(policy['policy_id']); _label(policy['policy_revision'])
    if policy['recognition_model'] != 'supplied_calendar_segments-1' or policy['fees'] != 'immediate_earnings':
        raise ArithmeticError('mixed reviewed supplied backbook recognition policy required')
    dated = raw['dated']
    _keys(dated, 'schema as_of_ordinal reporting_horizon_days runs owners sources accrual_plans', 'mixed/backbook/dated')
    if dated['schema'] != 'dated-store-1' or dated['reporting_horizon_days'] != spec['horizon_days']:
        raise ArithmeticError('mixed dated source/report calendar differs')
    if type(dated['as_of_ordinal']) is not int or not 1 <= dated['as_of_ordinal'] <= 3652059-spec['horizon_days']:
        raise ArithmeticError('mixed exact bounded as-of calendar required')
    if not dated['owners'] or not dated['sources']:
        raise ArithmeticError('mixed complete nonempty actual backbook ownership required')
    if len(dated['owners'])>512 or len(dated['sources'])>512 or len(dated['accrual_plans'])>250000:
        raise ArithmeticError('mixed bounded owner/source/plan counts exceeded')
    expected_runs = {('baseline', 1.)} | {(s['name'], 1.) for s in spec['scenarios']}
    if len(dated['runs']) != len(expected_runs) or {(r['scenario_id'], r['severity']) for r in dated['runs']} != expected_runs:
        raise ArithmeticError('mixed exact baseline/scenario execution coverage required')
    runs = {r['run_id']: r for r in dated['runs']}
    if len(runs) != len(dated['runs']):
        raise ArithmeticError('mixed duplicate dated run identity')
    for run in runs.values():
        _keys(run, 'run_id scenario_id severity input_revision', 'mixed/backbook/run')
        for field in ('run_id', 'scenario_id', 'input_revision'):
            _label(run[field])
    owners = {o['position_id']: o for o in dated['owners']}
    positions = {p['id']: p for p in spec['positions']}
    accounts = {a['id']: a for a in spec['accounts']}
    if len(owners) != len(dated['owners']) or owners.keys() != positions.keys():
        raise ArithmeticError('mixed every actual backbook position needs one owner')
    for owner in owners.values():
        _keys(owner, 'position_id account_id currency direction ownership source_id source_revision', 'mixed/backbook/owner')
        for value in owner.values():
            _label(value)
        if len(owner['currency']) != 3 or not owner['currency'].isascii() or not owner['currency'].isupper() or not owner['currency'].isalpha():
            raise ArithmeticError('mixed exact currency code required')
        position = positions[owner['position_id']]
        asset = position['kind'] in {'loan', 'security', 'reverse_repo'}
        if (owner['account_id'] != position['account'] or owner['currency'] != accounts[position['account']]['currency']
                or owner['direction'] != ('receipt' if asset else 'payment')
                or owner['ownership'] not in {'contractual_dated_native_state', 'fully_scenario_generated'}):
            raise ArithmeticError('mixed backbook account/currency/direction ownership differs')
        zero_fields = 'start_day allowance commitment annual_pd migration_rate draw_fraction monthly_runoff floating_beta encumbered_fraction eligible_fraction'.split()
        if (any(position[k] != 0. for k in zero_fields) or position['collateral_position'] or position['collateral_pool']
                or position['classification'] not in {'ac', 'htm'} or position['opening_market_price'] != 1.):
            raise ArithmeticError('mixed backbook credit/draw/mark/collateral history unsupported')
    plans = {}
    segment_count=0
    for pi,plan in enumerate(dated['accrual_plans']):
        if pi % 256 == 0:
            _check(check)
        _keys(plan, 'position_id event_id run segments', 'mixed/backbook/plan')
        segment_count+=len(plan['segments'])
        if segment_count>250000:
            raise ArithmeticError('mixed bounded explicit recognition segment count exceeded')
        selector = _selector(plan['run'], runs)
        key = (plan['position_id'], plan['event_id'], selector)
        if key in plans or not plan['segments']:
            raise ArithmeticError('mixed duplicate/empty explicit recognition plan')
        plans[key] = plan['segments']
    events = []; coverage = {}; seen = set(); full = dated['as_of_ordinal']+spec['horizon_days']; source_ids=set()
    economic_selectors={}; event_counts=defaultdict(int); named_coverage=set()
    for source in dated['sources']:
        _check(check)
        _keys(source, 'source_id source_revision schedule coverage', 'mixed/backbook/source')
        _label(source['source_id']); _label(source['source_revision'])
        sid=source['source_id'],source['source_revision']
        if sid in source_ids:
            raise ArithmeticError('mixed duplicate raw schedule source identity')
        source_ids.add(sid)
        schedule = source['schedule']
        _keys(schedule, 'schema as_of_ordinal horizon_days scenarios events', 'mixed/backbook/schedule')
        if (schedule['schema'] != 'dated-cashflow-1' or schedule['as_of_ordinal'] != dated['as_of_ordinal']
                or set(schedule['scenarios']) != runs.keys() or len(schedule['scenarios']) != len(runs)
                or type(schedule['horizon_days']) is not int or schedule['horizon_days'] < spec['horizon_days']):
            raise ArithmeticError('mixed backbook original source calendar/run completeness differs')
        end = schedule['as_of_ordinal']+schedule['horizon_days']; full = max(full, end)
        if end-dated['as_of_ordinal'] > 1080:
            raise ArithmeticError('mixed complete actual-date tail admission exceeded')
        if end>3652059 or len(events)+len(schedule['events'])>250000 or len(coverage)+len(source['coverage'])>250000:
            raise ArithmeticError('mixed complete source date/event/coverage admission exceeded')
        for row in source['coverage']:
            _keys(row, 'position_id run status', 'mixed/backbook/coverage')
            key = (row['position_id'], _selector(row['run'], runs))
            owner = owners.get(row['position_id'])
            if key in coverage or owner is None or (owner['source_id'], owner['source_revision']) != (source['source_id'], source['source_revision']):
                raise ArithmeticError('mixed duplicate/mismatched source ownership coverage')
            coverage[key] = row['status']
            if key[1] is not None:
                named_coverage.add(key[0])
        previous_payment=dated['as_of_ordinal']
        for ei,event in enumerate(schedule['events']):
            if ei % 256 == 0:
                _check(check)
            _keys(event, 'position_id event_id scenario payment_ordinal day_offset accrual_period currency components measure provenance adjustment', 'mixed/backbook/event')
            owner = owners.get(event['position_id']); selector = _selector(event['scenario'], runs)
            key = (event['position_id'], event['event_id'], selector)
            if key in seen or owner is None or (owner['source_id'], owner['source_revision']) != (source['source_id'], source['source_revision']):
                raise ArithmeticError('mixed duplicate/unknown economic source event')
            seen.add(key); _label(event['event_id'])
            selectors=economic_selectors.setdefault(key[:2],set())
            if (selector is None and selectors) or (selector is not None and None in selectors):
                raise ArithmeticError('mixed all/named economic event identity overlaps')
            selectors.add(selector);event_counts[key[0],selector]+=1
            provenance=event['provenance'];kind=provenance.get('kind')
            fields={'contractual':'kind source_id source_revision','modeled':'kind model_id model_version input_revision','assumed':'kind assumption_id rationale'}
            if kind not in fields:
                raise ArithmeticError('mixed explicit original provenance required')
            _keys(provenance,fields[kind],'mixed/backbook/provenance')
            for field,text in provenance.items():
                _label(text,4096 if field=='rationale' else 256)
            if (event['currency'] != owner['currency'] or event['measure'] not in {'physical', 'contractual'}
                    or (owner['ownership'] == 'contractual_dated_native_state' and event['measure'] != 'contractual')
                    or event['adjustment'] != {'kind': 'none'}):
                raise ArithmeticError('mixed pricing/survival overlay source unsupported')
            day = event['payment_ordinal']
            if type(day) is not int or not dated['as_of_ordinal'] <= day <= end or type(event['day_offset']) is not int or event['day_offset'] != day-dated['as_of_ordinal']:
                raise ArithmeticError('mixed exact settlement date/offset required')
            if day<previous_payment:
                raise ArithmeticError('mixed raw dated events must retain chronological source order')
            previous_payment=day
            components = event['components']
            _keys(components, 'principal cash_interest accrued_interest book_amortization fees', 'mixed/backbook/components')
            for amount in components.values():
                product._finite(amount)
            side = 1. if owner['direction'] == 'receipt' else -1.
            if side*components['principal'] < 0. or components['fees'] != 0.:
                raise ArithmeticError('mixed explicit principal polarity; no draw/fee source admitted')
            segments = plans.get(key, [])
            period = event['accrual_period']
            if period is not None:
                _keys(period,'start_ordinal end_ordinal','mixed/backbook/period')
                if type(period['start_ordinal']) is not int or type(period['end_ordinal']) is not int or not 1<=period['start_ordinal']<period['end_ordinal']<=end:
                    raise ArithmeticError('mixed original accrual period calendar domain')
            if segments:
                _keys(period, 'start_ordinal end_ordinal', 'mixed/backbook/accrual_period')
                cursor = period['start_ordinal']
                for segment in segments:
                    _keys(segment, 'start_ordinal end_ordinal accrued_interest book_amortization', 'mixed/backbook/segment')
                    if type(segment['start_ordinal']) is not int or type(segment['end_ordinal']) is not int or segment['start_ordinal'] != cursor or not cursor < segment['end_ordinal'] <= period['end_ordinal']:
                        raise ArithmeticError('mixed recognition segments must exactly partition original period')
                    product._finite(segment['accrued_interest']); product._finite(segment['book_amortization']); cursor = segment['end_ordinal']
                if cursor != period['end_ordinal']:
                    raise ArithmeticError('mixed recognition source tail missing')
                for field in ('accrued_interest', 'book_amortization'):
                    _equal(math.fsum(s[field] for s in segments), components[field], 'mixed/backbook/recognition_total')
            elif components['accrued_interest'] != 0. or components['book_amortization'] != 0.:
                raise ArithmeticError('mixed nonzero recognition lacks explicit calendar plan')
            events.append(dict(event=event, source=source, selector=selector, segments=segments))
    if set(plans)-seen:
        raise ArithmeticError('mixed orphaned recognition plans')
    for position in owners:
        common = coverage.get((position, None))
        if common is not None and position in named_coverage:
            raise ArithmeticError('mixed common/named source coverage overlaps')
        for run in runs:
            status = common if common is not None else coverage.get((position, run))
            selected = event_counts[position,None]+event_counts[position,run]
            if status is None or (status.get('kind') == 'complete_event_set') != bool(selected) or status.get('kind') not in {'complete_event_set', 'no_events'}:
                raise ArithmeticError('mixed missing/false complete event coverage')
            _keys(status,'kind' if status['kind']=='complete_event_set' else 'kind rationale','mixed/backbook/coverage_status')
            if status['kind']=='no_events':
                _label(status['rationale'],4096)
    # Stable original source order breaks equal-date ties independently of the
    # producer's arrays; the admitted dated schedule orders by settlement date.
    events.sort(key=lambda e:e['event']['payment_ordinal'])
    return owners, runs, events, full

def _path_rights(side, coupon, value):
    """Signed monetary right, classified by the contractual interest direction."""
    return ((max(value, 0.), 0., 0., min(value, 0.)) if side*coupon > 0.
            else (0., min(value, 0.), max(value, 0.), 0.))

def _side(position):
    return 1. if position['kind'] in {'loan', 'security', 'reverse_repo'} else -1.

def _principal(position):
    return 'asset_principal' if _side(position) > 0. else 'funding_principal'

def _rights_rows(selected, state, scenario, runid, end, pools, pool_for_event, acquisitions):
    rows = []
    path_ids = {a['event_id']:a['path_id'] for a in acquisitions}
    for e, value in zip(selected,state,strict=True):
        event = e['event']; owner = e['owner']; components = event['components']; settled = value['settled']
        paid = components['cash_interest'] if settled else 0.; contribution = value['earned']-paid
        row = dict(scenario=scenario,run_id=runid,account=owner['account_id'],position_id=owner['position_id'],event_id=event['event_id'],
            source_id=owner['source_id'],source_revision=owner['source_revision'],payment_ordinal=event['payment_ordinal'],
            full_contract_end_ordinal=end,cash_settled=settled,earned_interest=value['earned'],cash_interest_settled=paid,
            signed_interest_right=contribution,recognized_book_amortization=value['book'],
            remaining_principal_settlement=0. if settled else components['principal'],remaining_cash_interest=0. if settled else components['cash_interest'],
            remaining_fees=0. if settled else components['fees'],remaining_interest_recognition=components['accrued_interest']-value['earned'],
            remaining_book_recognition=components['book_amortization']-value['book'])
        pool = pool_for_event.get((runid,event['event_id']))
        if pool is not None:
            row.update(right_ownership='physical_path_net_rights-1',path_id=path_ids[pool],event_right_contribution=contribution,signed_interest_right=pools[pool])
        rows.append(row)
    return rows

def _reporting_rights(raw, events, acquisitions, pools, signs):
    ordinal = raw['backbook']['dated']['as_of_ordinal']+raw['backbook']['dated']['reporting_horizon_days']; output = []
    for run in raw['backbook']['dated']['runs']:
        runid = run['run_id']; selected = [e for e in events if e['selector'] in {None,runid}]
        for e in selected:
            event = e['event']; owner = e['owner']
            if (runid,event['event_id']) in pools:
                continue
            earned = _cumulative(e['segments'],ordinal,'accrued_interest'); book = _cumulative(e['segments'],ordinal,'book_amortization')
            settled = event['payment_ordinal'] <= ordinal; paid = event['components']['cash_interest'] if settled else 0.; net = earned-paid
            output.append(dict(scenario=run['scenario_id'],run_id=runid,position_id=owner['position_id'],account=owner['account_id'],
                source_id=owner['source_id'],source_revision=owner['source_revision'],event_id=event['event_id'],ordinal=ordinal,
                right_ownership='original_event_rights-1',signed_interest_right=net,gross_rights=list(_rights(earned,paid,settled)),
                earned_interest=earned,cash_interest_settled=paid,recognized_book_amortization=book))
        for a in acquisitions:
            if a['run_id'] != runid:
                continue
            earned = paid = book = 0.; ids = []
            for e in selected:
                event = e['event']
                if pools.get((runid,event['event_id'])) != a['event_id']:
                    continue
                earned += _cumulative(e['segments'],ordinal,'accrued_interest'); book += _cumulative(e['segments'],ordinal,'book_amortization')
                paid += event['components']['cash_interest'] if event['payment_ordinal'] <= ordinal else 0.; ids.append(event['event_id'])
            net = earned-paid; side = 1. if next(e['owner']['direction'] for e in selected if pools.get((runid,e['event']['event_id']))==a['event_id'])=='receipt' else -1.
            output.append(dict(scenario=run['scenario_id'],run_id=runid,position_id=a['position_id'],account=a['account_id'],
                source_id=a['source']['id'],source_revision=a['source']['revision'],event_id=a['event_id'],path_id=a['path_id'],ordinal=ordinal,
                right_ownership='physical_path_net_rights-1',coupon_sign=signs[a['event_id']],signed_interest_right=net,
                gross_rights=list(_path_rights(side,signs[a['event_id']],net)),earned_interest=earned,cash_interest_settled=paid,
                recognized_book_amortization=book,native_event_ids=sorted(ids)))
    return output

def _observe(spec, positions, accounts, rights, accrued, scenario, day):
    """Independently derive disclosed research ratios from live raw exposures."""
    output = []
    ratio = lambda a, b: a/b if b > 0. else None
    for a in accounts:
        assets = a['cash']; liabilities = rwa = outflow = rsf = collateral = undrawn = l2 = inflow = htm = 0.
        hqla = max(a['cash'], 0.); asf = max(a['equity'], 0.)
        for p in positions:
            if p['account'] != a['id']:
                continue
            carrying = p['balance']+p['book_adjustment']-p['allowance']
            if _side(p) > 0.:
                assets += carrying
                rwa += max(carrying, 0.)*p['risk_weight']*(1.+p['watch_fraction'])
                rsf += max(carrying, 0.)*p['rsf_weight']
                free = p['balance']*max(1.-p['encumbered_fraction'], 0.)
                if p['hqla_level'] == 'level2a':
                    l2 += free*p['hqla_weight']
                else:
                    hqla += free*p['hqla_weight']
                inflow += p['balance']*p['lcr_inflow_weight']
                # All admitted sources exclude collateral and undrawn owners.
            else:
                liabilities += p['balance']+p['book_adjustment']
                asf += p['balance']*p['asf_weight']
                outflow += p['balance']*p['lcr_outflow_weight']
            r = rights[p['id']]
            assets += r[0]+r[2]; liabilities -= r[1]+r[3]
            if p['classification'] == 'htm':
                htm += p['balance']+p['book_adjustment']
        hqla += min(l2, hqla*.4/(1.-.4))
        outflow = max(outflow-min(inflow, .75*outflow), 0.)
        error = assets-liabilities-a['equity']
        cet1 = a['equity']-a['capital_deductions']; exposure = max(assets, 0.)+undrawn
        row = dict(scenario=scenario, account=a['id'], entity=a['entity'], currency=a['currency'], day=day,
            cash=a['cash'], restricted_cash=0., cash_floor=a['cash_floor'], assets=assets, liabilities=liabilities,
            equity=a['equity'], aoci=0., cet1=cet1, rwa=rwa, cet1_ratio=ratio(cet1, rwa),
            leverage_ratio=ratio(cet1, exposure), usable_collateral=collateral, undrawn=undrawn,
            hqla_proxy=hqla, lcr_proxy=ratio(hqla, outflow), nsfr_proxy=ratio(asf, rsf), reconciliation_error=error,
            cash_headroom=a['cash']-a['cash_floor'], cet1_headroom=cet1-a['cet1_floor']*rwa,
            leverage_headroom=cet1-a['leverage_floor']*exposure, lcr_headroom=hqla-a['lcr_floor']*outflow,
            nsfr_headroom=asf-a['nsfr_floor']*rsf, htm_headroom=a['htm_asset_limit']*max(assets, 0.)-htm)
        if abs(error) > 1e-8*max(1., abs(assets), abs(liabilities)):
            raise ArithmeticError('mixed independent daily balance sheet fails')
        if a['cash'] < a['cash_floor']-1e-9:
            raise ArithmeticError('mixed pooled daily cash floor fails')
        output.append(row)
    return output
def statements(trial, accounts, scenarios=()):
    """Derive closing statements solely from independently replayed GL balances."""
    funding = {(r['scenario'], r['account'], r['instrument_id']) for r in trial if r['gl_account'] == 'funding_principal'}
    totals = defaultdict(lambda: dict(assets=0., liabilities=0., equity=0., cash=0., restricted_cash=0., intercompany_assets=0., intercompany_liabilities=0.))
    for scenario in scenarios:
        for account in accounts:
            totals[scenario, account]  # retain zero-balance accounts without fabricating entries
    for r in trial:
        key = (r['scenario'], r['account'])
        t, gl, value = totals[key], r['gl_account'], r['balance']
        if gl in {'cash', 'restricted_cash'}:
            t[gl] += value
        if gl.startswith('pnl:') or gl in {'opening_equity', 'oci', 'equity_distributions'}:
            t['equity'] -= value
        elif gl in {'funding_principal', 'secured_funding', 'received_margin', 'intercompany_payable',
                    'dated_interest_payable', 'dated_deferred_income'}:
            t['liabilities'] -= value
        elif gl in {'book_adjustment', 'accrued_interest'} and (*key, r['instrument_id']) in funding:
            t['liabilities'] -= value
        elif gl == 'derivative_value' and value < 0:
            t['liabilities'] -= value
        else:
            t['assets'] += value
        if gl == 'intercompany_receivable':
            t['intercompany_assets'] += value
        elif gl == 'intercompany_payable':
            t['intercompany_liabilities'] -= value
    rows, consolidated = [], defaultdict(lambda: dict(assets=0., liabilities=0., equity=0., cash=0., restricted_cash=0.))
    for (scenario, account), t in totals.items():
        currency = accounts[account].currency
        error = t['assets']-t['liabilities']-t['equity']
        if abs(error) > 1e-8*max(1., abs(t['assets']), abs(t['liabilities'])):
            raise ArithmeticError('journal-derived financial statements do not balance')
        rows.append(dict(scenario=scenario, account=account, currency=currency, **t, reconciliation_error=error))
        group = consolidated[scenario, currency]
        for field in group:
            group[field] += t[field]
        group['assets'] -= t['intercompany_assets']
        group['liabilities'] -= t['intercompany_liabilities']
    return rows, [dict(scenario=s, currency=c, **r) for (s, c), r in consolidated.items()]
