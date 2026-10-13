"""Hand financial identities for independent commercial publication oracles."""
from copy import deepcopy
from pathlib import Path
import json
import math
import pytest
from portfolio_risk.analytics.commercial_source_reference import project, tau, compare
from portfolio_risk.analytics.commercial_credit_reference import replay
from portfolio_risk.analytics.commercial_accounting_reference import groups

ROOT=Path(__file__).resolve().parents[3]


def request():return deepcopy(json.loads((ROOT/'examples/models/commercial-credit-ledger-1.json').read_text())['commercial'][0])


def draw_request(eligible=True):
    raw=request();source=deepcopy(raw['source']);origin=source['current']['asof_ordinal'];amount=20.;annual=.07
    for role in ('original','current'):
        current=source[role];current['contract']['id']='draw-contract';current['position_id']='draw-contract';current.update(asof_ordinal=origin+10,reporting_end_ordinal=origin+50,tail_end_ordinal=origin+50,opening_principal=amount,principal_plan=dict(kind='supplied',principal_payments=[amount]))
        current['periods'][0].update(period_id='draw-coupon',start_ordinal=origin+10,end_ordinal=origin+50,payment_ordinal=origin+50,coupon=dict(kind='fixed',annual_rate=.15))
    cost=(amount+amount*.15*40/365)*math.exp(-math.log1p(annual)*40/365);source['acquisition']['identity']['id']='draw-acquisition';source['acquisition']['original_yield_anchor']['id']='draw-original-yield';source['acquisition'].update(cost=cost,original_annual_effective_rate=annual)
    raw['performing_draw_tranches']=[dict(review=dict(id='draw-review',revision='1'),draw_ordinal=origin+10,eligible_origins=[eligible,not eligible,False],source=source)];raw['credit']['opening_undrawn_commitment']=40.;raw['credit']['intervals'][0]['end_year']=50/365
    return raw,cost


def test_independent_draw_uses_actual_cost_distinct_nominal_and_original_tranche_eir():
    raw,cost=draw_request();result=replay(raw);path=result['credit']['result']['paths'][0]
    draw=next(e for e in path['events'] if e['kind']=='draw');funded=next(p for p in result['carry_points'] if p['kind']=='funded_draw')
    assert draw['principal']==20. and draw['cash']==-cost and funded['undrawn_commitment']==20.
    assert funded['principal']==120. and funded['draw_tranches'][0]['gross_carry']==cost
    assert result['native_source_cash'][1]['cash']['tranche_id']=='draw-review'
    following=next(p for p in result['carry_points'] if p['kind']=='live_sojourn' and p['year']>10/365)
    assert following['draw_tranches'][0]['gross_carry']==pytest.approx(cost*math.pow(1.07,following['year']-10/365),abs=1e-12)
    book,_=groups(result)
    assert all(abs(sum(line['posting']['amount'] for line in group['postings']))<1e-12 for group in book)


def test_independent_ineligible_draw_retains_unfunded_commitment_and_no_coupon():
    raw,_=draw_request(False);result=replay(raw);path=result['credit']['result']['paths'][0]
    assert not any(e['kind']=='draw' for e in path['events']) and path['cash_funded']==0.
    assert path['closing_undrawn_commitment']==40.
    assert len(result['native_source_cash'])==1
    assert all(c['coupon_id']!='draw-coupon' for c in result['earned_origin_contributions'])


@pytest.mark.parametrize('payment_day',[10,30,35])
def test_cash_timing_preserves_period_opening_entitlement(payment_day):
    raw=request();source=raw['source']['current'];source['periods'][0]['payment_ordinal']=source['asof_ordinal']+payment_day
    result=replay(raw);book,closing=groups(result)
    assert sum(c['contractual_earned'] for c in result['earned_origin_contributions'])==pytest.approx(100*.1*30/365,abs=1e-12)
    assert result['native_source_cash'][0]['year']==payment_day/365
    assert closing[0]['gross_carry']==pytest.approx(0.,abs=1e-12)
    if payment_day<30:
        assert any(c['principal']==0. and c['earned_interest']>0. for c in result['credit']['sojourns'])
        paid=next(g for g in book if g['phase']=='contractual_cash')
        assert paid['next']['legal_interest']<0.


