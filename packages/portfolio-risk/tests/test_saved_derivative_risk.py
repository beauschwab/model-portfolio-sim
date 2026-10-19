"""saved-derivative-risk-1: native graph paired curve risk against the independent reference.

The native graph is the only financial runtime; Python transports the raw request. The
reference revalues every bumped swap and hedged item with separate verifier code.
"""
import copy
import json
from pathlib import Path

import pytest

from portfolio_risk.analytics.saved_derivative_risk_reference import reference, verify
from portfolio_risk.core.graph_native import NativeSavedDerivativeRisk

ROOT = Path(__file__).resolve().parents[3]


def raw():
    return json.loads((ROOT / 'examples/models/saved-derivative-risk-1.json').read_text())


def forbid_callbacks(monkeypatch):
    from portfolio_risk.core import native
    from portfolio_risk.analytics import derivative_verification, hedge_accounting_reference

    def forbidden(*a, **k):
        raise AssertionError('Python financial callback inside the native graph')
    monkeypatch.setattr(native, 'discount_backend', forbidden)
    monkeypatch.setattr(derivative_verification, '_swap', forbidden)
    monkeypatch.setattr(hedge_accounting_reference, '_unit_clean', forbidden)


def additive():
    request = raw()
    request['projection_policy'] = 'additive_projected_observations'
    del request['projection_curves']
    return request


@pytest.mark.parametrize('policy', ['named', 'additive'])
def test_native_graph_matches_independent_reference(monkeypatch, policy):
    request = raw() if policy == 'named' else additive()
    with monkeypatch.context() as m:
        forbid_callbacks(m)
        graph = NativeSavedDerivativeRisk(request)
        warm = graph.price(request)
    expected = verify(request, graph.initial)
    verify(request, warm)
    assert expected['relationships'][0]['hedged_item'] is not None
    assert all(c['computed'] == 0 for c in warm['counts'].values())


@pytest.mark.parametrize('policy', ['named', 'additive'])
@pytest.mark.parametrize('change', ['curve', 'projection', 'bump', 'keys', 'notional'])
def test_edits_reprice_and_still_match_the_reference(change, policy):
    request = raw() if policy == 'named' else additive()
    graph = NativeSavedDerivativeRisk(request)
    edited = copy.deepcopy(request)
    swap = edited['positions'][0]['swap']
    if change == 'curve':
        swap['discount_curve']['points'][2]['discount_factor'] = 0.989
    elif change == 'projection' and policy == 'named':
        # Move the named curve and its supplied fixings together so they still reconcile.
        curve = edited['projection_curves'][0]['points']
        curve[3]['discount_factor'] *= 0.999
        tau = (curve[3]['ordinal'] - curve[2]['ordinal']) / 360.
        rate = (curve[2]['discount_factor'] / curve[3]['discount_factor'] - 1.) / tau
        for p in edited['positions']:
            for period in p['swap']['floating_periods']:
                for o in period['coupon']['observations']:
                    if o['interest_start_ordinal'] == curve[2]['ordinal']:
                        o['rate'] = rate
    elif change == 'projection':
        for p in swap['floating_periods']:
            for o in p['coupon']['observations']:
                if o['source']['kind'] == 'conditional_projection':
                    o['rate'] += 0.0025
    elif change == 'bump':
        edited['bumps']['bump_bp'] = 10.
    elif change == 'keys':
        edited['bumps']['key_rate_ordinals'] = edited['bumps']['key_rate_ordinals'][1:]
    else:
        edited['relationships'][0]['hedged_item']['hedged_notional'] = 50.
    verify(edited, graph.price(edited))


def test_only_projected_observations_are_bumped():
    request = raw()
    kinds = [o['source']['kind'] for p in request['positions'] for period in p['swap']['floating_periods']
             for o in period['coupon']['observations']]
    assert 'observed' in kinds and 'conditional_projection' in kinds
    result = NativeSavedDerivativeRisk(request).initial
    assert result['projected_observations_bumped'] == kinds.count('conditional_projection')
    assert result['observed_fixings_bumped'] is False


def test_tampered_native_results_fail_independent_verification():
    request = raw()
    result = NativeSavedDerivativeRisk(request).initial
    for path in (('positions', 0, 'dv01'), ('netting_sets', 0, 'clean_value'),
                 ('relationships', 0, 'hedged_item', 'key_rate_dv01', 1), ('total', 'dv01')):
        bad = copy.deepcopy(result)
        target = bad
        for k in path[:-1]:
            target = target[k]
        target[path[-1]] += 1e-6
        with pytest.raises((ValueError, ArithmeticError)):
            verify(request, bad)
    bad = copy.deepcopy(result)
    bad['observed_fixings_bumped'] = True
    with pytest.raises((ValueError, ArithmeticError)):
        verify(request, bad)


def test_reference_key_rates_sum_to_parallel_for_each_trade():
    expected = reference(raw())
    for p in expected['positions']:
        assert sum(p['key_rate_dv01']) == pytest.approx(p['dv01'], rel=1e-6)


def test_reference_refuses_unreconciled_named_curves():
    request = raw()
    request['projection_curves'][0]['points'][2]['discount_factor'] = 0.9895
    with pytest.raises((ValueError, ArithmeticError)):
        reference(request)
