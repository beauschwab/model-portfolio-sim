"""Independent raw dated-backbook/acquisition journal publication oracle.

This module is never a production execution backend. Backbook recognition is
reviewed supplied calendar recognition; candidate EIR economics are independently
reconstructed. Neither policy establishes observed or joint credit accuracy.
"""
from collections import defaultdict
from copy import deepcopy
from dataclasses import asdict
import json
import math
import unicodedata

from . import product_candidate_reference as candidate
from . import product_strategy_reference as product
from .balance_stress import Account, Position, Scenario, _decode

SCHEMA = 'product-mixed-dated-1'
RIGHTS = ('dated_interest_receivable', 'dated_interest_payable',
          'dated_prepaid_expense', 'dated_deferred_income')
MAX_WORK = 8_000_000
MAX_BYTES = 128 * 1024 * 1024


def _check(check):
    if check():
        raise InterruptedError('mixed source verification cancelled')


def _keys(value, names, path):
    candidate._keys(value, names, path)


def _equal(actual, expected, path):
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or actual.keys() != expected.keys():
            raise ArithmeticError(path + ': exact fields differ')
        for key, value in expected.items():
            _equal(actual[key], value, path + '/' + key)
    elif isinstance(expected, list):
        if not isinstance(actual, list) or len(actual) != len(expected):
            raise ArithmeticError(path + ': complete ordered rows differ')
        for index, value in enumerate(expected):
            _equal(actual[index], value, path + '/' + str(index))
    elif isinstance(expected, int) and not isinstance(expected, bool) and any(
            key in path.split('/') for key in ('day', 'day_offset', 'transaction_number',
                'reporting_horizon_days', 'admitted_work', 'full_end_ordinal',
                'reporting_end_ordinal', 'reporting_ordinal', 'original_container_end_ordinal')):
        if type(actual) is not int or actual != expected:
            raise ArithmeticError(path + ': exact integer differs')
    else:
        candidate._equal(actual, expected, path)


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


def _identity(value):
    _keys(value, 'id revision', 'mixed/identity')
    for text in value.values():
        _label(text, 120)


