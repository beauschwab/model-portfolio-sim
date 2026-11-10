"""Integrated raw commercial graph/units fee ownership and corruption controls."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts
from portfolio_risk.analytics.commercial_graph_reference import verify_graph,verify_units
from portfolio_risk.analytics.commercial_reference_math import _df

ROOT=Path(__file__).resolve().parents[3]


def fixture(schema):
    return json.loads((ROOT/'examples/models'/f'{schema}.json').read_text())


def with_fees(graph=None,sign=1.,spread=0.,day_zero=False):
    raw=deepcopy(graph)if graph is not None else fixture('commercial-product-graph-1')
    p=raw['positions'][0];origin=raw['as_of_ordinal'];policy=fixture('commercial-fee-journal-1')['fees'][0]
    for fee in policy['fees']:fee['entitlement']*=sign
    for event in policy['events']:
        event['amount']*=sign
        if day_zero:event['ordinal']=origin
    year=0. if day_zero else 40./365.
    quote=sign*5.*_df(raw['original_scenario']['collateral_curve'],year)*math.exp(-spread*year)
    review=dict(id='explicit-fee-acquisition-policy',revision='1')
    selection=dict(schema='commercial-fee-adoption-1',identity=review,discount_spread=spread,original_signed_fee_quote=quote,
        total_purchase_cost=p['original_signed_cash_quote']+quote,allocations=[dict(fee_id='service-fee',allocated_cost=quote,review=review)],original_pricing=[],histories=[])
    def history(h,physical):
        row=dict(path_id=h['path_id'],policy=deepcopy(policy))
        if physical:
            row['acquisition_releases']=[dict(fee_id='service-fee',ordinal=origin if day_zero else origin+10,sequence=0,amount=quote*.4,review=review),
                dict(fee_id='service-fee',ordinal=origin if day_zero else origin+40,sequence=1,amount=quote-quote*.4,review=review)]
        return row
    selection['original_pricing']=[history(h,False)for h in p['original_pricing']]
    for hs in p['histories']:
        selection['histories'].append(dict(scenario_id=hs['scenario_id'],**{role:[history(h,role=='physical')for h in hs[role]]for role in('physical','pricing','pricing_down','pricing_up')}))
    p['fees']=selection
    return raw


def fee_units(**kwargs):
    raw=fixture('commercial-units-1');raw['units'][0]['graph']=with_fees(**kwargs)
    return raw


@pytest.mark.parametrize('sign',[1.,-1.])
@pytest.mark.parametrize('spread',[0.,.2,-.1])
def test_separate_fee_pv_dv01_and_full_source_replay_keep_original_loan_anchors(sign,spread):
    plain=fixture('commercial-product-graph-1');baseline=model_contracts.analyze_commercial_graph(plain)
    raw=with_fees(plain,sign,spread);actual=model_contracts.analyze_commercial_graph(raw)
    assert verify_graph(raw,actual)['verified']
    assert actual['original_oas']==baseline['original_oas']
    expected=raw['positions'][0]['fees']['original_signed_fee_quote']
    assert actual['metrics'][0]['pv']-baseline['metrics'][0]['pv']==pytest.approx(expected,abs=1e-11)
    fee_dv01=expected*(math.exp(1e-4*40./365.)-math.exp(-1e-4*40./365.))/2.
    assert actual['metrics'][0]['dv01']-baseline['metrics'][0]['dv01']==pytest.approx(fee_dv01,abs=1e-11)
    for old,new in zip(baseline['native_sources'],actual['native_sources'],strict=True):
        assert new['projection']==old['projection']and new['cash']==old['cash']
        assert new['fee_projection']['bound_path']['source']['raw_request']['measure']==('physical'if new['role']=='physical'else'pricing')


@pytest.mark.parametrize('sign',[1.,-1.])
def test_fee_acquisition_changes_purchase_funding_and_separate_income_coefficients(sign):
    raw=fee_units(sign=sign);actual=model_contracts.prepare_commercial_units(raw)
    assert verify_units(raw,actual)['coefficients_recomputed']
    assert actual['values'][0]==0.
    assert actual['fee_income_values']==pytest.approx([sign*.03],abs=1e-12)
    purchase=next(e for e in actual['funding']['cash_events']if e['path_id']=='purchase')
    assert purchase['purchase_cash']==-raw['units'][0]['graph']['positions'][0]['fees']['total_purchase_cost']/100.
    fee_cash=next(e for e in actual['funding']['cash_events']if e['source_event_id'].startswith('separate-fee:'))
    assert fee_cash['ordinal']==738926 and fee_cash['fee_cash']==sign*.05
    assert actual['funding']['demand'][-1]==pytest.approx(0.,abs=1e-12)


def test_purchase_day_fee_cash_is_cum_acquisition_and_ex_cash_marked():
    raw=fee_units(day_zero=True);actual=model_contracts.prepare_commercial_units(raw)
    assert verify_units(raw,actual)['verified']
    assert actual['fee_income_values']==pytest.approx([0.],abs=1e-12)
    graph=actual['native_graphs'][0]
    assert graph['metrics'][0]['pv']==pytest.approx(graph['original_oas'][0]['original_signed_cash_quote'],abs=1e-11)
    cash=actual['funding']['cash_events'];initial=sum(e['bank_cash']for e in cash if e['ordinal']==738886)
    assert initial==-graph['original_oas'][0]['original_signed_cash_quote']/100.


@pytest.mark.parametrize('weights',[(.25,.75),(0.,1.)])
def test_native_fee_paths_apply_weights_once_and_preserve_zero_weight_sources(weights):
    graph=fixture('commercial-product-graph-1');p=graph['positions'][0]
    def expand(rows):
        first=deepcopy(rows[0]);second=deepcopy(rows[0]);first['weight'],second['weight']=weights
        second['path_id']='borrower-two';second['source']['credit']['paths'][0].update(path_id='borrower-two',path_key=18)
        return [first,second]
    p['original_pricing']=expand(p['original_pricing'])
    for h in p['histories']:
        for role in ('physical','pricing','pricing_down','pricing_up'):h[role]=expand(h[role])
    raw=fee_units(graph=graph)
    physical=raw['units'][0]['graph']['positions'][0]['fees']['histories'][0]['physical']
    physical[1]['policy']['events'][0]['ordinal']=graph['as_of_ordinal']+20
    actual=model_contracts.prepare_commercial_units(raw)
    assert verify_units(raw,actual)['verified']
    assert actual['fee_income_values']==pytest.approx([(weights[0]*3.-weights[1]*2.)/100.],abs=1e-12)
    sources=actual['native_graphs'][0]['native_sources']
    assert len([s for s in sources if s['role']=='physical'])==2
    assert all(s['fee_projection']['bound_path']['source']['groups']for s in sources)
    expected=graph['positions'][0]['original_signed_cash_quote']+5.
    assert actual['native_graphs'][0]['metrics'][0]['pv']==pytest.approx(expected,abs=1e-10)


def test_fee_default_waiver_recovery_uses_own_review_and_retains_original_acquisition():
    graph=fixture('commercial-product-graph-1');p=graph['positions'][0]
    stream=dict(kind='supplied',stream_id='fee-default',stream_revision='1',paths=[dict(path_key=17,pairs=[dict(waiting_uniform=math.exp(-100.*12./365.),destination_uniform=.5)])])
    for h in p['original_pricing']:h['source']['credit']['draw_stream']=deepcopy(stream)
    for hs in p['histories']:
        for role in ('physical','pricing','pricing_down','pricing_up'):
            for h in hs[role]:
                source=h['source'];source['credit']['draw_stream']=deepcopy(stream)
                source['credit']['intervals'][0]['hazards_per_year'][0][3]=100.
                source['credit']['intervals'][0]['loss_given_default']=[1.]*3
    raw=fee_units(graph=graph);actual=model_contracts.prepare_commercial_units(raw)
    assert verify_units(raw,actual)['verified']
    graph=actual['native_graphs'][0]
    assert graph['original_oas'][0]['original_signed_cash_quote']==p['original_signed_cash_quote']
    physical=next(s for s in graph['native_sources']if s['role']=='physical')
    fees=physical['fee_projection'];assert fees['bound_path']['exit']['reason']=='default'
    assert fees['cash'][0]['fees']==2.5
    assert physical['cash'][0]['principal']==0.
    assert fees['cash'][0]['year']==pytest.approx(17./365.,abs=1e-15)
    assert actual['fee_income_values']==pytest.approx([-.02],abs=1e-12)


def test_fee_library_adapter_retains_separate_income_exactly_and_keeps_nii_unchanged():
    from portfolio_risk.strategy.commercial import library_from_result
    import numpy as np
    raw=fee_units();actual=model_contracts.prepare_commercial_units(raw);verify_units(raw,actual)
    library=library_from_result(raw,actual)
    assert np.array_equal(library['nonintegral_fee_income'],np.asarray(actual['fee_income_values']).reshape(1,1,1)[0])
    assert np.array_equal(library['fee_income_vectors'][('commercial',0)],library['nonintegral_fee_income'][0])
    assert np.array_equal(library['nii'],np.zeros((1,1)))


def fee_candidate(sign=1.,alias=False,purchase_offset=False):
    raw=fixture('commercial-candidate-journal-1')
    raw['commercial']['units'][0]['graph']=with_fees(raw['commercial']['units'][0]['graph'],sign=sign)
    if alias:
        raw['owners'][0]['position_id']='mapped-commercial'
        raw['specification']['positions'][0]['id']='mapped-commercial'
    if purchase_offset:
        raw['as_of_ordinal']-=5
        raw['commercial']['reporting_ordinals'].insert(0,raw['as_of_ordinal'])
    return raw


def joint_fee_units(timing='before_scheduled_cash',coincident=False,factor=1.02):
    raw=fee_units();g=raw['units'][0]['graph'];p=g['positions'][0];origin=g['as_of_ordinal']
    loan_quote=100.*factor+100.*.1*10./365.;fee_quote=0. if timing=='before_scheduled_cash'else 5.
    p['original_signed_cash_quote']=loan_quote
    p['exercise']=dict(schema='commercial-native-exercise-1',identity=dict(id='joint-fee-call',revision='1'),information_policy='native_full_observed_prefix-1',exercise_dates=[dict(ordinal=origin+10,timing=timing,call_clean_price_factor=factor,put_clean_price_factor=None,call_fee=0.,put_fee=0.)],simultaneous_priority='issuer')
    histories=list(p['original_pricing'])+[h for hs in p['histories']for role in('physical','pricing','pricing_down','pricing_up')for h in hs[role]]
    for h in histories:h['source']['source']['acquisition']['cost']=loan_quote
    p['original_physical']=deepcopy(p['histories'][0]['physical'])
    for h in p['original_physical']:h['source']['source']['current']=deepcopy(h['source']['source']['original'])
    selection=p['fees'];selection.update(original_signed_fee_quote=fee_quote,total_purchase_cost=loan_quote+fee_quote)
    selection['allocations'][0]['allocated_cost']=fee_quote
    for h in selection['original_pricing']+[h for hs in selection['histories']for role in('physical','pricing','pricing_down','pricing_up')for h in hs[role]]:
        policy=h['policy'];policy['coincident_order']='native_exit_follows_native_exercise_timing-1'
        policy['exercise'].update(cancel_future_service=True,cancel_future_cash=True,cancel_unearned_entitlement=True)
        for e in policy['events']:
            if coincident:e['ordinal']=origin+10
            elif e['action']=='earn':e['ordinal']=origin+20
        if fee_quote==0.:h.pop('acquisition_releases',None)
    selection['original_physical']=deepcopy(selection['histories'][0]['physical'])
    return raw


@pytest.mark.parametrize('timing,coincident,factor',[
    ('before_scheduled_cash',False,1.02),('before_scheduled_cash',True,1.02),('after_scheduled_cash',True,.99),
])
def test_joint_fee_bellman_policy_and_actual_stopped_cash_have_independent_replay(timing,coincident,factor):
    raw=joint_fee_units(timing,coincident,factor);actual=model_contracts.prepare_commercial_units(raw)
    assert verify_units(raw,actual)['verified']
    graph=actual['native_graphs'][0];policy=graph['exercise_policies'][0]['policy']
    assert policy['nodes'][0]['decision']=='call'
    assert policy['joint_fee_values']['loan_value']==pytest.approx(raw['units'][0]['graph']['positions'][0]['original_signed_cash_quote'],abs=1e-10)
    assert graph['original_oas'][0]['base_oas']==0.
    fee_cash=0. if timing=='before_scheduled_cash'else 5.
    for source in graph['native_sources']:
        fees=source['fee_projection'];assert fees['bound_path']['exit']['exercise_timing']==timing
        assert sum(c['fees']for c in fees['cash'])==fee_cash
    if timing=='before_scheduled_cash':assert not policy['nodes'][0]['information']['separate_fees']['events']


def test_joint_fee_exercise_candidates_post_the_same_selected_loan_and_fee_owners():
    from portfolio_risk.core.commercial_candidate_native import run
    from portfolio_risk.analytics.commercial_candidate_reference import verify
    raw=fixture('commercial-candidate-journal-1');commercial=joint_fee_units('after_scheduled_cash',True,.99)
    # Preserve the candidate's explicit AC classification and two-run mapping.
    commercial['units'][0]['accounting_category']='AC'
    graph=commercial['units'][0]['graph'];position=graph['positions'][0]
    scenario=deepcopy(graph['scenarios'][0]);scenario.update(scenario_id='stress',run_id='stress-run');graph['scenarios'].append(scenario)
    for histories in (position['histories'],position['fees']['histories']):
        stress=deepcopy(histories[0]);stress['scenario_id']='stress';histories.append(stress)
    raw['commercial']=commercial
    actual,_=run(raw);assert verify(raw,actual)['purchased_journal_replayed']
    assert actual['producer_completion']==dict(complete=True,protocol=7)
    assert any(r['native_fee_group']['event']['action']=='cash'and r['day']==10 for r in actual['tables']['commercial_fee_lineage'])


def test_joint_fee_original_calibration_matches_separate_quotes_and_retained_effective_yield():
    raw=joint_fee_units('after_scheduled_cash',True,.99)['units'][0]['graph'];p=raw['positions'][0]
    expected=.037;target=p['original_signed_cash_quote']*math.exp(-expected*10./365.)
    p['original_signed_cash_quote']=target;p['oas_target']=dict(kind='calibrate_positive_cash',target_cash_price=target,source_id='joint-original-loan-only',source_revision='1')
    p['fees']['total_purchase_cost']=target+5.
    for h in p['original_pricing']+p['original_physical']+[h for hs in p['histories']for role in('physical','pricing','pricing_down','pricing_up')for h in hs[role]]:
        acquisition=h['source']['source']['acquisition'];acquisition.update(cost=target,original_annual_effective_rate=math.expm1(expected))
        for rate in h['source']['accounting_rates']:rate['annual_effective_rates']=[math.expm1(expected)]*len(rate['annual_effective_rates'])
    actual=model_contracts.analyze_commercial_graph(raw)
    assert verify_graph(raw,actual)['verified']
    assert actual['original_oas'][0]['base_oas']==pytest.approx(expected,abs=1e-9)
    root=actual['exercise_policies'][0]['policy']['joint_fee_values']
    assert root['loan_value']==pytest.approx(target,abs=1e-9);assert root['fee_value']==5.
    assert actual['metrics'][0]['pv']==pytest.approx(target+5.,abs=1e-9)


def test_unequal_spreads_choose_root_total_in_one_numeraire_instead_of_local_component_sum():
    raw=joint_fee_units()['units'][0]['graph'];p=raw['positions'][0];origin=raw['as_of_ordinal']
    loan_spread=.1;fee_spread=.5;decision=10/365.;loan_cash=100.+100.*.1*30./365.;call_cash=104.90
    loan_quote=loan_cash*math.exp(-loan_spread*35/365.);fee_quote=5.*math.exp(-fee_spread*40/365.)
    p['original_signed_cash_quote']=loan_quote;p['oas_target']['base_oas']=loan_spread
    p['exercise']['exercise_dates'][0]['call_clean_price_factor']=(call_cash-100.*.1*decision)/100.
    for h in p['original_pricing']+p['original_physical']+[h for scene in p['histories']for role in('physical','pricing','pricing_down','pricing_up')for h in scene[role]]:
        h['source']['source']['acquisition'].update(cost=loan_quote,original_annual_effective_rate=math.expm1(loan_spread))
        for rate in h['source']['accounting_rates']:rate['annual_effective_rates']=[math.expm1(loan_spread)]*len(rate['annual_effective_rates'])
    fees=p['fees'];fees.update(discount_spread=fee_spread,original_signed_fee_quote=fee_quote,total_purchase_cost=loan_quote+fee_quote);fees['allocations'][0]['allocated_cost']=fee_quote
    review=deepcopy(fees['allocations'][0]['review'])
    for h in fees['original_physical']+[h for scene in fees['histories']for h in scene['physical']]:
        h['acquisition_releases']=[dict(fee_id='service-fee',ordinal=origin+10,sequence=0,amount=.4*fee_quote,review=review),dict(fee_id='service-fee',ordinal=origin+40,sequence=1,amount=fee_quote-.4*fee_quote,review=review)]
    local_loan=loan_cash*math.exp(-loan_spread*(35/365.-decision));local_fee=5.*math.exp(-fee_spread*(40/365.-decision))
    assert local_loan+local_fee>call_cash
    root_continuation=loan_quote+fee_quote;root_call=call_cash*math.exp(-loan_spread*decision)
    assert root_continuation<root_call
    actual=model_contracts.analyze_commercial_graph(raw);assert verify_graph(raw,actual)['verified']
    policy=actual['exercise_policies'][0]['policy'];node=policy['nodes'][0]
    assert node['decision']=='continue'
    assert node['continuation_value']==pytest.approx(root_continuation/math.exp(-loan_spread*decision),abs=1e-10)
    assert policy['value']==pytest.approx(root_continuation,abs=1e-10)
    assert policy['joint_fee_values']['decision_criterion']=='root_total_loan_and_fee_in_loan_numeraire-1'


def test_joint_negative_service_obligation_keeps_its_own_signed_stop_cash():
    raw=joint_fee_units('after_scheduled_cash',True,.99);p=raw['units'][0]['graph']['positions'][0];selection=p['fees']
    selection['original_signed_fee_quote']=-5.;selection['total_purchase_cost']=p['original_signed_cash_quote']-5.;selection['allocations'][0]['allocated_cost']=-5.
    for h in selection['original_pricing']+selection['original_physical']+[h for hs in selection['histories']for role in('physical','pricing','pricing_down','pricing_up')for h in hs[role]]:
        for f in h['policy']['fees']:f['entitlement']*=-1.
        for e in h['policy']['events']:e['amount']*=-1.
        for r in h.get('acquisition_releases',[]):r['amount']*=-1.
    actual=model_contracts.prepare_commercial_units(raw);assert verify_units(raw,actual)['verified']
    assert actual['native_graphs'][0]['exercise_policies'][0]['policy']['joint_fee_values']['fee_value']==-5.


@pytest.mark.parametrize('reviewed_waiver',[False,True])
def test_joint_cancelled_future_cash_requires_reviewed_disposition_of_already_earned_right(reviewed_waiver):
    raw=joint_fee_units()['units'][0]['graph'];selection=raw['positions'][0]['fees'];origin=raw['as_of_ordinal']
    for h in selection['original_pricing']+selection['original_physical']+[h for hs in selection['histories']for role in('physical','pricing','pricing_down','pricing_up')for h in hs[role]]:
        for event in h['policy']['events']:
            if event['action']=='earn':event['ordinal']=origin+5
        h['policy']['exercise']['waive_unpaid_earned_fraction']=1. if reviewed_waiver else 0.
    if not reviewed_waiver:
        with pytest.raises(ValueError,match='remaining service/gross rights'):model_contracts.analyze_commercial_graph(raw)
    else:
        actual=model_contracts.analyze_commercial_graph(raw);assert verify_graph(raw,actual)['verified']
        for source in actual['native_sources']:
            state=source['fee_projection']['bound_path']['source']['closing'][0]['state']
            assert state['waived_earned']==5. and state['fee_receivable']==0.


def test_joint_stop_conditionally_averages_unpublished_post_exit_coupons_without_splitting_information():
    from test_commercial_observable_coupons import graph_request,terminal_policy
    raw=with_fees(graph_request());p=raw['positions'][0];origin=raw['as_of_ordinal']
    # Both paths have identical observed state at day1. Their first served
    # overnight rate is published at day3+8h, after this exact native stop.
    quote=100.+100.*(.01+.06)/365.
    p['original_signed_cash_quote']=quote
    p['exercise']=dict(schema='commercial-native-exercise-1',identity=dict(id='joint-pending-call',revision='1'),information_policy='native_full_observed_prefix-1',simultaneous_priority='issuer',exercise_dates=[dict(ordinal=origin+1,timing='after_scheduled_cash',call_clean_price_factor=1.,put_clean_price_factor=None,call_fee=0.,put_fee=0.)])
    loan_sets=[p['original_pricing']]+[hs[role]for hs in p['histories']for role in('physical','pricing','pricing_down','pricing_up')]
    for hs in loan_sets:
        base=deepcopy(hs[0]);hs.clear()
        for key,rate in((17,.04),(18,.08)):
            h=deepcopy(base);h.update(path_id=f'borrower-{key}',weight=.5);source=h['source'];terminal_policy(source,None)
            source['credit']['paths'][0].update(path_key=key,path_id=h['path_id'])
            if source['credit']['draw_stream'].get('paths'):
                source['credit']['draw_stream']['paths'][0]['path_key']=key
            source['source']['acquisition']['cost']=quote
            for role in('original','current'):
                period=source['source'][role]['periods'][0];observation=period['coupon']['observations'][0]
                publication=(origin+3-719163)*86400+8*3600
                observation.update(rate=rate,interest_end_ordinal=origin+3,observation_end_ordinal=origin+3,selection_cutoff_unix_seconds=publication)
                observation['source']['publication_unix_seconds']=publication;period['coupon']['observations']=[observation]
            hs.append(h)
    # Physical expectations are supplied separately: one mean-rate path keeps
    # this joint public-graph control inside the existing aggregate budget.
    for scene in p['histories']:
        scene['physical']=scene['physical'][:1];scene['physical'][0]['weight']=1.
        for role in('original','current'):
            scene['physical'][0]['source']['source'][role]['periods'][0]['coupon']['observations'][0]['rate']=.06
    p['original_physical']=deepcopy(p['histories'][0]['physical'])
    for h in p['original_physical']:h['source']['source']['current']=deepcopy(h['source']['source']['original'])
    selection=p['fees'];selection.update(original_signed_fee_quote=0.,total_purchase_cost=quote);selection['allocations'][0]['allocated_cost']=0.
    fee_sets=[selection['original_pricing']]+[hs[role]for hs in selection['histories']for role in('physical','pricing','pricing_down','pricing_up')]
    for hs in fee_sets:
        base=deepcopy(hs[0]);hs.clear()
        for key in(17,18):
            h=deepcopy(base);h['path_id']=f'borrower-{key}';h.pop('acquisition_releases',None);policy=h['policy']
            policy['coincident_order']='native_exit_follows_native_exercise_timing-1';policy['exercise'].update(cancel_future_service=True,cancel_future_cash=True,cancel_unearned_entitlement=True)
            for e in policy['events']:e['ordinal']=origin+(2 if e['action']=='earn'else 4)
            hs.append(h)
    for scene in selection['histories']:scene['physical']=scene['physical'][:1]
    selection['original_physical']=deepcopy(selection['histories'][0]['physical'])
    actual=model_contracts.analyze_commercial_graph(raw);assert verify_graph(raw,actual)['verified']
    policy=actual['exercise_policies'][0]['policy']
    assert len(policy['nodes'])==1 and policy['nodes'][0]['probability']==1.
    assert policy['nodes'][0]['decision']=='call'
    assert policy['joint_fee_values']['loan_value']==pytest.approx(quote,abs=1e-10)
    assert 'rate'not in policy['nodes'][0]['information']['periods'][0]['coupon']['observations'][0]
    sources=[s for s in actual['native_sources']if s['role']=='pricing']
    assert all(any(c['kind']=='pending_coupon_cash'and c['year']==5/365. for c in s['cash'])for s in sources)
    assert sum(s['weight']*sum(c['interest']for c in s['cash'])for s in sources)==pytest.approx(quote-100.,abs=1e-10)
    control=deepcopy(raw);control['positions'][0].pop('fees')
    loan_only=model_contracts.analyze_commercial_graph(control);assert verify_graph(control,loan_only)['verified']
    assert loan_only['exercise_policies'][0]['policy']['value']==pytest.approx(quote,abs=1e-10)
    assert len(loan_only['exercise_policies'][0]['policy']['nodes'])==1
    assert [s['cash']for s in loan_only['native_sources']]==[s['cash']for s in actual['native_sources']]


@pytest.mark.parametrize('sign',[1.,-1.])
@pytest.mark.parametrize('alias,purchase_offset',[(False,False),(True,True)])
def test_same_sim_candidate_replays_fee_acquisition_basis_and_full_signed_tails(sign,alias,purchase_offset):
    from portfolio_risk.core.commercial_candidate_native import run
    from portfolio_risk.analytics.commercial_candidate_reference import verify
    raw=fee_candidate(sign,alias,purchase_offset);actual,_=run(raw)
    assert verify(raw,actual)['purchased_journal_replayed']
    assert actual['producer_completion']==dict(complete=True,protocol=7)
    assert actual['full_end_ordinal']==738931
    acquisition=[r for r in actual['tables']['commercial_fee_lineage']if r['native_fee_group']['event']['action']=='acquire']
    assert len(acquisition)==2 and all(r['day']==(5 if purchase_offset else 0)for r in acquisition)
    assert all(r['cash']==pytest.approx(1000.,abs=1e-9)for r in actual['tables']['closing_statements'])
    stock='acquired_fee_asset'if sign>0. else'acquired_fee_liability'
    assert any(r['native_fee_snapshot'].get(stock)==sign*3. for r in actual['tables']['commercial_fee_daily'])


@pytest.mark.parametrize('target',['acquired_basis','release','cash','gross_class','completion'])
def test_candidate_persisted_replay_refuses_fee_basis_journal_and_gross_corruption(target):
    from portfolio_risk.core.commercial_candidate_native import run
    from portfolio_risk.analytics.commercial_candidate_reference import verify
    raw=fee_candidate(sign=-1.);actual,_=run(raw)
    if target=='acquired_basis':actual['tables']['commercial_fee_daily'][0]['native_fee_snapshot']['acquired_cost']-=.01
    elif target=='release':actual['tables']['commercial_fee_lineage'][0]['native_fee_group']['event']['amount']-=.01
    elif target=='cash':actual['tables']['journal'][1]['value']+=.01
    elif target=='gross_class':actual['tables']['path'][0]['liabilities']-=.01
    else:actual['producer_completion']['protocol']=6
    with pytest.raises((ArithmeticError,ValueError)):verify(raw,actual)


@pytest.mark.parametrize('target',['raw_fee_history','bound_source','cash','basis','fee_coefficients'])
def test_independent_fee_replay_rejects_source_cash_basis_and_coefficient_corruption(target):
    raw=fee_units();actual=model_contracts.prepare_commercial_units(raw)
    projection=actual['native_graphs'][0]['native_sources'][0]['fee_projection']
    if target=='raw_fee_history':projection['raw_fee_history']['policy']['events'][0]['amount']+=1e-12
    elif target=='bound_source':projection['bound_path']['source']['raw_request']['events'][0]['amount']+=1e-12
    elif target=='cash':projection['cash'][0]['fees']+=.01
    elif target=='basis':projection['raw_fee_history']['acquisition_releases'][0]['amount']+=.01
    else:actual['fee_income_values'][0]+=.01
    with pytest.raises((ArithmeticError,ValueError)):verify_units(raw,actual)


@pytest.mark.parametrize('target',['missing_path','extra_path','missing_release','wrong_sign','quote','pricing_release','changed_contract','double_release'])
def test_native_fee_graph_refuses_incomplete_or_inconsistent_acquired_rights(target):
    raw=with_fees();selection=raw['positions'][0]['fees'];physical=selection['histories'][0]['physical'][0]
    if target=='missing_path':selection['histories'][0]['pricing_up']=[]
    elif target=='extra_path':selection['original_pricing'].append(deepcopy(selection['original_pricing'][0]))
    elif target=='missing_release':physical['acquisition_releases'].pop()
    elif target=='wrong_sign':selection['allocations'][0]['allocated_cost']=-5.
    elif target=='quote':selection['discount_spread']=.1
    elif target=='pricing_release':selection['original_pricing'][0]['acquisition_releases']=deepcopy(physical['acquisition_releases'])
    elif target=='changed_contract':selection['histories'][0]['pricing_up'][0]['policy']['fees'][0]['service']['revision']='different'
    else:physical['acquisition_releases'][1]['amount']=4.
    with pytest.raises((ValueError,RuntimeError)):model_contracts.analyze_commercial_graph(raw)
