"""Independent raw-economics and ordered candidate journal publication gates."""
from copy import deepcopy
import json
import math
from pathlib import Path

import pytest

from portfolio_risk.analytics.product_candidate_reference import verify
from portfolio_risk.core.product_candidate_native import run

ROOT = Path(__file__).resolve().parents[3]


def example():
    return json.loads((ROOT/'examples/models/product-candidate-journal-1.json').read_text())


@pytest.fixture(scope='module')
def valid():
    raw = example()
    return raw, run(raw, threads=1)[0]


def forbid_native(monkeypatch):
    from portfolio_risk.core import quant_native, decision_native, product_candidate_native
    def forbidden(*args, **kwargs):
        pytest.fail('candidate verification called native finance')
    monkeypatch.setattr(quant_native, 'term_call', forbidden)
    monkeypatch.setattr(decision_native.NativeDecision, 'call', forbidden)
    monkeypatch.setattr(product_candidate_native, 'run', forbidden)


def test_independent_purchase_income_and_terminal_book_hand_calculations(valid, monkeypatch):
    raw, result = valid
    forbid_native(monkeypatch)
    receipt = verify(raw, result)
    assert receipt['ordered_candidate_journal_recomputed']
    assert receipt['complete_cash_rights_basis_tail_verified']
    assert not receipt['dynamic_mixed_acceptance']
    assert not receipt['empirical_validation'] and not receipt['global_optimality_proven']
    assert receipt == verify(json.loads(json.dumps(raw)), json.loads(json.dumps(result)))
    units = raw['product']['units']
    cost = sum(a['face']*u['quote_price_per_face_unit'] for a, u in zip(raw['allocation'], units, strict=True))
    for a in result['complete_closing_accounts']:
        assert a['cash'] == pytest.approx(1000-cost+104.45, abs=1e-10)
        assert a['cumulative_net_effective_income'] == pytest.approx(104.45-cost, abs=1e-10)
    first = result['journals'][0]
    assert first['phase'] == 'acquisition'
    assert [p['gl_account'] for p in first['postings']] == ['asset_principal', 'asset_book_adjustment', 'cash']
    assert [p['value'] for p in first['postings']] == pytest.approx([20, 20*(units[0]['quote_price_per_face_unit']-1), -20*units[0]['quote_price_per_face_unit']])
    recognition = next(j for j in result['journals'] if j['phase'] == 'recognition')
    y = units[0]['income_policy']['original_annual_effective_rate']
    effective = 20*units[0]['quote_price_per_face_unit']*math.expm1(math.log1p(y)/365)
    assert recognition['postings'][-1]['value'] == pytest.approx(-effective, abs=1e-12)
    assert recognition['postings'][-2]['value'] == pytest.approx(effective-20*.365/365, abs=1e-12)
    assert [j['transaction_id'] for j in result['journals']] == list(range(1, len(result['journals'])+1))
    assert all(abs(sum(p['value'] for p in j['postings'])) < 1e-9 for j in result['journals'])
    assert all(abs(s['gross_carrying_value']) < 1e-9 and s['allocated_principal'] == 0 for s in result['complete_closing_positions'])


