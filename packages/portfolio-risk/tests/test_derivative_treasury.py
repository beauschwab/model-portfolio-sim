"""Capital from the standalone derivative journal: independent rows/capital replay, interim selection and refusals."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import pytest
from portfolio_risk.analytics.model_contracts import prepare_derivative_treasury
from portfolio_risk.analytics.derivative_treasury_reference import verify

ROOT = Path(__file__).resolve().parents[3]


def raw():
    return json.loads((ROOT / 'examples/models/derivative-treasury-1.json').read_text())


@pytest.fixture(scope='module')
def accepted():
    request = raw()
    return request, prepare_derivative_treasury(request)


def row(result, gl):
    return sum(r['balance'] for r in result['rows'] if r['gl_account'] == gl)


def test_rows_and_capital_verify_independently_without_hedge_accounting(accepted):
    request, result = accepted
    receipt = verify(request, result)
    assert receipt['verified'] and receipt['derivative_closing_capital_recomputed'] and receipt['hedge_accounting_applied'] is False
    assert result['closing_date'] == '2024-04-01' and result['interim_capital'] is False and result['hedge_accounting_applied'] is False
    closing = {b['account']: b['debit_balance'] for b in result['derivative']['daily'][-1]['financial_trial_balance']}
    assert abs(row(result, 'cash') - closing['cash']) < 1e-9 and abs(row(result, 'pnl:derivative_mark') - closing['mark_gain_loss']) < 1e-9
    assert all('hedge' not in r['gl_account'] for r in result['rows'])
    audit = result['capital']['capital_bridge'][0]
    assert audit['book_aoci'] == 0. and audit['eligible_aoci'] == 0. and len(result['capital']['capital_metrics']) == 13


def test_interim_report_reconciles_to_the_reporting_node():
    request = raw(); request.update(interim_report=True); request['treasury']['as_of'] = '2024-03-01'; request['policies'][0]['capital']['period'] = '2024-03-01'
    result = prepare_derivative_treasury(request); receipt = verify(request, result)
    assert receipt['interim_capital'] is True and result['closing_date'] == '2024-03-01'
    day = next(d for d in result['derivative']['daily'] if d['ordinal'] == result['closing_ordinal'])
    balances = {b['account']: b['debit_balance'] for b in day['financial_trial_balance']}
    assert abs(row(result, 'derivative_value') - balances.get('derivative_asset', 0.) - balances.get('derivative_liability', 0.)) < 1e-9
    untreated = raw(); untreated['policies'][0].pop('aoci_eligibility')
    result = prepare_derivative_treasury(untreated); assert verify(untreated, result)['verified']
    assert 'aoci_opt_out_elected' not in result['capital']['capital_bridge'][0]


@pytest.mark.parametrize('target', ['row', 'audit', 'flag', 'derivative', 'interim', 'raw'])
def test_independent_replay_rejects_tampered_results(accepted, target):
    request, result = accepted; changed = deepcopy(result); raw_request = request
    if target == 'row': changed['rows'][0]['balance'] += 1e-6
    elif target == 'audit': changed['capital']['capital_bridge'][0]['retained_earnings'] += 1e-6
    elif target == 'flag': changed['hedge_accounting_applied'] = True
    elif target == 'derivative': changed['derivative']['daily'][5]['trades'][0]['clean_carrying'] += 1e-6
    elif target == 'interim': changed['interim_capital'] = True
    else:
        raw_request = deepcopy(request); raw_request['amount_multiplier'] = 2.
    with pytest.raises((ValueError, ArithmeticError)): verify(raw_request, changed)


@pytest.mark.parametrize('kind', ['as_of', 'period', 'scenario', 'multiplier', 'capital', 'weight', 'unknown', 'interim_tail'])
def test_native_refusals(kind):
    request = raw()
    if kind == 'as_of': request['treasury']['as_of'] = '2024-03-01'
    elif kind == 'period': request['policies'][0]['capital']['period'] = '2024-03-01'
    elif kind == 'scenario': request['policies'][0]['scenario'] = 'other'
    elif kind == 'multiplier': request['amount_multiplier'] = -1.
    elif kind == 'capital': request['treasury']['capital'] = [dict(entity='bank')]
    elif kind == 'weight': request['policies'][0]['gl_risk_weights']['not_an_asset_gl'] = 1.
    elif kind == 'unknown': request['hedge'] = {}
    else: request['interim_report'] = True
    with pytest.raises(ValueError): prepare_derivative_treasury(request)


def test_treasury_process_is_native_with_python_financial_callbacks_forbidden():
    script = '''
from portfolio_risk.core import quant_native, pricing
def forbidden(*a,**k):raise AssertionError('Python financial callback')
quant_native.term_call=forbidden
pricing.pv_from_A=forbidden
pricing.solve_oas_from_A=forbidden
import json
from pathlib import Path
from portfolio_risk.analytics.model_contracts import prepare_derivative_treasury
r=json.loads(Path('examples/models/derivative-treasury-1.json').read_text())
out=prepare_derivative_treasury(r)
assert out['hedge_accounting_applied'] is False and len(out['capital']['capital_metrics'])==13
'''
    child = subprocess.run([sys.executable, '-c', script], cwd=ROOT, capture_output=True, text=True)
    assert child.returncode == 0, child.stderr
