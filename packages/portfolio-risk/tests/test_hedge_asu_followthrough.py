"""Remaining ASU 2025-09 interest-rate items: native results, independent replay, tampers and refusals.

Fixtures: scripts/make_hedge_asu_followthrough_examples.py (tenor-changing election, forecasted
issuance, replacement debt, qualitative similarity); the expired written option reuses the
compound-option variant.
"""
from copy import deepcopy
import datetime as dt
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics.model_contracts import account_hedge_relationships
from portfolio_risk.analytics.hedge_accounting_reference import verify

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / 'packages/portfolio-risk-native/tests/fixtures'
FEB1, MAR1, MAR15, APR1 = (dt.date(2024, m, d).toordinal() for m, d in ((2, 1), (3, 1), (3, 15), (4, 1)))
NAMES = ('hedge-cyr-tenor', 'hedge-cyr-issuance', 'hedge-cyr-replacement', 'hedge-qualitative-similarity')


def load(name):
    return json.loads((FIXTURES / f'{name}.json').read_text())


def df(days):
    return 1. / (1. + .04 * days / 360.)


def par(periods):
    w = [(e - s) / 360. * df(e - FEB1) for s, e, _ in periods]
    return sum(r * x for (_, _, r), x in zip(periods, w)) / sum(w)


def near(a, b):
    assert abs(a - b) <= 1e-9 * (1. + max(abs(a), abs(b))), (a, b)


def reclass(result, rid):
    return [(i['ordinal'], i['postings'][0]['debit_amount']) for i in result['intents']
            if i['event_id'].startswith('cash_flow_hedge_reclassification') and i['relationship_id'] == rid]


def record(result, kind):
    return next(l for l in result['lifecycle'] if l['kind'] == kind)


@pytest.fixture(scope='module')
def results():
    return {name: (load(name), account_hedge_relationships(load(name))) for name in NAMES}


@pytest.mark.parametrize('name', NAMES)
def test_native_results_verify_independently(results, name):
    request, result = results[name]
    assert verify(request, result)['verified'] is True


def test_tenor_change_hand_values(results):
    _, r = results['hedge-cyr-tenor']
    k = par([(FEB1, MAR1, .04), (MAR1, APR1, .045)]); k_w = par([(FEB1, MAR1, .04), (MAR1, MAR15, .045), (MAR15, APR1, .045)])
    got = reclass(r, 'cfh-1')
    assert [x[0] for x in got] == [MAR1, MAR15, APR1]
    near(got[0][1], (.04 - k) * 100. * 29. / 360.); near(got[1][1], (.045 - k_w) * 100. * 14. / 360.); near(got[2][1], (.045 - k_w) * 100. * 17. / 360.)
    assert record(r, 'rate_election')['index_id'] == 'term-2w'


def test_issuance_and_replacement_hand_values(results):
    _, r = results['hedge-cyr-issuance']
    assert (record(r, 'best_estimate_change')['ordinal'], record(r, 'best_estimate_change')['index_id']) == (FEB1, 'term-1m-alt')
    issued = record(r, 'debt_issued'); assert (issued['ordinal'], issued['reason']) == (MAR1, 'documentation_updated')
    near(reclass(r, 'cfh-fwd')[0][1], (.046 - .06) * 100. * 31. / 360.)
    _, r = results['hedge-cyr-replacement']
    replaced = record(r, 'debt_replacement'); assert (replaced['index_id'], replaced['status_after']) == ('term-1m-alt', 'qualified')
    near(reclass(r, 'cfh-1')[-1][1], (.046 - par([(FEB1, MAR1, .04), (MAR1, APR1, .046)])) * 100. * 31. / 360.)


def test_qualitative_similarity_basis(results):
    _, r = results['hedge-qualitative-similarity']
    group = [a for a in r['assessments'] if a['relationship_id'] == 'cfh-1']
    assert 'basis' not in group[0]['similar_risk'] and group[0]['similar_risk']['similar'] is True
    assert group[1]['similar_risk']['basis'] == 'qualitative' and group[1]['similar_risk']['similar'] is True


