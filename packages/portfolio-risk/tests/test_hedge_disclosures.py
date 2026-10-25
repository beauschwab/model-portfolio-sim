"""Opt-in ASC 815 disclosure tables on hedge-accounting-1.

The native owner tabulates them from its own replay at one reporting node; the independent
reference rebuilds them from its own replay and compares. Without a disclosure policy the
result keeps its historical keys.
"""
from copy import deepcopy
import datetime as dt
import json
import math
from pathlib import Path

import pytest

from portfolio_risk.analytics.hedge_accounting_reference import verify
from portfolio_risk.analytics.model_contracts import account_hedge_relationships

ROOT = Path(__file__).resolve().parents[3]
JAN1, FEB1, MAR1, APR1 = (dt.date(2024, m, 1).toordinal() for m in (1, 2, 3, 4))


def raw():
    return json.loads((ROOT / 'examples/models/hedge-accounting-1.json').read_text())


def event(kind, rid, ordinal, sequence, **fields):
    return dict(event_id=f'{kind}-{rid}-{ordinal}-{sequence}', relationship_id=rid, ordinal=ordinal, sequence=sequence, kind=kind,
                review=dict(id='review', revision='r1'), **fields)


def disclosed(request, at):
    request = deepcopy(request)
    request['disclosure_policy'] = dict(identity=dict(id='disclosure-policy', revision='r1'), reporting_ordinal=at)
    result = account_hedge_relationships(request)
    assert verify(request, result)['verified']
    return request, result


def near(a, b):
    assert abs(a - b) <= 1e-9 * (1. + max(abs(a), abs(b))), (a, b)


EARNINGS = {'mark_gain_loss', 'contractual_income', 'collateral_interest_income', 'counterparty_loss_expense',
            'posted_collateral_return_loss_expense', 'hedged_item_basis_gain_loss', 'hedged_item_basis_amortization',
            'hedged_item_basis_derecognition', 'hedged_forecast_reclassification'}


def test_without_a_policy_the_result_is_unchanged():
    result = account_hedge_relationships(raw())
    assert 'disclosures' not in result


@pytest.mark.parametrize('day', [FEB1 + 9, MAR1, MAR1 + 10, APR1])
def test_tables_reconcile_to_the_journal_and_daily_balances(day):
    request, result = disclosed(raw(), day)
    d = result['disclosures']
    assert (d['period_start_ordinal'], d['reporting_ordinal'], d['twelve_month_end_ordinal']) == (JAN1, day, day + 365)
    node = next(x for x in result['derivative']['daily'] if x['ordinal'] == day)
    values = {t['mapped_position_id']: t['clean_carrying'] for t in node['trades']}
    for row in d['derivative_fair_values']:
        near(row['fair_value'], values[row['position_id']])
        near(row['fair_value_hedge'] + row['cash_flow_hedge'] + row['not_designated'], row['fair_value'])
    # Every earnings posting up to the reporting node appears once, whatever its hedge type.
    totals = {}
    for i in result['intents']:
        if i['ordinal'] <= day:
            for p in i['postings']:
                if p['account'] in EARNINGS: totals[p['account']] = totals.get(p['account'], 0.) - p['debit_amount']
    for account, total in totals.items():
        near(math.fsum(x['amount'] for x in d['earnings'] if x['account'] == account), total)
    daily = {x['relationship_id']: x for x in next(x for x in result['daily'] if x['ordinal'] == day)['relationships']}
    for row in d['fair_value_hedged_items']:
        near(row['live_basis_adjustment'], daily[row['relationship_id']]['basis_adjustment_balance'])
        near(row['discontinued_basis_adjustment'], daily[row['relationship_id']]['frozen_basis_balance'])
    for row in d['cash_flow_hedge_aoci']:
        near(row['opening'] + row['recognized'] - row['reclassified'], row['closing'])
        x = daily[row['relationship_id']]
        near(row['closing'], x['oci_balance'] + x['frozen_oci_balance'])


