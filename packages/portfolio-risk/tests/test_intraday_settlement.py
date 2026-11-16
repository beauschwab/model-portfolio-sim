"""Intraday settlement replay (`intraday-settlement-1`).

Golden cases are built by hand: cash exhaustion, partial settlement, collateral
movement delay, missed cut-offs, outages, double-pledge prevention, legal netting
and the stop. Every case runs through the Rust replay and the independent Python
reference and must agree on the whole event journal.
"""
import random
from copy import deepcopy
from dataclasses import replace

import pytest

from portfolio_risk.analytics import intraday_settlement_reference as reference
from portfolio_risk.analytics.intraday_settlement import run_intraday_settlement
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import RunConfig, run_context

pytestmark = pytest.mark.skipif(not library_path().is_file(), reason='build native products')
RUST = RunConfig(compute_backend='rust')
WIRE = dict(id='wire', open=540, close=1020, cycle_minutes=60)          # 09:00 to 17:00, hourly


def book(**parts):
    return dict(schema='intraday-settlement-1', opening_cash=100., horizon_minute=1439, systems=[dict(WIRE)]) | parts


def same(a, b):
    if isinstance(a, float) or isinstance(b, float):
        return a == pytest.approx(b, rel=1e-12, abs=1e-9)
    if isinstance(a, dict):
        return a.keys() == b.keys() and all(same(a[k], b[k]) for k in a)
    if isinstance(a, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b))
    return a == b


def run(request):
    """Both implementations; the whole result must agree. Returns the Rust result."""
    expected = reference.replay(deepcopy(request))
    with run_context(RUST):
        actual = run_intraday_settlement(deepcopy(request))
    assert [e['event'] for e in actual['events']] == [e['event'] for e in expected['events']]
    assert same(actual, expected)
    return actual


def events(result, name):
    return [e for e in result['events'] if e['event'] == name]


def pay(ident, minute, amount, **extra):
    return dict(id=ident, minute=minute, amount=amount, system='wire', **extra)


def test_cash_exhaustion_pays_first_in_first_out_and_partially():
    result = run(book(payments=[pay('a', 500, 60.), pay('b', 530, 70.), pay('c', 535, 10.)]))
    paid = {p['id']: p for p in result['payments']}
    # 09:00 cycle: a in full, b gets the remaining 40; c does not jump the queue.
    assert (paid['a']['paid'], paid['a']['settled_minute']) == (60., 540)
    assert (paid['b']['paid'], paid['b']['unpaid'], paid['b']['first_paid_minute']) == (40., 30., 540)
    assert (paid['c']['paid'], paid['c']['unpaid']) == (0., 10.)
    first = events(result, 'shortfall')[0]
    assert (first['minute'], first['amount'], first['cash']) == (540, 40., 0.)
    assert result['summary']['first_shortfall_minute'] == 540
    assert result['summary']['shortfall_cycles'] == 9                       # every hourly cycle from 09:00 to 17:00
    assert result['summary']['requested'] == 140. and result['summary']['paid'] == 100.
    assert result['summary']['unpaid'] == 40. and result['summary']['closing_cash'] == 0.
    assert result['summary']['peak_queue'] == 140. and result['summary']['peak_queue_minute'] == 535


def test_cycle_capacity_limits_settlement_without_a_cash_shortfall():
    system = dict(WIRE, capacity_per_cycle=25.)
    result = run(book(systems=[system], payments=[pay('a', 500, 60.)]))
    assert [e['amount'] for e in events(result, 'payment_paid')] == [25., 25., 10.]
    assert [e['minute'] for e in events(result, 'capacity_limited')] == [540, 600]
    assert not events(result, 'shortfall') and result['summary']['first_shortfall_minute'] is None
    assert result['payments'][0]['settled_minute'] == 660


def test_missed_cutoff_waits_for_the_next_open_day_and_closed_days_are_skipped():
    system = dict(WIRE, closed_days=[1])
    result = run(book(horizon_minute=3*1440-1, systems=[system],
                      payments=[pay('late', 1021, 10.), pay('on_time', 1020, 5.)]))
    paid = {p['id']: p for p in result['payments']}
    assert paid['on_time']['after_cutoff'] is False and paid['on_time']['settled_minute'] == 1020
    assert paid['late']['after_cutoff'] is True
    assert paid['late']['settled_minute'] == 2*1440 + 540                  # day 1 is closed; 09:00 on day 2
    assert events(result, 'payment_requested')[1]['detail'] == 'after_cutoff'