@pytest.mark.parametrize('annual_rate',[.1,0.,-.1])
@pytest.mark.parametrize('recognition',['full_accrual','legal_only_income_stopped'])
def test_signed_rights_and_reviewed_nonaccrual_are_distinct(annual_rate,recognition):
    raw=request();source=raw['source']['current'];source['periods'][0]['coupon']['annual_rate']=annual_rate
    raw['accounting']['recognition'][0]['by_origin']=[recognition]*3
    result=replay(raw);book,_=groups(result);earned=next(g for g in book if g['phase']=='live_sojourn' and g['next']['lots'])['next']
    assert earned['legal_interest']==pytest.approx(100*annual_rate/365,abs=1e-12)
    assert earned['recognized_interest']==pytest.approx(earned['legal_interest'] if recognition=='full_accrual' else 0.,abs=1e-12)
    assert earned['gross_carry']==pytest.approx(earned['principal']+earned['recognized_interest']+earned['book_adjustment'],abs=1e-12)


def default_request(destination=3):
    raw=request();credit=raw['credit'];hazard=10.;credit['intervals'][0]['hazards_per_year'][0][destination]=hazard
    credit['draw_stream']=dict(kind='supplied',stream_id='observed-controls',stream_revision='1',paths=[dict(path_key=17,pairs=[dict(waiting_uniform=math.exp(-hazard*12.5/365),destination_uniform=.5)])])
    return raw


def test_fractional_default_time_ead_and_complete_recovery_tail():
    raw=default_request();credit=raw['credit'];credit['opening_undrawn_commitment']=40.;credit['intervals'][0]['credit_conversion_factor']=[.5]*3
    credit['intervals'][0]['loss_given_default']=[.25]*3
    credit['intervals'][0]['complete_recovery_tails']=[[dict(lag_years=50.25/365,weight=1.)] for _ in range(3)]
    result=replay(raw);economic=result['credit']['result']['paths'][0];claim=economic['default']
    assert claim['year']==pytest.approx(12.5/365,abs=1e-15)
    assert claim['principal']==120.
    assert economic['cash_funded']==20.
    assert claim['economic_principal_loss']==30.
    assert result['complete_recovery_receipts'][0]['year']==pytest.approx(62.75/365,abs=1e-15)
    assert result['complete_recovery_receipts'][0]['applied_in_horizon']
    _,closing=groups(result)
    assert closing[0]['principal']==30.
    assert closing[0]['in_default']
    assert sum(l['legal_remaining'] for l in closing[0]['lots'])==pytest.approx(claim['accrued_interest']*.5,abs=1e-12)


def test_contractual_prepay_fee_realizes_basis_without_inventing_principal():
    raw=default_request(4);raw['prepayment_terms']=dict(review=dict(id='premium',revision='1'),principal_premium_fraction=.03)
    result=replay(raw);economic=result['credit']['result']['paths'][0];event=economic['events'][0]
    assert event['cash']==pytest.approx(100+100*.1*12.5/365+3.,abs=1e-12)
    assert event['book_adjustment_recognition']==pytest.approx(raw['source']['acquisition']['cost']-event['cash'],abs=1e-12)
    assert economic['book_adjustment_recognized']==event['book_adjustment_recognition']
    book,closing=groups(result);exit_=next(g for g in book if g['phase']=='full_live_exit')
    assert exit_['source_event_id']==event['event_id'];assert closing[0]['gross_carry']==0.


def test_integral_fee_changes_cash_and_basis_once():
    raw=request();raw['source']['current']['periods'][0]['fee']=2.;raw['source']['acquisition']['cost']+=2.
    result=replay(raw);paid=result['native_source_cash'][0]['cash']
    assert paid['fee']==2.
    assert result['credit']['result']['paths'][0]['cash_received']==pytest.approx(102+100*.1*30/365,abs=1e-12)
    assert groups(result)[1][0]['gross_carry']==pytest.approx(0.,abs=1e-12)


