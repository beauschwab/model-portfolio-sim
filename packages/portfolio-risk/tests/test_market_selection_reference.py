"""Training-only selected scenario identity and independent distance controls."""
from copy import deepcopy
import json
from pathlib import Path

import numpy as np
import pytest

from portfolio_risk.analytics.market_selection_reference import verify
from portfolio_risk.analytics.model_contracts import fit_market_drivers


@pytest.fixture
def raw():
    return json.loads((Path(__file__).resolve().parents[3] /
        'examples/models/market-driver-calibration-1.json').read_text())


def replay(raw, result):
    train = np.asarray(raw['values'][:30])
    mean = train.mean(axis=0)
    delta = train - mean
    verify(raw, result['diagnostics'], mean, delta.T @ delta / 29)


@pytest.mark.parametrize('rank', range(5))
@pytest.mark.parametrize('seed', range(5))
@pytest.mark.parametrize('count', [None, 1, 8])
def test_native_selection_replays_independent_rank_family(raw, rank, seed, count, monkeypatch):
    rng = np.random.default_rng(seed)
    loading = rng.normal(size=(rank, 4)) * .001
    raw['values'] = (rng.normal(size=(40, rank)) @ loading + [0., 0., .2, 1.]).tolist()
    raw['scenario_count'] = count
    result = fit_market_drivers(raw)
    from portfolio_risk.core import quant_native
    monkeypatch.setattr(quant_native, 'term_call', lambda *a, **k: pytest.fail('native verifier callback'))
    replay(raw, result)


@pytest.mark.parametrize('change', ['empty', 'duplicate', 'count', 'order', 'interior', 'holdout'])
def test_substituted_stress_selection_rejected(raw, change):
    if change == 'order':
        raw['values'][29][0] = .4
    result = fit_market_drivers(raw)
    selected = result['diagnostics']['selected']
    result['diagnostics']['selected'] = {
        'empty': [], 'duplicate': [29, 29], 'count': [29], 'order': selected[::-1],
        'interior': [15, 14], 'holdout': [39, 38],
    }[change]
    with pytest.raises(ValueError, match='selected scenarios'):
        replay(raw, result)


def test_equal_observation_ties_retain_reverse_index_order(raw):
    raw['values'] = [[0., 0., 0., 0.]] * 40
    raw['scenario_count'] = 8
    result = fit_market_drivers(raw)
    assert result['diagnostics']['selected'] == list(range(29, 21, -1))
    replay(raw, result)
    result['diagnostics']['selected'][-1] = 0
    with pytest.raises(ValueError, match='tie order'):
        replay(raw, result)


def test_holdout_changes_never_select_or_change_training_rows(raw):
    before = fit_market_drivers(raw)
    altered = deepcopy(raw)
    altered['values'][30:] = [[.49, -.49, .9, 19.]] * 10
    after = fit_market_drivers(altered)
    assert before['diagnostics']['selected'] == after['diagnostics']['selected']
    replay(altered, after)


def test_selected_source_domain_and_cancellation(raw):
    raw['values'][29][0] = .9
    result = dict(diagnostics=dict(selected=[29, 0]))
    with pytest.raises(ValueError, match='selected scenarios'):
        replay(raw, result)
    for stop in (1, 2, 3, 4):
        calls = [0]
        def check():
            calls[0] += 1
            return calls[0] == stop
        with pytest.raises(InterruptedError):
            verify(raw, result['diagnostics'], [0.] * 4, np.eye(4), check)
