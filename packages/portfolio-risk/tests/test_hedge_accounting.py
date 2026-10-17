"""Independent hand values, native ownership, lifecycle cases and tamper controls for hedge-accounting-1.

Fixture: scripts/make_hedge_accounting_example.py (daily nodes 1 Jan-1 Apr 2024, three ACT/360
periods, 6% simple curve dropping to 4% on 1 February, floating 6.5% / 4% / 4.5%).
"""
from copy import deepcopy
import datetime as dt
import json
from pathlib import Path
import subprocess
import sys
import pytest
from portfolio_risk.core import model_native
from portfolio_risk.analytics.model_contracts import account_hedge_relationships
from portfolio_risk.analytics.hedge_accounting_reference import verify

ROOT = Path(__file__).resolve().parents[3]
JAN1, FEB1, MAR1, APR1 = (dt.date(2024, m, 1).toordinal() for m in (1, 2, 3, 4))
PERIODS = [(JAN1, FEB1), (FEB1, MAR1), (MAR1, APR1)]


def raw():
    return json.loads((ROOT / 'examples/models/hedge-accounting-1.json').read_text())


def run(request):
    return account_hedge_relationships(request)


def df(rate, days):
    return 1. / (1. + rate * days / 360.)


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


def swap_value(t, fixed):
    r = curve(t)
    return sum((floating(i, t) - fixed) * 100. * (e - s) / 360. * df(r, e - t) for i, (s, e) in enumerate(PERIODS) if e > t)


def cfh_fixed(request):
    return request['derivative']['derivative']['operation']['request']['paths'][0]['nodes'][0]['trades'][1]['request']['fixed_rate']


def event(kind, rid, ordinal, sequence, **fields):
    return dict(event_id=f'{kind}-{rid}-{ordinal}-{sequence}', relationship_id=rid, ordinal=ordinal, sequence=sequence, kind=kind,
                review=dict(id='review', revision='r1'), **fields)


def near(a, b):
    assert abs(a - b) <= 1e-9 * (1. + max(abs(a), abs(b))), (a, b)


@pytest.fixture(scope='module')
def accepted():
    request = raw()
    return request, run(request)


def test_example_hand_values_verify_independently(accepted):
    request, result = accepted
    receipt = verify(request, result)
    assert receipt['verified'] and receipt['formal_hedge_relationships_replayed'] and receipt['accounting_standard_approval'] is False
    jan1 = item_unit(JAN1)
    for a, ordinal in zip(result['assessments'][:2], (FEB1, MAR1)):
        d = swap_value(ordinal, .06) - swap_value(JAN1, .06); e = -100. * (item_unit(ordinal) - jan1)
        assert (a['relationship_id'], a['ordinal'], a['qualified']) == ('fvh-1', ordinal, True)
        near(a['cumulative_instrument_change'], d); near(a['cumulative_hedged_change'], e); near(a['ratio'], d / e)
        assert .8 <= a['ratio'] <= 1.25
    k = cfh_fixed(request); cfh = result['assessments'][2]
    assert (cfh['relationship_id'], cfh['ordinal'], cfh['qualified']) == ('cfh-1', MAR1, True)
    near(cfh['ratio'], 1.); assert abs(swap_value(FEB1, k)) < 1e-12
    near(result['closing'][0]['basis_closing'], 100. * (1. - jan1))
    s2, s3 = (.04 - k) * 100. * 29. / 360., (.045 - k) * 100. * 31. / 360.
    near(result['closing'][1]['oci_recognized'], s2 + s3); near(result['closing'][1]['oci_reclassified'], s2 + s3)
    near(result['closing'][1]['oci_closing'], 0.)
    reclass = [i for i in result['intents'] if i['event_id'].startswith('cash_flow_hedge_reclassification')]
    assert [i['ordinal'] for i in reclass] == [MAR1, APR1]
    marks = [i for i in result['intents'] if i['event_id'].startswith('clean_mark') and i['mapped_position_id'] == 'swap-cfh' and i['ordinal'] > FEB1]
    assert marks and all(p['account'] != 'mark_gain_loss' for i in marks for p in i['postings'])
    assert all(rel['status'] == 'qualified' for rel in result['relationships'])
    assert result['saved_book_adoption'] is False and result['persisted_journal_verified'] is False


