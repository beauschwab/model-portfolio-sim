"""Generated joint loan/fee stopping, separate numeraire and raw-source replay."""
from copy import deepcopy
import json
import math
from pathlib import Path

import pytest
from portfolio_risk.analytics.commercial_stochastic_reference import evaluate, verify
from portfolio_risk.core import model_native

ROOT = Path(__file__).resolve().parents[3]


def fixture():
    return json.loads((ROOT / "examples/models/commercial-generated-fee-exercise-1.json").read_text())


def native(raw):
    return model_native.run(raw["schema"], raw, timeout=120.)[0]


def test_fee_continuation_changes_actual_learned_call_and_preserves_training_only():
    raw = fixture()
    plain = deepcopy(raw)
    del plain["nonintegral_fees"]
    baseline = native(plain)
    actual = native(raw)
    assert verify(raw, actual)["generated_joint_market_credit"]
    assert baseline["evaluation_value"] == pytest.approx(100., abs=1e-11)
    assert all(p["first_exercise_decision"] == "continue" for p in baseline["evaluation_paths"])
    assert actual["evaluation_value"] == pytest.approx(100.01, abs=1e-11)
    assert all(p["first_exercise_decision"] == "call" for p in actual["evaluation_paths"])
    assert not actual["evaluation_outcomes_used_for_fit"]
    assert not actual["physical_journal_eligible"]
    assert actual["evaluation_clusters"] == 64
    assert len(actual["regressions"][0]["means"]) == len(baseline["regressions"][0]["means"]) + 11


@pytest.mark.parametrize("sign", [1., -1.])
@pytest.mark.parametrize("oas, spread, bump", [(0.0, .2, 0.0), (.13, -.2, 1e-4)])
def test_option_free_fee_tail_uses_separate_discount_not_loan_eir(sign, oas, spread, bump):
    raw = fixture()
    raw["policy"]["exercise_dates"] = []
    raw["fixed_base_oas"] = oas
    raw["parallel_bump"] = bump
    raw["nonintegral_fees"]["discount_spread"] = spread
    raw["nonintegral_fees"]["policy"]["fees"][0]["entitlement"] *= sign
    for event in raw["nonintegral_fees"]["policy"]["events"]:
        event["amount"] *= sign
    actual = native(raw)
    assert verify(raw, actual)["generated_joint_market_credit"]
    expected = 100. * math.exp(-(oas + bump) * 2. / 365.) + sign * 3. * math.exp(-(spread + bump) * 3. / 365.)
    assert actual["evaluation_value"] == pytest.approx(expected, abs=1e-10)
    assert actual["raw_source"]["source"]["source"]["acquisition"]["original_annual_effective_rate"] == 0.


def test_fee_origin_cash_is_included_once_while_original_loan_root_remains_ex_cash():
    raw = fixture()
    raw["policy"]["exercise_dates"] = []
    origin = raw["time_ordinals"][0]
    for event in raw["nonintegral_fees"]["policy"]["events"]:
        event["ordinal"] = origin
    actual = native(raw)
    assert verify(raw, actual)["generated_joint_market_credit"]
    assert actual["evaluation_value"] == pytest.approx(103., abs=1e-10)


@pytest.mark.parametrize("mutation", ["spread", "transfer", "tranche", "clock", "features"])
def test_bad_fee_law_refuses_before_generation(mutation):
    raw = fixture()
    law = raw["nonintegral_fees"]
    if mutation == "spread": law["discount_spread"] = 5.01
    elif mutation == "transfer": law["policy"]["transfer"] = {}
    elif mutation == "tranche": law["policy"]["fees"][0]["tranche_id"] = "drawn"
    elif mutation == "clock": law["policy"]["coincident_order"] = "native_exit_before_coincident_service_cash-1"
    else:
        fee = law["policy"]["fees"][0]
        law["policy"]["fees"] = [{**fee, "fee_id": f"fee-{i}"} for i in range(6)]
        law["policy"]["events"] = []
    with pytest.raises((RuntimeError, ValueError)):
        native(raw)


def test_corrupted_native_fee_law_or_joint_money_cannot_verify():
    raw = fixture()
    actual = native(raw)
    corrupt = deepcopy(actual)
    corrupt["evaluation_value"] += .01
    with pytest.raises(ArithmeticError): verify(raw, corrupt)
    corrupt = deepcopy(actual)
    corrupt["raw_source"]["nonintegral_fees"]["discount_spread"] += .01
    with pytest.raises(ArithmeticError): verify(raw, corrupt)


