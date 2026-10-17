"""Seasoned hedge relationships (designated before the derivative opening date) in hedge-accounting-1:
reviewed retained opening balances, native ownership, independent replay, tamper and refusal controls.

Fixture: the shared hedge example (daily nodes 1 Jan-1 Apr 2024) with one extra period 1 Dec 2023-1 Jan
2024 paid on 2 January on both swaps, the loan and the forecast exposure, so both relationships can be
designated on 1 December with retained cumulative changes and balances at the 1 January opening.
"""
from copy import deepcopy
import datetime as dt
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics.model_contracts import account_hedge_relationships
from portfolio_risk.analytics.hedge_accounting_reference import verify

ROOT = Path(__file__).resolve().parents[3]
DEC1 = dt.date(2023, 12, 1).toordinal()
JAN1, FEB1, MAR1, APR1 = (dt.date(2024, m, 1).toordinal() for m in (1, 2, 3, 4))
JAN2 = JAN1 + 1
PERIODS = [(JAN1, FEB1), (FEB1, MAR1), (MAR1, APR1)]
TAU0 = 31. / 360.


def raw():
    return json.loads((ROOT / 'examples/models/hedge-accounting-1.json').read_text())


def run(request):
    return account_hedge_relationships(request)


def df(rate, days):
    return 1. / (1. + rate * days / 360.)


def utc(ordinal, hour=9):
    return (ordinal - 719163) * 86400 + hour * 3600


def ident(i):
    return dict(id=i, revision='r1')


def curve(t):
    return .06 if t < FEB1 else .04


def floating(period, t):
    return (.065, .06 if t < FEB1 else .04, .06 if t < FEB1 else .045)[period]


def item_unit(t):
    """Benchmark-component clean value per unit after payments (6% coupon, principal 1 April)."""
    r = curve(t); dirty = accrued = 0.
    for s, e in PERIODS:
        if e > t: dirty += .06 * (e - s) / 360. * df(r, e - t)
        if s <= t < e: accrued += .06 * (t - s) / 360.
    if APR1 > t: dirty += df(r, APR1 - t)
    return dirty - accrued


def item_unit_opening():
    """1 January after-payment unit value including the fully accrued December period paid on 2 January."""
    return item_unit(JAN1) + .06 * TAU0 * (df(.06, 1) - 1.)


def swap_value(t, fixed):
    r = curve(t)
    return sum((floating(i, t) - fixed) * 100. * (e - s) / 360. * df(r, e - t) for i, (s, e) in enumerate(PERIODS) if e > t)


def cfh_fixed(request):
    return request['derivative']['derivative']['operation']['request']['paths'][0]['nodes'][0]['trades'][1]['request']['fixed_rate']


def nodes(request):
    return request['derivative']['derivative']['operation']['request']['paths'][0]['nodes']


def near(a, b):
    assert abs(a - b) <= 1e-9 * (1. + max(abs(a), abs(b))), (a, b)


def december_observation(template):
    o = deepcopy(template)
    o.update(interest_start_ordinal=DEC1, interest_end_ordinal=JAN1, observation_start_ordinal=DEC1, observation_end_ordinal=JAN1,
             fixing_ordinal=DEC1, selection_cutoff_unix_seconds=utc(DEC1), rate=.06,
             source=dict(kind='observed', fixing=ident('observed-reset-0'), publication_unix_seconds=utc(DEC1, 8)))
    return o


def period0(prefix):
    return dict(period_id=prefix + '0', start_ordinal=DEC1, end_ordinal=JAN1, payment_ordinal=JAN2, day_count='act360')


def seasoned_variant(cum=(.5, .5, .3, .3), oci=.2):
    v = raw(); k = cfh_fixed(v)
    for n in nodes(v):
        for t in n['trades']:
            s = t['request']
            s['fixed_periods'].insert(0, period0('f'))
            l0 = deepcopy(s['floating_periods'][0]); l0.update(period_id='l0', start_ordinal=DEC1, end_ordinal=JAN1, payment_ordinal=JAN2)
            l0['coupon']['reset_cutoff_unix_seconds'] = utc(DEC1); l0['coupon']['observations'] = [december_observation(l0['coupon']['observations'][0])]
            s['floating_periods'].insert(0, l0)
            if n['ordinal'] == JAN1: s['discount_curve']['points'].insert(1, dict(ordinal=JAN2, discount_factor=df(.06, 1)))
    template = nodes(v)[0]['trades'][0]['request']['floating_periods'][0]['coupon']['observations'][0]
    for position in ('swap-fvh', 'swap-cfh'):
        v['derivative']['physical_index_paths'].append(dict(position_id=position, period_id='l0', path=ident('physical-path'), observations=[december_observation(template)]))
    # The derivative owner's after-payment clean value carries the matured, still unpaid December coupon in full.
    c0 = (.06 - k) * 100. * TAU0 * df(.06, 1)
    v['derivative']['opening']['trades'][1]['clean_value'] += c0; v['derivative']['opening']['equity_credit_balance'] += c0
    v['hedged_items'][0]['periods'].insert(0, period0('p')); v['forecast_exposures'][0]['periods'].insert(0, period0('e'))
    v['relationships'][0].update(inception_ordinal=DEC1, opening=dict(evidence=ident('fvh-1-opening'), hedged_value=item_unit_opening(),
                                                                       cumulative_instrument_change=cum[0], cumulative_hedged_change=cum[1], basis_balance=-cum[1], oci_balance=0.))
    hyp = [dict(ordinal=n['ordinal'], request=dict(deepcopy(n['trades'][1]['request']), position_id='hypothetical')) for n in nodes(v)]
    v['relationships'][1].update(inception_ordinal=DEC1, matched_critical_terms_evidence=None, hypothetical=dict(evidence=ident('hyp-evidence'), nodes=hyp),
                                 opening=dict(evidence=ident('cfh-1-opening'), hedged_value=v['derivative']['opening']['trades'][1]['clean_value'],
                                              cumulative_instrument_change=cum[2], cumulative_hedged_change=cum[3], basis_balance=0., oci_balance=oci))
    return v


