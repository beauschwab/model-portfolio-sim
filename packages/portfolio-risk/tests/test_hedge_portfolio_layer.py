"""ASU 2022-01 portfolio layers: native results verified by the independent replay, tampers and refusals."""
from copy import deepcopy
import datetime as dt
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics.model_contracts import account_hedge_relationships
from portfolio_risk.analytics.hedge_accounting_reference import verify

ROOT = Path(__file__).resolve().parents[3]
JAN1, FEB1, FEB15, MAR1, MAR15, APR1 = (dt.date(2024, m, d).toordinal() for m, d in ((1, 1), (2, 1), (2, 15), (3, 1), (3, 15), (4, 1)))


def example():
    return json.loads((ROOT / 'examples/models/hedge-accounting-1.json').read_text())


def ident(s):
    return dict(id=s, revision='r1')


def portfolio(balance, projections):
    return dict(members=[dict(asset_id='loan-a', source=ident('loan-a'), prepayable=True), dict(asset_id='loan-b', source=ident('loan-b'), prepayable=True)],
                balance_source=ident('servicing-balances'), balances=[dict(ordinal=t, outstanding=balance(t)) for t in range(JAN1, APR1 + 1)],
                projection_evidence=ident('prepayment-projection'),
                projections=[dict(ordinal=o, term_end_ordinal=e, anticipated=a) for o, e, a in projections])


def layer(rid, proportion, hedged, end=None, schedule=None):
    rel = dict(relationship_id=rid, predecessor_relationship_id=None, designation='fair_value_hedge', hedging_instrument_position_id='swap-fvh',
               designated_proportion=proportion, hedged_item_id='pool-1', hedged_notional=hedged, forecast_exposure_id=None,
               hedged_risk='benchmark_interest_rate', benchmark_rate=.06, inception_ordinal=JAN1, risk_management_objective=ident(rid + '-objective'),
               documentation=ident(rid + '-documentation'), hypothetical=None, matched_critical_terms_evidence=None)
    if end is not None: rel['hedged_term_end_ordinal'] = end
    if schedule is not None: rel['layer_amortization'] = [dict(period_end_ordinal=e, amount=a) for e, a in schedule]
    return rel


def assess(rid, ordinal, sequence):
    return dict(event_id=f'assess-{rid}-{ordinal}', relationship_id=rid, ordinal=ordinal, sequence=sequence, kind='assess', review=ident('assessment'))


def case(order, pool, layers, events):
    v = example()
    v['policy']['adoption']['asu_2022_01_portfolio_layer'] = True
    v['policy']['portfolio_layer'] = dict(identity=ident('layer-policy'), breach_order=order)
    item = dict(v['hedged_items'][0], item_id='pool-1', kind='closed_portfolio', portfolio=pool)
    v.update(hedged_items=[item], forecast_exposures=[], relationships=layers, events=events)
    return v


def sequential():
    return case('last_designated_first', portfolio(lambda t: 100. if t < MAR15 else 70., [(JAN1, APR1, 100.), (FEB1, APR1, 80.), (MAR1, APR1, 80.)]),
                [layer('layer-1', .7, 70.), layer('layer-2', .3, 30.)],
                [assess('layer-1', FEB1, 0), assess('layer-2', FEB1, 1), assess('layer-1', MAR1, 0), assess('layer-2', MAR1, 1)])


def amortizing():
    return case('pro_rata', portfolio(lambda t: 100. if t < FEB15 else 50., [(JAN1, MAR1, 100.), (JAN1, APR1, 70.)]),
                [layer('layer-a', .6, 60., schedule=[(FEB1, 60.), (MAR1, 40.), (APR1, 20.)]), layer('layer-b', .3, 30., end=MAR1)], [])


def near(a, b):
    assert abs(a - b) <= 1e-9 * (1. + max(abs(a), abs(b))), (a, b)


@pytest.fixture(scope='module')
def results():
    out = {}
    for name, build in (('sequential', sequential), ('amortizing', amortizing)):
        request = build(); out[name] = (request, account_hedge_relationships(request))
    return out


@pytest.mark.parametrize('name', ['sequential', 'amortizing'])
def test_layers_verify_independently(results, name):
    request, result = results[name]
    assert verify(request, result)['verified'] is True
    assert result['portfolio_layer_supported'] is True
    for r in result['relationships']:
        assert r['basis_excluded_from_credit_loss_measurement'] is True


