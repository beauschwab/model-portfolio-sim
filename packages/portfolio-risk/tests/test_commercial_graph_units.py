"""Raw independent graph, default/continuation, coefficient and corruption gates."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts
from portfolio_risk.analytics.commercial_graph_reference import verify_graph,verify_units
ROOT=Path(__file__).resolve().parents[3]
def fixture(schema):return json.loads((ROOT/'examples/models'/f'{schema}.json').read_text())
def histories(raw):
    position=raw['positions'][0]
    yield from position['original_pricing']
    for set_ in position['histories']:
        for role in ('physical','pricing','pricing_down','pricing_up'):yield from set_[role]
def default_graph():
    raw=fixture('commercial-product-graph-1')
    stream=dict(kind='supplied',stream_id='hand-default',stream_revision='1',paths=[dict(path_key=17,pairs=[dict(waiting_uniform=math.exp(-100.*2./365.),destination_uniform=0.5)])])
    for h in histories(raw):h['source']['credit']['draw_stream']=deepcopy(stream)
    for role in ('physical','pricing','pricing_down','pricing_up'):
        source=raw['positions'][0]['histories'][0][role][0]['source'];interval=source['credit']['intervals'][0]
        interval['hazards_per_year'][0][3]=100.;interval['loss_given_default']=[0.4]*3
        for tail in interval['complete_recovery_tails']:tail[0]['lag_years']=40./365.
    return raw
def exercise_graph():
    raw=fixture('commercial-product-graph-1');p=raw['positions'][0]
    p['exercise']=dict(schema='commercial-native-exercise-1',identity=dict(id='call',revision='1'),information_policy='native_full_observed_prefix-1',exercise_dates=[dict(ordinal=738896,timing='after_scheduled_cash',call_clean_price_factor=1.,put_clean_price_factor=None,call_fee=0.,put_fee=0.)],simultaneous_priority='issuer')
    p['original_signed_cash_quote']=100.+100.*0.1*10./365.
    return raw
def exercised_units():
    raw=fixture('commercial-units-1');g=exercise_graph();p=g['positions'][0];rate=.03
    cost=p['original_signed_cash_quote']*math.exp(-math.log1p(rate)*10./365.)
    p['original_signed_cash_quote']=cost;p['oas_target']['base_oas']=math.log1p(rate)
    for h in histories(g):
        h['source']['source']['acquisition'].update(cost=cost,original_annual_effective_rate=rate)
        for r in h['source']['accounting_rates']:r['annual_effective_rates']=[rate]*3
    p['original_physical']=deepcopy(p['histories'][0]['physical'])
    for h in p['original_physical']:h['source']['source']['current']=deepcopy(h['source']['source']['original'])
    raw['units'][0]['graph']=g
    return raw
@pytest.mark.parametrize('factory',[lambda:fixture('commercial-product-graph-1'),default_graph,exercise_graph])
def test_all_credit_economics_and_native_continuation_have_independent_source_replay(factory):
    raw=factory();result=model_contracts.analyze_commercial_graph(raw);receipt=verify_graph(raw,result)
    assert receipt['verified'] and receipt['pricing_cash_recomputed'] and receipt['physical_accounting_recomputed']
    assert not receipt['empirical_validation']
def test_native_commercial_coefficients_reconcile_full_cash_and_source_interest_journal():
    raw=fixture('commercial-units-1');result=model_contracts.prepare_commercial_units(raw);assert verify_units(raw,result)['coefficients_recomputed']
    assert 738921 in result['funding']['ordinals']

def test_native_acquired_exercise_coefficients_reconcile_original_physical_anchor():
    raw=exercised_units();result=model_contracts.prepare_commercial_units(raw)
    assert verify_units(raw,result)['coefficients_recomputed']
    assert 738896 in result['funding']['ordinals'] and 738921 not in result['funding']['ordinals']
    assert result['values'][0]>0.

def test_acquired_exercise_partition_receipt_cannot_replace_independent_original_cash():
    raw=exercised_units();result=model_contracts.prepare_commercial_units(raw)
    result['native_graphs'][0]['original_acquisition_proofs'][0]['acquired_cash_partitions'][0]['base'][0]['amount']+=1.
    with pytest.raises(ArithmeticError):verify_units(raw,result)

def acquired_draw_units():
    raw=fixture('commercial-units-1');g=exercise_graph();p=g['positions'][0]
    origin=g['as_of_ordinal'];rate=.07
    g['reporting_ordinals']=[origin,origin+5];raw['reporting_ordinals']=[origin,origin+5]
    p['exercise']['exercise_dates'][0]['ordinal']=origin+10
    base_cash=100.+100.*.1*10./365.
    base_cost=base_cash*math.exp(-math.log1p(rate)*10./365.)
    draw_cash=20.+20.*.15*5./365.
    draw_cost=draw_cash*math.exp(-math.log1p(rate)*5./365.)
    p['original_signed_cash_quote']=base_cost;p['oas_target']['base_oas']=math.log1p(rate)
    for h in histories(g):
        owner=h['source']
        for source in (owner['source']['original'],owner['source']['current']):
            source.update(reporting_end_ordinal=origin+5,tail_end_ordinal=origin+20)
            source['periods'][0].update(end_ordinal=origin+20,payment_ordinal=origin+20)
        owner['source']['acquisition'].update(cost=base_cost,original_annual_effective_rate=rate)
        for row in owner['accounting_rates']:row['annual_effective_rates']=[rate]*3
        draw=deepcopy(owner['source'])
        for source in (draw['original'],draw['current']):
            source['contract']['id']='draw-contract';source['position_id']='draw-contract'
            source.update(asof_ordinal=origin+5,reporting_end_ordinal=origin+20,tail_end_ordinal=origin+20,opening_principal=20.)
            source['principal_plan']['principal_payments']=[20.]
            source['periods'][0].update(period_id='draw-coupon',start_ordinal=origin+5,end_ordinal=origin+20,payment_ordinal=origin+20,coupon=dict(kind='fixed',annual_rate=.15))
        draw['acquisition']['identity']['id']='draw-acquisition'
        draw['acquisition']['original_yield_anchor']['id']='draw-original-yield'
        draw['acquisition']['cost']=draw_cost
        owner['performing_draw_tranches']=[dict(review=dict(id='draw-review',revision='1'),draw_ordinal=origin+5,eligible_origins=[True,False,False],source=draw)]
        owner['credit']['opening_undrawn_commitment']=40.
        owner['credit']['intervals'][0]['end_year']=20./365.
    p['original_physical']=deepcopy(p['histories'][0]['physical'])
    for h in p['original_physical']:
        h['source']['source']['current']=deepcopy(h['source']['source']['original'])
        for draw in h['source']['performing_draw_tranches']:draw['source']['current']=deepcopy(draw['source']['original'])
    raw['units'][0]['graph']=g
    return raw,base_cost,draw_cost,base_cash+draw_cash

def test_acquired_draw_graph_and_coefficients_keep_each_conditional_yield_and_cash_owner():
    raw,base_cost,draw_cost,exit_cash=acquired_draw_units()
    actual=model_contracts.prepare_commercial_units(raw)
    assert verify_units(raw,actual)['coefficients_recomputed']
    graph=actual['native_graphs'][0];proof=graph['original_acquisition_proofs'][0]
    assert proof['original_stopped_cash_quote']==pytest.approx(base_cost,abs=1e-10)
    tranche=proof['conditional_tranche_proofs'][0]
    assert tranche['activation_probability']==1.
    assert tranche['conditional_cash_quote']==pytest.approx(draw_cost,abs=1e-10)
    assert tranche['original_annual_effective_rate']==.07
    source=next(s for s in graph['native_sources'] if s['role']=='physical')
    assert source['source_request']['source']['acquisition']['cost']==base_cost
    cash=source['cash']
    total=lambda c:c['principal']+c['interest']+c['fees']
    assert next(total(c) for c in cash if c['year']==5./365.)==pytest.approx(-draw_cost,abs=1e-10)
    assert sum(total(c) for c in cash if c['year']==10./365.)==pytest.approx(exit_cash,abs=1e-10)
    bad=deepcopy(actual);bad['native_graphs'][0]['original_acquisition_proofs'][0]['conditional_tranche_proofs'][0]['conditional_cash_quote']+=1.
    with pytest.raises(ArithmeticError):verify_units(raw,bad)
@pytest.mark.parametrize('path',[
    ('metrics',0,'pv'),('metrics',0,'dv01'),('original_oas',0,'base_oas'),
    ('native_sources',0,'cash',0,'principal'),('native_sources',0,'projection','accounting','groups',1,'next','legal_interest'),
    ('native_sources',1,'projection','carry','carry_points',1,'gross_carry'),
    ('native_sources',1,'projection','current_source','source','audit',0,'opening_principal'),
])
def test_graph_publication_rejects_corrupted_financial_and_original_sources(path):
    raw=fixture('commercial-product-graph-1');actual=model_contracts.analyze_commercial_graph(raw);bad=deepcopy(actual);at=bad
    for key in path[:-1]:at=at[key]
    at[path[-1]]+=1.
    with pytest.raises(ArithmeticError):verify_graph(raw,bad)
@pytest.mark.parametrize('path',[('values',0),('funding','demand',0),('funding','cash_events',0,'bank_cash'),('units',0,'quote_price_per_face_unit')])
def test_commercial_unit_publication_rejects_coefficient_and_cash_corruption(path):
    raw=fixture('commercial-units-1');actual=model_contracts.prepare_commercial_units(raw);bad=deepcopy(actual);at=bad
    for key in path[:-1]:at=at[key]
    at[path[-1]]+=1.
    with pytest.raises(ArithmeticError):verify_units(raw,bad)
def test_explicit_acquired_exercise_requires_original_physical_stopping_anchor():
    raw=fixture('commercial-units-1');raw['units'][0]['graph']=exercise_graph()
    with pytest.raises(ValueError,match='original stopped Physical acquisition'):model_contracts.prepare_commercial_units(raw)

@pytest.mark.parametrize('target,value', [('pv',float('nan')),('dv01',True)])
def test_nonfinite_and_boolean_financial_values_do_not_pass_publication(target,value):
    raw=fixture('commercial-product-graph-1');actual=model_contracts.analyze_commercial_graph(raw)
    actual['metrics'][0][target]=value
    with pytest.raises(ArithmeticError):verify_graph(raw,actual)

def test_boolean_native_ownership_flags_remain_exact():
    raw=fixture('commercial-product-graph-1');actual=model_contracts.analyze_commercial_graph(raw)
    actual['shared_draws_retained']=1
    with pytest.raises(ArithmeticError):verify_graph(raw,actual)

def generated_graph():return fixture('commercial-generated-graph-1')

def generated_active_graph():
    raw=generated_graph();g=raw['positions'][0]['generated_policy']
    for law in [g['pricing_training'],g['original_physical']]+g['current_pricing']+g['current_physical']:
        law['policy']['exercise_dates'][0]['call_clean_price_factor']=1.01
    for law in g['current_pricing']+g['current_physical']:
        law['drivers'][0].update(initial_level=0.,long_run_mean=1.,mean_reversion=100.)
    return raw

def test_generated_active_q_policy_stops_current_physical_cash_at_fixed_original_anchors():
    raw=generated_active_graph();actual=model_contracts.analyze_commercial_graph(raw)
    assert verify_graph(raw,actual)['verified']
    assert actual['metrics'][0]['pv']==pytest.approx(101.,abs=1e-9)
    assert actual['original_oas'][0]['original_signed_cash_quote']==100.
    physical=[s for s in actual['native_sources'] if s['role']=='physical']
    assert all(len(s['projection']['native_stops'])==1 for s in physical)
    assert all(s['source_request']['source']['acquisition']['original_annual_effective_rate']==0. for s in physical)

def test_generated_graph_owns_native_q_training_and_separate_p_recipe_book():
    raw=generated_graph();actual=model_contracts.analyze_commercial_graph(raw)
    assert verify_graph(raw,actual)['verified']
    assert actual['generated_physical_recipes_retained'] is True
    assert actual['physical_accounting_source_retained'] is False

def test_generated_library_coefficients_reconcile_independent_raw_policy_and_physical_book():
    raw=fixture('commercial-units-1');raw['units'][0]['graph']=generated_graph()
    actual=model_contracts.prepare_commercial_units(raw)
    assert verify_units(raw,actual)['coefficients_recomputed']

@pytest.mark.parametrize('field',['book_point','fitted_policy','original_recipe'])
def test_generated_compact_receipts_cannot_replace_independent_laws_and_original_cash(field):
    raw=generated_graph();actual=model_contracts.analyze_commercial_graph(raw)
    if field=='book_point':
        source=next(s for s in actual['native_sources'] if s['role']=='physical')
        source['projection']['book_points'][0]['principal']+=1.
    elif field=='fitted_policy':actual['exercise_policies'][0]['policy']['training_value']+=1.
    else:actual['original_acquisition_proofs'][0]['generated_original_physical'][0]['raw_request']['source']['acquisition']['cost']+=1.
    with pytest.raises(ArithmeticError):verify_graph(raw,actual)
