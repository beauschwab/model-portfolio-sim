"""ASU 2025-09 elections in hedge-accounting-1: native ownership, independent replay and refusals.

Fixture: the shared hedge example (daily nodes 1 Jan-1 Apr 2024, 4% simple curve from 1 February,
`swap-cfh` pays the 1 February par rate `k` on term-1m with a 4.5% March fixing). The alternative
index `term-1m-alt` is the same exposure with a 4.6% March fixing and its own par rate.
"""
from copy import deepcopy
import datetime as dt
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics.model_contracts import account_hedge_relationships
from portfolio_risk.analytics.hedge_accounting_reference import verify

ROOT = Path(__file__).resolve().parents[3]
JAN1, FEB1, MAR1, APR1 = (dt.date(2024, m, 1).toordinal() for m in (1, 2, 3, 4))
FEB15 = FEB1 + 14
TAU2, TAU3 = 29. / 360., 31. / 360.


def raw():
    return json.loads((ROOT / 'examples/models/hedge-accounting-1.json').read_text())


def run(request):
    return account_hedge_relationships(request)


def df(rate, days):
    return 1. / (1. + rate * days / 360.)


def par(march):
    w2, w3 = TAU2 * df(.04, 29), TAU3 * df(.04, 60)
    return (.04 * w2 + march * w3) / (w2 + w3)


def value(t, notional, k, march):
    v = 0.
    if t < MAR1: v += (.04 - k) * notional * TAU2 * df(.04, MAR1 - t)
    if t < APR1: v += (march - k) * notional * TAU3 * df(.04, APR1 - t)
    return v


def settlement(period, notional, k, march):
    return (.04 - k) * notional * TAU2 if period == 2 else (march - k) * notional * TAU3


def cfh_fixed(request):
    return request['derivative']['derivative']['operation']['request']['paths'][0]['nodes'][0]['trades'][1]['request']['fixed_rate']


def near(a, b):
    assert abs(a - b) <= 1e-9 * (1. + max(abs(a), abs(b))), (a, b)


def hypothetical(request, hid, index, notional, fixed, march):
    nodes = []
    for n in request['derivative']['derivative']['operation']['request']['paths'][0]['nodes']:
        if n['ordinal'] < FEB1: continue
        req = deepcopy(n['trades'][1]['request']); req.update(position_id=hid, notional=notional, fixed_rate=fixed)
        for p in req['floating_periods']:
            p['coupon']['index']['index_id'] = index
            if p['period_id'] == 'l3': p['coupon']['observations'][0]['rate'] = march
        nodes.append(dict(ordinal=n['ordinal'], request=req))
    return dict(evidence=dict(id=hid + '-evidence', revision='r1'), nodes=nodes)


def adopt(request, election):
    request['policy']['adoption']['asu_2025_09'] = dict(adopted=True, adoption_ordinal=JAN1, elections=dict(
        similar_risk_grouping=election == 'similar_risk_grouping', selectable_rate_debt=election == 'selectable_rate_debt',
        swap_written_option_exception=False, dual_hedge_basis=False))


def group_variant(method='instrument_highly_effective_each_risk'):
    v = raw(); adopt(v, 'similar_risk_grouping'); k = cfh_fixed(v)
    v['forecast_exposures'][0]['notional'] = 60.
    second = deepcopy(v['forecast_exposures'][0]); second.update(exposure_id='float-2', notional=40., index_id='term-1m-alt')
    v['forecast_exposures'].append(second)
    rel = v['relationships'][1]
    rel.update(forecast_exposure_id=None, matched_critical_terms_evidence=None, group=dict(
        member_exposure_ids=['float-1', 'float-2'], similar_risk_method=method, hypotheticals=[
            dict(exposure_id='float-1', hypothetical=hypothetical(v, 'hyp-1', 'term-1m', 60., k, .045)),
            dict(exposure_id='float-2', hypothetical=hypothetical(v, 'hyp-2', 'term-1m-alt', 40., par(.046), .046))]))
    return v