def test_sequential_breaches_scale_the_example_basis(results):
    request, result = results['sequential']
    base = account_hedge_relationships(example())
    ex = {d['ordinal']: d['relationships'][0]['basis_adjustment_balance'] for d in base['daily']}
    breaches = [x for x in result['lifecycle'] if x['kind'] == 'portfolio_layer_breach']
    assert [(b['ordinal'], b['reason'], b['relationship_id']) for b in breaches] == [(FEB1, 'anticipated_breach', 'layer-2'), (MAR15, 'actual_breach', 'layer-2')]
    near(breaches[0]['breach_amount'], 20.); near(breaches[0]['frozen_basis'], .2 * ex[FEB1])
    near(breaches[1]['breach_amount'], 10.); near(breaches[1]['derecognized_basis'], .1 * ex[MAR15])
    l1 = next(c for c in result['closing'] if c['relationship_id'] == 'layer-1')
    near(l1['basis_closing'], .7 * base['closing'][0]['basis_closing'])
    l2 = next(c for c in result['closing'] if c['relationship_id'] == 'layer-2')
    near(l2['basis_amortized'], .2 * ex[FEB1]); near(l2['basis_closing'], 0.)


def test_pro_rata_amortizing_breach(results):
    _, result = results['amortizing']
    breaches = [x for x in result['lifecycle'] if x['kind'] == 'portfolio_layer_breach']
    assert [(b['ordinal'], b['relationship_id']) for b in breaches] == [(FEB15, 'layer-a'), (FEB15, 'layer-b')]
    near(breaches[0]['breach_amount'], 40. * 20. / 70.); near(breaches[1]['breach_amount'], 30. * 20. / 70.)
    near(next(r for r in result['relationships'] if r['relationship_id'] == 'layer-a')['layer_amount'], 60. * 5. / 7.)


@pytest.mark.parametrize('target', ['breach_amount', 'frozen', 'derecognized', 'reason', 'layer_amount', 'credit_flag', 'drop_breach', 'raw_projection', 'raw_balance', 'raw_order'])
def test_independent_replay_rejects_tampered_layer_results(results, target):
    request, result = deepcopy(results['sequential'])
    breach = next(x for x in result['lifecycle'] if x['kind'] == 'portfolio_layer_breach')
    if target == 'breach_amount': breach['breach_amount'] += 1e-6
    elif target == 'frozen': breach['frozen_basis'] += 1e-6
    elif target == 'derecognized': result['lifecycle'][-1]['derecognized_basis'] += 1e-6
    elif target == 'reason': breach['reason'] = 'actual_breach'
    elif target == 'layer_amount': result['relationships'][1]['layer_amount'] += 1e-6
    elif target == 'credit_flag': result['relationships'][0]['basis_excluded_from_credit_loss_measurement'] = False
    elif target == 'drop_breach': result['lifecycle'].remove(breach)
    elif target == 'raw_projection': request['hedged_items'][0]['portfolio']['projections'][1]['anticipated'] = 85.
    elif target == 'raw_balance':
        for b in request['hedged_items'][0]['portfolio']['balances']:
            if MAR1 <= b['ordinal'] < MAR15: b['outstanding'] = 75.
    else: request['policy']['portfolio_layer']['breach_order'] = 'pro_rata'
    with pytest.raises((ValueError, ArithmeticError)):
        verify(request, result)


@pytest.mark.parametrize('kind', ['no_adoption', 'no_policy', 'liability', 'increasing', 'missing_balance', 'missing_projection', 'not_expected', 'schedule'])
def test_native_layer_refusals(kind):
    request = case('last_designated_first', portfolio(lambda t: 100., [(JAN1, APR1, 100.)]), [layer('layer-1', 1., 100.)], [])
    item = request['hedged_items'][0]; pool = item['portfolio']
    if kind == 'no_adoption': request['policy']['adoption']['asu_2022_01_portfolio_layer'] = False
    elif kind == 'no_policy': request['policy'].pop('portfolio_layer')
    elif kind == 'liability': item['direction'] = 'liability'
    elif kind == 'increasing': pool['balances'][5]['outstanding'] = 100.5
    elif kind == 'missing_balance': pool['balances'].pop(10)
    elif kind == 'missing_projection': pool['projections'] = []
    elif kind == 'not_expected': pool['projections'][0]['anticipated'] = 90.
    else: request['relationships'][0]['layer_amortization'] = [dict(period_end_ordinal=FEB1, amount=100.)]
    with pytest.raises(ValueError):
        account_hedge_relationships(request)