def test_leap_and_year_crossing_day_count_is_actual():
    from datetime import date
    start=date(2023,12,30).toordinal();end=date(2024,1,3).toordinal()
    assert tau(start,end,'act_act_isda')==2/365+2/366


@pytest.mark.parametrize('changed',[True,float('nan'),float('inf'),1.00001])
def test_calculated_scalar_does_not_accept_boolean_nonfinite_or_amount_drift(changed):
    with pytest.raises(ArithmeticError):compare(1.,changed)


def test_oracle_checks_cancellation_during_raw_factor_credit_and_books():
    raw=request();calls=0
    def check():
        nonlocal calls
        calls+=1
        if calls==11:raise InterruptedError('test interruption')
    with pytest.raises(InterruptedError):replay(raw,check)
    assert calls==11


def exercise(raw,call=.8,put=None):
    return dict(schema='commercial-native-exercise-1',identity=dict(id='exercise',revision='1'),information_policy='native_full_observed_prefix-1',simultaneous_priority='issuer',exercise_dates=[dict(ordinal=raw['source']['current']['asof_ordinal']+15,timing='after_scheduled_cash',call_clean_price_factor=call,put_clean_price_factor=put,call_fee=0.,put_fee=0.)])


def curve():return dict(currency='USD',times_years=[0.,1.],discount_factors=[1.,1.],right_tail='reject')


@pytest.mark.parametrize('call,put,action,principal',[(.8,None,'call',80.),(None,1.1,'put',110.)])
def test_independent_exercise_and_stopped_credit_loop_agree(call,put,action,principal):
    from portfolio_risk.analytics.commercial_exercise_reference import compile_policy,stops_for
    raw=request();compiled=compile_policy(exercise(raw,call,put),[dict(source=raw,weight=1.)],curve(),0.)
    assert compiled['result']['nodes'][0]['decision']==action
    assert compiled['result']['value']==pytest.approx(principal+100*.1*15/365,abs=1e-12)
    stopped=replay(raw,stops=stops_for(compiled,raw));assert stopped['credit']['result']['paths'][0]['cash_received']==pytest.approx(compiled['result']['value'],abs=1e-12)
    book,closing=groups(stopped);assert closing[0]['gross_carry']==0.;assert closing[0]['legal_interest']==0.
    assert next(g for g in book if g['phase']=='full_live_exit')['source_event_id'].startswith('native-exercise:')


def test_exercise_future_default_is_not_observable_at_prior_decision():
    from portfolio_risk.analytics.commercial_exercise_reference import compile_policy
    a=request();b=default_request();b['credit']['paths'][0]['path_key']=18;b['credit']['draw_stream']['paths'][0]['path_key']=18
    b['credit']['draw_stream']['paths'][0]['pairs'][0]['waiting_uniform']=math.exp(-10.*20/365)
    b['credit']['intervals'][0]['loss_given_default']=[.4]*3
    result=compile_policy(exercise(a,1.),[dict(source=a,weight=.5),dict(source=b,weight=.5)],curve(),0.)['result']
    assert len(result['nodes'])==1;assert result['nodes'][0]['probability']==1.
    assert result['nodes'][0]['decision']=='continue';assert result['value']<90.


def test_exercise_physical_uncovered_information_and_nested_boolean_reject():
    from portfolio_risk.analytics.commercial_exercise_reference import compile_policy,stops_for,verify_policy
    raw=request();policy=exercise(raw);histories=[dict(source=raw,weight=1.)];compiled=compile_policy(policy,histories,curve(),0.)
    changed=deepcopy(raw);changed['source']['current']['periods'][0]['coupon']['annual_rate']=.2
    with pytest.raises(ArithmeticError,match='absent'):stops_for(compiled,changed)
    actual=deepcopy(compiled['result']);actual['nodes'][0]['legal_interest']=True
    with pytest.raises(ArithmeticError):verify_policy(policy,histories,curve(),0.,0.,actual)