def rate_variant(base_march=.045):
    v = raw(); adopt(v, 'selectable_rate_debt'); k = cfh_fixed(v) if base_march == .045 else par(base_march)
    v['forecast_exposures'][0]['choose_your_rate'] = dict(evidence=dict(id='cyr-terms', revision='r1'),
                                                          documented_index_ids=['term-1m', 'term-1m-alt'], debt_classification='liability')
    rel = v['relationships'][1]
    rel.update(matched_critical_terms_evidence=None, hypothetical=hypothetical(v, 'hyp-base', 'term-1m', 100., k, base_march),
               rate_hypotheticals=[dict(index_id='term-1m-alt', hypothetical=hypothetical(v, 'hyp-alt', 'term-1m-alt', 100., par(.046), .046))])
    v['events'].pop()
    v['events'].append(dict(event_id='elect', relationship_id='cfh-1', ordinal=MAR1, sequence=1, kind='rate_election',
                            review=dict(id='election-review', revision='r1'), index_id='term-1m-alt'))
    return v


def record(result, kind):
    return next(l for l in result['lifecycle'] if l['kind'] == kind and l['relationship_id'] == 'cfh-1')


def intents(result, kind):
    return [i for i in result['intents'] if i['event_id'].startswith(kind)]


def test_similar_risk_group_verifies_independently_with_hand_values():
    v = group_variant(); k = cfh_fixed(v); kp = par(.046)
    result = run(v); receipt = verify(v, result)
    assert receipt['verified'] and receipt['asu_2025_09_similar_risk_supported'] is True
    a = result['assessments'][2]; sr = a['similar_risk']
    d = value(MAR1, 100., k, .045); e1 = value(MAR1, 60., k, .045); e2 = value(MAR1, 40., kp, .046)
    near(a['cumulative_instrument_change'], d); near(a['cumulative_hedged_change'], e1 + e2); near(a['ratio'], d / (e1 + e2))
    assert a['qualified'] and sr['similar'] and sr['method'] == 'instrument_highly_effective_each_risk'
    near(sr['members'][0]['ratio'], 1.); near(sr['members'][1]['ratio'], .4 * d / e2); assert .8 <= sr['members'][1]['ratio'] <= .9
    b = run(group_variant('hedged_risks_similar_to_each_other')); sb = b['assessments'][2]['similar_risk']
    near(sb['members'][1]['ratio'], (e2 / 40.) / (e1 / 60.)); assert sb['similar']
    reclass = [(i['ordinal'], i['contract_position_id'], i['postings'][0]['debit_amount']) for i in intents(result, 'cash_flow_hedge_reclassification')]
    expected = [(MAR1, 'float-1', settlement(2, 60., k, .045)), (MAR1, 'float-2', settlement(2, 40., kp, .046)),
                (APR1, 'float-1', settlement(3, 60., k, .045)), (APR1, 'float-2', settlement(3, 40., kp, .046))]
    assert [(o, x) for o, x, _ in reclass] == [(o, x) for o, x, _ in expected]
    for (_, _, amt), (_, _, want) in zip(reclass, expected): near(amt, want)
    closing = result['closing'][1]
    near(closing['oci_recognized'], settlement(2, 100., k, .045) + settlement(3, 100., k, .045))
    near(closing['oci_reclassified'], sum(x[2] for x in expected)); near(closing['oci_closing'], closing['oci_recognized'] - closing['oci_reclassified'])
    assert result['relationships'][1]['member_exposure_ids'] == ['float-1', 'float-2'] and 'members' not in result['daily'][0]['relationships'][0]
    mar1 = next(x for x in result['daily'] if x['ordinal'] == MAR1)['relationships'][1]
    assert mar1['members'][1]['index_id'] == 'term-1m-alt'; near(mar1['members'][1]['hypothetical_value'], e2)


