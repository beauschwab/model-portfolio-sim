"""Synthetic `saved-hedge-mixed-1` fixtures: the reviewed hedge-accounting-1 example in the shared daily Sim.

The saved book is derived exactly from the example's Physical netting source (opening node trades and
legal mapping); the hedge overlay is every hedge-accounting-1 field except `derivative`. The hedged
fixed-rate loan is a fully dated backbook position with the item's own coupon dates, so its contractual
cash is owned by the dated owner and the hedge owner posts only basis, OCI and reclassification.

    uv run --project packages/portfolio-risk python scripts/make_hedge_mixed_example.py
"""
from copy import deepcopy
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / 'packages/portfolio-risk/tests/fixtures/saved_derivative_mixed/base.json'
HEDGE = ROOT / 'examples/models/hedge-accounting-1.json'
SEASONED = ROOT / 'packages/portfolio-risk/tests/fixtures/hedge_seasoned.json'
OUT = ROOT / 'packages/portfolio-risk/tests/fixtures/saved_hedge_mixed'
DECISION = ROOT / 'packages/portfolio-decision-native/tests/fixtures/hedge-mixed.json'
EXAMPLE = ROOT / 'examples/models/saved-hedge-mixed-1.json'


def saved_request(physical):
    csa = physical['derivative']['operation']['request']
    trades = deepcopy(csa['paths'][0]['nodes'][0]['trades'])
    book = dict(identity=dict(id='saved-hedge-book', revision='r1'), asof_ordinal=csa['asof_ordinal'],
                entity=physical['entity'], currency=physical['currency'], monetary_unit=physical['monetary_unit'],
                legacy_hedges=[], trades=trades)
    return dict(schema='saved-derivative-source-1', model_version='saved-derivative-source-1', book=book,
                contractual_upgrade_review=None, legacy_bindings=[], physical=deepcopy(physical))


def dated_loan(base, item, asof, horizon):
    """The hedged item as a contractual dated loan: coupons on the item's own periods, principal at the end."""
    dated = deepcopy(base['dated'])
    spec = dated['specification']
    spec['horizon_days'] = horizon
    days = {'act360': 360., 'act365_fixed': 365.}
    loan = dict(id=item['item_id'], account='a', kind='loan', balance=item['notional'], rate=0.,
                maturity_day=item['principal_ordinal'] - asof)
    spec['positions'] = [loan]
    events, plans = [], []
    for k, period in enumerate(item['periods']):
        interest = item['notional'] * item['contractual_rate'] * (period['end_ordinal'] - period['start_ordinal']) / days[period['day_count']]
        last = k == len(item['periods']) - 1
        event_id = 'coupon-' + period['period_id']
        events.append(dict(position_id=item['item_id'], event_id=event_id, scenario=dict(kind='all'),
                           payment_ordinal=period['payment_ordinal'], day_offset=period['payment_ordinal'] - asof,
                           accrual_period=dict(start_ordinal=period['start_ordinal'], end_ordinal=period['end_ordinal']),
                           currency='USD', components=dict(principal=item['notional'] if last else 0., cash_interest=interest,
                                                           accrued_interest=interest, book_amortization=0., fees=0.),
                           measure='contractual', provenance=dict(kind='contractual', source_id=item['source']['id'],
                                                                  source_revision=item['source']['revision']),
                           adjustment=dict(kind='none')))
        plans.append(dict(position_id=item['item_id'], event_id=event_id, run=dict(kind='all'),
                          segments=[dict(start_ordinal=period['start_ordinal'], end_ordinal=period['end_ordinal'],
                                         accrued_interest=interest, book_amortization=0.)]))
    d = dated['dated']
    d['reporting_horizon_days'] = horizon

    d['owners'][0]['position_id'] = item['item_id']
    source = d['sources'][0]
    source['coverage'][0]['position_id'] = item['item_id']
    source['schedule']['horizon_days'] = horizon
    source['schedule']['events'] = events
    source['schedule']['scenarios'] = [r['run_id'] for r in d['runs']]
    d['accrual_plans'] = plans
    return dated


def short_hedge(step=10):
    """The reviewed hedge example rebuilt on `step`-day periods (fixed rates and curves recomputed).

    The mixed lane needs two scenario runs and charges the derivative source's structural replay bound
    (quadratic in the horizon) per run, so the 92-day example is rebuilt on a 30-day horizon.
    """
    import importlib.util
    spec = importlib.util.spec_from_file_location('hedge_example', ROOT / 'scripts/make_hedge_accounting_example.py')
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    start = m.JAN1
    m.JAN1, m.FEB1, m.MAR1, m.APR1, m.MAY1 = (start + k * step for k in range(5))
    m.LAST = m.APR1
    m.PERIODS = [('1', m.JAN1, m.FEB1), ('2', m.FEB1, m.MAR1), ('3', m.MAR1, m.APR1)]
    m.EXTENDED = m.PERIODS + [('4', m.APR1, m.MAY1)]
    # Default arguments were bound to the monthly calendar when the module was defined.
    m.par_rate_feb1.__defaults__ = (m.PERIODS,)
    m.curve.__defaults__ = (m.APR1,)
    m.swap.__defaults__ = (m.PERIODS, None, m.LAST)
    m.periods.__defaults__ = (m.PERIODS,)
    m.FIXED = {'swap-fvh': .06, 'swap-cfh': m.par_rate_feb1()}
    return m.request()


def build(hedge):
    base = json.loads(BASE.read_text())
    physical = hedge['derivative']
    csa = physical['derivative']['operation']['request']
    asof, tail = csa['asof_ordinal'], csa['policy']['tail_return_ordinal']
    item = hedge['hedged_items'][0]
    dated = dated_loan(base, item, asof, tail - asof)
    opening = physical['opening']
    net = sum(max(t['clean_value'], 0.) for t in opening['trades']) - sum(max(-t['clean_value'], 0.) for t in opening['trades'])
    basis = sum(r['opening']['basis_balance'] + sum(t['amount'] - t['amortized'] for t in r['opening'].get('frozen_basis', []))
                for r in hedge['relationships'] if r.get('opening') and r['designation'] == 'fair_value_hedge')
    account = dated['specification']['accounts'][0]
    account.update(cash=opening['cash'], equity=opening['cash'] + item['notional'] + net + basis)
    overlay = {k: deepcopy(v) for k, v in hedge.items() if k != 'derivative'}
    # The loan settles at its principal date: the reviewed maturity derecognizes the remaining basis,
    # so every hedge balance closes within the shared Sim's complete tail.
    overlay['events'].append(dict(event_id='matured-fvh-1', relationship_id='fvh-1', ordinal=item['principal_ordinal'],
                                  sequence=0, kind='discontinue', review=dict(id='loan-1-maturity', revision='r1'),
                                  reason='hedged_item_matured'))
    runs = [dict(ledger_scenario=s, saved=saved_request(physical), hedge=deepcopy(overlay)) for s in ('baseline', 'zero')]
    return dict(schema='saved-hedge-mixed-1', dated=dated, owners=[dict(account_id='a', runs=runs)])


if __name__ == '__main__':
    OUT.mkdir(parents=True, exist_ok=True)
    base = build(short_hedge())
    for path in (OUT / 'base.json', DECISION, EXAMPLE):
        path.write_text(json.dumps(base, separators=(',', ':')) + '\n')
        print(path, path.stat().st_size, 'bytes')
