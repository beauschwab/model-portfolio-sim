"""Two #35 critical regression rows not covered by a dedicated test elsewhere.

- Wrong forecast amount or date, a changed assessment method, and a failed assessment cannot
  keep a qualification: native admission refuses them, and the independent reference refuses
  an earlier qualifying result offered against the changed request.
- Transition: elections take effect only from an explicit actual adoption. A date alone, or
  elections without adoption, is refused; the engine carries no mandatory effective date.
"""
from copy import deepcopy
import json
from pathlib import Path

import pytest

from portfolio_risk.analytics.hedge_accounting_reference import verify
from portfolio_risk.analytics.model_contracts import account_hedge_relationships

ROOT = Path(__file__).resolve().parents[3]


def raw():
    return json.loads((ROOT / 'examples/models/hedge-accounting-1.json').read_text())


@pytest.fixture(scope='module')
def qualified():
    request = raw()
    result = account_hedge_relationships(request)
    assert verify(request, result)['verified']
    assert all(a['qualified'] for a in result['assessments'])
    return request, result


def forecast(request):
    return request['forecast_exposures'][0]


@pytest.mark.parametrize('change', ['amount', 'payment_date', 'period_end', 'method', 'event_method'])
def test_wrong_forecast_terms_or_changed_method_cannot_keep_a_qualification(change, qualified):
    request, result = deepcopy(qualified[0]), qualified[1]
    if change == 'amount': forecast(request)['notional'] = 90.
    elif change == 'payment_date': forecast(request)['periods'][0]['payment_ordinal'] += 1
    elif change == 'period_end':
        period = forecast(request)['periods'][0]; period['end_ordinal'] += 1; period['payment_ordinal'] += 1
    elif change == 'method': request['policy']['assessment']['method'] = 'regression'
    else: request['events'][2]['method'] = 'regression'
    with pytest.raises(ValueError):
        account_hedge_relationships(request)
    # The earlier qualifying result does not carry over to the changed request.
    with pytest.raises((ValueError, ArithmeticError)):
        verify(request, result)


def test_a_failed_assessment_discontinues_and_no_later_assessment_can_requalify():
    request = raw(); request['policy']['assessment'].update(lower_bound=.99, upper_bound=1.01)
    request['events'] = [e for e in request['events'] if e['relationship_id'] == 'fvh-1'][:1] +         [e for e in request['events'] if e['relationship_id'] == 'cfh-1']
    result = account_hedge_relationships(request)
    assert verify(request, result)['verified']
    failed = next(a for a in result['assessments'] if a['relationship_id'] == 'fvh-1')
    assert failed['qualified'] is False
    status = {r['relationship_id']: r['status'] for r in result['relationships']}
    assert status['fvh-1'] == 'discontinued'
    # A later assessment of the discontinued relationship is refused rather than requalifying it.
    later = deepcopy(request); later['events'].insert(1, dict(raw()['events'][1]))
    with pytest.raises(ValueError, match='not qualified'):
        account_hedge_relationships(later)


def elected():
    request = raw()
    asu = request['policy']['adoption']['asu_2025_09']
    asu.update(adopted=True, adoption_ordinal=min(r['inception_ordinal'] for r in request['relationships']))
    asu['elections']['similar_risk_grouping'] = True
    return request


@pytest.mark.parametrize('case', ['date_without_adoption', 'adoption_without_date'])
def test_an_adoption_needs_both_the_decision_and_its_actual_date(case):
    request = elected(); asu = request['policy']['adoption']['asu_2025_09']
    if case == 'date_without_adoption': asu['adopted'] = False
    else: asu['adoption_ordinal'] = None
    with pytest.raises(ValueError):
        account_hedge_relationships(request)
    with pytest.raises((ValueError, ArithmeticError)):
        verify(request, account_hedge_relationships(raw()))


def test_elections_without_an_adoption_are_inert():
    request = elected(); request['policy']['adoption']['asu_2025_09'].update(adopted=False, adoption_ordinal=None)
    result = account_hedge_relationships(request); base = account_hedge_relationships(raw())
    assert verify(request, result)['verified']
    assert [i['postings'] for i in result['intents']] == [i['postings'] for i in base['intents']]
    # Using an election without an actual adoption on or before designation is refused:
    # test_hedge_asu_2025_09.py::test_election_refusals_without_fallback[group_election, before_adoption].


@pytest.mark.parametrize('owner', ['request', 'relationship', 'item', 'item_period', 'exposure', 'event', 'review'])
def test_unknown_request_fields_reject_natively_and_independently(owner, qualified):
    request, result = deepcopy(qualified[0]), qualified[1]
    target = {'request': request, 'relationship': request['relationships'][0], 'item': request['hedged_items'][0],
              'item_period': request['hedged_items'][0]['periods'][0], 'exposure': forecast(request),
              'event': request['events'][0], 'review': request['events'][0]['review']}[owner]
    target['unreviewed_note'] = 'x'
    with pytest.raises(ValueError):
        account_hedge_relationships(request)
    with pytest.raises((ValueError, ArithmeticError)):
        verify(request, result)