def test_dissimilar_group_dedesignates_as_of_the_last_similar_date():
    v = group_variant(); k = cfh_fixed(v); kp = par(.046)
    v['policy']['assessment'].update(lower_bound=.95, upper_bound=1.05)
    v['events'] = [dict(event_id='assess', relationship_id='cfh-1', ordinal=MAR1, sequence=0, kind='assess', review=dict(id='review', revision='r1'))]
    result = run(v); assert verify(v, result)['verified']
    rec = record(result, 'assessment'); sr = result['assessments'][0]['similar_risk']
    assert not sr['similar'] and sr['members'][0]['similar'] and not sr['members'][1]['similar']
    assert (rec['effective_ordinal'], rec['reason'], rec['status_after']) == (FEB1, 'similar_risk_failed', 'discontinued')
    s2 = settlement(2, 100., k, .045); g = s2 + value(MAR1, 100., k, .045)
    near(rec['earnings_catch_up'], g); near(rec['reversed_reclassification'], settlement(2, 60., k, .045) + settlement(2, 40., kp, .046))
    near(rec['frozen_oci'], 0.); near(rec['released_oci'], 0.)
    catch = intents(result, 'similar_risk_earnings_catch_up'); assert len(catch) == 1 and catch[0]['postings'][1]['account'] == 'mark_gain_loss'
    near(catch[0]['postings'][1]['debit_amount'], -g)
    assert len(intents(result, 'similar_risk_reclassification_reversal')) == 1
    closing = result['closing'][1]; near(closing['oci_recognized'], 0.); near(closing['oci_reclassified'], 0.); near(closing['oci_closing'], 0.)
    v['events'][0]['effective_ordinal'] = FEB15
    result = run(v); assert verify(v, result)['verified']
    rec = record(result, 'assessment'); through = value(FEB15, 100., k, .045)
    assert rec['effective_ordinal'] == FEB15
    near(rec['earnings_catch_up'], g - through); near(rec['frozen_oci'], through); near(rec['released_oci'], through * 29. / 60.)
    releases = intents(result, 'frozen_oci_release')
    assert [i['ordinal'] for i in releases] == [MAR1, MAR1, APR1, APR1]
    near(releases[0]['postings'][0]['debit_amount'], through * .29); near(sum(i['postings'][0]['debit_amount'] for i in releases), through)
    near(result['closing'][1]['oci_closing'], 0.)


def test_rate_election_switches_the_hypothetical_and_verifies_independently():
    v = rate_variant(); k = cfh_fixed(v); kp = par(.046)
    result = run(v); receipt = verify(v, result)
    assert receipt['verified'] and receipt['asu_2025_09_selectable_rate_supported'] is True
    a = result['assessments'][2]; assert a['index_id'] == 'term-1m' and a['qualified'] and 'similar_risk' not in a; near(a['ratio'], 1.)
    rec = record(result, 'rate_election')
    assert (rec['index_id'], rec['reason'], rec['status_after']) == ('term-1m-alt', 'qualified', 'qualified') and 'earnings_catch_up' not in rec
    h_alt = value(MAR1, 100., kp, .046)
    mar1 = next(x for x in result['daily'] if x['ordinal'] == MAR1)['relationships'][1]
    near(mar1['hypothetical_value'], h_alt); near(mar1['cumulative_hedged_change'], h_alt); assert mar1['members'][0]['index_id'] == 'term-1m-alt'
    s2, s3, s3_alt = settlement(2, 100., k, .045), settlement(3, 100., k, .045), settlement(3, 100., kp, .046)
    reclass = intents(result, 'cash_flow_hedge_reclassification')
    near(reclass[0]['postings'][0]['debit_amount'], s2); near(reclass[1]['postings'][0]['debit_amount'], s3_alt)
    closing = result['closing'][1]
    near(closing['oci_recognized'], s2 + s3); near(closing['oci_reclassified'], s2 + s3_alt); near(closing['oci_closing'], s3 - s3_alt)
    assert result['relationships'][1]['selected_index_id'] == 'term-1m-alt'
    v = rate_variant(.05); k50 = par(.05)
    result = run(v); assert verify(v, result)['verified']
    a = result['assessments'][2]; assert not a['qualified'] and a['ratio'] < .8
    rec = record(result, 'rate_election')
    assert (rec['reason'], rec['effective_ordinal'], rec['status_after']) == ('ineffective_period_in_earnings', FEB1, 'qualified')
    g = s2 + value(MAR1, 100., k, .045); near(rec['earnings_catch_up'], g)
    assert len(intents(result, 'rate_election_earnings_catch_up')) == 1
    closing = result['closing'][1]
    near(closing['oci_recognized'], s2 + s3 - g); near(closing['oci_reclassified'], settlement(2, 100., k50, .05) + s3_alt)