def test_partial_dedesignation_sale_and_forecast_outcomes_verify_independently():
    request = raw(); request['events'] = [event('assess', 'fvh-1', FEB1, 0), event('discontinue', 'fvh-1', FEB1, 1, reason='dedesignation', proportion=.5)]
    result = run(request); assert verify(request, result)['verified']
    basis_feb1 = 100. * (item_unit(FEB1) - item_unit(JAN1))
    record = next(l for l in result['lifecycle'] if l['kind'] == 'discontinuation')
    near(record['frozen_basis'], .5 * basis_feb1); near(record['proportion_after'], .5)
    mid = next(d for d in result['daily'] if d['ordinal'] == FEB1 + 14)
    near(mid['relationships'][0]['frozen_basis_balance'], .5 * basis_feb1 * (1. - 14. / 60.))
    near(result['closing'][0]['basis_amortized'], .5 * basis_feb1); near(result['closing'][0]['basis_closing'], .5 * 100. * (1. - item_unit(JAN1)))
    sold = raw(); sold['events'] = [event('discontinue', 'fvh-1', FEB1, 0, reason='hedged_item_sold')]
    result = run(sold); assert verify(sold, result)['verified']
    near(result['closing'][0]['basis_derecognized'], basis_feb1); near(result['closing'][0]['basis_closing'], 0.)
    assert result['relationships'][0]['status'] == 'discontinued'
    pnto = raw(); k = cfh_fixed(pnto); pnto['events'] = [event('forecast_status', 'cfh-1', FEB1 + 9, 0, status='probable_not_to_occur')]
    result = run(pnto); assert verify(pnto, result)['verified']
    oci = swap_value(FEB1 + 9, k) - swap_value(FEB1, k)
    record = next(l for l in result['lifecycle'] if l['kind'] == 'forecast_status')
    near(record['released_oci'], oci); assert record['status_after'] == 'discontinued'
    assert not any(i['event_id'].startswith('cash_flow_hedge_reclassification') for i in result['intents'])
    near(result['closing'][1]['oci_closing'], 0.)
    nlp = raw(); nlp['events'] = [event('forecast_status', 'cfh-1', FEB1 + 9, 0, status='no_longer_probable')]
    result = run(nlp); assert verify(nlp, result)['verified']
    releases = [i for i in result['intents'] if i['event_id'].startswith('frozen_oci_release')]
    assert [i['ordinal'] for i in releases] == [MAR1, APR1]
    near(releases[0]['postings'][0]['debit_amount'], oci * 29. / 60.); near(result['closing'][1]['oci_closing'], 0.)


def test_narrow_band_fails_assessment_and_supplied_hypothetical_matches_matched_terms():
    narrow = raw(); narrow['events'] = [event('assess', 'fvh-1', FEB1, 0), event('assess', 'cfh-1', MAR1, 0)]
    narrow['policy']['assessment'].update(lower_bound=.99, upper_bound=1.01)
    result = run(narrow); assert verify(narrow, result)['verified']
    assert result['assessments'][0]['qualified'] is False and result['assessments'][1]['qualified'] is True
    record = next(l for l in result['lifecycle'] if l['relationship_id'] == 'fvh-1' and l['kind'] == 'assessment')
    assert (record['reason'], record['status_after']) == ('failed_assessment', 'discontinued')
    near(result['closing'][0]['basis_amortized'], record['frozen_basis']); near(result['relationships'][0]['frozen_basis_balance'], 0.)
    hyp = raw(); nodes = hyp['derivative']['derivative']['operation']['request']['paths'][0]['nodes']
    hyp['relationships'][1].update(matched_critical_terms_evidence=None, hypothetical=dict(evidence=dict(id='hyp-evidence', revision='r1'), nodes=[
        dict(ordinal=n['ordinal'], request=dict(deepcopy(n['trades'][1]['request']), position_id='hypothetical')) for n in nodes if n['ordinal'] >= FEB1]))
    supplied = run(hyp); assert verify(hyp, supplied)['verified']
    matched = run(raw())
    near(supplied['assessments'][2]['ratio'], 1.); near(supplied['closing'][1]['oci_reclassified'], matched['closing'][1]['oci_reclassified'])
    assert len(supplied['intents']) == len(matched['intents'])
    for n in hyp['relationships'][1]['hypothetical']['nodes']: n['request']['fixed_rate'] = .05
    with pytest.raises(ValueError, match='zero clean value at inception'): run(hyp)


