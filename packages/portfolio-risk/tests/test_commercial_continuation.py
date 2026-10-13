"""Complete-origin holding windows, gross rights, reviewed credit and CRN gates."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import pytest
from portfolio_risk.core import model_native
from portfolio_risk.analytics.commercial_continuation_reference import verify

ROOT = Path(__file__).resolve().parents[3]

def raw():
    return json.loads((ROOT/'examples/models/commercial-holding-continuation-1.json').read_text())

@pytest.fixture(scope='module')
def accepted():
    request = raw(); result, _ = model_native.run(request['schema'], request)
    return request, result

def test_commercial_holding_replays_original_yield_and_actual_postings(accepted):
    request, result = accepted
    assert verify(request, result)['verified']
    opening = result['openings'][0]
    assert opening['book']['legal_interest'] == pytest.approx(100.*.1*15./365., abs=1e-12)
    assert opening['book']['principal'] == 100.
    assert opening['book']['gross_carry'] != request['completed_source']['source']['acquisition']['cost']
    groups = result['revised']['accounting']['groups']
    assert opening['subsequent_cash'] == sum(groups[i]['cash'] for i in opening['subsequent_native_group_indices'])
    assert all(groups[i]['year'] > 15./365. for i in opening['subsequent_native_group_indices'])
    assert result['revised']['raw_request']['source']['acquisition'] == result['completed']['raw_request']['source']['acquisition']

@pytest.mark.parametrize('side', ['completed_source', 'revised_source'])
def test_commercial_holding_optional_empty_forms_use_native_dto_identity(side):
    request = raw(); request[side]['performing_draw_tranches'] = []
    result, _ = model_native.run(request['schema'], request)
    assert verify(request, result)['verified']

@pytest.mark.parametrize('kind', ['draw', 'nonaccrual', 'reviewed', 'post_principal'])
def test_commercial_holding_retains_draw_yields_gross_rights_and_reviewed_default(kind):
    from test_commercial_reference import draw_request, reviewed_request, request as base
    if kind == 'draw': source, _ = draw_request(); cut = 15
    elif kind == 'reviewed': source = reviewed_request(); cut = 10
    else:
        source = base(); cut = 15
        if kind == 'nonaccrual': source['accounting']['recognition'][0]['by_origin'] = ['legal_only_income_stopped']*3
        else:
            for role in ('original', 'current'):
                source['source'][role]['periods'][0]['payment_ordinal'] = source['source'][role]['asof_ordinal']+10
    request = dict(schema=raw()['schema'], cut_ordinal=source['source']['current']['asof_ordinal']+cut,
                   completed_source=source, revised_source=deepcopy(source))
    result, _ = model_native.run(request['schema'], request); assert verify(request, result)['verified']
    opening = result['openings'][0]
    if kind == 'draw':
        assert opening['book']['principal'] == 120. and opening['carry']['undrawn_commitment'] == 20.
        assert opening['carry']['draw_tranches'][0]['original_annual_effective_rate'] == .07
    elif kind == 'reviewed':
        assert opening['book']['in_default'] and opening['book']['reviewed_credit_state']['legal_principal'] == 90.
        assert any(result['revised']['accounting']['groups'][i]['phase'] == 'default_actual_cash' for i in opening['subsequent_native_group_indices'])
    elif kind == 'post_principal':
        assert opening['book']['principal'] == 0. and opening['book']['legal_interest'] < 0.
    else:
        assert opening['book']['legal_interest'] > 0. and opening['book']['recognized_interest'] == 0.

def test_commercial_future_hazard_change_keeps_elapsed_original_waiting_hazard():
    import math
    request = raw()
    for field in ('completed_source', 'revised_source'):
        credit = request[field]['credit']
        for interval in credit['intervals']: interval['hazards_per_year'][0][3] = 10.
        credit['draw_stream'] = dict(kind='supplied', stream_id='retained-wait', stream_revision='1',
            paths=[dict(path_key=17, pairs=[dict(waiting_uniform=math.exp(-10.*20./365.), destination_uniform=.5)])])
    request['revised_source']['credit']['intervals'][1]['hazards_per_year'][0][3] = 20.
    result, _ = model_native.run(request['schema'], request); assert verify(request, result)['verified']
    original = result['completed']['carry']['credit']['result']['paths'][0]['default']['year']
    revised = result['revised']['carry']['credit']['result']['paths'][0]['default']['year']
    assert original == pytest.approx(20./365., abs=1e-14)
    assert revised == pytest.approx(17.5/365., abs=1e-14)

@pytest.mark.parametrize('target', ['principal', 'rights', 'carry', 'index', 'cash', 'original_cost', 'prefix', 'clock', 'opening_evidence'])
def test_commercial_holding_independent_receipt_tampering_rejects(accepted, target):
    request, result = deepcopy(accepted); opening = result['openings'][0]
    if target == 'principal': opening['book']['principal'] += 1.
    elif target == 'rights': opening['book']['lots'][0]['legal_remaining'] += 1.
    elif target == 'carry': opening['carry']['gross_carry'] += 1.
    elif target == 'index': opening['subsequent_native_group_indices'].pop()
    elif target == 'cash': opening['subsequent_cash'] += 1.
    elif target == 'original_cost': result['revised']['raw_request']['source']['acquisition']['cost'] += 1.
    elif target == 'prefix': result['completed']['accounting']['groups'][1]['next']['principal'] += 1.
    elif target == 'clock': result['credit_clock_restarted'] = True
    else: result['verified_prior_publication'] = True
    with pytest.raises((ArithmeticError, ValueError, KeyError)): verify(request, result)

@pytest.mark.parametrize('target', ['coupon', 'stream', 'yield', 'hazard', 'date', 'cut'])
def test_commercial_holding_history_and_anchor_edits_reject(target):
    request = raw(); future = request['revised_source']
    if target == 'coupon': future['source']['current']['periods'][0]['coupon']['annual_rate'] += .01
    elif target == 'stream': future['credit']['draw_stream']['seed'] += 1
    elif target == 'yield': future['source']['acquisition']['original_annual_effective_rate'] += .01
    elif target == 'hazard': future['credit']['intervals'][0]['hazards_per_year'][0][3] += 1.
    elif target == 'date': future['source']['current']['periods'][1]['payment_ordinal'] += 1
    else: request['cut_ordinal'] = future['source']['current']['asof_ordinal']
    with pytest.raises(ValueError): model_native.run(request['schema'], request)

def test_commercial_holding_cancelled_transport_has_no_result():
    with pytest.raises(InterruptedError): model_native.run(raw()['schema'], raw(), cancelled=lambda: True)

def test_commercial_holding_direct_process_requires_original_deadline():
    request = raw(); envelope = dict(schema=request['schema'], threads=1, request=request)
    process = subprocess.run([str(model_native.binary_path())], input=json.dumps(envelope).encode(), capture_output=True, timeout=30)
    response = json.loads(process.stdout)
    assert process.returncode == 1 and response['ok'] is False and 'result' not in response
    assert 'execution_timeout_ms' in response['error']

def test_commercial_holding_fresh_default_cabi_forbids_python_finance():
    script = '''import json,sys
from portfolio_risk.core import runtime
from portfolio_risk.analytics import model_contracts,commercial_continuation_reference,commercial_credit_reference,commercial_accounting_reference
assert runtime._CURRENT.get() is None and runtime.RunConfig().compute_backend == "rust"
def forbidden(*a,**k):raise AssertionError("Python finance reached native continuation")
commercial_continuation_reference.verify=forbidden
commercial_credit_reference.replay=forbidden
commercial_accounting_reference.groups=forbidden
r=json.load(open(sys.argv[1]));v=model_contracts.continue_commercial_holding(r)
assert v["complete_origin_replayed"] is True and v["credit_clock_restarted"] is False
assert v["openings"][0]["book"]["principal"]==100.
print("fresh native holding default passed")
'''
    process = subprocess.run([sys.executable, '-c', script, str(ROOT/'examples/models/commercial-holding-continuation-1.json')], capture_output=True, text=True, timeout=60)
    assert process.returncode == 0, process.stdout+process.stderr