def _admit(raw, preparation, check):
    """Check actual openings and immutable economic source before any replay."""
    _check(check)
    _keys(raw, 'schema candidate backbook combined_specification policy opening_identity', 'mixed/raw')
    if raw['schema'] != SCHEMA or candidate._size(raw) > 32*1024*1024:
        raise ArithmeticError('mixed version/input byte admission exceeded')
    _identity(raw['opening_identity'])
    policy = raw['policy']
    _keys(policy, 'identity expectation phase backbook_basis tail fees defaults marks', 'mixed/policy')
    _identity(policy['identity'])
    product._fields(policy, dict(expectation='separately_supplied_physical_expected_paths-1',
        phase='opening_acquisition_recognition_settlement-1',
        backbook_basis='reviewed_supplied_dated_recognition-1', tail='retain_complete-1',
        fees='reject', defaults='reject', marks='reject'), 'mixed/policy')
    back = _normalize(raw['backbook']['specification'])
    spec = _normalize(raw['combined_specification'])
    for field in ('accounts', 'scenarios', 'horizon_days', 'reverse_severities', 'ruleset'):
        _equal(spec[field], back[field], 'mixed/actual_opening/'+field)
    for account in spec['accounts']:
        if any(account[k] != 0. for k in ('annual_fees', 'annual_costs', 'tax_rate', 'annual_dividends', 'capital_deductions')):
            raise ArithmeticError('mixed unreviewed account earnings/tax owner')
        for k in ('cash', 'equity', 'cash_floor'):
            product._finite(account[k])
        if account['cash'] < 0.:
            raise ArithmeticError('mixed actual opening cash must be nonnegative')
    back_owners, runs, events, end = _backbook(raw['backbook'], back, check)
    for p in back['positions']:
        if any(p[k] != 0. for k in ('rate', 'rollover', 'maturity_day', 'watch_fraction', 'pledged_face')) or p['collateral_pool'] or p['classification'] not in {'ac', 'htm'}:
            raise ArithmeticError('mixed backbook has unsupported generic overlays')
    selection = raw['candidate']
    _keys(selection, 'product allocation owners policy run_mapping', 'mixed/candidate')
    units = selection['product']['units']; owners = selection['owners']; maps = selection['run_mapping']
    allocations = selection['allocation']
    if not 1 <= len(units) <= 256 or len(owners) != len(units) or len(allocations) != len(units):
        raise ArithmeticError('mixed complete ordered candidate ownership required')
    p = selection['policy']
    _keys(p, 'identity book_basis acquisition_phase fees defaults marks tail', 'mixed/candidate/policy')
    _identity(p['identity'])
    product._fields(p, dict(book_basis='reviewed_original_eir-1', acquisition_phase='after_verified_opening_before_recognition-1',
        fees='reject', defaults='reject', marks='reject', tail='retain_complete-1'), 'mixed/candidate/policy')
    for owner in owners:
        _keys(owner, 'unit_id position_id account_id source ownership accounting_category ledger_classification pre_purchase_principal pre_purchase_accrued pre_purchase_basis', 'mixed/candidate/owner')
        _identity(owner['source'])
    if len({o['position_id'] for o in owners}) != len(owners) or len({tuple(o['source'].values()) for o in owners}) != len(owners):
        raise ArithmeticError('mixed unique candidate owners and sources required')
    positions = {p['id']:p for p in spec['positions']}; accounts = {a['id']:a for a in spec['accounts']}
    candidate_ids = {o['position_id'] for o in owners}
    if candidate_ids & set(back_owners) or set(positions) != candidate_ids | set(back_owners):
        raise ArithmeticError('mixed exact nonoverlapping actual/candidate position coverage required')
    _equal([p for p in spec['positions'] if p['id'] in back_owners], back['positions'], 'mixed/original_backbook_positions')
    for u, o, a in zip(units, owners, allocations, strict=True):
        _keys(a, 'unit_id face', 'mixed/allocation')
        if (a['unit_id'] != u['unit_id'] or o['unit_id'] != u['unit_id']
                or o['accounting_category'] != u['accounting_category'] or o['ownership'] != 'native_product_candidate-1'
                or o['ledger_classification'] not in {'ac', 'htm'}
                or any(o[k] != 0. for k in ('pre_purchase_principal', 'pre_purchase_accrued', 'pre_purchase_basis'))
                or product._finite(a['face']) < 0.):
            raise ArithmeticError('mixed reviewed zero pre-purchase candidate mapping required')
        pos = positions[o['position_id']]; account = accounts[o['account_id']]; deposit = u['purchase_graph']['kind'] == 'deposit'
        zeros = 'balance opening_accrued book_adjustment allowance start_day rate annual_pd watch_fraction migration_rate commitment draw_fraction monthly_runoff floating_beta rollover eligible_fraction encumbered_fraction pledged_face maturity_day'.split()
        if (pos['account'] != o['account_id'] or account['currency'] != selection['product']['currency']
                or pos['kind'] not in ({'deposit'} if deposit else {'loan', 'security'})
                or pos['classification'] != o['ledger_classification'] or any(pos[k] != 0. for k in zeros)
                or pos['collateral_position'] or pos['collateral_pool'] or pos['opening_market_price'] != 1.):
            raise ArithmeticError('mixed legacy ownership or nonzero candidate opening')
    for m in maps:
        _keys(m, 'source_scenario_id ledger_scenario_id severity', 'mixed/run_mapping')
    if (len(maps) != len(runs) or len({m['source_scenario_id'] for m in maps}) != len(maps)
            or {(m['ledger_scenario_id'], m['severity']) for m in maps} != {(r['scenario_id'], r['severity']) for r in runs.values()}):
        raise ArithmeticError('mixed exact candidate/backbook scenario mapping required')
    for m in maps:
        if (m['source_scenario_id'] == selection['product']['base_scenario_id']) != (m['ledger_scenario_id'] == 'baseline'):
            raise ArithmeticError('mixed baseline mapping differs')
    asof = raw['backbook']['dated']['as_of_ordinal']; report = asof + spec['horizon_days']
    if selection['product']['reporting_ordinals'][0] != asof or selection['product']['reporting_ordinals'][-1] != report:
        raise ArithmeticError('mixed exact actual/candidate report calendar required')
    _keys(preparation, 'schema units preparation', 'mixed/native_preparation')
    _equal(preparation['schema'], 'product-strategy-preparation-1', 'mixed/preparation_schema')
    product.verify_preparation(selection['product'], preparation['preparation'], check)
    _equal(preparation['units'], candidate._unit_identities(selection['product'], preparation['preparation']), 'mixed/unit_identities')
    if {m['source_scenario_id'] for m in maps} != set(preparation['preparation']['scenario_ids']):
        raise ArithmeticError('mixed complete physical source scenario coverage required')
    context = dict(selection, as_of_ordinal=asof, reporting_end_ordinal=report, specification=raw['combined_specification'])
    # Repeated backbook/event work and escaped metadata are charged before
    # reconstructing the first candidate daily source or journal row.
    bounds = [end, report]
    candidate_rows = 0
    for u in preparation['preparation']['units']:
        for income in u['income']:
            last = income['native_income']['native_income_result']['rows'][-1]['payment_ordinal']
            bounds.append(last); candidate_rows += last-u['purchase_ordinal']
    full_days = max(bounds)-asof+1
    scan_work = full_days*(len(events)*4+len(spec['positions'])*len(spec['accounts'])*4)+candidate_rows*32
    if scan_work > MAX_WORK:
        raise ArithmeticError('mixed aggregate independent replay work exceeded before daily generation')
    _capacity(raw,preparation,spec,events,max(bounds),check)
    capture, candidate_end = candidate._capture(context, preparation['preparation'], check)
    end = max(end, candidate_end, report)
    if end-asof > 1080:
        raise ArithmeticError('mixed complete source horizon exceeds explicit daily tier')
    return spec, back_owners, runs, events, capture, end, context