def test_information_masks_future_fixing_even_if_publication_selection_is_early():
    from portfolio_risk.analytics.commercial_exercise_reference import information
    raw=request();projection=replay(raw);asof=raw['source']['current']['asof_ordinal']
    observations=[dict(fixing_ordinal=asof+20,selection_cutoff_unix_seconds=(asof-719163)*86400,rate=.02,source=dict(id='forward',revision='1'))]
    a=deepcopy(raw);b=deepcopy(raw)
    for source in (a,b):source['source']['current']['periods'][0]['coupon']=dict(kind='indexed',observations=deepcopy(observations))
    b['source']['current']['periods'][0]['coupon']['observations'][0]['rate']=.07
    assert information(a,projection,asof+15)==information(b,projection,asof+15)
    assert information(a,projection,asof+20)!=information(b,projection,asof+20)


def test_reviewed_exact_legal_maturity_closes_financial_basis_with_no_cash_plug():
    raw=request();raw['maturity_basis_policy']=dict(review=dict(id='basis-disposal',revision='1'),disposition='dispose_financial_basis_after_exact_legal_close-1')
    result=replay(raw);path=result['credit']['result']['paths'][0];book,closing=groups(result)
    event=path['events'][-1];assert event['kind']=='maturity';assert event['cash']==0.
    assert closing[0]['gross_carry']==0.;assert closing[0]['principal']==0.;assert closing[0]['legal_interest']==0.
    assert book[-2]['phase']=='full_live_exit';assert book[-2]['basis_realization']==pytest.approx(raw['source']['acquisition']['cost']-100-100*.1*30/365,abs=1e-12)
    changed=deepcopy(raw);changed['source']['current']['principal_plan']['principal_payments']=[99.]
    with pytest.raises(ArithmeticError,match='exact legal close'):replay(changed)


def test_canonical_coupon_closure_is_exact_without_erasing_origin_diagnostics():
    raw=request();raw['source']['current']['periods'][0]['coupon']['annual_rate']=.037
    result=replay(raw);closed=result['carry_points'][-1]['canonical_legal_coupons'][0]
    assert closed['contractual_earned']==100*.037*30/365
    assert closed['actual_interest_collected']==closed['contractual_earned']
    assert closed['legal_remaining']==0.
    assert result['credit']['result']['paths'][0]['closing_accrued_interest']==0.
    assert groups(result)[1][0]['legal_interest']==0.


def test_before_cash_exercise_uses_exact_earned_endpoint_and_once_only_stop_cash():
    from portfolio_risk.analytics.commercial_exercise_reference import compile_policy,stops_for
    raw=request();policy=exercise(raw);date=policy['exercise_dates'][0];date.update(ordinal=raw['source']['current']['tail_end_ordinal'],timing='before_scheduled_cash')
    compiled=compile_policy(policy,[dict(source=raw,weight=1.)],curve(),0.)
    node=compiled['result']['nodes'][0];assert node['live_principal']==100.;assert node['decision']=='call';assert node['information']['events']==[]
    assert node['continuation_value']==pytest.approx(100+100*.1*30/365)
    stopped=replay(raw,stops=stops_for(compiled,raw))
    assert stopped['native_source_cash']==[]
    assert stopped['credit']['result']['paths'][0]['cash_received']==pytest.approx(80+100*.1*30/365)
    date['timing']='after_scheduled_cash';after=compile_policy(policy,[dict(source=raw,weight=1.)],curve(),0.)
    assert after['result']['nodes'][0]['live_principal']==0.
    assert after['result']['value']==pytest.approx(100+100*.1*30/365)


def test_rights_only_termination_requires_review_and_preserves_signed_settlement():
    from portfolio_risk.analytics.commercial_exercise_reference import compile_policy,stops_for
    raw=request();source=raw['source']['current'];source['periods'][0]['payment_ordinal']=source['asof_ordinal']+10;policy=exercise(raw)
    with pytest.raises(ArithmeticError,match='rights-only'):compile_policy(policy,[dict(source=raw,weight=1.)],curve(),0.)
    policy['rights_only_termination']=dict(review=dict(id='rights-close',revision='1'),disposition='settle_earned_cancel_future-1')
    compiled=compile_policy(policy,[dict(source=raw,weight=1.)],curve(),0.);stop=stops_for(compiled,raw)[0]
    assert stop['rights_only_review']==policy['rights_only_termination']['review']
    path=replay(raw,stops=[stop])['credit']['result']['paths'][0]
    assert path['events'][-1]['principal']==0.;assert path['events'][-1]['cash']<0.
    assert path['cash_received']==pytest.approx(100+100*.1*15/365)