def facility_book(**parts):
    lots = [dict(id='ust', value=200., location='custodian', haircuts={'window': .05, 'fhlb': .1}),
            dict(id='mbs', value=100., location='fed', haircuts={'window': .1})]
    facilities = [dict(id='window', location='fed', open=480, close=1080, settlement_lag_minutes=30, capacity=500.),
                  dict(id='fhlb', location='fhlb', open=480, close=960, settlement_lag_minutes=120, capacity=500.)]
    return book(lots=lots, facilities=facilities, **parts)


def test_a_delayed_draw_cannot_cure_an_earlier_shortfall():
    result = run(facility_book(payments=[pay('a', 500, 150.)],
                               draws=[dict(id='d', facility='window', minute=545, lots=['mbs'])]))
    # 09:00: 100 paid, 50 short. The draw at 09:05 lends 100 x 0.9 = 90 and settles at 09:35.
    assert events(result, 'shortfall')[0]['minute'] == 540
    assert result['summary']['first_shortfall_minute'] == 540              # stays recorded
    settled = events(result, 'draw_settled')[0]
    assert (settled['minute'], settled['amount']) == (575, 90.)
    assert result['payments'][0]['settled_minute'] == 600                  # paid at the next cycle, not at 09:00
    assert result['summary']['closing_cash'] == pytest.approx(40.)
    assert len(events(result, 'shortfall')) == 1


def test_collateral_must_arrive_before_it_can_be_pledged():
    move = dict(id='m', lot='ust', to='fed', minute=600, transfer_minutes=180, window_open=480, window_close=900)
    draws = [dict(id='early', facility='window', minute=700, lots=['ust']),
             dict(id='wrong_place', facility='fhlb', minute=590, lots=['ust']),
             dict(id='arrived', facility='window', minute=780, lots=['ust']),
             dict(id='twice', facility='window', minute=790, lots=['ust']),
             dict(id='not_eligible', facility='fhlb', minute=800, lots=['mbs'])]
    result = run(facility_book(moves=[move], draws=draws))
    reasons = {e['id']: e['detail'] for e in events(result, 'draw_blocked')}
    assert reasons == {'early': 'collateral_in_transit', 'wrong_place': 'collateral_not_at_facility',
                       'twice': 'already_pledged', 'not_eligible': 'collateral_not_at_facility'}
    assert [(e['event'], e['minute']) for e in result['events'] if e['id'] == 'm'] == \
        [('move_started', 600), ('move_completed', 780)]
    assert events(result, 'draw_requested')[0]['amount'] == pytest.approx(190.)   # 200 x 0.95, at 13:00
    assert events(result, 'draw_settled')[0]['minute'] == 810
    assert result['summary']['draws_blocked'] == 4


def test_custodian_cutoff_delays_the_move_to_the_next_morning():
    move = dict(id='m', lot='ust', to='fed', minute=901, transfer_minutes=60, window_open=480, window_close=900)
    late = dict(id='same_day', facility='window', minute=1000, lots=['ust'])
    next_day = dict(id='next_day', facility='window', minute=1440 + 560, lots=['ust'])
    result = run(facility_book(horizon_minute=2*1440-1, moves=[move], draws=[late, next_day]))
    assert [(e['event'], e['minute']) for e in result['events'] if e['id'] == 'm'] == \
        [('move_waiting', 901), ('move_started', 1440 + 480), ('move_completed', 1440 + 540)]
    assert events(result, 'draw_blocked')[0]['detail'] == 'collateral_not_at_facility'
    assert events(result, 'draw_requested')[0]['id'] == 'next_day'


def test_outages_closed_windows_capacity_and_the_cash_condition_block_draws():
    request = facility_book(draws=[
        dict(id='outage', facility='window', minute=600, lots=['mbs']),
        dict(id='closed', facility='window', minute=1100, lots=['mbs']),
        dict(id='not_needed', facility='window', minute=700, lots=['mbs'], only_if_cash_below=50.),
        dict(id='capped', facility='window', minute=710, lots=['mbs'], amount=20.)])
    request['facilities'][0]['outages'] = [[590, 620]]
    request['facilities'][0]['capacity'] = 15.
    result = run(request)
    assert {e['id']: e['detail'] for e in events(result, 'draw_blocked')} == \
        {'outage': 'outage', 'closed': 'facility_closed', 'not_needed': 'not_needed'}
    assert events(result, 'draw_requested')[0]['amount'] == 15.            # facility capacity binds before 20 or 90


