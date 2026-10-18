"""Own-credit DVA memo on the CSA owner: native result against the independent reference.

The memo is explicitly selected, Pricing only and additive. Without it the request
and result keep their historical keys. It never changes clean value, CVA or FVA.
"""
import copy
import json
import math
from pathlib import Path

import pytest

from portfolio_risk.analytics.derivative_verification import _native, _same
from portfolio_risk.analytics.model_contracts import evaluate_derivative_contract

ROOT = Path(__file__).resolve().parents[3]


def receiver_request():
    raw = json.loads((ROOT / 'examples/models/derivative-netting-1.json').read_text())['request']
    c = raw['operation']['request']
    c['policy'].update(post_threshold=1e9, receive_threshold=1e9)
    for path in c['paths']:
        for node in path['nodes']:
            for trade in node['trades']:
                trade['request']['side'] = 'receiver'
    second = copy.deepcopy(c['paths'][0])
    second['path_id'] = 'p2'
    for i, node in enumerate(second['nodes']):
        if i:
            node['discount_from_asof'] = 0.97
        for trade in node['trades']:
            trade['request']['scenario_id'] = 'p2'
    c['paths'][0]['probability'] = 0.6
    second['probability'] = 0.4
    c['paths'].append(second)
    return raw


def with_own_credit(raw, recovery=0.4):
    raw = copy.deepcopy(raw)
    c = raw['operation']['request']
    days = [n['ordinal'] for n in c['paths'][0]['nodes']]
    c['own_credit'] = dict(identity=dict(id='own-credit', revision='r1'), recovery=recovery,
                           survival=[dict(ordinal=d, survival_probability=s)
                                     for d, s in zip(days, [1.0, 0.995, 0.97, 0.965])])
    return raw


def unwrap(result):
    return result.get('result', result)


def test_absent_own_credit_keeps_historical_keys():
    native = unwrap(evaluate_derivative_contract(receiver_request()))
    for key in ('dva', 'own_credit_policy', 'clean_less_cva_fva_plus_dva'):
        assert key not in native
    assert all('discounted_own_default_benefit' not in p for p in native['paths'])
    reference = _native(receiver_request())
    assert 'dva' not in reference


@pytest.mark.parametrize('recovery', [0.0, 0.4, 1.0])
def test_native_dva_matches_independent_reference(recovery):
    raw = with_own_credit(receiver_request(), recovery)
    plain = unwrap(evaluate_derivative_contract(receiver_request()))
    native = unwrap(evaluate_derivative_contract(raw))
    reference = _native(raw)
    _same(native['dva'], reference['dva'], 'dva')
    _same(native['clean_less_cva_fva_plus_dva'], reference['clean_less_cva_fva_plus_dva'], 'adjusted value')
    for a, b in zip(native['paths'], reference['paths']):
        _same(a['discounted_own_default_benefit'], b['discounted_own_default_benefit'], 'path benefit')
    for key in ('clean_value', 'cva', 'fva', 'clean_less_cva_fva'):
        _same(native[key], plain[key], key)
    # Hand recomputation from the reported exposures and the supplied curve.
    c = raw['operation']['request']
    survival = {p['ordinal']: p['survival_probability'] for p in c['own_credit']['survival']}
    expected = 0.
    for path, result in zip(c['paths'], native['paths']):
        nodes = path['nodes']
        for k in range(1, len(nodes)):
            e = [x for x in result['exposures'] if x['ordinal'] == nodes[k]['ordinal']][0]
            payable = max(-e['total_closeout_claim'] - max(-e['vm'], 0.) - e['posted_im'], 0.)
            expected += (path['probability'] * (survival[nodes[k - 1]['ordinal']] - survival[nodes[k]['ordinal']])
                         * (1 - recovery) * nodes[k]['discount_from_asof'] * payable)
    assert math.isclose(native['dva'], expected, rel_tol=1e-12, abs_tol=1e-12)
    assert (native['dva'] > 0) == (recovery < 1)
    assert native['own_credit_policy'] == c['own_credit']['identity']


@pytest.mark.parametrize('tamper', ['physical', 'gap', 'increasing', 'start', 'recovery', 'unknown'])
def test_invalid_own_credit_fails_in_both_owners(tamper):
    raw = with_own_credit(receiver_request())
    c = raw['operation']['request']
    curve = c['own_credit']['survival']
    if tamper == 'physical':
        c['measure'] = 'physical'
    elif tamper == 'gap':
        del curve[2]
    elif tamper == 'increasing':
        curve[2]['survival_probability'] = 0.999
    elif tamper == 'start':
        curve[0]['survival_probability'] = 0.99
    elif tamper == 'recovery':
        c['own_credit']['recovery'] = -0.1
    else:
        c['own_credit']['hazard'] = 0.01
    with pytest.raises(Exception):
        evaluate_derivative_contract(raw)
    with pytest.raises(Exception):
        _native(raw)