@pytest.mark.parametrize('path', [
    ('raw_request', 'verified_opening_identity', 'revision'),
    ('raw_request', 'product', 'identity', 'crn_id'),
    ('normalized_specification', 'positions', 0, 'rate'),
    ('native_preparation', 'units', 0, 'capital_policy_revision'),
    ('native_preparation', 'preparation', 'values', 0),
    ('native_preparation', 'preparation', 'funding', 'demand', 0),
    ('source_capture', 'paths', 0, 'source_weight'),
    ('source_capture', 'paths', 0, 'allocation_factor'),
    ('source_capture', 'paths', 0, 'native_source_index'),
    ('source_capture', 'paths', 0, 'cash', 0, 'event_id'),
    ('source_capture', 'paths', 0, 'cash', 0, 'principal'),
    ('source_capture', 'paths', 0, 'recognition', 0, 'interest'),
    ('source_capture', 'paths', 0, 'daily_income_request', 'original_yield_anchor', 'revision'),
    ('source_capture', 'paths', 0, 'daily_income', 'rows', 0, 'effective_interest'),
    ('source_capture', 'paths', 0, 'daily_income', 'rows', 0, 'basis_amortization'),
    ('journals', 0, 'transaction_id'),
    ('journals', 0, 'ordinal'),
    ('journals', 0, 'source', 'revision'),
    ('journals', 0, 'policy', 'revision'),
    ('journals', 0, 'native_event_ids', 0),
    ('journals', 0, 'postings', 0, 'value'),
    ('daily_positions', 0, 'book_adjustment'),
    ('daily_positions', 0, 'path_id'),
    ('daily_accounts', 0, 'cash'),
    ('daily_accounts', 1, 'equity'),
    ('reported_positions', 0, 'effective_interest_to_date'),
    ('complete_closing_positions', 0, 'full_end_ordinal'),
    ('complete_closing_accounts', 0, 'cumulative_net_effective_income'),
    ('trial_balance', 0, 'balance'),
    ('trial_balance', 0, 'path_id'),
    ('admitted_work',),
    ('full_end_ordinal',),
])
def test_all_finance_source_and_lineage_tampering_fails(valid, path):
    raw, result = valid
    bad = deepcopy(result); obj = bad
    for key in path[:-1]:
        obj = obj[key]
    key = path[-1]
    obj[key] = obj[key]+'-tampered' if isinstance(obj[key], str) else obj[key]+1
    with pytest.raises(ArithmeticError):
        verify(raw, bad)


@pytest.mark.parametrize('kind', ['posting_order', 'journal_order', 'missing_zero_line', 'missing_tail', 'extra_field', 'date_type', 'index_type', 'unsupported_claim'])
def test_exact_order_shape_typed_keys_and_scope_fail_closed(valid, kind):
    raw, result = valid; bad = deepcopy(result)
    if kind == 'posting_order':bad['journals'][0]['postings'].reverse()
    elif kind == 'journal_order':bad['journals'][0], bad['journals'][1] = bad['journals'][1], bad['journals'][0]
    elif kind == 'missing_zero_line':bad['journals'][2]['postings'].pop(0)
    elif kind == 'missing_tail':bad['source_capture']['paths'][0]['cash'].pop()
    elif kind == 'extra_field':bad['invented_receipt'] = True
    elif kind == 'date_type':bad['journals'][0]['ordinal'] = float(bad['journals'][0]['ordinal'])
    elif kind == 'index_type':bad['journals'][0]['native_source_index'] = float(bad['journals'][0]['native_source_index'])
    else:bad['independent_persisted_verification'] = True
    with pytest.raises(ArithmeticError):verify(raw, bad)


def test_zero_allocation_retains_all_zero_postings_and_complete_original_sources():
    raw = example()
    for a in raw['allocation']:a['face'] = 0.
    result = run(raw, threads=1)[0]
    assert verify(raw, result)['verified']
    assert result['journals'] and all(p['value'] == 0 for j in result['journals'] for p in j['postings'])
    assert result['source_capture']['paths'][0]['cash'][0]['principal'] > 0
    assert all(a['cash'] == 1000 for a in result['complete_closing_accounts'])


def test_cancellation_and_aggregate_work_fail_before_daily_source_generation(valid, monkeypatch):
    raw, result = valid
    with pytest.raises(InterruptedError):verify(raw, result, check=lambda:True)
    from portfolio_risk.analytics import product_candidate_reference as reference
    def forbidden(*args, **kwargs):pytest.fail('daily generation preceded aggregate work admission')
    monkeypatch.setattr(reference, '_daily_income', forbidden)
    monkeypatch.setattr(reference, 'MAX_WORK', 1)
    with pytest.raises(ArithmeticError, match='work admission'):verify(raw, result)


