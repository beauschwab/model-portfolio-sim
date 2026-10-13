"""Independent deposit equations and immutable modeled source publication gates."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import deposit_schedule_reference as reference
from portfolio_risk.analytics import model_contracts as models, model_ledger as engine
from portfolio_risk.analytics.credit_path_verification import equal

ROOT = Path(__file__).resolve().parents[3]

def example():
    return json.loads((ROOT / 'examples/models/deposit-graph-dated-ledger-1.json').read_text())

def contract():
    return example()['graph']['positions'][0]['histories'][0]['physical'][0]['request']

def test_daily_expense_is_earned_before_cash_and_complete_tail_is_retained():
    raw = contract(); result = reference.project(raw)
    assert result['total_interest_expense'] == pytest.approx(31.)
    assert result['points'][30]['accrued_interest'] == pytest.approx([30.])
    assert result['points'][30]['balances'] == pytest.approx([1000.])
    assert result['total_bank_cash_interest'] == pytest.approx(-31.)
    assert result['closing_balance'] == result['closing_accrued_interest'] == 0.
    assert len(result['cashflows']) == 2
    assert [v['kind'] for v in result['cashflows']] == ['interest_payment', 'contractual_maturity']
    equal(models.simulate_deposit_state(raw), result, 'independent_deposit')

def test_competing_runoff_settles_opening_day_once_and_conserves():
    raw = contract(); policy = raw['segments'][0]['retention']
    policy['withdrawal_per_year'] = 2.; policy['closure_per_year'] = 1.
    result = reference.project(raw); exited = -math.expm1(-3./365.)
    opening = result['cashflows'][:2]
    assert [v['ordinal'] for v in opening] == [raw['as_of_ordinal']] * 2
    assert opening[0]['principal'] == pytest.approx(-1000. * exited * 2./3.)
    assert opening[1]['principal'] == pytest.approx(-1000. * exited /3.)
    assert result['interest'][0]['amount'] == pytest.approx(1. * (1. - exited))
    assert sum(v['principal'] for v in result['cashflows']) == pytest.approx(-1000.)
    equal(models.simulate_deposit_state(raw), result, 'competing_deposit')

def test_observation_lag_and_asymmetric_half_life_have_independent_rate_oracle():
    raw = contract(); seg = raw['segments'][0]
    seg['opening_paid_rate'] = .02
    seg['repricing'].update(observation_lag_days=1, rising_half_life_days=1., falling_half_life_days=2.)
    seg['repricing']['target'].update(intercept=0., beta=1.)
    previous = deepcopy(raw['drivers'][0]); previous['ordinal'] -= 1
    raw['drivers'].insert(0, previous)
    for driver in raw['drivers']:
        driver['reference_rate'] = .06 if driver['ordinal'] < raw['as_of_ordinal'] else 0.
    result = reference.project(raw)
    assert result['interest'][0]['amount'] == pytest.approx(1000. * .04/365.)
    second = .04 + (1. - 2.**(-.5)) * (0. - .04)
    assert result['interest'][1]['amount'] == pytest.approx(1000. * second/365.)
    equal(models.simulate_deposit_state(raw), result, 'lagged_deposit')

def test_competition_velocity_and_confidence_enter_competing_exit_once():
    raw = contract(); segment = raw['segments'][0]
    segment['opening_paid_rate'] = .02; segment['repricing']['target']['intercept'] = .02
    segment['retention'].update(withdrawal_per_year=.2, closure_per_year=.1,
        opportunity_gap_sensitivity=2., absolute_daily_velocity_sensitivity=3.,
        uninsured_stress_sensitivity=4., closure_stress_sensitivity=.4)
    for driver in raw['drivers']:
        driver['reference_rate'] = .04 if driver['ordinal'] >= raw['as_of_ordinal'] else 0.
        driver['competitor_rates'] = [.07]; driver['confidence_stress'] = [.5]
    result = reference.project(raw)
    withdrawal = .2 + 2. * (.07-.02) + 3. * .04 + .5 * 4. * .5
    closure = .1 + .4 * .5; total = withdrawal + closure
    exit_fraction = -math.expm1(-total/365.)
    assert result['cashflows'][0]['principal'] == pytest.approx(-1000. * exit_fraction * withdrawal/total)
    assert result['cashflows'][1]['principal'] == pytest.approx(-1000. * exit_fraction * closure/total)
    assert result['interest'][0]['amount'] == pytest.approx(1000. * (1.-exit_fraction) * .02/365.)
    equal(models.simulate_deposit_state(raw), result, 'competition_deposit')

def test_multiple_segments_actual360_leap_and_assumed_terminal_keep_lineage():
    raw = contract(); other = deepcopy(raw['segments'][0]); other['segment_id'] = 'second'
    other['opening_balance'] = 200.; raw['segments'].append(other)
    raw['day_count'] = 'act360'; raw['terminal'] = {'kind':'assumed_liquidation','rationale':'explicit synthetic tail'}
    for segment in raw['segments']: segment['maturity_ordinal'] = None
    for driver in raw['drivers']:
        for name in ['competitor_rates','offered_spread_stress','confidence_stress','external_inflows']:
            driver[name].append(driver[name][0])
    result = reference.project(raw)
    assert result['total_interest_expense'] == pytest.approx(1200. * .365 *31./360.)
    assert result['total_assumed_terminal_payout'] == pytest.approx(1200.)
    assert {v['segment_id'] for v in result['interest']} == {'retail','second'}
    equal(models.simulate_deposit_state(raw), result, 'segment_deposit')

@pytest.mark.parametrize('case', ['migration','inflow','opening','negative','tail','measure'])
def test_unsupported_capture_ownership_is_not_silently_pruned(case):
    raw = contract()
    if case == 'migration': raw['migrations'] = [{}]
    elif case == 'inflow': raw['drivers'][0]['external_inflows'][0] = 1.
    elif case == 'opening': raw['segments'][0]['opening_accrued_interest'] = 1.
    elif case == 'negative': raw['segments'][0]['repricing']['floor'] = -.01
    elif case == 'measure': raw['identity']['measure'] = 'pricing'
    else: raw['segments'][0]['maturity_ordinal'] = None
    with pytest.raises(ArithmeticError): reference.project(raw)

def test_reference_cancellation_is_cooperative():
    checks = [0]
    def cancel(): checks[0] += 1; return checks[0] == 3
    with pytest.raises(InterruptedError): reference.project(contract(), cancel)

def test_analysis_signed_rate_measure_is_explicit_and_does_not_expand_ledger_guard():
    raw = contract(); raw['identity']['measure'] = 'pricing'
    segment = raw['segments'][0]
    segment['opening_paid_rate'] = -.01; segment['repricing']['floor'] = -.02
    segment['repricing']['target']['intercept'] = -.01
    result = reference.project_declared(raw, measure='pricing')
    assert result['total_interest_expense'] == pytest.approx(-1000. * .01 *31./365.)
    assert result['total_bank_cash_interest'] == pytest.approx(1000. * .01 *31./365.)
    with pytest.raises(ArithmeticError): reference.project(raw)
    with pytest.raises(ArithmeticError): reference.project_declared(raw, measure='physical')
    # The standalone deposit-state protocol is physical-only. The graph owns
    # explicitly supplied Q policies; identical declared policy math is tested
    # against a separately declared physical source here.
    physical = deepcopy(raw); physical['identity']['measure'] = 'physical'
    equal(models.simulate_deposit_state(physical), reference.project_declared(physical, measure='physical'), 'signed_declared_deposit')

@pytest.fixture(scope='module')
def saved(tmp_path_factory):
    raw = example(); path = engine.run_model_ledger(raw, tmp_path_factory.mktemp('deposit-ledger'), partition_rows=7)
    manifest = json.loads(path.read_text())
    source = json.loads((path.parent / manifest['modeled_source']['path']).read_text())
    return raw, source, manifest

def test_native_source_and_persisted_journal_have_separate_independent_gates(saved):
    raw, source, manifest = saved
    result = reference.verify_preparation(raw, source['prepared_request'], source['source_receipt'])
    assert result['physical_deposit_source_recomputed'] and result['accrual_origins_retained']
    assert not result['pricing_graph_independently_verified']
    assert manifest['validation']['journal_replayed'] and manifest['validation']['dated_source_reconciled']

@pytest.mark.parametrize('tamper', ['raw','earned','origin','cash','segment','anchor','run'])
def test_source_economics_origin_and_anchor_tampering_reject(saved, tamper):
    raw, source, _ = saved; source = deepcopy(source); receipt = source['source_receipt']
    if tamper == 'raw': receipt['raw_request']['graph']['currency'] = 'EUR'
    elif tamper == 'earned': next(s for s in receipt['graph']['native_sources'] if s['role'] == 'physical')['projection']['interest'][0]['amount'] += 1.
    elif tamper == 'origin': receipt['capture']['accrual_plans'][0]['segments'][0]['origin_id'] = 'forged'
    elif tamper == 'cash': receipt['capture']['sources'][0]['schedule']['events'][0]['components']['cash_interest'] -= 1.
    elif tamper == 'segment': receipt['cash_origins'][0]['segment_id'] = 'forged'
    elif tamper == 'anchor': source['prepared_request']['specification']['positions'][0]['balance'] += 1.
    else: source['prepared_request']['dated']['runs'][2]['severity'] = .25
    with pytest.raises(ArithmeticError): reference.verify_preparation(raw, source['prepared_request'], receipt)