@pytest.mark.parametrize('target', ['member_ratio', 'similar', 'catch_up', 'index', 'members', 'selected', 'flag'])
def test_independent_replay_rejects_tampered_election_results(target):
    if target in ('member_ratio', 'similar', 'members'):
        v = group_variant(); result = run(v); changed = deepcopy(result)
        if target == 'member_ratio': changed['assessments'][2]['similar_risk']['members'][1]['ratio'] += 1e-6
        elif target == 'similar': changed['assessments'][2]['similar_risk']['similar'] = False
        else: changed['daily'][-1]['relationships'][1]['members'][1]['hypothetical_value'] += 1e-6
    elif target == 'catch_up':
        v = group_variant(); v['policy']['assessment'].update(lower_bound=.95, upper_bound=1.05)
        v['events'] = [dict(event_id='assess', relationship_id='cfh-1', ordinal=MAR1, sequence=0, kind='assess', review=dict(id='review', revision='r1'))]
        result = run(v); changed = deepcopy(result); record(changed, 'assessment')['earnings_catch_up'] += 1e-6
    else:
        v = rate_variant(); result = run(v); changed = deepcopy(result)
        if target == 'index': changed['assessments'][2]['index_id'] = 'term-1m-alt'
        elif target == 'selected': changed['relationships'][1]['selected_index_id'] = 'term-1m'
        else: changed['asu_2025_09_selectable_rate_supported'] = False
    assert verify(v, result)['verified']
    with pytest.raises(ValueError): verify(v, changed)


@pytest.mark.parametrize('kind', ['group_election', 'before_adoption', 'member_order', 'cyr_asset', 'undocumented', 'unchanged', 'ordinary',
                                  'effective_single', 'missing_alternative', 'off_market', 'accrual_date', 'dual'])
def test_election_refusals_without_fallback(kind):
    if kind == 'group_election': v = group_variant(); v['policy']['adoption']['asu_2025_09']['elections']['similar_risk_grouping'] = False
    elif kind == 'before_adoption': v = group_variant(); v['policy']['adoption']['asu_2025_09']['adoption_ordinal'] = MAR1
    elif kind == 'member_order': v = group_variant(); v['relationships'][1]['group']['member_exposure_ids'] = ['float-2', 'float-1']
    elif kind == 'cyr_asset': v = rate_variant(); v['forecast_exposures'][0]['direction'] = 'receipt'
    elif kind == 'undocumented': v = rate_variant(); v['events'][2]['index_id'] = 'prime'
    elif kind == 'unchanged': v = rate_variant(); v['events'][2]['index_id'] = 'term-1m'
    elif kind == 'ordinary':
        v = raw(); adopt(v, 'selectable_rate_debt'); v['events'][2].update(kind='rate_election', index_id='term-1m-alt')
    elif kind == 'effective_single':
        v = rate_variant(); v['events'][2] = dict(event_id='a', relationship_id='cfh-1', ordinal=MAR1, sequence=1, kind='assess',
                                                  review=dict(id='r', revision='r1'), effective_ordinal=FEB15)
    elif kind == 'missing_alternative': v = rate_variant(); v['relationships'][1]['rate_hypotheticals'] = []
    elif kind == 'off_market':
        v = rate_variant()
        for n in v['relationships'][1]['rate_hypotheticals'][0]['hypothetical']['nodes']: n['request']['fixed_rate'] = .05
    elif kind == 'accrual_date': v = rate_variant(); v['events'][2]['ordinal'] = FEB15
    else: v = raw(); v['policy']['adoption']['asu_2025_09']['elections']['dual_hedge_basis'] = True
    with pytest.raises(ValueError): run(v)


def test_elections_without_use_keep_the_base_result():
    v = raw(); adopt(v, 'similar_risk_grouping'); elected = run(v); base = run(raw())
    assert verify(v, elected)['verified'] and len(elected['intents']) == len(base['intents'])
    near(elected['closing'][1]['oci_closing'], base['closing'][1]['oci_closing'])
    assert all('similar_risk' not in a and 'index_id' not in a for a in elected['assessments'])
