"""Standardized-schedule dynamic initial margin on the CSA owner against the independent reference.

The native collateral owner recomputes two-way BCBS-IOSCO schedule IM at each node and calls it
to the requirement; the derivative accounting owner books the movements. The verifier recomputes
both from raw inputs. Without the model the request and result keep their historical keys.
"""
import copy
import json
from pathlib import Path

import pytest

from portfolio_risk.analytics.derivative_verification import _native, verify_derivative_accounting
from portfolio_risk.analytics.model_contracts import account_derivative_contract, evaluate_derivative_contract

ROOT = Path(__file__).resolve().parents[3]
MODEL = dict(identity=dict(id='standardized-schedule', revision='r1'), minimum_transfer=0.,
             schedule=[dict(max_remaining_years=2., rate=.01), dict(max_remaining_years=5., rate=.02), dict(rate=.04)])


def netting(model=True):
    raw = json.loads((ROOT / 'examples/models/derivative-netting-1.json').read_text())['request']
    if model:
        raw['operation']['request']['policy']['initial_margin_model'] = copy.deepcopy(MODEL)
    return raw


def unwrap(result):
    return result.get('result', result)


def _rows(native, expected, keys):
    assert len(native) == len(expected)
    for a, b in zip(native, expected):
        for k in keys:
            assert a[k] == pytest.approx(b[k], rel=1e-12, abs=1e-9), (k, a, b)


def test_netting_paths_match_reference_and_conserve_margin():
    raw = netting()
    native = unwrap(evaluate_derivative_contract(raw))
    expected = _native(raw)
    for a, b in zip(native['paths'], expected['paths']):
        _rows(a['movements'], b['movements'], ('ordinal', 'entity_cash', 'vm_delta', 'received_im_delta', 'posted_im_delta', 'custodian_cash'))
        assert [m['kind'] for m in a['movements']] == [m['kind'] for m in b['movements']]
        _rows(a['exposures'], b['exposures'], ('received_im', 'posted_im', 'clean_value'))
        assert any(m['kind'] == 'initial_margin' for m in a['movements'])
        assert a['residual_received_im'] == pytest.approx(0., abs=1e-12)
        assert a['residual_posted_im'] == pytest.approx(0., abs=1e-12)
    plain = unwrap(evaluate_derivative_contract(netting(False)))
    assert native['clean_value'] == pytest.approx(plain['clean_value'], rel=1e-15)
    assert all(m['kind'] != 'initial_margin' for p in plain['paths'] for m in p['movements'])


def accounting(model=True):
    raw = json.loads((ROOT / 'examples/models/hedge-accounting-1.json').read_text())['derivative']
    if model:
        raw['derivative']['operation']['request']['policy']['initial_margin_model'] = copy.deepcopy(MODEL)
    return raw


def test_accounting_books_margin_once_and_verifies_independently():
    raw = accounting()
    result = account_derivative_contract(raw)
    verify_derivative_accounting(raw, result)
    events = [i for i in result['intents'] if i['event_id'].startswith('initial_margin')]
    assert events, 'initial margin movements reach the journal'
    accounts = {p['account'] for i in events for p in i['postings']}
    assert 'posted_im_asset' in accounts and 'cash' in accounts
    # Margin moves cash and the posted-margin asset only; it never reaches earnings.
    assert not any(a.startswith('pnl') or a.endswith('_income') for a in accounts)


@pytest.mark.parametrize('change', ['empty', 'unsorted', 'rate', 'transfer', 'unknown'])
def test_invalid_models_fail_in_both_owners(change):
    raw = netting()
    m = raw['operation']['request']['policy']['initial_margin_model']
    if change == 'empty':
        m['schedule'] = []
    elif change == 'unsorted':
        m['schedule'][0]['max_remaining_years'] = 6.
    elif change == 'rate':
        m['schedule'][2]['rate'] = 1.5
    elif change == 'transfer':
        m['minimum_transfer'] = -1.
    else:
        m['method'] = 'simm'
    with pytest.raises(Exception):
        evaluate_derivative_contract(raw)
    with pytest.raises(Exception):
        _native(raw)
