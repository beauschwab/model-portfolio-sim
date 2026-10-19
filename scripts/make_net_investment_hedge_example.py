"""Write the net-investment-hedge-1 examples (synthetic fixtures, not production inputs).

Base example: a USD reporter with a EUR foreign operation of EUR 1,000 net assets. A sold EUR 400
forward (spot method, amortized forward points) and EUR 700 debt hedging EUR 600 hedge the net
investment. The operation earns EUR 20, distributes EUR 50 and is 20% sold on 1 April, so the
designated EUR 1,000 later exceeds the net assets and the excess goes to earnings.

Dual example: EUR 100 fixed-rate debt is the net-investment hedging instrument and the hedged
item in a EUR fair value hedge of interest rate risk (ASU 2025-09, 815-20-55-38). The fair value
hedge is the hedge-accounting-1 example re-expressed in EUR with a receive-fixed swap and a
liability item.
"""
import datetime as dt
import json
import math
from copy import deepcopy
from pathlib import Path

import make_hedge_accounting_example as hedge

ROOT = Path(__file__).resolve().parents[1]
JAN1, FEB1, MAR1, APR1, MAY1 = (dt.date(2024, m, 1).toordinal() for m in (1, 2, 3, 4, 5))
SPOTS = {JAN1: 1.10, FEB1: 1.08, MAR1: 1.12, APR1: 1.09, MAY1: 1.11}
USD_RATE, EUR_RATE = .05, .035


def ident(i, r='r1'):
    return dict(id=i, revision=r)


def curve(t, rate, maturity):
    return [dict(ordinal=t, discount_factor=1.)] + ([dict(ordinal=maturity, discount_factor=math.exp(-rate * (maturity - t) / 365.))]
                                                     if maturity > t else [])


def date(t, maturity=None):
    out = dict(ordinal=t, spot=SPOTS[t], spot_source=ident('ecb-reference-rate'))
    if maturity is not None and t < maturity:
        out.update(reporting_discount=curve(t, USD_RATE, maturity), foreign_discount=curve(t, EUR_RATE, maturity))
    return out


def policy(dual=False):
    out = dict(identity=ident('nih-policy'), assessment_lower_bound=.8, assessment_upper_bound=1.25)
    if dual:
        out['dual_hedge_adoption'] = dict(review=ident('asu-2025-09-adoption'), adoption_ordinal=JAN1)
    return out


def base():
    forward_rate = SPOTS[JAN1] * math.exp(-EUR_RATE * (MAY1 - JAN1) / 365.) / math.exp(-USD_RATE * (MAY1 - JAN1) / 365.)
    return dict(
        schema='net-investment-hedge-1', model_version='net-investment-hedge-1', run=ident('nih-run'), entity='bank',
        reporting_currency='USD', foreign_currency='EUR', monetary_unit='currency_units', policy=policy(),
        dates=[date(t, MAY1) for t in sorted(SPOTS)],
        foreign_operation=dict(operation_id='de-sub', source=ident('de-sub-ledger'), opening_net_assets=1000.,
                               opening_translation_cta=25., movements=[
                                   dict(movement_id='feb-income', ordinal=FEB1, kind='income', amount=20., rate=1.09,
                                        rate_source=ident('january-average')),
                                   dict(movement_id='apr-dividend', ordinal=APR1, kind='distribution', amount=50., rate=1.09,
                                        rate_source=ident('april-spot'))]),
        instruments=[
            dict(kind='fx_forward', position_id='fwd-1', source=ident('fwd-1-confirmation'), side='sell_foreign',
                 notional=400., forward_rate=forward_rate, trade_ordinal=JAN1, maturity_ordinal=MAY1),
            dict(kind='foreign_debt', position_id='debt-1', source=ident('debt-1-terms'), notional=700.,
                 maturity_ordinal=dt.date(2026, 1, 1).toordinal())],
        relationships=[
            dict(relationship_id='nih-fwd', instrument_position_id='fwd-1', documentation=ident('nih-fwd-documentation'),
                 designation_ordinal=JAN1, hedged_net_investment=400., method='spot', excluded_component='amortization'),
            dict(relationship_id='nih-debt', instrument_position_id='debt-1', documentation=ident('nih-debt-documentation'),
                 designation_ordinal=JAN1, hedged_net_investment=600., method='spot')],
        events=[dict(event_id='sell-20pct', ordinal=APR1, kind='disposal', review=ident('disposal-review'),
                     fraction=.2, proceeds=230.)])


def eur_fair_value_hedge():
    raw = json.loads(json.dumps(hedge.request()).replace('"USD"', '"EUR"'))
    raw['run'] = ident('eur-fvh-run')
    der = raw['derivative']
    for node in der['derivative']['operation']['request']['paths'][0]['nodes']:
        for trade in node['trades']:
            if trade['request']['position_id'] == 'swap-fvh':
                trade['request']['side'] = 'receiver'
    opening = der['opening']
    for trade in opening['trades']:
        if trade['position_id'] == 'swap-fvh':
            trade['clean_value'] = -trade['clean_value']
    opening['equity_credit_balance'] = opening['cash'] + sum(t['clean_value'] for t in opening['trades'])
    item = raw['hedged_items'][0]
    item.update(item_id='eur-debt', direction='liability', source=ident('eur-debt-terms'),
                carrying_source=ident('eur-debt-carrying'))
    raw['relationships'] = [dict(r, hedged_item_id='eur-debt') for r in raw['relationships'] if r['relationship_id'] == 'fvh-1']
    raw['forecast_exposures'] = []
    raw['events'] = [e for e in raw['events'] if e['relationship_id'] == 'fvh-1']
    return raw


def dual():
    return dict(
        schema='net-investment-hedge-1', model_version='net-investment-hedge-1', run=ident('nih-dual-run'), entity='bank',
        reporting_currency='USD', foreign_currency='EUR', monetary_unit='currency_units', policy=policy(dual=True),
        dates=[date(t) for t in (JAN1, FEB1, MAR1, APR1)],
        foreign_operation=dict(operation_id='de-sub', source=ident('de-sub-ledger'), opening_net_assets=500.,
                               opening_translation_cta=0., movements=[]),
        instruments=[dict(kind='foreign_debt', position_id='eur-debt', source=ident('eur-debt-terms'), notional=100.,
                          maturity_ordinal=APR1)],
        relationships=[dict(relationship_id='nih-dual', instrument_position_id='eur-debt',
                            documentation=ident('nih-dual-documentation'), designation_ordinal=JAN1,
                            hedged_net_investment=100., method='spot',
                            dual_fair_value_hedge=dict(relationship_id='fvh-1', request=eur_fair_value_hedge()))],
        events=[])


def write(path, value):
    (ROOT / path).write_text(json.dumps(value, indent=1, sort_keys=True) + '\n', encoding='utf-8', newline='\n')


if __name__ == '__main__':
    write('examples/models/net-investment-hedge-1.json', base())
    write('packages/portfolio-risk-native/tests/fixtures/net-investment-hedge.json', base())
    write('packages/portfolio-risk-native/tests/fixtures/net-investment-dual.json', dual())
