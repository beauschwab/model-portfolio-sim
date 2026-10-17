"""Closing capital from the hedge journal: independent rows/capital replay, policy choices and refusals."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import pytest
from portfolio_risk.analytics.model_contracts import prepare_hedge_treasury
from portfolio_risk.analytics.hedge_treasury_reference import verify

ROOT = Path(__file__).resolve().parents[3]


def raw():
    return json.loads((ROOT / 'examples/models/hedge-treasury-1.json').read_text())


@pytest.fixture(scope='module')
def accepted():
    request = raw()
    return request, prepare_hedge_treasury(request)


def row(result, gl):
    return sum(r['balance'] for r in result['rows'] if r['gl_account'] == gl)


def test_closing_rows_and_capital_verify_independently(accepted):
    request, result = accepted
    receipt = verify(request, result)
    assert receipt['closing_capital_recomputed'] and receipt['cash_flow_hedge_aoci_eligible'] is False and receipt['accounting_standard_approval'] is False
    closing = {b['account']: b['debit_balance'] for b in result['hedge']['daily'][-1]['financial_trial_balance']}
    assert abs(row(result, 'cash') - closing['cash']) < 1e-9
    oci = row(result, 'cash_flow_hedge_oci')
    assert abs(oci) > 1e-6 and abs(oci - closing['cash_flow_hedge_oci']) < 1e-9
    audit = result['capital']['capital_bridge'][0]
    assert abs(audit['cash_flow_hedge_aoci'] + oci) < 1e-9 and audit['eligible_aoci'] == 0.
    assert result['closing_date'] == '2024-04-01' and result['ftp_selected'] is False and result['interim_capital'] is False
    assert len(result['capital']['capital_metrics']) == 13 and result['capital']['ftp_positions'] == []
    assert result['hedge']['relationships'][1]['pending_reclassification_periods'] == ['e4']


def test_reviewed_policy_makes_cash_flow_hedge_aoci_eligible():
    request = raw(); request['policies'][0]['cash_flow_hedge_aoci_eligible'] = True
    result = prepare_hedge_treasury(request); receipt = verify(request, result)
    assert receipt['cash_flow_hedge_aoci_eligible'] is True
    audit = result['capital']['capital_bridge'][0]
    assert abs(audit['eligible_aoci'] - audit['book_aoci']) < 1e-9 and abs(audit['book_aoci']) > 1e-6


@pytest.mark.parametrize('target', ['row', 'audit', 'metric', 'flag', 'eligible', 'hedge', 'policy'])
def test_independent_replay_rejects_tampered_results(accepted, target):
    request, result = accepted; changed = deepcopy(result); raw_request = request
    if target == 'row': changed['rows'][0]['balance'] += 1e-6
    elif target == 'audit': changed['capital']['capital_bridge'][0]['eligible_aoci'] += 1e-6
    elif target == 'metric': changed['capital']['capital_metrics'][0]['status'] = 'pass' if changed['capital']['capital_metrics'][0]['status'] != 'pass' else 'breach'
    elif target == 'flag': changed['accounting_standard_approval'] = True
    elif target == 'eligible': changed['cash_flow_hedge_aoci_eligible'] = True
    elif target == 'hedge': changed['hedge']['closing'][1]['oci_closing'] += 1e-6
    else:
        raw_request = deepcopy(request); raw_request['policies'][0]['include_aoci'] = False
    with pytest.raises((ValueError, ArithmeticError)): verify(raw_request, changed)


@pytest.mark.parametrize('kind', ['as_of', 'period', 'scenario', 'multiplier', 'capital', 'weight', 'basis_weight', 'unknown'])
def test_native_refusals(kind):
    request = raw()
    if kind == 'as_of': request['treasury']['as_of'] = '2024-03-01'
    elif kind == 'period': request['policies'][0]['capital']['period'] = '2024-03-01'
    elif kind == 'scenario': request['policies'][0]['scenario'] = 'stress'
    elif kind == 'multiplier': request['amount_multiplier'] = 0.
    elif kind == 'capital': request['treasury']['capital'] = [dict(entity='bank')]
    elif kind == 'weight': request['policies'][0]['gl_risk_weights'] = {'cash': 0.}
    elif kind == 'basis_weight': request['policies'][0]['gl_risk_weights']['hedged_item_basis_adjustment'] = 1.
    else: request['policies'][0]['ftp'] = True
    with pytest.raises(ValueError): prepare_hedge_treasury(request)


def test_treasury_process_is_native_with_python_financial_callbacks_forbidden():
    script = '''
from portfolio_risk.core import quant_native, pricing
def forbidden(*a,**k):raise AssertionError('Python financial callback')
quant_native.term_call=forbidden
pricing.pv_from_A=forbidden
pricing.solve_oas_from_A=forbidden
import json
from pathlib import Path
from portfolio_risk.analytics.model_contracts import prepare_hedge_treasury
out=prepare_hedge_treasury(json.loads(Path('examples/models/hedge-treasury-1.json').read_text()))
assert len(out['capital']['capital_metrics'])==13
'''
    child = subprocess.run([sys.executable, '-c', script], cwd=ROOT, capture_output=True, text=True)
    assert child.returncode == 0, child.stderr


def test_independent_replay_cancellation(accepted):
    request, result = accepted
    def stop(): raise InterruptedError('cancelled')
    with pytest.raises(InterruptedError): verify(request, result, stop)


def test_integer_categories_from_browser_json_verify():
    """Re-serialized policy numbers (0 for 0.0) are f64 fields; the echo and audit stay comparable."""
    request = raw(); p = request['policies'][0]
    for key in ('equity_exclusions', 'intangible_assets', 'leverage_addon', 'leverage_deductions'): p[key] = 0
    p['gl_risk_weights'] = {k: int(v) for k, v in p['gl_risk_weights'].items()}
    p['capital']['common_equity'] = 0; p['capital']['requirements']['cet1']['buffer'] = 0
    request['amount_multiplier'] = 1
    result = prepare_hedge_treasury(request); assert verify(request, result)['closing_capital_recomputed']
