"""Write the nonfinancial-hedge-1 example (synthetic fixture, not production inputs).

A plastics buyer hedges forecasted purchases of 100 t of polyethylene in March and April with a
receive-index swap at 1,000 on the Gulf polyethylene index. The purchase price has no pricing
formula, so the hedged component is clearly and closely related to the asset (ASU 2025-09,
815-20-25-22C(a)) and needs the entity's adoption. A forecasted sale of 50 t in May has a pricing
formula that explicitly references the same index (a contractually specified component) and is
hedged by a pay-index swap. A European polyethylene index is supplied for basis-risk variants.
Consumption events release the purchase OCI to cost of sales; the sale releases its OCI at delivery.
"""
import datetime as dt
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
JAN1, FEB1, MAR1, APR1, MAY1, JUN1 = (dt.date(2024, m, 1).toordinal() for m in (1, 2, 3, 4, 5, 6))
DELIVERIES = (MAR1, APR1, MAY1, JUN1)
GULF = {JAN1: (1000, 1000, 1000, 1000), FEB1: (1050, 1040, 1030, 1020), MAR1: (1080, 1060, 1050, 1040),
        APR1: (None, 1020, 1010, 1000), MAY1: (None, None, 990, 995), JUN1: (None, None, None, 1000)}
EUROPE = {JAN1: (1000, 1000, 1000, 1000), FEB1: (1045, 1036, 1028, 1018), MAR1: (1070, 1055, 1046, 1036),
          APR1: (None, 1022, 1012, 1002), MAY1: (None, None, 992, 996), JUN1: (None, None, None, 1001)}


def ident(i, r='r1'):
    return dict(id=i, revision=r)


def curve(index, table, t):
    return dict(index_id=index, source=ident(index + '-assessment'),
                forwards=[dict(delivery_ordinal=d, price=float(p)) for d, p in zip(DELIVERIES, table[t]) if p is not None])


def market(t):
    return dict(ordinal=t, discount=[dict(ordinal=t, discount_factor=1.)] + [
        dict(ordinal=d, discount_factor=math.exp(-.05 * (d - t) / 365.)) for d in DELIVERIES if d > t],
        indices=[curve('pe-gulf', GULF, t), curve('pe-europe', EUROPE, t)])


def request():
    return dict(
        schema='nonfinancial-hedge-1', model_version='nonfinancial-hedge-1', run=ident('nfh-run'), entity='bank',
        currency='USD', monetary_unit='currency_units',
        policy=dict(identity=ident('nfh-policy'), assessment_lower_bound=.8, assessment_upper_bound=1.25,
                    asu_2025_09_adoption=dict(review=ident('asu-2025-09-adoption'), adoption_ordinal=JAN1)),
        dates=[market(t) for t in (JAN1, FEB1, MAR1, APR1, MAY1, JUN1)],
        exposures=[
            dict(exposure_id='pe-purchases', source=ident('procurement-plan'), probability_evidence=ident('purchase-history'),
                 direction='purchase', asset='polyethylene', quantity_unit='metric_ton',
                 component=dict(kind='clearly_and_closely_related', index_id='pe-gulf', market='forward',
                                evidence=ident('pe-gulf-clearly-and-closely-related')),
                 deliveries=[dict(delivery_id='mar', ordinal=MAR1, quantity=100.),
                             dict(delivery_id='apr', ordinal=APR1, quantity=100.)]),
            dict(exposure_id='pe-sales', source=ident('sales-contract'), probability_evidence=ident('sales-contract-signed'),
                 direction='sale', asset='polyethylene', quantity_unit='metric_ton',
                 component=dict(kind='contractually_specified', index_id='pe-gulf', contract=ident('sales-pricing-formula')),
                 deliveries=[dict(delivery_id='may', ordinal=MAY1, quantity=50.)])],
        instruments=[
            dict(position_id='swap-purchase', source=ident('swap-purchase-confirmation'), index_id='pe-gulf',
                 side='receive_index', fixed_price=1000., trade_ordinal=JAN1,
                 legs=[dict(leg_id='mar', settlement_ordinal=MAR1, quantity=100.),
                       dict(leg_id='apr', settlement_ordinal=APR1, quantity=100.)]),
            dict(position_id='swap-sale', source=ident('swap-sale-confirmation'), index_id='pe-gulf', side='pay_index',
                 fixed_price=1000., trade_ordinal=JAN1, legs=[dict(leg_id='may', settlement_ordinal=MAY1, quantity=50.)])],
        relationships=[
            dict(relationship_id='cfh-purchases', instrument_position_id='swap-purchase', exposure_id='pe-purchases',
                 documentation=ident('cfh-purchases-documentation'), designation_ordinal=JAN1,
                 legs=[dict(leg_id='mar', delivery_id='mar'), dict(leg_id='apr', delivery_id='apr')]),
            dict(relationship_id='cfh-sales', instrument_position_id='swap-sale', exposure_id='pe-sales',
                 documentation=ident('cfh-sales-documentation'), designation_ordinal=JAN1,
                 legs=[dict(leg_id='may', delivery_id='may')])],
        events=[
            dict(event_id='use-mar-40', ordinal=APR1, kind='consumption', review=ident('inventory-relief'),
                 exposure_id='pe-purchases', delivery_id='mar', fraction=.4),
            dict(event_id='use-mar-60', ordinal=JUN1, kind='consumption', review=ident('inventory-relief'),
                 exposure_id='pe-purchases', delivery_id='mar', fraction=.6),
            dict(event_id='use-apr', ordinal=JUN1, kind='consumption', review=ident('inventory-relief'),
                 exposure_id='pe-purchases', delivery_id='apr', fraction=1.)])


if __name__ == '__main__':
    body = json.dumps(request(), indent=1, sort_keys=True) + '\n'
    for path in ('examples/models/nonfinancial-hedge-1.json', 'packages/portfolio-risk-native/tests/fixtures/nonfinancial-hedge.json'):
        (ROOT / path).write_text(body, encoding='utf-8', newline='\n')