@pytest.mark.parametrize('day', [FEB1 + 9, MAR1 + 10])
def test_twelve_month_expectation_matches_later_reclassification_when_fixings_equal_projections(day):
    # In this fixture the Physical fixings equal the Pricing projections, so the reporting-node
    # estimate equals what the replay later reclassifies; it uses no later information.
    _, result = disclosed(raw(), day)
    row = next(x for x in result['disclosures']['cash_flow_hedge_aoci'] if x['relationship_id'] == 'cfh-1')
    later = math.fsum(p['debit_amount'] for i in result['intents'] if day < i['ordinal'] <= day + 365
                      for p in i['postings'] if p['account'] == 'cash_flow_hedge_oci' and i['event_id'].startswith('cash_flow_hedge_reclassification'))
    near(row['expected_reclassification_within_twelve_months'], later)
    assert row['latest_hedged_payment_ordinal'] == APR1
    assert result['disclosures']['maximum_forecast_horizon_days'] == APR1 - day


def test_discontinued_basis_and_forecast_not_occurring_releases_are_disclosed():
    halved = raw(); halved['events'] = [event('assess', 'fvh-1', FEB1, 0), event('discontinue', 'fvh-1', FEB1, 1, reason='dedesignation', proportion=.5)]
    _, result = disclosed(halved, FEB1 + 14)
    item = result['disclosures']['fair_value_hedged_items'][0]
    assert item['discontinued_basis_adjustment'] != 0. and item['designated_hedged_notional'] == pytest.approx(50.)
    swap = next(x for x in result['disclosures']['derivative_fair_values'] if x['position_id'] == 'swap-fvh')
    near(swap['fair_value_hedge'], .5 * swap['fair_value'])
    pnto = raw(); pnto['events'] = [event('forecast_status', 'cfh-1', FEB1 + 9, 0, status='probable_not_to_occur')]
    _, result = disclosed(pnto, MAR1)
    row = next(x for x in result['disclosures']['cash_flow_hedge_aoci'] if x['relationship_id'] == 'cfh-1')
    record = next(x for x in result['lifecycle'] if x['kind'] == 'forecast_status')
    near(row['reclassified_forecast_not_occurring'], record['released_oci'])
    assert row['status'] == 'discontinued' and row['closing'] == 0. and row['expected_reclassification_within_twelve_months'] == 0.
    assert result['disclosures']['maximum_forecast_horizon_days'] is None
    nlp = raw(); nlp['events'] = [event('forecast_status', 'cfh-1', FEB1 + 9, 0, status='no_longer_probable')]
    _, result = disclosed(nlp, FEB1 + 10)
    row = next(x for x in result['disclosures']['cash_flow_hedge_aoci'] if x['relationship_id'] == 'cfh-1')
    # Frozen OCI is released on its two scheduled dates, both inside the window: all of it.
    near(row['expected_reclassification_within_twelve_months'], row['closing'])


@pytest.mark.parametrize('field', ['amount', 'expected', 'fair_value', 'basis', 'drop'])
def test_tampered_disclosures_fail_independent_verification(field):
    request, result = disclosed(raw(), MAR1 + 10)
    changed = deepcopy(result); d = changed['disclosures']
    if field == 'amount': d['earnings'][0]['amount'] += 1e-6
    elif field == 'expected': d['cash_flow_hedge_aoci'][0]['expected_reclassification_within_twelve_months'] += 1e-6
    elif field == 'fair_value': d['derivative_fair_values'][0]['not_designated'] += 1e-6
    elif field == 'basis': d['fair_value_hedged_items'][0]['live_basis_adjustment'] += 1e-6
    else: del changed['disclosures']
    with pytest.raises((ValueError, ArithmeticError)):
        verify(request, changed)


@pytest.mark.parametrize('case', ['off_node', 'unknown_field', 'missing_identity'])
def test_invalid_disclosure_policies_are_refused(case):
    request = raw(); request['disclosure_policy'] = dict(identity=dict(id='d', revision='r1'), reporting_ordinal=MAR1)
    if case == 'off_node': request['disclosure_policy']['reporting_ordinal'] = APR1 + 30
    elif case == 'unknown_field': request['disclosure_policy']['window_days'] = 365
    else: del request['disclosure_policy']['identity']
    with pytest.raises(ValueError):
        account_hedge_relationships(request)
    with pytest.raises((ValueError, ArithmeticError, KeyError)):
        verify(request, account_hedge_relationships(raw()))