@pytest.mark.parametrize('field', ['annual_pd', 'rate', 'start_day', 'opening_accrued', 'allowance'])
def test_legacy_financial_ownership_cannot_overlay_candidate_sources(valid, field):
    raw, result = valid; raw = deepcopy(raw); result = deepcopy(result)
    raw['specification']['positions'][0][field] = 1
    result['raw_request'] = deepcopy(raw)
    result['normalized_specification']['positions'][0][field] = 1
    with pytest.raises(ArithmeticError, match='ownership overlap'):verify(raw, result)


def candidate_for(product):
    """Add explicit, equal-economic stress sources; never infer a scenario."""
    raw = example(); raw['product'] = deepcopy(product)
    start = product['reporting_ordinals'][0]
    raw['as_of_ordinal'] = start
    raw['reporting_end_ordinal'] = product['reporting_ordinals'][-1]
    base = product['base_scenario_id']
    for u in raw['product']['units']:
        g = u['purchase_graph']['request']; kind = u['purchase_graph']['kind']
        scene = deepcopy(g['scenarios'][0]); scene['scenario_id'] = 'stress'; scene['run_id'] = 'explicit-stress'
        g['scenarios'].append(scene)
        location = g['positions'][0]['product'] if kind == 'consumer_installment' else g['positions'][0]
        histories = deepcopy(location['histories'][0]); histories['scenario_id'] = 'stress'
        if kind == 'deposit':
            for role in ('physical', 'pricing', 'pricing_down', 'pricing_up'):
                for h in histories[role]:h['request']['identity']['run_id'] = scene['run_id']
        location['histories'].append(histories)
        branches = []
        for scene_id in [base, 'stress']:
            branch = deepcopy(u['purchase_graph']); bg = branch['request']
            bg['base_scenario_id'] = scene_id
            bg['scenarios'] = [deepcopy(s) for s in g['scenarios'] if s['scenario_id'] == scene_id]
            loc = bg['positions'][0]['product'] if kind == 'consumer_installment' else bg['positions'][0]
            loc['histories'] = [deepcopy(h) for h in location['histories'] if h['scenario_id'] == scene_id]
            path = loc['histories'][0]['physical'][0]['path_id']
            branches.extend(dict(ordinal=day, scenario_id=scene_id, physical_path_id=path, graph=deepcopy(branch)) for day in product['reporting_ordinals'][:-1] if day >= u['purchase_ordinal'])
        u['age_branches'] = branches
    raw['allocation'] = [dict(unit_id=u['unit_id'], face=100.) for u in raw['product']['units']]
    owner = deepcopy(raw['owners'][0]); u = raw['product']['units'][0]
    owner.update(unit_id=u['unit_id'], position_id='candidate-'+u['unit_id'], accounting_category=u['accounting_category'])
    raw['owners'] = [owner]
    raw['specification']['positions'] = [dict(id=owner['position_id'], account='bank', kind='deposit' if u['purchase_graph']['kind'] == 'deposit' else 'loan', balance=0.)]
    raw['specification']['horizon_days'] = raw['reporting_end_ordinal']-start
    raw['specification']['scenarios'] = [dict(name='stress')]
    raw['run_mapping'] = [dict(source_scenario_id=base, ledger_scenario_id='baseline', severity=1.), dict(source_scenario_id='stress', ledger_scenario_id='stress', severity=1.)]
    return raw


