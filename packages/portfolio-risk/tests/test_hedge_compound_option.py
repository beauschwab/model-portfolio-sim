"""ASU 2025-09 Issue 4 (815-20-25-88) in hedge-accounting-1: swap plus written cash-settled swaption
as one designated instrument; native ownership, independent replay, tamper and refusal controls.

Fixture: the shared hedge example (6% simple curve dropping to 4% on 1 February; `swap-cfh` pays
the 1 February par rate `k`). The written option `opt-cfh` expires and cash-settles on 1 March on
a one-period payer swap (1 March-1 April, notional 100, strike K) with one terminal state carrying
the node's projected March rate.
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
TAU2, TAU3 = 29. / 360., 31. / 360.
STRIKE = .0448


def raw():
    return json.loads((ROOT / 'examples/models/hedge-accounting-1.json').read_text())


def run(request):
    return account_hedge_relationships(request)


def df(rate, days):
    return 1. / (1. + rate * days / 360.)


def utc(ordinal, hour=9):
    return (ordinal - 719163) * 86400 + hour * 3600


def curve_rate(t):
    return .06 if t < FEB1 else .04


def projected(t):
    return .06 if t < FEB1 else .045


def ident(i):
    return dict(id=i, revision='r1')


def swap_value(t, k):
    v = 0.
    if t < MAR1: v += (.04 - k) * 100. * TAU2 * df(.04, MAR1 - t)
    if t < APR1: v += (.045 - k) * 100. * TAU3 * df(.04, APR1 - t)
    return v


def option_value(t):
    r = curve_rate(t); intrinsic = max(projected(t) - STRIKE, 0.) * 100. * TAU3 * df(r, 31)
    return -intrinsic * df(r, MAR1 - t) if t < MAR1 else 0.


def cfh_fixed(request):
    return request['derivative']['derivative']['operation']['request']['paths'][0]['nodes'][0]['trades'][1]['request']['fixed_rate']


def nodes(request):
    return request['derivative']['derivative']['operation']['request']['paths'][0]['nodes']


def near(a, b):
    assert abs(a - b) <= 1e-9 * (1. + max(abs(a), abs(b))), (a, b)


def option_request(request, asof, info):
    node = next(n for n in nodes(request) if n['ordinal'] == info); swap = node['trades'][1]['request']; r = curve_rate(info)
    u = deepcopy(swap)
    u.update(position_id='opt-under', asof_ordinal=MAR1, valuation_cutoff_unix_seconds=utc(MAR1), fixed_rate=STRIKE,
             fixed_periods=[deepcopy(swap['fixed_periods'][2])], floating_periods=[deepcopy(swap['floating_periods'][2])])
    u['discount_curve'].update(asof_ordinal=MAR1, information_unix_seconds=utc(MAR1),
                               points=[dict(ordinal=MAR1, discount_factor=1.), dict(ordinal=APR1, discount_factor=df(r, 31))])
    return dict(contract=ident('opt-cfh-contract'), exercise_policy=ident('european-positive-value'), conditional_model=ident('index-model'),
                crn=ident('crn'), position_id='opt-cfh', scenario_id='physical', currency='USD', monetary_unit='currency_units', measure='pricing',
                asof_ordinal=asof, expiry_ordinal=MAR1, settlement_ordinal=MAR1, tail_end_ordinal=APR1, side='short',
                settlement='collateralized_cash_price',
                states=[dict(state_id='single', probability=1., discount_to_expiry=df(r, MAR1 - asof) if asof < MAR1 else 1.,
                             discount_source=ident('discount'), underlying=u, delivered_position_id='opt-under', netting_set_id='ns')])


def compound_variant():
    v = raw()
    v['policy']['adoption']['asu_2025_09'] = dict(adopted=True, adoption_ordinal=JAN1, elections=dict(
        similar_risk_grouping=False, selectable_rate_debt=False, swap_written_option_exception=True, dual_hedge_basis=False))
    terminal = option_request(v, MAR1, MAR1)
    for n in nodes(v):
        t = n['ordinal']
        n['trades'].append(dict(kind='european_swaption', request=option_request(v, t, t)) if t < MAR1 else
                           dict(kind='expired_european', terminal=deepcopy(terminal), election=ident('election'), delivered_underlying=None, settlement_discount=None))
    csa = v['derivative']['derivative']['operation']['request']
    csa['mapping'].append(dict(position_id='opt-cfh', contract=ident('opt-cfh-contract'), entity='bank', counterparty='cp', currency='USD',
                               netting_set_id='ns', legal_review=ident('legal'), enforceable_netting=True))
    v['derivative']['policy']['trades'].append(dict(position_id='opt-cfh', designation='undesignated_economic_hedge', designation_review=ident('designation'),
                                                    mark_recognition='clean_value_changes_in_earnings', coupon_recognition='settle_derivative_carrying', delivery_review=None))
    v0 = option_value(JAN1)
    v['derivative']['opening']['trades'].append(dict(position_id='opt-cfh', clean_value=v0)); v['derivative']['opening']['equity_credit_balance'] += v0
    k = cfh_fixed(v)
    hyp = [dict(ordinal=n['ordinal'], request=dict(deepcopy(n['trades'][1]['request']), position_id='hypothetical', fixed_rate=k)) for n in nodes(v) if n['ordinal'] >= FEB1]
    v['relationships'][1].update(matched_critical_terms_evidence=None, hypothetical=dict(evidence=ident('hyp-evidence'), nodes=hyp), written_option_position_id='opt-cfh')
    return v


@pytest.fixture(scope='module')
def accepted():
    v = compound_variant()
    return v, run(v)


def test_combined_instrument_verifies_independently_with_hand_values(accepted):
    v, result = accepted; k = cfh_fixed(v)
    receipt = verify(v, result); assert receipt['verified'] and receipt['asu_2025_09_swap_written_option_supported'] is True
    o1 = option_value(FEB1); o2 = -(.045 - STRIKE) * 100. * TAU3 * df(.04, 31)
    feb1 = next(d for d in result['daily'] if d['ordinal'] == FEB1)['relationships'][1]; near(feb1['instrument_designated_value'], o1)
    a = result['assessments'][2]; d = swap_value(MAR1, k) - o1
    near(a['cumulative_instrument_change'], d); near(a['cumulative_hedged_change'], swap_value(MAR1, k)); near(a['ratio'], d / swap_value(MAR1, k))
    assert a['qualified'] and 1.05 <= a['ratio'] <= 1.15
    mar1 = next(d_ for d_ in result['daily'] if d_['ordinal'] == MAR1)['relationships'][1]; near(mar1['instrument_designated_value'], swap_value(MAR1, k))
    marks = [i for i in result['intents'] if i['event_id'].startswith('clean_mark') and i['mapped_position_id'] == 'opt-cfh' and i['ordinal'] > FEB1]
    assert marks and all(p['account'] != 'mark_gain_loss' for i in marks for p in i['postings']) and all(i['relationship_id'] == 'cfh-1' for i in marks)
    s2, s3 = (.04 - k) * 100. * TAU2, (.045 - k) * 100. * TAU3
    closing = result['closing'][1]
    near(closing['oci_recognized'], s2 + s3 + o2 - o1); near(closing['oci_reclassified'], s2 + s3); near(closing['oci_closing'], o2 - o1)
    assert result['relationships'][1]['written_option_position_id'] == 'opt-cfh' and 'written_option_position_id' not in result['relationships'][0]
    alone = deepcopy(v); alone['relationships'][1]['written_option_position_id'] = None
    out = run(alone); assert verify(alone, out)['verified']; near(out['assessments'][2]['ratio'], 1.); near(out['closing'][1]['oci_closing'], 0.)


@pytest.mark.parametrize('target', ['value', 'ratio', 'closing_option', 'mark_owner', 'flag'])
def test_independent_replay_rejects_tampered_combination_results(accepted, target):
    v, result = accepted; changed = deepcopy(result)
    if target == 'value': changed['daily'][40]['relationships'][1]['instrument_designated_value'] += 1e-6
    elif target == 'ratio': changed['assessments'][2]['ratio'] += 1e-6
    elif target == 'closing_option': changed['relationships'][1]['written_option_position_id'] = 'swap-cfh'
    elif target == 'mark_owner':
        mark = next(i for i in changed['intents'] if i['event_id'].startswith('clean_mark') and i['mapped_position_id'] == 'opt-cfh' and i['ordinal'] > FEB1)
        mark['relationship_id'] = None
    else: changed['asu_2025_09_swap_written_option_supported'] = False
    with pytest.raises(ValueError): verify(v, changed)


@pytest.mark.parametrize('kind', ['election', 'before_adoption', 'long', 'notional', 'matched', 'same', 'unmapped', 'over', 'coupon'])
def test_combination_refusals_without_fallback(kind):
    v = compound_variant()
    if kind == 'election': v['policy']['adoption']['asu_2025_09']['elections']['swap_written_option_exception'] = False
    elif kind == 'before_adoption': v['policy']['adoption']['asu_2025_09']['adoption_ordinal'] = MAR1
    elif kind in ('long', 'notional'):
        for n in nodes(v):
            t = n['trades'][2]; o = t['terminal'] if t['kind'] == 'expired_european' else t['request']
            if kind == 'long': o['side'] = 'long'
            else: o['states'][0]['underlying']['notional'] = 50.
        if kind == 'long':
            v0 = v['derivative']['opening']['trades'][2]['clean_value']; v['derivative']['opening']['trades'][2]['clean_value'] = -v0
            v['derivative']['opening']['equity_credit_balance'] -= 2. * v0
    elif kind == 'matched': v['relationships'][1].update(hypothetical=None, matched_critical_terms_evidence=ident('cfh-1-critical-terms'))
    elif kind == 'same': v['relationships'][1]['written_option_position_id'] = 'swap-cfh'
    elif kind == 'unmapped': v['relationships'][1]['written_option_position_id'] = 'opt-other'
    elif kind == 'over': v['relationships'][0]['written_option_position_id'] = 'opt-cfh'
    else: v['derivative']['policy']['trades'][2]['coupon_recognition'] = 'contractual_income_at_due_with_mark_offset'
    with pytest.raises(ValueError): run(v)
