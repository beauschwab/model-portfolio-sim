"""Independent hand controls through the installed native process boundary."""
import copy
import json
from pathlib import Path

import pytest

from portfolio_risk.analytics.model_contracts import (
    analyze_consumer_macro, project_consumer_macro_products,
    project_consumer_behaviour_products,
)
from portfolio_risk.core import model_native

ROOT = Path(__file__).resolve().parents[3]


@pytest.fixture(autouse=True)
def installed_native():
    if not model_native.binary_path().is_file():
        pytest.skip('matching native product executable required')


def example(name):
    return json.loads((ROOT/'examples/models'/f'{name}.json').read_text())


def test_macro_native_covariance_and_chronological_holdout_are_independent():
    request = example('consumer-macro-scenarios-1')
    result = analyze_consumer_macro(request)
    from portfolio_risk.analytics.consumer_macro_reference import verify
    assert verify(request, result)['verified']
    train = result['diagnostics']['training']
    assert train['mean_annualized_standardized_innovation'] == [0., 0.]
    for row, expected in zip(train['sample_annual_covariance'], [[4/3, 8/3], [8/3, 16/3]]):
        assert row == pytest.approx(expected, rel=0, abs=1e-12)
    changed = copy.deepcopy(request)
    changed['history']['observations'][4]['end_values'][1] += .5
    changed['history']['observations'][5]['start_values'] = changed['history']['observations'][4]['end_values'][:]
    other = analyze_consumer_macro(changed)
    assert result['training_identity_sha256'] == other['training_identity_sha256']
    assert train == other['diagnostics']['training']
    assert result['source_sha256'] != other['source_sha256']
    assert other['raw_source'] == changed


def test_macro_native_daily_cash_has_one_owner_and_exact_zero_effect_identity():
    request = example('consumer-macro-products-1')
    original = copy.deepcopy(request)
    baseline = project_consumer_behaviour_products(request['products'][0]['template'])
    result = project_consumer_macro_products(request)
    assert len(result['cases']) == 4
    for case in result['cases']:
        assert case['result']['products'] == baseline['products']
        assert case['result']['credit_states']['paths'] == baseline['credit_states']['paths']
        assert case['result']['raw_source']['projection']['scenario_id'] == case['scenario_id']
    assert request == original
    assert not result['adoption_eligible']
    assert not result['pricing_measure_permission']
    assert not result['empirical_accuracy_validated']


def test_macro_native_refuses_pricing_mismatched_draws_and_cancellation():
    macro = example('consumer-macro-scenarios-1')
    macro['measure'] = 'pricing'
    with pytest.raises(ValueError, match='Physical'):
        analyze_consumer_macro(macro)
    request = example('consumer-macro-products-1')
    request['products'][0]['template']['projection']['path_source']['draw_stream']['paths'][0]['pairs'][0]['waiting_uniform'] = .1
    with pytest.raises(ValueError, match='actual shared draws'):
        project_consumer_macro_products(request)
    with pytest.raises(InterruptedError, match='cancelled'):
        project_consumer_macro_products(request, cancelled=lambda: True)


def test_macro_native_covariance_preserves_small_signed_residuals():
    from portfolio_risk.analytics.consumer_macro_reference import verify
    case=json.loads((Path(__file__).parent/'fixtures/consumer-macro-cancellation.json').read_bytes())
    result=analyze_consumer_macro(case['request'])
    assert verify(case['request'],result)['verified']