def test_negative_fee_continuation_changes_actual_put_decision():
    raw = fixture()
    date = raw['policy']['exercise_dates'][0]
    date.update(call_clean_price_factor=None, put_clean_price_factor=.9999)
    raw['nonintegral_fees']['policy']['fees'][0]['entitlement'] *= -1.
    for row in raw['nonintegral_fees']['policy']['events']: row['amount'] *= -1.
    actual = native(raw)
    assert verify(raw,actual)['generated_joint_market_credit']
    assert all(p['first_exercise_decision']=='put' for p in actual['evaluation_paths'])
    assert actual['evaluation_value']==pytest.approx(99.99,abs=1e-10)


def test_joint_numeraire_ratio_changes_the_call_vs_naively_rebased_fee():
    raw=fixture()
    raw['fixed_base_oas']=5.
    raw['nonintegral_fees']['discount_spread']=-5.
    raw['policy']['exercise_dates'][0]['call_clean_price_factor']=1.0176
    loan_now=math.exp(-5./365.)
    correct_continue=(100.*math.exp(-10./365.)+3.*math.exp(15./365.))/loan_now
    naive_continue=100.*math.exp(-5./365.)+3.*math.exp(10./365.)
    assert naive_continue<101.76<correct_continue
    actual=native(raw)
    assert verify(raw,actual)['generated_joint_market_credit']
    assert all(p['first_exercise_decision']=='call' for p in actual['evaluation_paths'])
    assert actual['evaluation_value']==pytest.approx(101.76*loan_now,abs=1e-10)


def test_two_reviewed_dates_use_the_same_joint_policy_and_complete_fee_tail():
    raw=fixture()
    raw['policy']['exercise_dates'][0]['call_clean_price_factor']=1.0002
    later=deepcopy(raw['policy']['exercise_dates'][0])
    later.update(ordinal=raw['time_ordinals'][-1],timing='before_scheduled_cash',call_clean_price_factor=1.0001)
    raw['policy']['exercise_dates'].append(later)
    actual=native(raw)
    assert verify(raw,actual)['generated_joint_market_credit']
    assert all(p['first_exercise_ordinal']==later['ordinal'] for p in actual['evaluation_paths'])
    assert actual['evaluation_value']==pytest.approx(100.01,abs=1e-10)


def graph_fixture():
    return json.loads((ROOT/'examples/models/commercial-generated-fee-graph-1.json').read_text())


def test_original_loan_oas_refits_joint_policy_but_targets_loan_quote_only():
    from portfolio_risk.analytics.commercial_generated_graph_reference import verify as verify_graph
    raw=graph_fixture();position=raw['positions'][0];g=position['generated_policy']
    # One genuine future service fee changes the policy; it is canceled on call.
    # Its separate original quote is zero. Do not absorb its unstopped cash into loan OAS.
    for law in [g['pricing_training'],g['original_physical'],*g['current_pricing'],*g['current_physical']]:
        policy=law['nonintegral_fees']['policy']
        policy['fees']=[f for f in policy['fees'] if f['fee_id']=='service-fee']
        policy['events']=[e for e in policy['events'] if e['fee_id']=='service-fee']
    a=g['fee_acquisition'];a['original_signed_fee_quote']=0.;a['total_purchase_cost']=100.01
    a['allocations']=[f for f in a['allocations'] if f['fee_id']=='service-fee']
    a['original_physical_releases']=[]
    for role in a['physical_releases_by_scenario']:role['acquisition_releases']=[]
    position['oas_target']={**position['oas_target'],'kind':'calibrate_positive_cash','target_cash_price':100.01}
    position['oas_target'].pop('base_oas')
    g['oas_calibration']=dict(policy='native_retained_q_refit_bracket-1',lower_oas=-.01,upper_oas=.01,max_trials=3)
    actual=native(raw)
    assert verify_graph(raw,actual)['verified']
    proof=actual['generated_oas_calibration']
    assert proof['target_cash_price']==100.01
    assert proof['selected_base_oas']==0.
    assert proof['continuation_refitted_each_trial'] is True
    assert proof['physical_outcomes_used_for_calibration'] is False