def _capacity(raw, preparation, spec, events, end, check=lambda:False):
    """Escape-aware complete metadata admission; never use amounts as truth."""
    asof = raw['backbook']['dated']['as_of_ordinal']; days = end-asof
    runs = raw['backbook']['dated']['runs']; selection = raw['candidate']; total = 0
    for ui,u in enumerate(preparation['preparation']['units']):
        _check(check)
        for income in u['income']:
            raw_income = income['native_income']['raw_income_request']
            rows = income['native_income']['native_income_result']['rows']; last = rows[-1]['payment_ordinal']; d = last-u['purchase_ordinal']
            source = next(s for s in u['complete_purchase_result']['native_sources'] if s['role']=='physical' and s['scenario_id']==income['scenario_id'] and s['path_id']==income['path_id'])
            metadata = [selection['owners'][ui],selection['policy'],raw['policy'],selection['run_mapping'],u['unit_id'],income['scenario_id'],income['path_id'],source['run_id'],source['projection'],raw_income['contract'],raw_income['policy'],raw_income['original_yield_anchor']]
            width = 4*candidate._size(metadata)+8192
            total += (d*24+days*2+32)*width
    owners = raw['backbook']['dated']['owners']; ranges = {}; cursor = 0
    for plan in raw['backbook']['dated']['accrual_plans']:
        selector = _selector(plan['run'],{r['run_id']:r for r in runs}); n = len(plan['segments'])
        ranges[plan['position_id'],plan['event_id'],selector] = [cursor,cursor+n]; cursor += n
    for index,e in enumerate(events):
        if index % 256 == 0:
            _check(check)
        event = e['event']; oi = next(i for i,o in enumerate(owners) if o['position_id']==event['position_id'])
        ri = None if e['selector'] is None else next(i for i,r in enumerate(runs) if r['run_id']==e['selector'])
        stored = dict(index=index,owner_index=oi,event_id=event['event_id'],run_index=ri,payment_ordinal=event['payment_ordinal'],
            day_offset=event['day_offset'],full_contract_end_ordinal=end,accrual_period=event['accrual_period'],components=event['components'],
            measure=event['measure'],provenance=event['provenance'],recognition_range=ranges.get((event['position_id'],event['event_id'],e['selector']),[0,0]))
        count = 8+sum(max(0,min(s['end_ordinal'],end)-max(s['start_ordinal'],asof)) for s in e['segments'])
        width = 4*candidate._size([stored,owners[oi],runs,raw['backbook']['recognition_policy']])+4096
        total += count*12*(len(runs) if ri is None else 1)*width
    for p in spec['positions']:
        total += (days+8)*len(runs)*(4*candidate._size([p,runs])+2048)
    for a in spec['accounts']:
        total += (days+8)*len(runs)*(4*candidate._size([a,runs])+8192)
    total += 4*(candidate._size(raw)+candidate._size(preparation))
    if total > MAX_BYTES:
        raise ArithmeticError('mixed aggregate escaped metadata capacity exceeded before daily generation')
    return total