def reviewed_request():
    raw=default_request();credit=raw['credit'];asof=raw['source']['current']['asof_ordinal'];credit['intervals'][0]['hazards_per_year'][0][3]=1.;credit['draw_stream']['paths'][0]['pairs'][0]['waiting_uniform']=math.exp(-5/365)
    credit['intervals'][0]['loss_given_default']=[.4]*3;credit['intervals'][0]['complete_recovery_tails']=[[dict(lag_years=10/365,weight=1.)]for _ in range(3)]
    policy=dict(identity=dict(id='reviewed-credit',revision='1'),accrued='transfer_until_reviewed_reversal',basis='transfer_until_reviewed_chargeoff',allowance_shortfall='expense_at_reviewed_chargeoff',excess_cash_recovery='recognize_in_earnings')
    raw['reviewed_credit_actions']=dict(policy=policy,allowance_reviews=[dict(path_key=17,review=dict(review=dict(id='provision',revision='1'),recognition_ordinal=asof+1,phase='before_credit',target_amount=50.))],chargeoffs=[dict(path_key=17,review=dict(review=dict(id='charge',revision='1'),claim_id='commercial-default:17',recognition_ordinal=asof+7,principal_amount=80.,book_adjustment_amount=0.,accrued_interest_reversal=0.))],legal_waivers=[dict(path_key=17,review=dict(id='waiver',revision='1'),recognition_ordinal=asof+8,claim_id='commercial-default:17',principal_amount=10.,accrued_interest_amount=0.,future_cash_disposition='retain_supplied_voluntary_recovery_cash-1')])
    return raw


def test_reviewed_chargeoff_waiver_and_recovery_preserve_separate_cash_book_and_legal_owners():
    raw=reviewed_request();economic=replay(raw);book,closing=groups(economic);s=closing[0]
    unchanged=deepcopy(raw);unchanged.pop('reviewed_credit_actions');assert replay(unchanged)['credit']==economic['credit']
    assert s['principal']==0. and s['reviewed_credit_state']['legal_principal']==30. and s['reviewed_credit_state']['allowance']==0.
    expense=sum(p['posting']['amount']for g in book for p in g['postings']if p['posting']['account']=='credit_loss_expense');recovery=-sum(p['posting']['amount']for g in book for p in g['postings']if p['posting']['account']=='recovery_income')
    assert expense==80. and recovery==40.
    assert all(abs(sum(p['posting']['amount']for p in g['postings']))<1e-12 for g in book)
    assert next(g for g in book if g['phase']=='reviewed_legal_waiver')['postings']==[]


def test_reviewed_default_income_reversal_and_basis_recognition_have_no_second_cash_loss():
    raw=reviewed_request();raw['reviewed_credit_actions']['policy'].update(accrued='reverse_income_at_default',basis='recognize_in_earnings_at_default');raw['reviewed_credit_actions']['chargeoffs']=[]
    economic=replay(raw);book,closing=groups(economic);d=next(g for g in book if g['phase']=='default_gross_transfer')
    assert d['next']['book_adjustment']==0. and d['next']['recognized_interest']==0.
    assert any(p['posting']['account']=='basis_recognition_expense'for p in d['postings'])
    assert sum(g['cash']for g in book)==pytest.approx(economic['credit']['result']['paths'][0]['cash_received']-raw['source']['acquisition']['cost'],abs=1e-12)


def test_independent_acquisition_cash_partitions_keep_draw_cost_and_own_yield_separate():
    from portfolio_risk.analytics.commercial_ledger_reference import compile_original_acquisition
    raw,cost=draw_request();raw['source']['current']=deepcopy(raw['source']['original']);raw['performing_draw_tranches'][0]['source']['current']=deepcopy(raw['performing_draw_tranches'][0]['source']['original']);proof=compile_original_acquisition([dict(source=raw,weight=1.)],[[]]);part=proof['acquired_cash_partitions'][0]
    assert part['acquired']==[True] and len(part['tranches'][0])==1
    assert proof['conditional_tranche_proofs'][0]['conditional_cash_quote']==pytest.approx(cost,abs=1e-12)
    assert all(p['amount']!=-cost for p in part['base'])
    assert proof['original_stopped_cash_quote']==pytest.approx(raw['source']['acquisition']['cost'],abs=1e-12)


