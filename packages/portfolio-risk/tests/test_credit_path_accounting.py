"""Independent actual-event and debit-positive journal checks on public Rust transport."""
from copy import deepcopy
import json
import math
from pathlib import Path

import pytest

from portfolio_risk.analytics.model_contracts import account_credit_paths
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.analytics.credit_path_reference import account_path

EXAMPLE=Path(__file__).resolve().parents[3]/'examples/models/credit-path-accounting-1.json'


def example():return json.loads(EXAMPLE.read_text())


def native(raw):
    with run_context(RunConfig(compute_backend='rust')):return account_credit_paths(raw)


def replay(path,raw):
    opening=raw['path']['paths'][0]
    balances=dict(live_principal=opening['principal'],live_accrued_interest=opening['accrued_interest'],
        live_book_adjustment=opening['book_adjustment'],allowance=-raw['bindings'][0]['opening_allowance'])
    for timed in path['intents']:
        intent=timed['intent']
        assert math.fsum(p['amount'] for p in intent['postings'])==pytest.approx(0.,abs=1e-9)
        for posting in intent['postings']:
            name=posting['account'];balances[name]=balances.get(name,0.)+posting['amount']
    return balances


def test_half_day_realized_default_recovers_absolute_cash_and_reviewed_chargeoff():
    raw=example();out=native(raw);path=out['paths'][0]
    hazard=raw['path']['intervals'][0]['hazards_per_year'][0][3]
    u=raw['path']['draw_stream']['paths'][0]['pairs'][0]['waiting_uniform']
    event_year=-math.log(u)/hazard
    principal=100.+20.*.5
    earned=100.*.365*event_year
    recoverable_principal=principal*(1.-.4)
    assert event_year*365.==pytest.approx(.5,abs=1e-12)
    assert path['economic']['default']['year']==pytest.approx(event_year,abs=1e-15)
    assert path['economic']['default']['principal']==pytest.approx(principal)
    assert path['economic']['earned_interest']==pytest.approx(earned,abs=1e-12)
    gl=replay(path,raw)
    assert gl['cash']==pytest.approx(-10.+recoverable_principal+earned,abs=1e-9)
    assert gl['interest_income']==pytest.approx(-earned,abs=1e-9)
    assert gl.get('default_principal',0.)==pytest.approx(0.,abs=1e-9)
    assert gl.get('default_accrued_interest',0.)==pytest.approx(0.,abs=1e-9)
    assert gl['allowance']==pytest.approx(0.,abs=1e-9)
    assert path['daily'][-1]['cumulative_profit']==pytest.approx(earned,abs=1e-9)
    for row in path['daily']:
        assert math.fsum(row['probability'])+row['matured_probability']==pytest.approx(1.)
    assert path['complete_recoveries'][0]['principal']==pytest.approx(recoverable_principal)
    assert path['complete_recoveries'][0]['interest']==pytest.approx(earned,abs=1e-9)


def test_negative_coupon_is_a_payable_and_cash_is_signed_once():
    raw=example();raw['path']['intervals'][0]['accrual_rates'][0]=-.365
    path=native(raw)['paths'][0];gl=replay(path,raw)
    earned=-.05
    assert path['economic']['earned_interest']==pytest.approx(earned,abs=1e-12)
    assert gl['cash']==pytest.approx(56.+earned,abs=1e-9)
    assert gl['interest_income']==pytest.approx(-earned,abs=1e-9)
    assert path['closing']['claims'][0]['interest_recovery_cash_collected']==pytest.approx(earned)
    assert path['closing']['claims'][0]['outstanding_accrued_interest']==pytest.approx(0.)


def test_complete_recovery_tail_survives_reporting_with_exact_raw_time():
    raw=example()
    for tail in raw['path']['intervals'][0]['complete_recovery_tails']:tail[0]['lag_years']=20.
    path=native(raw)['paths'][0]
    recovery=path['complete_recoveries'][0]
    assert recovery['after_reporting']
    assert recovery['exact_time']['elapsed_year']==pytest.approx(20.+.5/365.,abs=1e-12)
    assert path['closing']['claims'][0]['remaining_recoveries'][0]['amount']==pytest.approx(66.)
    gl=replay(path,raw)
    assert gl['cash']==pytest.approx(-10.)
    assert gl['default_principal']==pytest.approx(66.)
    assert gl['default_accrued_interest']==pytest.approx(.05,abs=1e-9)


@pytest.mark.parametrize('mutation', ['pricing','source','missing_binding','unknown_field'])
def test_incompatible_inputs_fail_without_fallback(mutation):
    raw=deepcopy(example())
    if mutation=='pricing':raw['path']['measure']='pricing'
    elif mutation=='source':raw['bindings'][0]['economic_source']['revision']='different'
    elif mutation=='missing_binding':raw['bindings']=[]
    else:raw['unexpected']=1
    with pytest.raises(ValueError):native(raw)


def independent_equal(expected,actual,path=''):
    if isinstance(expected,dict):
        assert expected.keys()==actual.keys(),path
        for key in expected:independent_equal(expected[key],actual[key],path+'/'+key)
    elif isinstance(expected,list):
        assert len(expected)==len(actual),path
        for i,(a,b) in enumerate(zip(expected,actual)):independent_equal(a,b,path+'/'+str(i))
    elif isinstance(expected,(int,float)) and not isinstance(expected,bool):
        assert actual==pytest.approx(expected,rel=1e-12,abs=1e-9),path
    else:assert expected==actual,path


@pytest.mark.parametrize('seed',range(24))
def test_independent_full_source_replay_with_cures_draws_repayments_defaults_and_signed_rights(seed):
    raw=example();path=raw['path'];path['draw_stream']=dict(kind='counter_splitmix64_v1',stream_id='independent-crn',stream_revision='1',seed=seed)
    interval=path['intervals'][0]
    interval['hazards_per_year']=[[0,18,6,24,12],[10,0,6,40,14],[2,3,0,50,15]]
    interval['accrual_rates']=[.365,-.1,0.]
    path['paths'][0]['accrued_interest']=-2. if seed%3==0 else 0.
    path['boundaries']=[dict(event_id='day-one-draw',policy_id='terms',year=1./365.,
        action=dict(kind='draw',undrawn_fraction=[.2,.3,.1])),
        dict(event_id='day-two-repay',policy_id='terms',year=2./365.,action=dict(kind='repayment',
            principal_fraction=[.1,.2,.3],interest_fraction=[.5,.8,.7],cancel_undrawn_fraction=[0.,0.,0.])),
        dict(event_id='maturity',policy_id='terms',year=3./365.,action=dict(kind='maturity'))]
    binding=raw['bindings'][0];binding['chargeoffs']=[]
    if seed%2:binding['policy']['accrued']='reverse_income_at_default'
    expected=account_path(raw,path['paths'][0],binding)
    independent_equal(expected,native(raw)['paths'][0])