def test_legal_netting_needs_only_the_net_amount():
    netted = book(netting_sets=['ccp'], payments=[pay('out', 500, 300., netting_set='ccp')],
                  receipts=[dict(id='in', minute=530, amount=250., system='wire', netting_set='ccp')])
    result = run(netted)
    assert not events(result, 'shortfall')
    assert result['payments'][0]['settled_minute'] == 540
    assert result['summary']['closing_cash'] == pytest.approx(50.)         # 100 - (300 - 250)
    assert min(e['cash'] for e in result['events']) >= 0.
    # The same flows without legal netting: the gross payment is short until the receipt is cash.
    gross = book(payments=[pay('out', 500, 300.)], receipts=[dict(id='in', minute=570, amount=250.)])
    result = run(gross)
    assert events(result, 'shortfall')[0]['minute'] == 540
    assert result['payments'][0]['settled_minute'] == 600
    # A net amount larger than cash waits whole; no partial netting.
    waiting = deepcopy(netted)
    waiting['opening_cash'] = 40.
    result = run(waiting)
    assert events(result, 'netting_waiting')[0]['amount'] == pytest.approx(50.)
    assert events(result, 'netting_waiting')[0]['detail'] == 'cash'
    assert result['summary']['paid'] == 0. and result['summary']['received'] == 0.
    assert result['summary']['first_shortfall_minute'] == 540 and result['summary']['shortfall_cycles'] == 9
    # A waiting set does not hold up an unrelated payment behind it.
    waiting['payments'].append(pay('other', 510, 30.))
    result = run(waiting)
    assert {p['id']: p['paid'] for p in result['payments']} == {'out': 0., 'other': 30.}


def test_stop_separates_requested_paid_and_unpaid_and_ends_everything():
    request = facility_book(stop_minute=620, payments=[pay('a', 500, 150.), pay('never', 630, 1000.)],
                            draws=[dict(id='d', facility='window', minute=600, lots=['mbs'])],
                            receipts=[dict(id='late_cash', minute=625, amount=500.)])
    result = run(request)
    summary = result['summary']
    assert summary['requested'] == 150. and summary['paid'] == 100. and summary['unpaid'] == 50.
    assert summary['draws_unsettled'] == pytest.approx(90.) and summary['drawn'] == 0.   # settles at 10:30, after the stop
    assert summary['received'] == 0.
    never = result['payments'][1]
    assert never['requested'] is False and never['paid'] == 0. and never['unpaid'] == 0.
    assert result['events'][-1]['event'] == 'stopped' and result['events'][-1]['minute'] == 620
    assert max(e['minute'] for e in result['events'][:-1]) < 620


def test_later_requests_never_change_earlier_events():
    request = facility_book(payments=[pay('a', 500, 150.)],
                            draws=[dict(id='d', facility='window', minute=545, lots=['mbs'])])
    before = run(request)
    request['payments'].append(pay('later', 700, 5000.))
    request['draws'].append(dict(id='d2', facility='fhlb', minute=720, lots=['ust']))
    request['receipts'] = [dict(id='r', minute=800, amount=10.)]
    after = run(request)
    prefix = [e for e in after['events'] if e['minute'] < 700]
    assert prefix == [e for e in before['events'] if e['minute'] < 700]