@pytest.mark.parametrize('kind', ['card', 'mortgage'])
def test_complete_interest_rights_and_remittance_tail_survive_principal_end(kind, monkeypatch):
    from test_product_strategy_reference import product_for
    strategy, quote = product_for(kind); p = strategy['product']; u = p['units'][0]; g = u['purchase_graph']['request']; start = u['purchase_ordinal']
    p['reporting_ordinals'] = [start, start+30]; g['reporting_ordinals'] = p['reporting_ordinals']
    if kind == 'card':g['positions'][0]['contract']['horizon_days'] = 30
    else:
        # An exact .125 daily coupon and reviewed zero EIR isolate the
        # receivable/remittance tail from recurrence rounding at extinction.
        c = g['positions'][0]['contract']
        c.update(payment_dates=[start+30], original_payment=103.75, current_payment=103.75, current_note_rate=.45625)
        quote = 103.75; u['quote_price_per_face_unit'] = quote/100.; u['income_policy']['original_annual_effective_rate'] = 0.
        g['positions'][0]['oas_target']['base_oas'] = 0.
    raw = candidate_for(p); result = run(raw, threads=1)[0]
    forbid_native(monkeypatch)
    assert verify(raw, result)['complete_cash_rights_basis_tail_verified']
    path = result['source_capture']['paths'][0]
    day = 2 if kind == 'card' else 30
    state = next(s for s in result['daily_positions'] if s['source_scenario_id'] == p['base_scenario_id'] and s['ordinal'] == start+day)
    assert state['accrued_interest'] > 0
    if kind == 'card':assert state['allocated_principal'] == 0
    else:
        # Borrower principal has retired; investor principal and interest
        # remain enforceable remittance rights until day32.
        projection = result['native_preparation']['preparation']['units'][0]['complete_purchase_result']['native_sources'][0]['projection']
        assert sum(projection['points'][-1]['borrower_principal']) == 0
        assert state['allocated_principal'] == 100
    assert path['cash'][-1]['ordinal'] == start+(4 if kind == 'card' else 32)
    assert result['full_end_ordinal'] == start+(30 if kind == 'card' else 32)
    # Card cash earns 2 per 1000 original face; mortgage remits exact103.75.
    total_cash = 100*(1002/1000 if kind == 'card' else 103.75/100)
    close = result['complete_closing_accounts'][0]
    assert close['cash'] == pytest.approx(1000-100*quote/u['unit_face']+total_cash, abs=1e-10)
    assert close['cumulative_net_effective_income'] == pytest.approx(total_cash-100*quote/u['unit_face'], abs=1e-10)


def signed_deposit_product():
    p = json.loads((ROOT/'examples/models/product-units-1.json').read_text()); u = p['units'][0]; g = u['purchase_graph']['request']; start = u['purchase_ordinal']
    p['reporting_ordinals'] = [start, start+1, start+30]; g['reporting_ordinals'] = p['reporting_ordinals']
    quote = 999*math.exp(-math.log1p(-.01)*2/365)
    u['quote_price_per_face_unit'] = quote/1000
    u['income_policy']['original_annual_effective_rate'] = -.01
    g['positions'][0]['oas_target']['base_oas'] = math.log1p(-.01)-.05
    for histories in g['positions'][0]['histories']:
        for role in ('physical', 'pricing', 'pricing_down', 'pricing_up'):
            for h in histories[role]:
                request = h['request']; request['horizon_days'] = 30
                last = request['drivers'][-1]
                request['drivers'].extend(dict(deepcopy(last), ordinal=start+i) for i in range(2, 30))
                for segment in request['segments']:
                    segment['opening_paid_rate'] = -.1825
                    segment['repricing']['floor'] = -.2
                    segment['repricing']['target']['intercept'] = -.1825
    return p, quote


def test_signed_deposit_accrued_rights_are_assets_and_income_bank_signed(monkeypatch):
    p, quote = signed_deposit_product(); raw = candidate_for(p); result = run(raw, threads=1)[0]
    forbid_native(monkeypatch)
    assert verify(raw, result)['verified']
    recognition = next(j for j in result['journals'] if j['phase'] == 'recognition')
    assert recognition['postings'][1]['gl_account'] == 'dated_interest_receivable'
    assert recognition['postings'][1]['value'] == pytest.approx(.05, abs=1e-12)
    assert recognition['postings'][-1]['value'] < 0
    close = result['complete_closing_accounts'][0]
    assert close['cash'] == pytest.approx(1000+quote*.1-99.9, abs=1e-10)
    assert close['cumulative_net_effective_income'] == pytest.approx(quote*.1-99.9, abs=1e-10)


