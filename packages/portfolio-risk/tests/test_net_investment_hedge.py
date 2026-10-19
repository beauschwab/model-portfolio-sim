"""net-investment-hedge-1: native process output against the independent reference.

The native model process is the only financial runtime. The reference recomputes translation,
the hedge CTA, the dual-hedge basis (from the independent hedge reference) and the ordered journal.
"""
import copy
import json
from pathlib import Path

import pytest

from portfolio_risk.analytics.model_contracts import account_net_investment_hedges
from portfolio_risk.analytics.net_investment_reference import replay, verify

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / 'packages/portfolio-risk-native/tests/fixtures'


def raw(name='base'):
    path = FIXTURES / ('net-investment-hedge.json' if name == 'base' else 'net-investment-dual.json')
    return json.loads(path.read_text())


def forbid_callbacks(monkeypatch):
    from portfolio_risk.core import quant_native, native
    from portfolio_risk.analytics import hedge_accounting_reference

    def forbidden(*a, **k):
        raise AssertionError('Python financial callback inside the native process')
    for module, attribute in [(quant_native, 'term_call'), (native, 'discount_backend'),
                              (hedge_accounting_reference, 'replay')]:
        monkeypatch.setattr(module, attribute, forbidden)


def native(request, monkeypatch):
    with monkeypatch.context() as m:
        forbid_callbacks(m)
        return account_net_investment_hedges(request, timeout=300.)


@pytest.mark.parametrize('name', ['base', 'dual'])
def test_native_matches_independent_reference(monkeypatch, name):
    request = raw(name)
    receipt = verify(request, native(request, monkeypatch))
    assert receipt['verified'] and receipt['ordered_journal_recomputed']
    assert receipt['dual_hedge_basis_recomputed'] is (name == 'dual')


@pytest.mark.parametrize('variant', ['mark_to_market', 'forward_method', 'failed_assessment', 'dedesignate',
                                     'full_disposal', 'late_designation', 'fvh_dedesignated'])
def test_policy_variants_match_independent_reference(monkeypatch, variant):
    request = raw('dual' if variant == 'fvh_dedesignated' else 'base')
    if variant == 'mark_to_market':
        request['relationships'][0]['excluded_component'] = 'mark_to_market'
    elif variant == 'forward_method':
        request['relationships'][0].update(method='forward', excluded_component=None)
    elif variant == 'failed_assessment':
        # Designating more than the instrument covers fails the dollar-offset band.
        request['relationships'][0]['hedged_net_investment'] = 600.
        request['policy']['assessment_lower_bound'] = .9
    elif variant == 'dedesignate':
        request['events'].append(dict(event_id='stop-fwd', ordinal=request['dates'][2]['ordinal'], kind='dedesignate',
                                      review=dict(id='stop', revision='r1'), relationship_id='nih-fwd'))
    elif variant == 'full_disposal':
        request['events'][0]['fraction'] = 1.
        request['events'][0]['proceeds'] = 1100.
    elif variant == 'late_designation':
        request['relationships'][1]['designation_ordinal'] = request['dates'][2]['ordinal']
    else:
        hedge = request['relationships'][0]['dual_fair_value_hedge']['request']
        hedge['events'] = [e for e in hedge['events'] if e['ordinal'] < 738931]
        hedge['events'].append(dict(event_id='dedesignate-fvh-1', relationship_id='fvh-1', ordinal=738931, sequence=0,
                                    kind='discontinue', review=dict(id='fvh-dedesignation', revision='r1'),
                                    reason='dedesignation'))
    result = native(request, monkeypatch)
    verify(request, result)
    if variant == 'failed_assessment':
        assert any(p['assessment_passed'] is False for p in result['periods'])
    if variant == 'full_disposal':
        cta = {b['account']: b['debit_balance'] for b in result['closing']}
        assert abs(cta.get('cta_translation', 0.)) < 1e-9 and abs(cta.get('cta_net_investment_hedge', 0.)) < 1e-9


@pytest.mark.parametrize('target', ['journal', 'period', 'translation', 'closing', 'flag', 'order'])
def test_tampered_native_results_fail_independent_verification(monkeypatch, target):
    request = raw()
    result = copy.deepcopy(native(request, monkeypatch))
    if target == 'journal':
        result['journal'][5]['debit'] += 1e-6
    elif target == 'period':
        result['periods'][1]['cta'] += 1e-6
    elif target == 'translation':
        result['translation'][2]['translation_adjustment'] += 1e-6
    elif target == 'closing':
        result['closing'][0]['debit_balance'] += 1e-6
    elif target == 'flag':
        result['accounting_standard_approval'] = True
    else:
        result['journal'][0], result['journal'][1] = result['journal'][1], result['journal'][0]
    with pytest.raises((ValueError, ArithmeticError)):
        verify(request, result)


def test_reference_refuses_dual_hedge_without_adoption():
    request = raw('dual')
    del request['policy']['dual_hedge_adoption']
    with pytest.raises(ArithmeticError):
        replay(request)