def test_before_cash_draw_information_excludes_unfunded_same_clock_event_and_stock():
    from portfolio_risk.analytics.commercial_exercise_reference import information
    raw,_=draw_request();projection=replay(raw);at=raw['performing_draw_tranches'][0]['draw_ordinal']
    before=information(raw,projection,at,'before_scheduled_cash');after=information(raw,projection,at,'after_scheduled_cash')
    assert before['principal']==100. and after['principal']==120.
    assert not any(e['kind']=='draw'for e in before['events'])and any(e['kind']=='draw'for e in after['events'])
    assert len(before['gross_legal_coupons'])==1 and len(after['gross_legal_coupons'])==2


def test_native_maturity_disposal_cannot_cancel_exact_gross_reviewed_legal_claim():
    raw=reviewed_request();raw['reviewed_credit_actions']['legal_waivers']=[];_,closing=groups(replay(raw))
    assert closing[0]['reviewed_credit_state']['legal_principal']==40.
    assert closing[0]['principal']==0. and closing[0]['legal_interest']>0.


def test_late_review_extends_complete_book_calendar_without_new_income_or_cash():
    from portfolio_risk.analytics.commercial_credit_reference import full_end
    raw=reviewed_request();early=replay(raw);asof=raw['source']['current']['asof_ordinal']
    raw['reviewed_credit_actions']['legal_waivers'][0]['recognition_ordinal']=asof+50
    late=replay(raw);book,closing=groups(late)
    assert full_end(raw)==50/365
    assert late['prepared']['raw_credit_path']['intervals'][-1]['end_year']==50/365
    assert late['credit']['result']['paths'][0]['cash_received']==early['credit']['result']['paths'][0]['cash_received']
    assert late['credit']['result']['paths'][0]['earned_interest']==early['credit']['result']['paths'][0]['earned_interest']
    assert next(g for g in book if g['phase']=='reviewed_legal_waiver')['year']==50/365
    assert closing[0]['year']==50/365 and closing[0]['reviewed_credit_state']['legal_principal']==30.


def test_compact_original_acquisition_replays_raw_cash_without_retaining_full_books():
    from portfolio_risk.analytics.commercial_ledger_reference import compile_original_acquisition
    raw=request();raw['source']['current']=deepcopy(raw['source']['original']);histories=[dict(source=raw,weight=1.)]
    ordinary=compile_original_acquisition(histories,[[]]);compact=compile_original_acquisition(histories,[[]],compact_generated=True)
    assert compact['stopped_physical']==[]
    assert compact['generated_original_physical'][0]['native_stops']==[]
    for key in ('original_stopped_cash_quote','acquisition','weights','conditional_tranche_proofs','acquired_cash_partitions','original_bindings'):assert compact[key]==ordinary[key]
    changed=deepcopy(raw);changed['source']['current']['periods'][0]['coupon']['annual_rate']=.2
    with pytest.raises(ArithmeticError,match='source/anchors'):compile_original_acquisition([dict(source=changed,weight=1.)],[[]],compact_generated=True)


def available_indexed_request():
    raw=request();source=raw['source']['current'];period=source['periods'][0]
    template=json.loads((ROOT/'examples/models/commercial-generated-exercise-1.json').read_text())['source']['source']['current']
    coupon=deepcopy(next(p['coupon']for p in template['periods']if p['coupon']['kind']=='indexed'));observation=coupon['observations'][0];start=period['start_ordinal'];cutoff=(start-719163)*86400
    observation.update(interest_start_ordinal=start,interest_end_ordinal=period['end_ordinal'],observation_start_ordinal=start,observation_end_ordinal=period['end_ordinal'],fixing_ordinal=start,selection_cutoff_unix_seconds=cutoff,rate=.1)
    observation['source']=dict(kind='conditional_projection',model=dict(id='available-index',revision='1'),path=deepcopy(source['path']),information_unix_seconds=cutoff,measure=source['measure'])
    coupon['reset_cutoff_unix_seconds']=cutoff;period['coupon']=coupon
    return raw


