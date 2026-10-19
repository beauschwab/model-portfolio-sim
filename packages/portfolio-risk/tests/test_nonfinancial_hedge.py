"""nonfinancial-hedge-1: native process output against the independent reference."""
import copy
import json
from pathlib import Path

import pytest

from portfolio_risk.analytics.model_contracts import account_nonfinancial_hedges
from portfolio_risk.analytics.nonfinancial_hedge_reference import replay, verify

ROOT = Path(__file__).resolve().parents[3]
MAR1, APR1 = 738946, 738977


def raw():
    return json.loads((ROOT / 'examples/models/nonfinancial-hedge-1.json').read_text())


def native(request, monkeypatch):
    from portfolio_risk.core import quant_native, native as native_core

    def forbidden(*a, **k):
        raise AssertionError('Python financial callback inside the native process')
    with monkeypatch.context() as m:
        m.setattr(quant_native, 'term_call', forbidden)
        m.setattr(native_core, 'discount_backend', forbidden)
        return account_nonfinancial_hedges(request, timeout=300.)


@pytest.mark.parametrize('variant', ['base', 'basis_risk', 'failed', 'missed', 'not_probable', 'dedesignate',
                                     'late_designation', 'contractual'])
def test_native_matches_independent_reference(monkeypatch, variant):
    request = raw()
    if variant in ('basis_risk', 'failed'):
        request['instruments'][0]['index_id'] = 'pe-europe'
    if variant == 'failed':
        request['policy'].update(assessment_lower_bound=.99, assessment_upper_bound=1.01)
    if variant in ('missed', 'not_probable'):
        request['events'] = [e for e in request['events'] if e['delivery_id'] != 'apr']
        request['events'].append(dict(event_id='apr-status', ordinal=MAR1, kind='forecast_status',
                                      review=dict(id='forecast-review', revision='r1'), exposure_id='pe-purchases',
                                      delivery_id='apr', status='probable_not_to_occur' if variant == 'missed' else 'not_probable'))
        if variant == 'not_probable':
            request['events'].append(dict(event_id='use-apr', ordinal=APR1, kind='consumption',
                                          review=dict(id='inventory-relief', revision='r1'), exposure_id='pe-purchases',
                                          delivery_id='apr', fraction=1.))
    if variant == 'dedesignate':
        request['events'].append(dict(event_id='stop-sales', ordinal=MAR1, kind='dedesignate',
                                      review=dict(id='stop', revision='r1'), relationship_id='cfh-sales'))
    if variant == 'late_designation':
        request['relationships'][0]['designation_ordinal'] = 738917
    if variant == 'contractual':
        del request['policy']['asu_2025_09_adoption']
        request['exposures'][0]['component'] = dict(kind='contractually_specified', index_id='pe-gulf',
                                                    contract=dict(id='purchase-formula', revision='r1'))
    receipt = verify(request, native(request, monkeypatch))
    assert receipt['verified'] and receipt['ordered_journal_recomputed']


@pytest.mark.parametrize('target', ['journal', 'period', 'delivery', 'leg', 'flag', 'order'])
def test_tampered_native_results_fail_independent_verification(monkeypatch, target):
    request = raw()
    result = copy.deepcopy(native(request, monkeypatch))
    if target == 'journal':
        result['journal'][4]['credit'] += 1e-6
    elif target == 'period':
        result['periods'][0]['hypothetical_change'] += 1e-6
    elif target == 'delivery':
        result['deliveries'][0]['oci_recognized'] += 1e-6
    elif target == 'leg':
        result['legs'][2]['value'] += 1e-6
    elif target == 'flag':
        result['accounting_standard_approval'] = True
    else:
        result['journal'][0], result['journal'][2] = result['journal'][2], result['journal'][0]
    with pytest.raises((ValueError, ArithmeticError)):
        verify(request, result)


def test_reference_refuses_component_without_adoption():
    request = raw()
    del request['policy']['asu_2025_09_adoption']
    with pytest.raises(ArithmeticError):
        replay(request)