@pytest.mark.parametrize('values,sign', [([0.,0.],1.),([0.,.1,.2],1.),([-.1,0.,-.2],-1.)])
def test_contractual_direction_uses_complete_unweighted_cash_and_recognition(values,sign):
    from portfolio_risk.analytics.product_candidate_reference import _contractual_interest_sign
    path=dict(recognition=[dict(interest=v) for v in values[:-1]],cash=[dict(interest=values[-1])],source_weight=0.,allocation_factor=0.)
    assert _contractual_interest_sign(path)==sign


@pytest.mark.parametrize('earned,cash', [([.1,-.1],[0.]),([.1],[-.1]),([-.1],[.1])])
def test_sign_changing_contractual_rights_reject_even_when_net_zero_or_unallocated(earned,cash):
    from portfolio_risk.analytics.product_candidate_reference import _contractual_interest_sign
    path=dict(recognition=[dict(interest=v) for v in earned],cash=[dict(interest=v) for v in cash],source_weight=0.,allocation_factor=0.)
    with pytest.raises(ArithmeticError,match='mixed-sign contractual interest'):_contractual_interest_sign(path)


def test_signed_early_cash_is_deferred_income_and_later_earned_clears_it():
    from portfolio_risk.analytics.product_candidate_reference import _daily_income,_replay
    product,_=signed_deposit_product();raw=candidate_for(product);u=raw['product']['units'][0]
    start=raw['as_of_ordinal'];raw['reporting_end_ordinal']=start+2
    # Liability received98. The bank receives2 negative coupon early, then
    # returns100 principal. Zero EIR isolates contractual rights from basis.
    u['unit_face']=100.;u['quote_price_per_face_unit']=.98;u['income_policy']['original_annual_effective_rate']=0.
    cash=[dict(event_id='early-negative-coupon',ordinal=start+1,principal=0.,interest=-2.),dict(event_id='principal',ordinal=start+2,principal=100.,interest=0.)]
    recognition=[dict(origin_id='negative-coupon',start_ordinal=start,end_ordinal=start+2,interest=-2.)]
    request,income=_daily_income(u,'baseline','p1',cash,recognition,start+2,'USD',lambda:False)
    path=dict(unit_index=0,native_source_index=0,unit_id=u['unit_id'],product='deposit',scenario_id='baseline',run_id='baseline',path_id='p1',source_weight=1.,allocated_face=100.,unit_face=100.,allocation_factor=1.,side=-1.,interest_direction=-1.,purchase_ordinal=start,full_end_ordinal=start+2,cash=cash,recognition=recognition,daily_income_request=request,daily_income=income)
    out=_replay(raw,dict(accounts=[dict(id='bank',cash=1000.,equity=1000.,cash_floor=0.)]),dict(paths=[path]),start+2,['baseline'],lambda:False)
    earned=next(j for j in out['journals'] if j['phase']=='recognition')
    assert earned['postings'][1]==dict(gl_account='dated_interest_receivable',value=1.)
    early=next(j for j in out['journals'] if j['phase']=='settlement')
    assert early['postings'][-1]==dict(gl_account='dated_deferred_income',value=-1.)
    later=[j for j in out['journals'] if j['phase']=='recognition'][-1]
    assert later['postings'][0]==dict(gl_account='dated_deferred_income',value=1.)
    assert out['complete_closing_accounts'][0]['cash']==1000.
    assert all(abs(x['balance'])<1e-12 for x in out['trial_balance'] if x['gl_account'].startswith('dated_'))


