"""Observable commercial rates through the actual graph and independent books."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics.commercial_source_reference import project, observable_factor
from portfolio_risk.analytics.commercial_credit_reference import replay
from portfolio_risk.analytics.commercial_accounting_reference import groups

ROOT=Path(__file__).resolve().parents[3]


def source_request(raw=None, calculation='daily_simple', treatment='simple'):
    if raw is None:
        raw=deepcopy(json.loads((ROOT/'examples/models/commercial-credit-ledger-1.json').read_text())['commercial'][0])
    start=raw['source']['current']['asof_ordinal'];midnight=lambda day:(day-719163)*86400
    for role in ('original','current'):
        source=raw['source'][role];period=source['periods'][0]
        source['periods']=[period];source.update(reporting_end_ordinal=start+2,tail_end_ordinal=start+5,observed_publication_cutoff_unix_seconds=midnight(start))
        source['principal_plan']=dict(kind='supplied',principal_payments=[100.]);source['opening_principal']=100.
        period.update(start_ordinal=start,end_ordinal=start+3,payment_ordinal=start+5,day_count='act365_fixed',fee=0.)
        observations=[]
        for day,rate in enumerate((.04,.05,.06)):
            publication=midnight(start+day+1)+8*3600
            observations.append(dict(interest_start_ordinal=start+day,interest_end_ordinal=start+day+1,observation_start_ordinal=start+day,observation_end_ordinal=start+day+1,
                fixing_ordinal=start+day,selection_cutoff_unix_seconds=publication,rate=rate,
                source=dict(kind='conditional_published_projection',model=dict(id='synthetic-publication-model',revision='1'),path=deepcopy(source['path']),measure=source['measure'],
                    state_information_unix_seconds=midnight(start+day),publication_unix_seconds=publication)))
        period['coupon']=dict(kind='indexed',index=dict(index_id='synthetic-overnight',kind='overnight',tenor_months=None,average_calendar_days=None),
            calendar=dict(id='reviewed-daily-mapping',revision='1'),complete_calendar_mapping=True,calculation=calculation,margin_treatment=treatment,margin=.01,
            index_bounds=dict(floor=None,cap=None),all_in_bounds=dict(floor=None,cap=None),reset_cutoff_unix_seconds=None,lookback_business_days=0,observation_shift=False,observations=observations)
    if calculation=='daily_compounded':
        product=math.prod(1.+(r+(.01 if treatment=='compounded' else 0.))/365. for r in (.04,.05,.06))
        coupon=100.*(product-1.+(.01*3/365. if treatment=='simple' else 0.))
    else:coupon=100.*.18/365.
    raw['source']['acquisition'].update(cost=100.+coupon,original_annual_effective_rate=0.)
    raw['credit']['intervals'][0]['end_year']=5/365.
    for interval in raw['credit']['intervals']:interval['hazards_per_year']=[[0.]*5 for _ in range(3)]
    raw['factor_policy']['overnight']='observable_index_prefix_simple_margin-1'
    raw['factor_policy']['observation_mapping']='selected_observation_weight_over_interest_daycount-1'
    return raw


@pytest.mark.parametrize('calculation,treatment',[('daily_simple','simple'),('daily_compounded','simple'),('daily_compounded','compounded')])
def test_independent_observable_prefix_matches_hand_rates_and_full_cash(calculation,treatment):
    raw=source_request(calculation=calculation,treatment=treatment);result=replay(raw);points=result['carry_points']
    first=next(p for p in points if p['kind']=='live_sojourn' and p['year']==1/365.)
    assert first['contractual_accrued_interest']==pytest.approx(100*.01/365 if treatment=='simple' else 0.,abs=1e-12)
    publication=(86400.+8*3600)/31536000.
    at=next(p for p in points if p['kind']=='live_sojourn' and p['year']==publication)
    hand=100.*(.04/365.+.01*publication) if treatment=='simple' else 100.*.05/365.
    assert at['contractual_accrued_interest']==pytest.approx(hand,abs=1e-12)
    assert result['native_source_cash'][0]['cash']['interest']==pytest.approx(raw['source']['acquisition']['cost']-100.,abs=1e-12)
    assert groups(result)[1][0]['legal_interest']==pytest.approx(0.,abs=1e-12)


def graph_request(calculation='daily_simple',treatment='simple'):
    raw=json.loads((ROOT/'examples/models/commercial-product-graph-1.json').read_text());start=raw['as_of_ordinal'];raw['reporting_ordinals']=[start,start+2]
    position=raw['positions'][0]
    histories=list(position['original_pricing'])
    if position.get('original_physical'):histories+=position['original_physical']
    for scenarios in position['histories']:
        for role in ('physical','pricing','pricing_down','pricing_up'):histories+=scenarios[role]
    for history in histories:source_request(history['source'],calculation,treatment)
    position['original_signed_cash_quote']=histories[0]['source']['source']['acquisition']['cost']
    return raw


@pytest.mark.parametrize('calculation,treatment',[('daily_simple','simple'),('daily_compounded','simple'),('daily_compounded','compounded')])
def test_native_graph_observable_source_credit_and_accounting_replay(calculation,treatment):
    from portfolio_risk.analytics import model_contracts
    from portfolio_risk.analytics.commercial_graph_reference import verify_graph
    raw=graph_request(calculation,treatment);actual=model_contracts.analyze_commercial_graph(raw)
    assert verify_graph(raw,actual)['physical_accounting_recomputed']


def test_unpublished_future_rate_changes_no_prior_legal_or_recognized_amount():
    raw=source_request(calculation='daily_compounded');a=replay(raw);changed=deepcopy(raw)
    changed['source']['current']['periods'][0]['coupon']['observations'][2]['rate']=.9;b=replay(changed)
    select=lambda r:[p for p in r['carry_points'] if p['year']<=2/365.]
    assert select(a)==select(b)
    assert raw['source']['acquisition']==changed['source']['acquisition']


@pytest.mark.parametrize('destination',[3,4])
def test_unquantified_terminal_coupon_is_not_waived_or_read_from_future(destination):
    raw=source_request();raw['credit']['intervals'][0]['hazards_per_year'][0][destination]=10.
    raw['credit']['draw_stream']=dict(kind='supplied',stream_id='terminal-controls',stream_revision='1',paths=[dict(path_key=17,pairs=[dict(waiting_uniform=math.exp(-10*.5/365),destination_uniform=.5)])])
    with pytest.raises(ArithmeticError,match='unquantified'):replay(raw)


def test_pending_rate_and_known_margin_respect_nonaccrual():
    raw=source_request();raw['accounting']['recognition'][0]['by_origin']=['legal_only_income_stopped']*3
    book,_=groups(replay(raw));point=next(g for g in book if g['phase']=='live_sojourn' and g['year']==2/365.)
    assert point['next']['legal_interest']>0 and point['next']['recognized_interest']==0.


@pytest.mark.parametrize('mutation',['future_observed','future_state','publication_before_observation','early_cash'])
def test_available_prefix_guards_remain_strict(mutation):
    raw=source_request();source=raw['source']['current'];observation=source['periods'][0]['coupon']['observations'][0];fixing=observation['source']
    if mutation=='future_observed':observation['source']=dict(kind='observed',fixing=dict(id='forbidden-future',revision='1'),publication_unix_seconds=fixing['publication_unix_seconds'])
    elif mutation=='future_state':fixing['state_information_unix_seconds']+=1
    elif mutation=='publication_before_observation':fixing['publication_unix_seconds']-=86400
    else:source['periods'][0]['payment_ordinal']=source['asof_ordinal']+1
    with pytest.raises(ArithmeticError):replay(raw)


def terminal_policy(raw,destination,day=.5):
    # Only an absorbing default/prepayment is reachable in these controls.
    raw['credit']['max_jumps_per_path']=1
    raw['factor_policy']['terminal_obligations']=dict(review=dict(id='explicit-contingent-rights',revision='1'),live_exit_recognition='recognize_known_served_right_at_information-1',live_exit_settlement='retain_original_coupon_settlement_after_information-1',default_recognition='new_known_default_right_is_legal_only-1',default_recovery='original_signed_interest_fraction_and_tails_after_information-1')
    if destination is not None:
        raw['credit']['intervals'][0]['hazards_per_year'][0][destination]=10.
        raw['credit']['draw_stream']=dict(kind='supplied',stream_id='pending-terminal',stream_revision='1',paths=[dict(path_key=17,pairs=[dict(waiting_uniform=math.exp(-10*day/365),destination_uniform=.5)])])
    return raw


@pytest.mark.parametrize('destination',[3,4])
@pytest.mark.parametrize('calculation,treatment',[('daily_simple','simple'),('daily_compounded','simple'),('daily_compounded','compounded')])
def test_pending_terminal_source_releases_at_information_then_explicit_settlement(destination,calculation,treatment):
    raw=terminal_policy(source_request(calculation=calculation,treatment=treatment),destination)
    raw['credit']['intervals'][0]['interest_recovery_fraction']=[.25]*3
    result=replay(raw);book,closing=groups(result)
    terminal=next(p for p in result['carry_points'] if p['kind']==('default_gross_transfer' if destination==3 else 'full_live_exit'))
    assert terminal['year']==pytest.approx(.5/365.,abs=1e-15)
    fix=next(p for p in result['carry_points'] if p['kind']=='pending_coupon_fixing')
    paid=next(p for p in result['carry_points'] if p['kind']=='pending_coupon_cash')
    assert fix['year']==(86400.+8*3600)/31536000.
    assert paid['year']==(fix['year'] if destination==3 else 5/365.)
    assert paid['cash']==pytest.approx((.04 if treatment=='simple' else .05)*100*.5/365.*(.25 if destination==3 else 1.),abs=1e-12)
    assert all(abs(sum(line['posting']['amount'] for line in g['postings']))<1e-9 for g in book)
    if destination==3:assert fix['effective_interest']==0. and closing[0]['in_default']


@pytest.mark.parametrize('destination',[3,4])
@pytest.mark.parametrize('calculation,treatment',[('daily_simple','simple'),('daily_compounded','simple'),('daily_compounded','compounded')])
def test_native_pending_terminal_graph_has_independent_full_source_book_replay(destination,calculation,treatment):
    from portfolio_risk.analytics import model_contracts
    from portfolio_risk.analytics.commercial_graph_reference import verify_graph
    raw=graph_request(calculation,treatment)
    for scenes in raw['positions'][0]['histories']:
        for role in ('physical','pricing','pricing_down','pricing_up'):
            for history in scenes[role]:terminal_policy(history['source'],destination)
    # Retain the exact same CRN stream in original/current/bump histories.
    stream=deepcopy(raw['positions'][0]['histories'][0]['physical'][0]['source']['credit']['draw_stream'])
    for history in raw['positions'][0]['original_pricing']+raw['positions'][0].get('original_physical',[]):
        terminal_policy(history['source'],None);history['source']['credit']['draw_stream']=deepcopy(stream)
    # Original hazard law, base quote and acquisition remain unchanged.
    actual=model_contracts.analyze_commercial_graph(raw)
    assert verify_graph(raw,actual)['physical_accounting_recomputed']


def mapping_source(raw,mapping):
    for role in ('original','current'):
        source=raw['source'][role];coupon=source['periods'][0]['coupon'];observations=coupon['observations']
        if mapping=='lookback':
            coupon['lookback_business_days']=2
            for observation in observations:
                for key in ('observation_start_ordinal','observation_end_ordinal','fixing_ordinal'):observation[key]-=2
                observation['selection_cutoff_unix_seconds']-=2*86400
                observation['source']['state_information_unix_seconds']-=2*86400;observation['source']['publication_unix_seconds']-=2*86400
        elif mapping=='lockout':
            for key in ('observation_start_ordinal','observation_end_ordinal','fixing_ordinal','selection_cutoff_unix_seconds','rate','source'):observations[2][key]=deepcopy(observations[1][key])
        else:
            coupon['observation_shift']=True;observations[:]=observations[:1];o=observations[0];start=source['asof_ordinal'];midnight=(start-719163)*86400
            o.update(interest_end_ordinal=start+3,observation_start_ordinal=start-5,observation_end_ordinal=start,fixing_ordinal=start-5,selection_cutoff_unix_seconds=midnight+8*3600)
            o['source'].update(state_information_unix_seconds=midnight-5*86400,publication_unix_seconds=midnight+8*3600)
    coupon=raw['source']['original']['periods'][0]['coupon']
    if mapping=='unequal_shift':factor=.04*5/365.+.01*3/365.
    elif mapping=='lockout':factor=(.05+.06+.06)/365.
    else:factor=.18/365.
    raw['source']['acquisition']['cost']=100.+100.*factor


@pytest.mark.parametrize('mapping',['lookback','lockout','unequal_shift'])
def test_native_supplied_observation_mapping_and_calendar_day_weights(mapping):
    from portfolio_risk.analytics import model_contracts
    from portfolio_risk.analytics.commercial_graph_reference import verify_graph
    raw=graph_request();position=raw['positions'][0];histories=list(position['original_pricing'])
    for scenes in position['histories']:
        for role in ('physical','pricing','pricing_down','pricing_up'):histories+=scenes[role]
    for history in histories:mapping_source(history['source'],mapping)
    position['original_signed_cash_quote']=histories[0]['source']['source']['acquisition']['cost']
    actual=model_contracts.analyze_commercial_graph(raw)
    assert verify_graph(raw,actual)['physical_accounting_recomputed']


@pytest.mark.parametrize('destination',[3,4])
def test_native_pending_signed_rights_with_separated_late_recovery_clocks(destination):
    from portfolio_risk.analytics import model_contracts
    from portfolio_risk.analytics.commercial_graph_reference import verify_graph
    raw=graph_request();p=raw['positions'][0];histories=list(p['original_pricing'])
    for scenes in p['histories']:
        for role in ('physical','pricing','pricing_down','pricing_up'):histories+=scenes[role]
    for history in histories:
        source=history['source'];terminal_policy(source,destination if history not in p['original_pricing'] else None)
        for role in ('original','current'):
            for observation in source['source'][role]['periods'][0]['coupon']['observations']:observation['rate']=-.04
        source['source']['acquisition']['cost']=100.+100.*(-.03)*3/365.
        source['credit']['intervals'][0]['interest_recovery_fraction']=[.25]*3
        source['credit']['intervals'][0]['complete_recovery_tails']=[[dict(lag_years=.25/365.,weight=.4),dict(lag_years=4/365.,weight=.6)] for _ in range(3)]
    p['original_signed_cash_quote']=histories[0]['source']['source']['acquisition']['cost']
    stream=deepcopy(p['histories'][0]['physical'][0]['source']['credit']['draw_stream'])
    for history in p['original_pricing']:history['source']['credit']['draw_stream']=deepcopy(stream)
    actual=model_contracts.analyze_commercial_graph(raw);assert verify_graph(raw,actual)['physical_accounting_recomputed']


@pytest.mark.parametrize('destination,reviewed',[(3,False),(4,False),(3,True)])
def test_pending_coupon_shared_daily_journal_persists_source_memos_and_actual_cash(tmp_path,destination,reviewed):
    import polars as pl
    from portfolio_risk.analytics import model_ledger as engine
    from portfolio_risk.analytics.commercial_ledger_reference import verify_preparation,verify_stream
    raw=json.loads((ROOT/'examples/models/commercial-credit-ledger-1.json').read_text());raw['spec']['horizon_days']=30
    for source in raw['commercial']:
        terminal_policy(source_request(source),destination)
        if reviewed:
            # One supplied three-day overnight observation keeps the complete
            # reservation within existing caps; daily three-observation owners
            # are covered separately by native and graph controls.
            at=source['source']['current']['asof_ordinal'];information=(at+3-719163)*86400+8*3600
            for role in ('original','current'):
                coupon=source['source'][role]['periods'][0]['coupon'];observation=coupon['observations'][0]
                observation.update(interest_end_ordinal=at+3,observation_end_ordinal=at+3,selection_cutoff_unix_seconds=information)
                observation['source']['publication_unix_seconds']=information;coupon['observations']=[observation]
            source['source']['acquisition']['cost']=100.+100.*.05*3/365.
            if source['source']['current']['scenario_id']=='baseline':
                pending_reviews(source);source['reviewed_credit_actions']['legal_waivers'][0]['recognition_ordinal']=at+4
    path=engine.run_model_ledger(raw,tmp_path,partition_rows=2);manifest=json.loads(path.read_text());header=json.loads((path.parent/'prepared-source.json').read_text())
    prepared=verify_preparation(raw,header['prepared_request'],header['source_receipt'])
    class Frames:
        completion=dict(complete=True,protocol=6)
        def frames(self,name):
            for part in manifest['tables'][name]['parts']:yield pl.read_parquet(path.parent/part['path'])
        def finish(self):pass
    assert verify_stream(raw,prepared,Frames())==manifest['validation']
    source=header['source_receipt']['sources'][0]
    assert any(p.get('pending_coupon_obligation_indices') for p in source['carry']['carry_points'])
    assert any(g['phase']=='pending_coupon_cash' for g in source['accounting']['groups'])
    assert source['accounting']['closing'][0].get('pending_coupon_obligation_indices',[])==[]
    if reviewed:
        closing=source['accounting']['closing'][0]
        assert closing['reviewed_credit_state']['allowance']==30.
        assert len(closing['reviewed_credit_state']['legal_waivers'])==1
        assert any(l['coupon_id'].startswith('pending-coupon:') and l['legal_remaining']>0. for l in closing['lots'])
    changed=deepcopy(header);owned=changed['source_receipt']['sources'][0]
    point=next(p for p in owned['carry']['carry_points'] if p.get('pending_coupon_obligation_indices'))
    point['pending_coupon_obligation_indices']=[9999]
    with pytest.raises(ArithmeticError):verify_preparation(raw,changed['prepared_request'],changed['source_receipt'])


def pending_reviews(raw,sign=1.):
    terminal_policy(raw,3)
    at=raw['source']['current']['asof_ordinal'];terms=raw['credit']['intervals'][0]
    terms['loss_given_default']=[.4]*3;terms['interest_recovery_fraction']=[.25]*3
    terms['complete_recovery_tails']=[[dict(lag_years=4/365.,weight=1.)]for _ in range(3)]
    information=raw['source']['current']['periods'][0]['coupon']['observations'][0]['source']['publication_unix_seconds']
    late=f'commercial-interest:17:pending-coupon:0:{information}:0';delta=100.*.04*.5/365.*sign
    raw['reviewed_credit_actions']=dict(policy=dict(identity=dict(id='reviewed-pending-default',revision='1'),accrued='transfer_until_reviewed_reversal',basis='transfer_until_reviewed_chargeoff',allowance_shortfall='expense_at_reviewed_chargeoff',excess_cash_recovery='recognize_in_earnings'),
        allowance_reviews=[dict(path_key=17,review=dict(review=dict(id='provision',revision='1'),recognition_ordinal=at+1,phase='before_credit',target_amount=50.))],
        chargeoffs=[dict(path_key=17,review=dict(review=dict(id='chargeoff',revision='1'),claim_id='commercial-default:17',recognition_ordinal=at+2,principal_amount=20.,book_adjustment_amount=0.,accrued_interest_reversal=0.))],
        legal_waivers=[dict(path_key=17,review=dict(id='pending-waiver',revision='1'),recognition_ordinal=at+2,claim_id=late,principal_amount=0.,accrued_interest_amount=delta*.5,future_cash_disposition='retain_supplied_voluntary_recovery_cash-1')])
    return raw


@pytest.mark.parametrize('sign',[1.,-1.])
def test_independent_pending_default_reviews_preserve_late_legal_waiver_and_cash(sign):
    raw=source_request()
    if sign<0.:
        for role in ('original','current'):
            for o in raw['source'][role]['periods'][0]['coupon']['observations']:o['rate']=-.04
        raw['source']['acquisition']['cost']=100.+100.*(-.03)*3/365.
    pending_reviews(raw,sign);result=replay(raw);book,closing=groups(result);delta=100.*.04*.5/365.*sign
    late=next(l for l in closing[0]['lots']if l['coupon_id'].startswith('pending-coupon:'))
    assert late['legal_remaining']==pytest.approx(delta*.25,abs=1e-12)
    assert late['recognized_remaining']==0.
    assert closing[0]['reviewed_credit_state']['allowance']==30.
    paid=next(g for g in book if g['phase']=='pending_coupon_cash');assert paid['cash']==pytest.approx(delta*.25,abs=1e-12)
    assert paid['year']==pytest.approx(4.5/365.,abs=1e-15)
    assert len(closing[0]['reviewed_credit_state']['legal_waivers'])==1
    assert sum(len(c['settled_interest_recovery_ids'])for c in closing[0]['reviewed_credit_state']['claims']if c['claim_id'].startswith('commercial-legal-cash:17:pending-coupon:'))==1


@pytest.mark.parametrize('reverse',[False,True])
@pytest.mark.parametrize('sign',[1.,-1.])
def test_native_pending_default_reviewed_claims_have_independent_graph_replay(reverse,sign):
    from portfolio_risk.analytics import model_contracts
    from portfolio_risk.analytics.commercial_graph_reference import verify_graph
    raw=graph_request();p=raw['positions'][0]
    if sign<0.:
        histories=list(p['original_pricing'])+p.get('original_physical',[])
        for scenes in p['histories']:
            for role in ('physical','pricing','pricing_down','pricing_up'):histories+=scenes[role]
        for h in histories:
            for role in ('original','current'):
                for o in h['source']['source'][role]['periods'][0]['coupon']['observations']:o['rate']=-.04
            h['source']['source']['acquisition']['cost']=100.+100.*(-.03)*3/365.
        p['original_signed_cash_quote']=histories[0]['source']['source']['acquisition']['cost']
    for scenes in p['histories']:
        for role in ('physical','pricing','pricing_down','pricing_up'):
            for h in scenes[role]:
                terminal_policy(h['source'],3)
                if role=='physical':
                    pending_reviews(h['source'],sign)
                    if reverse:h['source']['reviewed_credit_actions']['policy'].update(accrued='reverse_income_at_default',basis='recognize_in_earnings_at_default')
    stream=deepcopy(p['histories'][0]['physical'][0]['source']['credit']['draw_stream'])
    for h in p['original_pricing']+p.get('original_physical',[]):
        terminal_policy(h['source'],None);h['source']['credit']['draw_stream']=deepcopy(stream)
    result=model_contracts.analyze_commercial_graph(raw)
    assert verify_graph(raw,result)['physical_accounting_recomputed']


def test_pending_waiver_cannot_precede_native_publication():
    raw=pending_reviews(source_request());raw['reviewed_credit_actions']['legal_waivers'][0]['recognition_ordinal']-=1
    with pytest.raises(ArithmeticError):groups(replay(raw))