def _coupon_sign(path):
    signs = {math.copysign(1., v) for v in
             [r['interest'] for r in path['recognition']] + [c['interest'] for c in path['cash']]
             if v != 0.}
    if len(signs) > 1:
        raise ArithmeticError('mixed sign-changing coupon requires explicit gross right ownership')
    return next(iter(signs), 1.)


def _path_rights(side, coupon, value):
    """Signed monetary right, classified by the contractual interest direction."""
    return ((max(value, 0.), 0., 0., min(value, 0.)) if side*coupon > 0.
            else (0., min(value, 0.), max(value, 0.), 0.))


def _side(position):
    return 1. if position['kind'] in {'loan', 'security', 'reverse_repo'} else -1.


def _principal(position):
    return 'asset_principal' if _side(position) > 0. else 'funding_principal'


def _combined(raw, back_owners, runs, back_events, capture, end):
    """Construct derived schedules only from admitted original sources."""
    asof = raw['backbook']['dated']['as_of_ordinal']; days = end-asof
    request = deepcopy(raw['backbook']); request['specification'] = deepcopy(raw['combined_specification'])
    request['specification']['horizon_days'] = days; request['dated']['reporting_horizon_days'] = days
    declared = []
    for source in request['dated']['sources']:
        declared.append(dict(source_id=source['source_id'], source_revision=source['source_revision'],
            original_container_end_ordinal=asof+source['schedule']['horizon_days']))
        source['schedule']['horizon_days'] = days
    acquisitions = dict(schema='dated-acquisition-1', policy=deepcopy(raw['policy']['identity']),
        events=[], right_ownership='physical_path_net_rights-1', reporting_ordinal=asof+raw['backbook']['dated']['reporting_horizon_days'])
    events = deepcopy(back_events); pools = {}; signs = {}
    for e in events:
        e['owner'] = deepcopy(back_owners[e['event']['position_id']])
    original_sources = {(o['source_id'],o['source_revision']) for o in back_owners.values()}
    for ui, o in enumerate(raw['candidate']['owners']):
        if (o['source']['id'],o['source']['revision']) in original_sources:
            raise ArithmeticError('mixed original/candidate source identity overlaps')
        u = raw['candidate']['product']['units'][ui]; side = -1. if u['purchase_graph']['kind']=='deposit' else 1.
        owner = dict(position_id=o['position_id'], account_id=o['account_id'], currency=raw['candidate']['product']['currency'],
            direction='receipt' if side>0. else 'payment', ownership='fully_scenario_generated',
            source_id=o['source']['id'], source_revision=o['source']['revision'])
        request['dated']['owners'].append(owner); source_events = []; coverage = []
        for path in capture['paths']:
            if path['unit_index'] != ui:
                continue
            mapping = next(m for m in raw['candidate']['run_mapping'] if m['source_scenario_id']==path['scenario_id'])
            run = next(r for r in runs.values() if r['scenario_id']==mapping['ledger_scenario_id'] and r['severity']==mapping['severity'])
            selector = dict(kind='named',scenario_id=run['run_id'])
            if not any(c['run']==selector for c in coverage):
                coverage.append(dict(position_id=o['position_id'],run=selector,status=dict(kind='complete_event_set')))
            factor = path['allocation_factor']*path['source_weight']; signed = side*factor; ids = []
            purchase = f"purchase:{ui}:{path['native_source_index']}"; coupon = _coupon_sign(path); signs[purchase] = coupon
            for row, cash in zip(path['daily_income']['rows'],path['daily_income_request']['events'],strict=True):
                ordinal = row['payment_ordinal']; event_id = f"candidate:{ui}:{path['native_source_index']}:{ordinal}"; ids.append(event_id)
                if cash['payment_ordinal'] != ordinal or cash['event_id'] != row['event_id']:
                    raise ArithmeticError('mixed original income/cash event identity differs')
                components = dict(principal=cash['principal_cash']*signed, cash_interest=cash['interest_cash']*signed,
                    accrued_interest=row['contractual_interest_earned']*signed, book_amortization=row['basis_amortization']*signed,fees=0.)
                interval = dict(start_ordinal=ordinal-1,end_ordinal=ordinal)
                event = dict(position_id=o['position_id'],event_id=event_id,scenario=deepcopy(selector), payment_ordinal=ordinal,
                    day_offset=ordinal-asof, accrual_period=interval, currency=owner['currency'], components=components,
                    measure='physical', provenance=dict(kind='modeled',model_id='product-candidate-source-1',model_version='product-candidate-source-1',
                        input_revision=raw['candidate']['product']['identity']['input_revision']), adjustment=dict(kind='none'))
                segment = dict(interval,accrued_interest=components['accrued_interest'],book_amortization=components['book_amortization'])
                source_events.append(event)
                request['dated']['accrual_plans'].append(dict(position_id=o['position_id'],event_id=event_id,run=deepcopy(selector),segments=[segment]))
                events.append(dict(event=event,owner=owner,selector=run['run_id'],segments=[segment]))
                pools[run['run_id'],event_id] = purchase
            acquisitions['events'].append(dict(event_id=purchase,run_id=run['run_id'],position_id=o['position_id'],account_id=o['account_id'],
                currency=owner['currency'],source=deepcopy(o['source']),path_id=path['path_id'],source_weight=path['source_weight'],
                allocation_factor=path['allocation_factor'],principal=path['unit_face']*factor,
                cost=path['daily_income_request']['opening_gross_carrying_value']*factor,ordinal=path['purchase_ordinal'],
                original_yield_anchor=deepcopy(u['income_policy']['original_yield_anchor']),measure='physical',recognition_event_ids=ids,coupon_sign=coupon))
        source_events.sort(key=lambda e:e['payment_ordinal'])
        request['dated']['sources'].append(dict(source_id=owner['source_id'],source_revision=owner['source_revision'],
            schedule=dict(schema='dated-cashflow-1',as_of_ordinal=asof,horizon_days=days,scenarios=list(runs),events=source_events),coverage=coverage))
    events.sort(key=lambda e:e['event']['payment_ordinal'])
    return request, acquisitions, declared, events, pools, signs