@pytest.mark.parametrize('target', ['ratio', 'qualified', 'posting', 'drop', 'status', 'oci', 'flag', 'derivative', 'order', 'proportion', 'raw'])
def test_independent_replay_rejects_tampered_results(accepted, target):
    request, result = accepted; changed = deepcopy(result); raw_request = request
    if target == 'ratio': changed['assessments'][0]['ratio'] += 1e-6
    elif target == 'qualified': changed['assessments'][0]['qualified'] = False
    elif target == 'posting': changed['intents'][-1]['postings'][0]['debit_amount'] += 1e-6
    elif target == 'drop': changed['intents'].pop()
    elif target == 'status': changed['relationships'][1]['status'] = 'discontinued'
    elif target == 'oci': changed['daily'][-1]['relationships'][1]['oci_balance'] += 1e-6
    elif target == 'flag': changed['accounting_standard_approval'] = True
    elif target == 'derivative': changed['derivative']['daily'][5]['trades'][0]['clean_carrying'] += 1e-6
    elif target == 'order': changed['intents'][-1]['postings'].reverse()
    elif target == 'proportion': changed['closing'][0]['basis_closing'] += 1e-6
    else:
        raw_request = deepcopy(request); raw_request['relationships'][0]['benchmark_rate'] = .05
    with pytest.raises((ValueError, ArithmeticError)): verify(raw_request, changed)


@pytest.mark.parametrize('kind', ['rule', 'election', 'inception', 'accrual', 'proportion', 'matched', 'order', 'coupon'])
def test_native_refusals_without_fallback(kind):
    request = raw()
    if kind == 'rule': request['policy']['rule_version'] = 'asc815-2024'
    elif kind == 'election': request['policy']['adoption']['asu_2025_09']['elections']['dual_hedge_basis'] = True
    elif kind == 'inception': request['events'][0]['ordinal'] = JAN1
    elif kind == 'accrual': request['events'][0]['ordinal'] = FEB1 + 3
    elif kind == 'proportion': request['relationships'][0]['designated_proportion'] = 1.5
    elif kind == 'matched':
        v0 = swap_value(JAN1, .06)
        for n in request['derivative']['derivative']['operation']['request']['paths'][0]['nodes']: n['trades'][1]['request']['fixed_rate'] = .06
        request['derivative']['opening']['trades'][1]['clean_value'] = v0
        request['derivative']['opening']['equity_credit_balance'] = 100. + 2. * v0
    elif kind == 'order': request['events'].reverse()
    else: request['derivative']['policy']['trades'][1]['coupon_recognition'] = 'contractual_income_at_due_with_mark_offset'
    with pytest.raises(ValueError): run(request)


def test_hedge_process_is_native_with_python_financial_callbacks_forbidden():
    script = '''
from portfolio_risk.core import quant_native, pricing
def forbidden(*a,**k):raise AssertionError('Python financial callback')
quant_native.term_call=forbidden
pricing.pv_from_A=forbidden
pricing.solve_oas_from_A=forbidden
import json
from pathlib import Path
from portfolio_risk.analytics.model_contracts import account_hedge_relationships
r=json.loads(Path('examples/models/hedge-accounting-1.json').read_text())
out=account_hedge_relationships(r)
assert len(out['assessments'])==3 and all(a['qualified'] for a in out['assessments'])
'''
    child = subprocess.run([sys.executable, '-c', script], cwd=ROOT, capture_output=True, text=True)
    assert child.returncode == 0, child.stderr


def test_independent_replay_cancellation(accepted):
    request, result = accepted
    def stop(): raise InterruptedError('cancelled during reference replay')
    with pytest.raises(InterruptedError): verify(request, result, stop)


def test_native_strict_scalar_categories_and_duplicate_keys():
    request = raw(); envelope = dict(schema=request['schema'], threads=1, execution_timeout_ms=60000, request=request)
    changed = deepcopy(envelope); changed['execution_timeout_ms'] = True
    child = subprocess.run([str(model_native.binary_path())], input=json.dumps(changed).encode(), capture_output=True)
    assert child.returncode == 1 and json.loads(child.stdout)['ok'] is False
    body = json.dumps(envelope).replace('"rule_version": "asc815-asu2017-12"', '"rule_version": "x", "rule_version": "asc815-asu2017-12"', 1).encode()
    assert body != json.dumps(envelope).encode()
    child = subprocess.run([str(model_native.binary_path())], input=body, capture_output=True)
    assert child.returncode == 1 and json.loads(child.stdout)['ok'] is False


def test_integer_categories_for_typed_money_verify_but_clock_categories_stay_exact():
    """Browser JSON re-serialization turns 1.0 into 1; f64 fields accept it, dates and sequences do not change."""
    request = raw()
    request['relationships'][0]['designated_proportion'] = 1; request['relationships'][1]['designated_proportion'] = 1
    request['relationships'][0]['hedged_notional'] = 100; request['hedged_items'][0]['notional'] = 100
    request['forecast_exposures'][0]['notional'] = 100; request['policy']['assessment']['upper_bound'] = 2
    result = run(request); assert verify(request, result)['verified']
    assert result['relationships'][0]['designated_proportion'] == 1.
    changed = deepcopy(result); changed['lifecycle'][0]['ordinal'] = float(changed['lifecycle'][0]['ordinal'])
    with pytest.raises(ValueError): verify(request, changed)