@pytest.mark.parametrize('name,target', [('hedge-cyr-tenor', 'reclass'), ('hedge-cyr-tenor', 'index'), ('hedge-cyr-issuance', 'kind'),
                                         ('hedge-cyr-issuance', 'reason'), ('hedge-cyr-replacement', 'kind'), ('hedge-qualitative-similarity', 'basis'),
                                         ('hedge-qualitative-similarity', 'raw_support'), ('hedge-cyr-tenor', 'raw_schedule')])
def test_independent_replay_rejects_tampers(results, name, target):
    request, result = deepcopy(results[name])
    if target == 'reclass':
        i = next(i for i in result['intents'] if i['event_id'].startswith('cash_flow_hedge_reclassification') and i['ordinal'] == MAR15)
        i['postings'][0]['debit_amount'] += 1e-6; i['postings'][1]['debit_amount'] -= 1e-6
    elif target == 'index': record(result, 'rate_election')['index_id'] = 'term-1m'
    elif target == 'kind': next(l for l in result['lifecycle'] if l['kind'] in ('debt_issued', 'debt_replacement'))['kind'] = 'rate_election'
    elif target == 'reason': record(result, 'debt_issued')['reason'] = 'qualified'
    elif target == 'basis': [a for a in result['assessments'] if a['relationship_id'] == 'cfh-1'][1]['similar_risk'].pop('basis')
    elif target == 'raw_support': next(e for e in request['events'] if e.get('qualitative_similarity_support'))['qualitative_similarity_support'] = None
    else: request['forecast_exposures'][0]['choose_your_rate']['option_schedules'][0]['periods'][3]['payment_ordinal'] += 1
    with pytest.raises((ValueError, ArithmeticError)):
        verify(request, result)


@pytest.mark.parametrize('name,kind', [('hedge-cyr-issuance', 'never_issued'), ('hedge-cyr-issuance', 'undocumented'), ('hedge-cyr-issuance', 'replace_first'),
                                       ('hedge-cyr-replacement', 'fixed_rate'), ('hedge-qualitative-similarity', 'no_election'),
                                       ('hedge-qualitative-similarity', 'qualitative_first'), ('hedge-cyr-tenor', 'initial_schedule')])
def test_native_refusals(name, kind):
    v = load(name)
    if kind == 'never_issued': v['events'] = [e for e in v['events'] if e['kind'] != 'debt_issued']
    elif kind == 'undocumented': next(e for e in v['events'] if e['kind'] == 'debt_issued')['index_id'] = 'term-9m'
    elif kind == 'replace_first': next(e for e in v['events'] if e['kind'] == 'debt_issued')['kind'] = 'debt_replacement'
    elif kind == 'fixed_rate': v['events'][-1]['index_id'] = 'fixed'
    elif kind == 'no_election': v['relationships'][1]['group'].pop('qualitative_similarity_election')
    elif kind == 'qualitative_first':
        for e in v['events']:
            if e['relationship_id'] == 'cfh-1': e['qualitative_similarity_support'] = dict(id='s', revision='r1')
    else: v['forecast_exposures'][0]['choose_your_rate']['option_schedules'][0]['index_id'] = 'term-1m'
    with pytest.raises(ValueError):
        account_hedge_relationships(v)


def test_written_option_expired_before_designation_is_not_part_of_the_instrument():
    from test_hedge_compound_option import compound_variant, nodes
    from portfolio_risk.analytics.hedge_accounting_reference import replay
    v = compound_variant()
    v['relationships'][1]['inception_ordinal'] = MAR1
    for n in nodes(v):
        for t in n['trades']:
            if t['kind'] == 'expired_european': t['terminal']['expiry_ordinal'] = MAR1 - 1
    with pytest.raises(ValueError, match='expired before designation'):
        replay(v)
    with pytest.raises(ValueError):
        account_hedge_relationships(v)