def _dated_work(spec, events, end, run_count, acquisitions=None):
    asof = end-spec['horizon_days']; work = len(spec['positions'])*spec['horizon_days']*run_count
    for e in events:
        repetitions = run_count if e['selector'] is None else 1
        work += repetitions*(2+sum(max(0,min(s['end_ordinal'],end)-max(s['start_ordinal'],asof)) for s in e['segments']))
    if acquisitions is not None:
        segments = sum(len(e['segments']) for e in events)
        work += (len(events)+segments)*run_count+len(spec['positions'])*2
        work += sum(len(a['recognition_event_ids']) for a in acquisitions)+len(acquisitions)*16
    return work


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


def _replay(raw, spec, events, acquisitions, end, pool_for_event, coupon_signs, check):
    """Own the complete journal ordering; producer postings are never inputs."""
    dated = raw['backbook']['dated']; asof = dated['as_of_ordinal']; horizon = end-asof
    run_by_scene = {r['scenario_id']:r for r in dated['runs']}
    scenarios = ['baseline']+[s['name'] for s in spec['scenarios']]
    output = {k:[] for k in ('journal', 'trial_balance', 'closing_statements', 'daily_balance',
        'dated_rights', 'event_lineage', 'acquisition_lineage', 'exposures', 'ledger',
        'reporting_rights', 'reported_daily_balance')}
    from .journal import statements
    for scenario in scenarios:
        _check(check); run = run_by_scene[scenario]; runid = run['run_id']
        positions = deepcopy(spec['positions']); by_id = {p['id']:p for p in positions}
        accounts = deepcopy(spec['accounts']); account_by_id = {a['id']:a for a in accounts}
        selected = [e for e in events if e['selector'] in {None, runid}]
        selected_acq = [a for a in acquisitions if a['run_id'] == runid]
        rights = {p['id']:[0., 0., 0., 0.] for p in positions}; accrued = {p['id']:p['opening_accrued'] for p in positions}
        state = []; pools = {a['event_id']:0. for a in selected_acq}; gl = {}; sequence = 0
        aggregates = {}
        for e in selected:
            interest = _cumulative(e['segments'], asof, 'accrued_interest'); book = _cumulative(e['segments'], asof, 'book_amortization')
            state.append(dict(earned=interest, book=book, settled=False))
            values = _rights(interest, 0., False)
            for i, value in enumerate(values):
                rights[e['event']['position_id']][i] += value
        for p in positions:
            _equal(sum(rights[p['id']]), _side(p)*p['opening_accrued'], 'mixed/opening_source_rights')

        def post(day, account, event, lines, cash=0., earnings=0., position='', aggregate=True):
            nonlocal sequence
            lines = list(lines)
            if aggregate:
                lines += [('cash', '', cash), ('pnl:'+event, position, -earnings)]
            lines = [line for line in lines if line[2] != 0.]
            if lines:
                if abs(math.fsum(l[2] for l in lines)) > 1e-9*max(1., sum(abs(l[2]) for l in lines)):
                    raise ArithmeticError('mixed independent journal event fails balance')
                sequence += 1
                for code, instrument, value in lines:
                    key = account, code, instrument
                    gl[key] = product._finite(gl.get(key, 0.)+value)
                    output['journal'].append(dict(transaction_number=sequence, scenario=scenario, day=day,
                        account=account, event=event, gl_account=code, instrument_id=instrument, value=value))
            if aggregate:
                a = account_by_id[account]; a['cash'] += cash; a['equity'] += earnings
                sums = aggregates.setdefault((account, event), [0., 0.]); sums[0] += cash; sums[1] += earnings
            return sequence if lines else None

        for a in accounts:
            lines = [('cash', '', a['cash']), ('opening_equity', '', -a['equity'])]
            for p in positions:
                if p['account'] == a['id']:
                    lines += [(_principal(p), p['id'], _side(p)*p['balance']),
                              ('book_adjustment', p['id'], _side(p)*p['book_adjustment']),
                              ('allowance', p['id'], -p['allowance'])]
                    lines += [(code, p['id'], value) for code, value in zip(RIGHTS, rights[p['id']], strict=True)]
            post(0, a['id'], 'opening', lines, aggregate=False)

        def apply_event(index, day, interest, book, settlement):
            e = selected[index]; event = e['event']; v = state[index]; pos = by_id[event['position_id']]
            pool = pool_for_event.get((runid, event['event_id']))
            before = (_path_rights(_side(pos), coupon_signs[pool], pools[pool]) if pool is not None
                      else _rights(v['earned'], event['components']['cash_interest'], v['settled']))
            if settlement and v['settled']:
                raise ArithmeticError('mixed duplicate settlement')
            v['earned'] += interest; v['book'] += book
            if settlement:
                v['settled'] = True
            if pool is not None:
                pools[pool] += interest-(event['components']['cash_interest'] if settlement else 0.)
                after = _path_rights(_side(pos), coupon_signs[pool], pools[pool])
            else:
                after = _rights(v['earned'], event['components']['cash_interest'], v['settled'])
            delta = [a-b for a, b in zip(after, before, strict=True)]
            for i, value in enumerate(delta):
                rights[pos['id']][i] += value
            accrued[pos['id']] = _side(pos)*sum(rights[pos['id']])
            pc = event['components']['principal'] if settlement else 0.; ic = event['components']['cash_interest'] if settlement else 0.
            if abs(pc) > pos['balance']+1e-9:
                raise ArithmeticError('mixed settlement exceeds live principal')
            pos['balance'] = max(pos['balance']-abs(pc), 0.); pos['book_adjustment'] += _side(pos)*book
            lines = [(_principal(pos), pos['id'], -_side(pos)*abs(pc)), ('book_adjustment', pos['id'], book)]
            lines += [(code, pos['id'], value) for code, value in zip(RIGHTS, delta, strict=True)]
            tx = post(day, pos['account'], 'dated_settlement' if settlement else 'dated_recognition', lines,
                      cash=pc+ic, earnings=interest+book, position=pos['id'])
            owner = e['owner']; provenance = event['provenance']
            output['event_lineage'].append(dict(scenario=scenario, run_id=runid, input_revision=run['input_revision'], day=day,
                transaction_number=tx, position_id=pos['id'], event_id=event['event_id'], phase='settlement' if settlement else 'recognition',
                source_id=owner['source_id'], source_revision=owner['source_revision'],
                provenance_json=json.dumps(provenance, separators=(',', ':'), ensure_ascii=False), measure=event['measure'], ownership=owner['ownership'],
                policy_id=raw['backbook']['recognition_policy']['policy_id'], policy_revision=raw['backbook']['recognition_policy']['policy_revision'],
                principal=pc, cash_interest=ic, accrued_interest=interest, book_amortization=book, fees=0.))

        for day in range(horizon+1):
            _check(check); ordinal = asof+day
            for a in selected_acq:
                if a['ordinal'] != ordinal:
                    continue
                pos = by_id[a['position_id']]; side = _side(pos); basis = a['cost']-a['principal']
                pos['balance'] += a['principal']; pos['book_adjustment'] += basis
                tx = post(day, pos['account'], 'dated_acquisition',
                          [(_principal(pos), pos['id'], side*a['principal']), ('book_adjustment', pos['id'], side*basis)],
                          cash=-side*a['cost'], position=pos['id'])
                output['acquisition_lineage'].append(dict(scenario=scenario, severity=1., run_id=runid, day=day, ordinal=ordinal,
                    position_id=pos['id'], account=pos['account'], event_id=a['event_id'], source=a['source'], path_id=a['path_id'],
                    source_weight=a['source_weight'], allocation_factor=a['allocation_factor'], original_yield_anchor=a['original_yield_anchor'],
                    measure='physical', policy=raw.get('policy',raw['candidate']['policy'])['identity'], principal=a['principal'], cost=a['cost'], basis=basis, transaction_number=tx))
            if day:
                for i, e in enumerate(selected):
                    interest = book = 0.; active = False
                    for s in e['segments']:
                        if s['start_ordinal'] <= ordinal-1 < s['end_ordinal']:
                            n = s['end_ordinal']-s['start_ordinal']; f0 = (ordinal-1-s['start_ordinal'])/n; f1 = (ordinal-s['start_ordinal'])/n
                            interest += s['accrued_interest']*f1-s['accrued_interest']*f0
                            book += s['book_amortization']*f1-s['book_amortization']*f0; active = True
                    if active:
                        apply_event(i, day, interest, book, False)
            for i, e in enumerate(selected):
                if e['event']['payment_ordinal'] == ordinal:
                    apply_event(i, day, 0., 0., True)
            rows = _observe(spec, positions, accounts, rights, accrued, scenario, day)
            if day <= 30 or day % 30 == 0 or day == horizon or day == spec['horizon_days']:
                output['daily_balance'] += rows
            if ordinal == asof+spec['horizon_days']:
                output['reported_daily_balance'] += rows
            if day == 0 or day % 30 == 0 or day == horizon:
                for p in positions:
                    output['exposures'].append(dict(scenario=scenario, day=day, account=p['account'], position=p['id'], kind=p['kind'],
                        balance=p['balance'], allowance=p['allowance'], watch_fraction=p['watch_fraction'], commitment=p['commitment'],
                        market_factor=1., encumbered_fraction=p['encumbered_fraction'], accrued_interest=accrued[p['id']]))
            for (account, event), sums in aggregates.items():
                if any(v != 0. for v in sums):
                    output['ledger'].append(dict(scenario=scenario, day=day, account=account, event=event, cash=sums[0], earnings=sums[1], aoci=0., memo_amount=0.))
            aggregates.clear()
        trial = [dict(scenario=scenario, account=a, gl_account=g, instrument_id=i, balance=v) for (a, g, i), v in sorted(gl.items())]
        output['dated_rights'] += _rights_rows(selected,state,scenario,runid,end,pools,pool_for_event,selected_acq)
        output['trial_balance'] += trial
        closing, _ = statements(trial, {a['id']:_decode(Account, a) for a in spec['accounts']}, [scenario])
        output['closing_statements'] += closing
        for p in positions:
            _equal(p['balance'], 0., 'mixed/complete_principal_tail')
            _equal(p['book_adjustment'], 0., 'mixed/complete_basis_tail')
            for value in rights[p['id']]:
                _equal(value, 0., 'mixed/complete_gross_right_tail')
    output['reporting_rights'] = _reporting_rights(raw,events,acquisitions,pool_for_event,coupon_signs)
    return output