def test_path_weights_once_and_zero_weight_sources_retained(valid):
    raw, original = valid; raw = deepcopy(raw)
    for u in raw['product']['units']:
        g = u['purchase_graph']['request']
        for histories in g['positions'][0]['product']['histories']:
            h = histories['physical'][0]
            histories['physical'] = [dict(deepcopy(h), path_id=path, weight=weight) for path, weight in [('p0', 0.), ('p1', .25), ('p2', .75)]]
        branches = []
        for b in u['age_branches']:
            for path in g['positions'][0]['product']['histories'][0]['physical']:
                branch = deepcopy(b); branch['physical_path_id'] = path['path_id']
                branch['graph']['request']['positions'][0]['product']['histories'][0]['physical'] = [dict(deepcopy(path), weight=1.)]
                branches.append(branch)
        u['age_branches'] = branches
    result = run(raw, threads=1)[0]
    assert verify(raw, result)['verified']
    assert [a['cash'] for a in result['complete_closing_accounts']] == pytest.approx([a['cash'] for a in original['complete_closing_accounts']], abs=1e-10)
    zero = [j for j in result['journals'] if j['path_id'] == 'p0']
    assert zero and all(p['value'] == 0 for j in zero for p in j['postings'])
    for scenario in result['native_preparation']['preparation']['scenario_ids']:
        faces = sum(j['postings'][0]['value'] for j in result['journals'] if j['source_scenario_id'] == scenario and j['phase'] == 'acquisition')
        assert faces == pytest.approx(100, abs=1e-12)


def test_supplied_future_purchase_is_zero_before_acquisition_and_retains_full_tail(valid):
    raw, original = valid; raw = deepcopy(raw); shift = 29
    def move(value, key=''):
        if isinstance(value, dict):return {k:move(v,k) for k,v in value.items()}
        if isinstance(value, list):return [move(v,key) for v in value]
        if isinstance(value, int) and (key.endswith('_ordinal') or key in {'reporting_ordinals', 'payment_dates', 'ordinals'}):return value+shift
        return value
    u = move(raw['product']['units'][1]); raw['product']['units'][1] = u
    reports = raw['product']['reporting_ordinals'][1:]
    u['purchase_graph']['request']['reporting_ordinals'] = reports
    u['age_branches'] = [b for b in u['age_branches'] if b['ordinal'] == u['purchase_ordinal']]
    for b in u['age_branches']:b['graph']['request']['reporting_ordinals'] = reports
    result = run(raw, threads=1)[0]
    assert verify(raw, result)['verified']
    before = [s for s in result['daily_positions'] if s['unit_id'] == u['unit_id'] and s['ordinal'] < u['purchase_ordinal']]
    assert before and all(not s['acquired'] and s['gross_carrying_value'] == s['allocated_principal'] == s['accrued_interest'] == s['book_adjustment'] == 0 for s in before)
    assert not any(j['unit_id'] == u['unit_id'] and j['ordinal'] < u['purchase_ordinal'] for j in result['journals'])
    assert result['full_end_ordinal'] == raw['as_of_ordinal']+89
    assert next(p for p in result['source_capture']['paths'] if p['unit_id'] == u['unit_id'])['cash'][-1]['ordinal'] == raw['as_of_ordinal']+89
    assert [a['cash'] for a in result['complete_closing_accounts']] == pytest.approx([a['cash'] for a in original['complete_closing_accounts']], abs=1e-10)
    assert all(s['ordinal'] == result['full_end_ordinal'] for s in result['complete_closing_positions'])


def test_reciprocal_quote_and_face_cannot_replace_original_gross_source(valid):
    raw, result = valid; raw = deepcopy(raw); result = deepcopy(result)
    u = raw['product']['units'][0]; u['unit_face'] *= 2; u['quote_price_per_face_unit'] /= 2
    result['raw_request'] = deepcopy(raw)
    with pytest.raises(ArithmeticError, match='opening face'):verify(raw, result)