@pytest.fixture(scope='module')
def accepted():
    v = seasoned_variant()
    return v, run(v)


def test_seasoned_relationships_verify_independently_with_hand_values(accepted):
    v, result = accepted
    receipt = verify(v, result); assert receipt['verified'] and receipt['seasoned_openings_supported'] is True
    assert [l['kind'] for l in result['lifecycle']][:2] == ['opening', 'opening'] and all(l['ordinal'] == JAN1 and l['status_after'] == 'qualified' for l in result['lifecycle'][:2])
    assert [i['event_id'].split(':')[0] for i in result['intents'][:2]] == ['hedge_opening', 'hedge_opening']
    opening = result['intents'][0]; near(opening['postings'][0]['debit_amount'], -.5); assert opening['postings'][1]['account'] == 'opening_equity'
    jan1 = item_unit_opening()
    for a, t in zip(result['assessments'][:2], (FEB1, MAR1)):
        d = .5 + swap_value(t, .06) - swap_value(JAN1, .06); e = .5 - 100. * (item_unit(t) - jan1)
        near(a['cumulative_instrument_change'], d); near(a['cumulative_hedged_change'], e); near(a['ratio'], d / e); assert a['qualified']
    cfh = result['assessments'][2]; near(cfh['ratio'], 1.); assert cfh['qualified']
    near(result['closing'][0]['basis_closing'], -.5 + 100. * (1. - jan1)); near(result['closing'][0]['basis_adjustments'], result['closing'][0]['basis_closing'])
    v_open = v['derivative']['opening']['trades'][1]['clean_value']
    near(result['closing'][1]['oci_closing'], .2 - v_open)
    marks = [i for i in result['intents'] if i['event_id'].startswith('clean_mark') and i['mapped_position_id'] == 'swap-cfh']
    assert marks and all(p['account'] != 'mark_gain_loss' for i in marks for p in i['postings'])
    reclass = [i['ordinal'] for i in result['intents'] if i['event_id'].startswith('cash_flow_hedge_reclassification')]
    assert reclass == [JAN2, FEB1, MAR1, APR1]
    assert all(rel['opening_evidence']['id'].endswith('-opening') for rel in result['relationships'])
    jan1_row = next(d_ for d_ in result['daily'] if d_['ordinal'] == JAN1)['relationships']
    near(jan1_row[0]['cumulative_hedged_change'], .5); near(jan1_row[1]['oci_balance'], .2 + 0.)
    base = run(raw()); assert 'opening_evidence' not in base['relationships'][0]


@pytest.mark.parametrize('target', ['opening_posting', 'cum', 'evidence', 'lifecycle', 'flag'])
def test_independent_replay_rejects_tampered_seasoned_results(accepted, target):
    v, result = accepted; changed = deepcopy(result)
    if target == 'opening_posting': changed['intents'][0]['postings'][0]['debit_amount'] += 1e-6
    elif target == 'cum': changed['daily'][0]['relationships'][0]['cumulative_instrument_change'] += 1e-6
    elif target == 'evidence': changed['relationships'][1]['opening_evidence']['revision'] = 'r2'
    elif target == 'lifecycle': changed['lifecycle'][0]['kind'] = 'inception'
    else: changed['seasoned_openings_supported'] = False
    with pytest.raises(ValueError): verify(v, changed)