def verify(raw, result, check=lambda:False):
    """Independently verify complete mixed economics, sources and ordered GL."""
    _check(check)
    _keys(result, 'schema preparation protocol complete journal trial_balance closing_statements daily_balance dated_rights event_lineage acquisition_lineage exposures ledger reporting_rights reported_daily_balance independent_persisted_verification dynamic_mixed_acceptance scope reporting_end_ordinal full_end_ordinal', 'mixed/result')
    _keys(result['preparation'], 'raw_request native_preparation candidate_capture combined_dated_request acquisitions backbook_source_ends reporting_end_ordinal full_end_ordinal admitted_work', 'mixed/preparation')
    if candidate._size(result) > 32*1024*1024:
        raise ArithmeticError('mixed retained complete output byte admission exceeded')
    if result['schema'] != SCHEMA or type(result['protocol']) is not int or result['protocol'] != 3 or result['complete'] is not True:
        raise ArithmeticError('mixed native complete protocol fence differs')
    if result['independent_persisted_verification'] is not False or result['dynamic_mixed_acceptance'] is not False:
        raise ArithmeticError('mixed producer cannot declare independent/dynamic acceptance')
    _equal(result['scope'], 'single pooled native full-tail expected dated backbook/candidate GL with reviewed original opening and candidate EIR anchors; supplied dated backbook recognition, separate physical expectations, end-of-day cash constraints; no observed-settlement, accounting-standard, empirical, joint-credit/stochastic or independently persisted verification claim', 'mixed/scope')
    p = result['preparation']; _equal(p['raw_request'],raw,'mixed/immutable_raw_source')
    spec, owners, runs, back_events, capture, end, context = _admit(raw,p['native_preparation'],check)
    _equal(p['candidate_capture'],capture,'mixed/independent_candidate_capture')
    combined, acquisitions, declared, events, pools, signs = _combined(raw,owners,runs,back_events,capture,end)
    _equal(p['combined_dated_request'],combined,'mixed/complete_derived_source')
    _equal(p['acquisitions'],acquisitions,'mixed/once_only_acquisitions')
    _equal(p['backbook_source_ends'],declared,'mixed/original_source_container_ends')
    report = context['reporting_end_ordinal']
    for container in (p,result):
        _equal(container['reporting_end_ordinal'],report,'mixed/reporting_end_ordinal')
        _equal(container['full_end_ordinal'],end,'mixed/full_end_ordinal')
    fullspec = deepcopy(spec); fullspec['horizon_days'] = end-context['as_of_ordinal']
    _, candidate_work = candidate._journal_admission(context,p['native_preparation']['preparation'],
        [path['full_end_ordinal'] for path in capture['paths']],capture['admitted_daily_rows'],capture['admitted_work'])
    indexing = (len(back_events)+sum(len(e['segments']) for e in back_events))*len(runs)
    # The combined dated admission already includes the entire backbook work.
    # Source inspection/indexing is separately charged, with no second replay.
    work = candidate_work+indexing+capture['admitted_daily_rows']*24
    work += _dated_work(fullspec,events,end,len(runs),acquisitions['events'])
    if work > MAX_WORK:
        raise ArithmeticError('mixed complete admitted aggregate work exceeded')
    _equal(p['admitted_work'],work,'mixed/admitted_work')
    _check(check)
    expected = _replay(raw,spec,events,acquisitions['events'],end,pools,signs,check)
    # Provenance JSON is a typed serialized identity. Dictionary key order is
    # not contractual; every field/value remains exact and no financial row is
    # reordered, grouped or netted during journal verification.
    actual_lineage = deepcopy(result['event_lineage']); expected_lineage = expected['event_lineage']
    for rows in (actual_lineage,expected_lineage):
        for index,row in enumerate(rows):
            if index % 256 == 0:
                _check(check)
            row['provenance_json'] = json.loads(row['provenance_json'])
    expected['event_lineage'] = expected_lineage
    for key, rows in expected.items():
        _check(check)
        _equal(actual_lineage if key=='event_lineage' else result[key],rows,'mixed/'+key)
    _check(check)
    return dict(verified=True,mixed_performing_only=True,ordered_mixed_journal_recomputed=True,
        complete_cash_rights_basis_tail_verified=True,original_candidate_eir_source_recomputed=True,
        supplied_backbook_calendar_source_recomputed=True,dynamic_credit_acceptance=False,
        empirical_validation=False,global_optimality_proven=False)
