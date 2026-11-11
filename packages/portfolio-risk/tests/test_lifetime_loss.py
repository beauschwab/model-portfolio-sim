"""Native complete-tail sensitivity with an independent raw-source oracle."""
from copy import deepcopy
import json
from pathlib import Path

import pytest

from portfolio_risk.analytics.model_contracts import evaluate_lifetime_credit_loss
from portfolio_risk.analytics.lifetime_loss_reference import reconstruct, verify

ROOT = Path(__file__).resolve().parents[3]


@pytest.fixture
def raw():
    return json.loads((ROOT / 'examples/models/lifetime-credit-loss-1.json').read_text())


@pytest.mark.parametrize('product', ['auto', 'personal', 'credit_card'])
@pytest.mark.parametrize('variation', ['base', 'negative_coupon', 'undrawn', 'supplied', 'zero_weight', 'zero_scale', 'integer_scalars'])
def test_all_paths_and_model_scenario_summaries_replay(raw, product, variation):
    cohort = raw['cohorts'][0]; cohort['product'] = product
    source = cohort['source']
    if variation == 'negative_coupon':
        source['intervals'][0]['accrual_rates'] = [-.1]*3
        source['intervals'][0]['interest_recovery_fraction'] = [.5]*3
    elif variation == 'undrawn':
        for opening in source['paths']:
            opening['undrawn_commitment'] = 40.
        source['intervals'][0]['credit_conversion_factor'] = [.5]*3
    elif variation == 'supplied':
        source['draw_stream'] = dict(kind='supplied', stream_id='supplied-hand', stream_revision='1',
            paths=[dict(path_key=p['path_key'], pairs=[dict(waiting_uniform=.99, destination_uniform=.1)]) for p in source['paths']])
    elif variation == 'zero_weight':
        cohort['probability_weights'] = [0.]*16 + [1/16]*16
    elif variation == 'zero_scale':
        cohort['monetary_multiplier'] = 0.
    elif variation == 'integer_scalars':
        cohort['monetary_multiplier'] = 1
        raw['drivers'][0]['offset'] = 0
    result = evaluate_lifetime_credit_loss(raw)
    receipt = verify(raw, result)
    assert receipt['complete_unweighted_paths_recomputed'] and not receipt['empirical_validation']
    assert len(result['cells']) == 9 and all(len(c['full_paths']) == 32 for c in result['cells'])
    if variation == 'zero_scale':
        assert all(c['expected_total_loss'] == 0. for c in result['cells'])
        assert any(p['default'] is not None for c in result['cells'] for p in c['full_paths'])
    if variation == 'negative_coupon':
        assert all(c['expected_signed_interest_loss'] < 0. for c in result['cells'])


@pytest.mark.parametrize('defect', ['path_cash', 'recovery_tail', 'band', 'aggregate', 'outcome', 'raw_seed', 'raw_signed_zero', 'admission', 'approval'])
def test_coherent_or_local_producer_changes_do_not_certify(raw, defect):
    result = evaluate_lifetime_credit_loss(raw)
    if defect == 'path_cash':
        result['cells'][0]['full_paths'][0]['cash_received'] += 1.
    elif defect == 'recovery_tail':
        path = next(p for p in result['cells'][0]['full_paths'] if p['default'])
        path['default']['recoveries'][0]['year'] += 1.
    elif defect == 'band':
        result['sensitivity'][0]['spread'] += 1.
    elif defect == 'aggregate':
        result['product_totals'][0]['expected_total_loss'] += 1.
    elif defect == 'outcome':
        result['holdout_outcomes'][0]['reported_minus_projected'] += 1.
    elif defect == 'raw_seed':
        changed = deepcopy(raw); changed['cohorts'][0]['source']['draw_stream']['seed'] += 1
        result = evaluate_lifetime_credit_loss(changed)
    elif defect == 'raw_signed_zero':
        result['raw_source']['drivers'][0]['offset'] = -0.
    elif defect == 'admission':
        result['admitted_work_upper'] -= 1
    else:
        result['adoption_eligible'] = True
    with pytest.raises(ArithmeticError, match='lifetime loss'):
        verify(raw, result)