def test_signed_original_fee_quote_keeps_acquired_liability_and_loan_anchor_separate():
    from portfolio_risk.analytics.commercial_generated_graph_reference import verify as verify_graph
    raw=graph_fixture();g=raw['positions'][0]['generated_policy']
    for law in [g['pricing_training'],g['original_physical'],*g['current_pricing'],*g['current_physical']]:
        policy=law['nonintegral_fees']['policy']
        policy['fees'][0]['entitlement']=-2.
        for event in policy['events']:
            if event['fee_id']=='origin-service':event['amount']=-2.
    a=g['fee_acquisition'];a['original_signed_fee_quote']=-2.;a['total_purchase_cost']=98.01
    a['allocations'][0]['allocated_cost']=-2.
    a['original_physical_releases'][0]['amount']=-2.
    for role in a['physical_releases_by_scenario']:role['acquisition_releases'][0]['amount']=-2.
    actual=native(raw)
    assert verify_graph(raw,actual)['verified']
    assert actual['raw_source']['positions'][0]['original_signed_cash_quote']==100.01
    assert all(source['source_request']['source']['acquisition']['original_annual_effective_rate']==0.
               for source in actual['native_sources'])


@pytest.mark.parametrize('target',['quote','release','transfer','work_selector','ambiguous_acquisition'])
def test_generated_fee_acquisition_refuses_inconsistent_raw_authority(target):
    raw=graph_fixture();g=raw['positions'][0]['generated_policy'];a=g['fee_acquisition']
    if target=='quote':a['original_signed_fee_quote']+=.01
    elif target=='release':a['original_physical_releases'][0]['amount']+=.01
    elif target=='transfer':
        for law in [g['pricing_training'],g['original_physical'],*g['current_pricing'],*g['current_physical']]:law['nonintegral_fees']['policy']['transfer']={}
    elif target=='work_selector':a.pop('source_work_policy')
    else:raw['positions'][0]['fees']={}
    with pytest.raises((RuntimeError,ValueError)):
        native(raw)


def test_nonzero_fee_quote_is_excluded_from_original_loan_oas_target():
    from portfolio_risk.analytics.commercial_generated_graph_reference import verify as verify_graph
    raw=graph_fixture();position=raw['positions'][0];g=position['generated_policy']
    # Keep nonzero acquired fee cash, genuine128 Q/P paths and all paired bumps.
    # One named scenario admits the extra calibration phase under unchanged caps.
    raw['scenarios']=[s for s in raw['scenarios'] if s['scenario_id']=='baseline']
    g['current_pricing']=[s for s in g['current_pricing'] if s['source']['source']['current']['scenario_id']=='baseline']
    g['current_physical']=[s for s in g['current_physical'] if s['source']['source']['current']['scenario_id']=='baseline']
    g['fee_acquisition']['physical_releases_by_scenario']=[s for s in g['fee_acquisition']['physical_releases_by_scenario'] if s['scenario_id']=='baseline']
    position['oas_target']={**position['oas_target'],'kind':'calibrate_positive_cash','target_cash_price':100.01}
    position['oas_target'].pop('base_oas')
    g['oas_calibration']=dict(policy='native_retained_q_refit_bracket-1',lower_oas=-.01,upper_oas=.01,max_trials=3)
    actual=native(raw)
    assert verify_graph(raw,actual)['verified']
    receipt=actual['generated_oas_calibration']
    assert receipt['target_cash_price']==100.01 and receipt['selected_base_oas']==0.
    assert g['fee_acquisition']['original_signed_fee_quote']==2.
    for trial in receipt['trials']:
        assert trial['evaluation_value']==pytest.approx(100.01*math.exp(-trial['base_oas']/365.),abs=1e-9)


def test_stochastic_observable_coupon_joint_fee_changes_learned_policy_on_fresh_paths():
    from test_commercial_policy_refinement import request
    raw=request('linear',128,two_dates=False)
    law=deepcopy(fixture()['nonintegral_fees'])
    origin=raw['time_ordinals'][0]
    law['policy']['complete_end_ordinal']=origin+40
    law['policy']['fees'][0]['entitlement']=.02
    for event in law['policy']['events']:
        event['ordinal']=origin+40
        event['amount']=.02
    plain=deepcopy(raw)
    raw['nonintegral_fees']=law
    actual=native(raw);baseline=native(plain)
    assert verify(raw,actual)['generated_joint_market_credit']
    assert any(a['first_exercise_decision']!=b['first_exercise_decision']
        for a,b in zip(actual['evaluation_paths'],baseline['evaluation_paths'],strict=True))
    assert len({p['value'] for p in actual['evaluation_paths']})>1
    assert actual['regressions'][0]['active_features']
    assert actual['evaluation_clusters']==64 and not actual['evaluation_outcomes_used_for_fit']