@pytest.mark.parametrize('kind', ['no_opening', 'unseasoned_opening', 'basis', 'cfh_basis', 'fvh_oci', 'predecessor', 'matched_value', 'boundary', 'unknown'])
def test_seasoned_refusals_without_fallback(kind):
    v = seasoned_variant()
    if kind == 'no_opening': v['relationships'][0]['opening'] = None
    elif kind == 'unseasoned_opening': v['relationships'][0]['inception_ordinal'] = JAN1
    elif kind == 'basis': v['relationships'][0]['opening']['basis_balance'] = -.4
    elif kind == 'cfh_basis': v['relationships'][1]['opening']['basis_balance'] = .1
    elif kind == 'fvh_oci': v['relationships'][0]['opening']['oci_balance'] = .1
    elif kind == 'predecessor':
        v['relationships'].append(dict(deepcopy(v['relationships'][1]), relationship_id='cfh-2', predecessor_relationship_id='cfh-1'))
    elif kind == 'matched_value':
        v['relationships'][1].update(hypothetical=None, matched_critical_terms_evidence=ident('cfh-1-critical-terms'))
        v['relationships'][1]['opening']['hedged_value'] += .01
    elif kind == 'boundary': v['relationships'][0]['inception_ordinal'] = DEC1 + 10
    else: v['relationships'][0]['opening']['frozen_basis'] = 0.
    with pytest.raises(ValueError): run(v)


DEC16 = DEC1 + 15
SPAN = APR1 - DEC16


def frozen_variant():
    """Partially discontinued fair-value hedge (scale 0.8, frozen basis tranche) and a cash-flow hedge with a frozen OCI tranche."""
    v = seasoned_variant()
    v['relationships'][0]['opening'].update(scale=.8, basis_balance=-.6, frozen_basis=[dict(amount=.1, start_ordinal=DEC16, amortized=.1 * 15. / SPAN)])
    v['relationships'][1]['opening'].update(frozen_oci=[dict(amount=.05, released=0., entries=[dict(payment_ordinal=p, weight=w / 91.) for p, w in ((FEB1, 31.), (MAR1, 29.), (APR1, 31.))])])
    return v


def test_frozen_tranches_and_scale_at_the_opening_continue_amortization_and_release():
    v = frozen_variant(); result = run(v); assert verify(v, result)['verified']
    jan1 = item_unit_opening()
    for a, t in zip(result['assessments'][:2], (FEB1, MAR1)):
        d = .5 + .8 * (swap_value(t, .06) - swap_value(JAN1, .06)); e = .5 - .8 * 100. * (item_unit(t) - jan1)
        near(a['cumulative_instrument_change'], d); near(a['cumulative_hedged_change'], e); assert a['qualified']
    opening = result['lifecycle'][:2]
    near(opening[0]['frozen_basis'], .1 - .1 * 15. / SPAN); near(opening[1]['frozen_oci'], .05); near(opening[0]['proportion_after'], .8)
    near(result['intents'][0]['postings'][0]['debit_amount'], -.6 + .1 - .1 * 15. / SPAN); near(result['intents'][1]['postings'][0]['debit_amount'], -.25)
    first = result['daily'][0]['relationships']
    near(first[0]['frozen_basis_balance'], .1 * (SPAN - 16.) / SPAN); near(first[0]['designated_proportion'], .8); near(first[1]['frozen_oci_balance'], .05)
    closing = result['closing'][0]
    near(closing['basis_amortized'], .1); near(closing['basis_closing'], -.6 + .8 * 100. * (1. - jan1)); near(result['relationships'][0]['frozen_basis_balance'], 0.)
    releases = [i for i in result['intents'] if i['event_id'].startswith('frozen_oci_release')]
    assert [i['ordinal'] for i in releases] == [FEB1, MAR1, APR1]
    near(releases[0]['postings'][0]['debit_amount'], .05 * 31. / 91.); near(sum(i['postings'][0]['debit_amount'] for i in releases), .05)
    v_open = v['derivative']['opening']['trades'][1]['clean_value']
    near(result['closing'][1]['oci_closing'], .2 - v_open); near(result['relationships'][1]['frozen_oci_balance'], 0.)


@pytest.mark.parametrize('kind', ['scale', 'identity', 'weights', 'early_entry', 'foreign_entry', 'fvh_oci', 'cfh_basis', 'late_start'])
def test_frozen_opening_refusals_without_fallback(kind):
    v = frozen_variant()
    if kind == 'scale': v['relationships'][0]['opening']['scale'] = 1.5
    elif kind == 'identity': v['relationships'][0]['opening']['basis_balance'] = -.5
    elif kind == 'weights': v['relationships'][1]['opening']['frozen_oci'][0]['released'] = .01
    elif kind == 'early_entry': v['relationships'][1]['opening']['frozen_oci'][0]['entries'][0]['payment_ordinal'] = DEC1
    elif kind == 'foreign_entry': v['relationships'][1]['opening']['frozen_oci'][0]['entries'][0]['payment_ordinal'] = FEB1 + 5
    elif kind == 'fvh_oci': v['relationships'][0]['opening']['frozen_oci'] = v['relationships'][1]['opening']['frozen_oci']
    elif kind == 'cfh_basis': v['relationships'][1]['opening']['frozen_basis'] = v['relationships'][0]['opening']['frozen_basis']
    else: v['relationships'][0]['opening']['frozen_basis'][0]['start_ordinal'] = JAN1
    with pytest.raises(ValueError): run(v)