def metadata_request(escaped=False):
    raw = example()
    for key in ('units',):raw['product'][key] = raw['product'][key][:1]
    raw['owners'] = raw['owners'][:1]; raw['allocation'] = raw['allocation'][:1]
    raw['specification']['positions'] = raw['specification']['positions'][:1]
    u = raw['product']['units'][0]; g = u['purchase_graph']['request']
    for histories in g['positions'][0]['product']['histories']:
        h = histories['physical'][0]
        histories['physical'] = [dict(deepcopy(h), path_id='p'+str(i), weight=.125) for i in range(8)]
    branches = []
    for b in u['age_branches']:
        for path in g['positions'][0]['product']['histories'][0]['physical']:
            branch = deepcopy(b); branch['physical_path_id'] = path['path_id']
            branch['graph']['request']['positions'][0]['product']['histories'][0]['physical'] = [dict(deepcopy(path), weight=1.)]
            branches.append(branch)
    u['age_branches'] = branches
    if not escaped:return raw
    def label(tag, length=120):return tag+('\\"'*60)[:length-len(tag)]
    owner = raw['owners'][0]
    aliases = {u['unit_id']:label('unit', 80), owner['position_id']:label('position'), owner['account_id']:label('account'), owner['source']['id']:label('source'), owner['source']['revision']:label('revision'), raw['policy']['identity']['id']:label('policy'), u['accounting_category']:label('category'), u['income_policy']['review']['id']:label('review'), u['income_policy']['review']['revision']:label('income-revision'), u['income_policy']['original_yield_anchor']['id']:label('yield')}
    aliases.update({'p'+str(i):label('path'+str(i), 80) for i in range(8)})
    def replace(value):
        if isinstance(value, dict):return {k:replace(v) for k,v in value.items()}
        if isinstance(value, list):return [replace(v) for v in value]
        return aliases.get(value, value) if isinstance(value, str) else value
    return replace(raw)


def test_escaped_metadata_capacity_precedes_daily_and_posting_allocation(monkeypatch):
    from portfolio_risk.analytics.model_contracts import prepare_product_units
    from portfolio_risk.analytics import product_candidate_reference as reference
    from portfolio_risk.analytics.product_strategy_reference import verify_preparation
    short = metadata_request(); prepared = prepare_product_units(short['product'])
    verify_preparation(short['product'], prepared)
    assert reference._capture(short, prepared, lambda:False)[0]['admitted_daily_rows'] == 960
    long = metadata_request(escaped=True); prepared = prepare_product_units(long['product'])
    verify_preparation(long['product'], prepared)
    def forbidden(*args, **kwargs):pytest.fail('escaped metadata admission followed daily allocation')
    monkeypatch.setattr(reference, '_daily_income', forbidden)
    with pytest.raises(ArithmeticError, match='aggregate journal/state/output work admission'):
        reference._capture(long, prepared, lambda:False)


def test_positive_eir_mortgage_retains_roundoff_right_and_basis_without_plug(monkeypatch):
    from test_product_strategy_reference import product_for
    strategy, quote = product_for('mortgage'); p = strategy['product']; u = p['units'][0]; g = u['purchase_graph']['request']; start = u['purchase_ordinal']
    p['reporting_ordinals'] = [start, start+30]; g['reporting_ordinals'] = p['reporting_ordinals']
    raw = candidate_for(p); result = run(raw, threads=1)[0]
    forbid_native(monkeypatch)
    assert verify(raw, result)['complete_cash_rights_basis_tail_verified']
    income = result['source_capture']['paths'][0]['daily_income']; row = income['rows'][-1]
    assert row['closing_principal'] == 0
    assert row['closing_gross_carrying_value'] == 0
    assert row['closing_accrued_interest'] != 0
    assert abs(row['closing_accrued_interest']) < 1e-15
    assert row['closing_book_adjustment'] == -row['closing_accrued_interest']
    # The tiny source right is preserved; account P&L is the original EIR,
    # with no arbitrary terminal income or accrual plug.
    close = result['complete_closing_accounts'][0]
    assert close['cumulative_net_effective_income'] == pytest.approx(100.1982661-quote, abs=1e-10)
    assert all(j['phase'] in {'acquisition', 'recognition', 'settlement'} for j in result['journals'])