@pytest.mark.parametrize('seed', range(40))
def test_random_books_agree_with_the_reference_and_reconcile(seed):
    rng = random.Random(seed)
    systems = [dict(id='wire', open=540, close=1020, cycle_minutes=rng.choice([15, 60, 240]),
                    capacity_per_cycle=rng.choice([None, 80., 300.]), closed_days=rng.choice([[], [1]])),
               dict(id='ach', open=480, close=600, cycle_minutes=120)]
    sets = ['n1', 'n2']
    payments = [dict(id=f'p{i}', minute=rng.randrange(0, 2800), amount=float(rng.randrange(1, 120)),
                     system=rng.choice(['wire', 'ach']), netting_set=rng.choice([None, None, 'n1', 'n2']))
                for i in range(rng.randrange(5, 40))]
    receipts = []
    for i in range(rng.randrange(0, 12)):
        netted = rng.random() < .5
        receipts.append(dict(id=f'r{i}', minute=rng.randrange(0, 2800), amount=float(rng.randrange(1, 150)),
                             **(dict(system=rng.choice(['wire', 'ach']), netting_set=rng.choice(sets)) if netted else {})))
    lots = [dict(id=f'l{i}', value=float(rng.randrange(20, 200)), location=rng.choice(['fed', 'custodian', 'fhlb']),
                 haircuts={k: rng.choice([.05, .2]) for k in rng.sample(['window', 'fhlb'], rng.randrange(0, 3))})
            for i in range(6)]
    facilities = [dict(id='window', location='fed', open=480, close=1080, settlement_lag_minutes=rng.choice([0, 30, 400]),
                       capacity=float(rng.randrange(50, 400)), outages=rng.choice([[], [[600, 700]]])),
                  dict(id='fhlb', location='fhlb', open=480, close=960, settlement_lag_minutes=120, capacity=300.)]
    moves = [dict(id=f'm{i}', lot=rng.choice(lots)['id'], to=rng.choice(['fed', 'fhlb', 'custodian']),
                  minute=rng.randrange(0, 2800), transfer_minutes=rng.choice([0, 45, 600]), window_open=480, window_close=900)
             for i in range(rng.randrange(0, 6))]
    draws = [dict(id=f'd{i}', facility=rng.choice(['window', 'fhlb']), minute=rng.randrange(0, 2800),
                  lots=rng.sample([l['id'] for l in lots], rng.randrange(1, 3)),
                  **rng.choice([{}, dict(amount=30.), dict(only_if_cash_below=60.)]))
             for i in range(rng.randrange(0, 8))]
    request = dict(schema='intraday-settlement-1', opening_cash=float(rng.randrange(0, 400)), horizon_minute=2879,
                   systems=systems, payments=payments, receipts=receipts, netting_sets=sets, lots=lots,
                   facilities=facilities, moves=moves, draws=draws)
    if rng.random() < .4:
        request['stop_minute'] = rng.randrange(1, 2879)
    result = run(request)
    summary = result['summary']
    assert abs(summary['reconciliation_error']) < 1e-9
    assert summary['requested'] == pytest.approx(summary['paid'] + summary['unpaid'])
    assert min(e['cash'] for e in result['events']) >= -1e-9
    assert all(e['minute'] <= f['minute'] for e, f in zip(result['events'], result['events'][1:]))
    pledged = [l for d in events(result, 'draw_requested') for l in next(x for x in draws if x['id'] == d['id'])['lots']]
    assert len(pledged) == len(set(pledged))                               # no lot is pledged twice


@pytest.mark.parametrize('change, message', [
    (dict(schema='intraday-settlement-2'), 'must select'),
    (dict(opening_cash=-1.), 'opening cash'),
    (dict(horizon_minute=15*1440), 'horizon'),
    (dict(stop_minute=5000), 'horizon'),
    (dict(systems=[dict(WIRE, cycle_minutes=0)]), 'system wire'),
    (dict(systems=[dict(WIRE, open=900, close=600)]), 'system wire'),
    (dict(payments=[pay('a', 10, 0.)]), 'payment a'),
    (dict(payments=[pay('a', 10, 5.), pay('a', 11, 5.)]), 'unique'),
    (dict(payments=[dict(id='a', minute=10, amount=5., system='swift')]), 'payment a'),
    (dict(payments=[pay('a', 10, 5., netting_set='unlisted')]), 'payment a'),
    (dict(receipts=[dict(id='r', minute=10, amount=5., netting_set='ccp')], netting_sets=['ccp']), 'receipt r'),
    (dict(draws=[dict(id='d', facility='window', minute=10, lots=['ghost'])]), 'draw d'),
    (dict(unknown=1), 'unknown field'),
])
def test_malformed_requests_are_rejected(change, message):
    request = facility_book() | change
    with run_context(RUST), pytest.raises(ValueError, match=message):
        run_intraday_settlement(request)


def test_cancellation_and_the_reference_boundary(monkeypatch):
    # The production path never calls the Python reference.
    def forbidden(*args, **kwargs):
        raise AssertionError('Python financial stage called')
    monkeypatch.setattr(reference, 'replay', forbidden)
    with run_context(RUST):
        result = run_intraday_settlement(book(payments=[pay('a', 500, 60.)]))
    assert result['summary']['paid'] == 60.
    with run_context(replace(RUST, compute_backend='python')), pytest.raises(AssertionError):
        run_intraday_settlement(book())
