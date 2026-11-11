"""Raw-source substitution, spectral and seeded replay publication controls."""
from copy import deepcopy
import json
from pathlib import Path

import pytest
import numpy as np

from portfolio_risk.analytics.joint_market_tape_reference import verify
from portfolio_risk.analytics.model_contracts import evaluate_joint_market_credit


@pytest.fixture
def raw():
    root = Path(__file__).resolve().parents[3]
    return json.loads((root / 'examples/models/joint-market-credit-1.json').read_text())


@pytest.mark.parametrize('case', ['seeded', 'supplied', 'odd', 'zero_variance',
                                  'zero_covariance', 'rank_one', 'truncated_tie', 'truncated_distinct'])
def test_independent_tape_replays_complete_raw_source(raw, case, monkeypatch):
    market = raw['market']
    if case == 'supplied':
        market['draws'] = dict(kind='supplied_standard_normals', values=[-.5, .2, 1.] * 8)
    elif case == 'odd':
        raw['positions'] = raw['positions'][:3]
        market['paths'] = 3
        market['draws']['seed'] = [0xffffffff, 3, 0]
    elif case == 'zero_variance':
        market['annual_covariance'][0] = 0.
    elif case == 'zero_covariance':
        market['annual_covariance'] = [0.] * 9
    elif case == 'rank_one':
        market['annual_covariance'] = [.01] * 9
    elif case == 'truncated_tie':
        market['retained_factors'] = 2
    elif case == 'truncated_distinct':
        market['annual_covariance'] = [.01, .002, .003, .002, .01, .001, .003, .001, .01]
        market['retained_factors'] = 2
    result = evaluate_joint_market_credit(raw)
    from portfolio_risk.core import quant_native
    monkeypatch.setattr(quant_native, 'term_call', lambda *a, **k: pytest.fail('native verifier callback'))
    assert verify(market, result['market']) == dict(raw_covariance_verified=True,
        raw_draws_replayed=True, truncation_subspace_certified=True)


@pytest.mark.parametrize('change', ['covariance', 'seed', 'factor_count'])
def test_coherent_alternative_native_result_cannot_certify_original_source(raw, change):
    alternative = deepcopy(raw)
    if change == 'covariance':
        alternative['market']['annual_covariance'] = [2 * x for x in raw['market']['annual_covariance']]
    elif change == 'seed':
        alternative['market']['draws']['seed'] = [123]
    else:
        alternative['market']['retained_factors'] = 2
    result = evaluate_joint_market_credit(alternative)
    with pytest.raises(ValueError, match='joint market tape'):
        verify(raw['market'], result['market'])


@pytest.mark.parametrize('field', ['retained_annual_covariance', 'discarded_covariance_frobenius',
                                  'roundoff_negative_eigenvalue', 'correlation_eigenvalues_descending'])
def test_covariance_diagnostics_are_not_producer_authority(raw, field):
    result = evaluate_joint_market_credit(raw)['market']
    if type(result['diagnostics'][field]) is list:
        result['diagnostics'][field][0] += .1
    else:
        result['diagnostics'][field] += .1
    with pytest.raises(ValueError, match='joint market tape'):
        verify(raw['market'], result)


@pytest.mark.parametrize('change', ['seed', 'negative_root', 'later_increment'])
def test_rank_deficient_certificate_does_not_trust_output_witnesses(raw, change):
    raw['market']['annual_covariance'] = [.01] * 9
    alternative = deepcopy(raw)
    if change == 'seed':
        alternative['market']['draws']['seed'] = [123]
    result = evaluate_joint_market_credit(alternative)['market']
    if change == 'negative_root':
        result['increments'] = [-x for x in result['increments']]
    elif change == 'later_increment':
        result['increments'][-1] += .001
    with pytest.raises(ValueError, match='joint market tape'):
        verify(raw['market'], result)


def test_one_supplied_vector_rank_deficient_certificate(raw):
    raw['positions'] = raw['positions'][:1]
    raw['periods'] = raw['periods'][:1]
    raw['positions'][0]['random_pairs'] = raw['positions'][0]['random_pairs'][:1]
    raw['market'].update(paths=1, durations_years=[1.], annual_covariance=[.01] * 9,
                         draws=dict(kind='supplied_standard_normals', values=[.3, -.2, .8]))
    result = evaluate_joint_market_credit(raw)['market']
    assert verify(raw['market'], result)['raw_draws_replayed']


@pytest.mark.parametrize('rank', [1, 2, 3])
@pytest.mark.parametrize('seed', range(5))
@pytest.mark.parametrize('factors', [1, 2, 3])
def test_correlated_covariance_rank_and_factor_family(raw, rank, seed, factors):
    loading = np.random.default_rng(seed).normal(size=(3, rank)) * .03
    covariance = loading @ loading.T
    raw['market']['annual_covariance'] = covariance.ravel().tolist()
    raw['market']['retained_factors'] = factors
    for period in raw['periods']:
        for name in ('behavioral_beta', 'credit_beta', 'prepayment_beta'):
            period[name] = [0., 0., 0.]
    result = evaluate_joint_market_credit(raw)['market']
    assert verify(raw['market'], result)['raw_covariance_verified']


@pytest.mark.parametrize('rank', [1, 8, 16])
def test_maximum_driver_dimension_with_fewer_draws_than_drivers(raw, rank):
    n = 16
    raw['market']['drivers'].extend(dict(driver_id=f'hpi-{i}',
        kind=dict(kind='regional_log_hpi', region_id=f'region-{i}')) for i in range(3, n))
    loading = np.random.default_rng(51).normal(size=(n, rank)) * .01
    raw['market']['annual_covariance'] = (loading @ loading.T).ravel().tolist()
    raw['market']['retained_factors'] = n
    for period in raw['periods']:
        for name in ('behavioral_beta', 'credit_beta', 'prepayment_beta'):
            period[name] = [0.] * n
    result = evaluate_joint_market_credit(raw)['market']
    assert verify(raw['market'], result)['raw_covariance_verified']


def test_verification_honors_initial_and_mid_replay_cancellation(raw):
    result = evaluate_joint_market_credit(raw)['market']
    for stop_at in (1, 5, 9):
        calls = [0]
        def check():
            calls[0] += 1
            return calls[0] == stop_at
        with pytest.raises(InterruptedError, match='cancelled'):
            verify(raw['market'], result, check)