def test_native_factor_owner_rejects_cash_before_fixing_without_restricting_pure_arithmetic():
    from portfolio_risk.analytics.commercial_source_reference import project
    from portfolio_risk.analytics.commercial_credit_reference import prepared
    raw=available_indexed_request();period=raw['source']['current']['periods'][0];observation=period['coupon']['observations'][0];start=period['start_ordinal']+10;cutoff=(start-719163)*86400
    period['start_ordinal']=start;period['payment_ordinal']=start-1
    observation.update(interest_start_ordinal=start,observation_start_ordinal=start,fixing_ordinal=start,selection_cutoff_unix_seconds=cutoff);observation['source']['information_unix_seconds']=cutoff;period['coupon']['reset_cutoff_unix_seconds']=cutoff
    assert project(raw['source']['current'])[0]['schedule']['events'][0]['payment_ordinal']==start-1
    with pytest.raises(ArithmeticError,match='actual cash precedes'):prepared(raw)


def test_delayed_publication_cannot_own_continuous_partial_earning():
    from portfolio_risk.analytics.commercial_source_reference import project
    from portfolio_risk.analytics.commercial_credit_reference import prepared
    raw=available_indexed_request();source=raw['source']['current'];period=source['periods'][0];observation=period['coupon']['observations'][0];publication=(period['start_ordinal']-719163)*86400+3600
    observation['source']=dict(kind='observed',fixing=dict(id='delayed-publication',revision='1'),publication_unix_seconds=publication);observation['selection_cutoff_unix_seconds']=publication;period['coupon']['reset_cutoff_unix_seconds']=publication;source['observed_publication_cutoff_unix_seconds']=publication
    assert project(source)[0]['audit'][0]['interest']==pytest.approx(100*.1*30/365)
    with pytest.raises(ArithmeticError,match='interest-segment start midnight'):prepared(raw)


def test_published_lookback_preserves_cash_and_partial_native_source_factor():
    from portfolio_risk.analytics.commercial_source_reference import validate_native_availability,project,factor
    raw=available_indexed_request();source=raw['source']['current'];period=source['periods'][0];observation=period['coupon']['observations'][0];start=period['start_ordinal'];publication=(start-1-719163)*86400
    observation['fixing_ordinal']=start-2;observation['source']=dict(kind='observed',fixing=dict(id='published-lookback',revision='1'),publication_unix_seconds=publication);source['observed_publication_cutoff_unix_seconds']=publication
    validate_native_availability(source);schedule,segments=project(source);result=replay(raw)
    assert factor(segments[0],start,.5)==pytest.approx(.1*.5/365,abs=1e-14)
    assert schedule['audit'][0]['interest']==100*.1*30/365
    assert result['credit']['result']['paths'][0]['cash_received']==pytest.approx(100+100*.1*30/365)


def test_fixed_early_retained_payment_cannot_reveal_future_indexed_balance():
    from portfolio_risk.analytics.commercial_source_reference import project,validate_native_availability
    raw=json.loads((ROOT/'examples/models/commercial-generated-exercise-1.json').read_text())['source']['source']['current'];last=deepcopy(raw['periods'][-1])
    last.update(period_id='future-fixed-balance',start_ordinal=last['end_ordinal'],end_ordinal=raw['tail_end_ordinal'],payment_ordinal=raw['asof_ordinal']+1,coupon=dict(kind='fixed',annual_rate=0.))
    raw['periods'].append(last);raw['principal_plan']=dict(kind='retained_payment',original_payment=20.,terminal=dict(kind='residual_payoff'))
    schedule,_=project(raw);assert schedule['audit'][-1]['opening_principal']<100.
    with pytest.raises(ArithmeticError,match='inherited indexed entitlement'):validate_native_availability(raw)
    raw['principal_plan']=dict(kind='supplied',principal_payments=[20.,20.,60.]);validate_native_availability(raw)