@pytest.mark.parametrize('defect', ['pricing', 'training', 'outcome_tail', 'weights', 'hazards', 'extra_field', 'missing_model', 'duplicate_scenario'])
def test_invalid_raw_contracts_refuse_natively_and_independently(raw, defect):
    c = raw['cohorts'][0]
    if defect == 'pricing': c['source']['measure'] = 'pricing'
    elif defect == 'training': raw['drivers'][0]['scaling_training_end'] = raw['training_end']+1
    elif defect == 'outcome_tail': raw['outcomes'][0]['observed_through_year'] = 1.
    elif defect == 'weights': c['probability_weights'][0] = .5
    elif defect == 'hazards': c['models'][0]['hazards_per_year'][0][0] = .2
    elif defect == 'extra_field': c['source']['invented_closing_plug'] = 1.
    elif defect == 'missing_model': c['models'].pop()
    else: raw['scenarios'][1]['scenario_id'] = 'base'
    with pytest.raises(ValueError): evaluate_lifetime_credit_loss(raw)
    with pytest.raises(ValueError): reconstruct(raw)


def test_verifier_has_no_producer_dependency_and_retains_cancellation(raw, monkeypatch):
    result = evaluate_lifetime_credit_loss(raw)
    from portfolio_risk.core import model_native
    monkeypatch.setattr(model_native, 'run', lambda *a, **k: pytest.fail('producer callback'))
    assert verify(raw, result)['verified']
    count = [0]
    def counted(): count[0] += 1; return False
    verify(raw, result, counted)
    for stop in (1, 30, count[0]):
        calls = [0]
        def cancelled(): calls[0] += 1; return calls[0] == stop
        with pytest.raises(InterruptedError): verify(raw, result, cancelled)


def test_same_panel_validation_pack_binds_native_fits_and_holdout_scores():
    from lifetime_loss_validation_pack import build
    pack = build()
    repeated = build()
    assert pack['diagnostics'] == repeated['diagnostics']
    assert pack['loss_result'] == repeated['loss_result']
    assert pack['zero_macro_spread_verified'] and pack['positive_macro_spread_verified']
    assert not pack['empirical_validation']
    rows = pack['diagnostics']['challenger']['holdout_predictions']
    assert all(r['account_id'].startswith('held-') for r in rows)
    assert len(pack['diagnostics']['comparisons']) == 3
    assert all(len(m['holdout_metrics']) == 4 for m in pack['diagnostics']['comparisons'])


def test_fresh_native_production_owner_does_not_call_independent_oracles():
    import os
    import subprocess
    import sys
    code = """
import json
from pathlib import Path
from portfolio_risk.analytics import lifetime_loss_reference, credit_path_reference, consumer_challenger_reference
from portfolio_risk.analytics.model_contracts import evaluate_lifetime_credit_loss
def forbidden(*a, **k): raise AssertionError('production invoked reference finance')
lifetime_loss_reference.reconstruct=forbidden
credit_path_reference.simulate_path=forbidden
consumer_challenger_reference.evaluate=forbidden
raw=json.loads(Path('examples/models/lifetime-credit-loss-1.json').read_text())
result=evaluate_lifetime_credit_loss(raw)
assert len(result['cells'])==9 and result['common_random_numbers_retained']
"""
    environment = os.environ.copy()
    environment['PYTHONPATH'] = str(ROOT/'packages/portfolio-risk/src')
    subprocess.run([sys.executable, '-c', code], cwd=ROOT, env=environment, check=True, timeout=60)


@pytest.mark.parametrize('defect', ['hidden_selector', 'missing_budget'])
def test_original_public_envelope_cannot_hide_selected_control_contract(raw, defect):
    import subprocess
    from portfolio_risk.core.model_native import binary_path
    envelope = dict(schema='lifetime-credit-loss-1', threads=1, execution_timeout_ms=30000, request=raw)
    if defect == 'missing_budget': envelope.pop('execution_timeout_ms')
    body = json.dumps(envelope, separators=(',', ':'))
    if defect == 'hidden_selector':
        body = body.replace('"schema":"lifetime-credit-loss-1"', '"schema":"lifetime-credit-loss-1","schema":"kpi-1"', 1)
    result = subprocess.run([str(binary_path())], input=body.encode(), capture_output=True, timeout=30)
    response = json.loads(result.stdout)
    assert result.returncode == 1 and response['ok'] is False
    assert ('duplicate' if defect == 'hidden_selector' else 'timeout') in response['error']
