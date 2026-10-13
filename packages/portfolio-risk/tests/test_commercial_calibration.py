"""Actual-date original Q calibration, unchanged P anchors and publication replay."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import pytest
from portfolio_risk.core import model_native
from portfolio_risk.analytics.commercial_graph_reference import verify_graph, verify_units

ROOT=Path(__file__).resolve().parents[3]
def raw(options=False):
    name='commercial-generated-calibrated-exercise-graph-1' if options else 'commercial-generated-calibrated-graph-1'
    return json.loads((ROOT/'examples/models'/f'{name}.json').read_text())

@pytest.fixture(scope='module')
def accepted():
    request=raw();result,_=model_native.run(request['schema'],request)
    return request,result

def test_commercial_original_oas_independent_cash_and_full_day35_tail(accepted):
    request,result=accepted
    receipt=verify_graph(request,result)
    assert receipt['verified'] and receipt['pricing_cash_recomputed']
    assert abs(result['original_oas'][0]['base_oas']-.017)<2e-10
    calibrated=result['generated_oas_calibration']
    assert calibrated['pricing_training_samples_generated']==128
    assert calibrated['pricing_evaluation_samples_generated']==128
    assert len(calibrated['trials'])>3
    assert all(p['policy']['request']['fixed_base_oas']==calibrated['selected_base_oas'] for p in result['exercise_policies'])
    assert result['empirical_validation'] is False
    assert any(c['year']==35/365. for s in result['native_sources'] for c in s['cash'])

def test_commercial_generated_exercise_quote_replays_refitted_q_and_separate_p():
    request=raw(True);result,_=model_native.run(request['schema'],request)
    assert verify_graph(request,result)['verified']
    assert abs(result['original_oas'][0]['base_oas']-.02)<1e-12
    original=result['native_sources'][:128]
    assert any(s['projection']['native_stops'] for s in original)
    assert any(not s['projection']['native_stops'] for s in original)
    assert result['generated_oas_calibration']['physical_outcomes_used_for_calibration'] is False
    acquisition=request['positions'][0]['generated_policy']['original_physical']['source']['source']['acquisition']
    assert acquisition['original_annual_effective_rate'] != .02
    assert all(s['source_request']['source']['acquisition']==acquisition for s in result['native_sources'] if s['role']=='physical')

def test_commercial_calibrated_coefficients_reconcile_original_yield_and_funding():
    request=json.loads((ROOT/'examples/models/commercial-units-1.json').read_text())
    request['units'][0]['graph']=raw()
    result,_=model_native.run('commercial-units-1',request)
    assert verify_units(request,result)['coefficients_recomputed']
    assert result['values'][0]>0.
    assert 738921 in result['funding']['ordinals']

@pytest.mark.parametrize('target',['spread','trial','policy','samples','measure','quote','yield'])
def test_commercial_calibration_receipt_and_original_acquisition_tampering_reject(accepted,target):
    request,result=deepcopy(accepted)
    receipt=result['generated_oas_calibration']
    if target=='spread':receipt['selected_base_oas']+=.001
    elif target=='trial':receipt['trials'][2]['evaluation_value']+=1.
    elif target=='policy':result['exercise_policies'][0]['policy']['regressions']=[{}]
    elif target=='samples':receipt['pricing_training_samples_generated']+=2
    elif target=='measure':receipt['physical_outcomes_used_for_calibration']=True
    elif target=='quote':receipt['target_cash_price']+=1.
    else:result['original_acquisition_proofs'][0]['acquisition']['original_annual_effective_rate']+=.01
    with pytest.raises((ArithmeticError,ValueError,KeyError)):verify_graph(request,result)

@pytest.mark.parametrize('target',['unreachable','trials','oversized','missing_policy'])
def test_commercial_generated_calibration_admission_and_unsolved_quotes_reject(target):
    request=raw();p=request['positions'][0];g=p['generated_policy']
    if target=='unreachable':p['original_signed_cash_quote']=p['oas_target']['target_cash_price']=200.
    elif target=='trials':g['oas_calibration']['max_trials']=3
    elif target=='oversized':g['oas_calibration']['max_trials']=129
    else:g.pop('oas_calibration')
    with pytest.raises(ValueError):model_native.run(request['schema'],request)

def test_commercial_calibration_native_original_entry_deadline_has_no_result():
    request=raw()
    envelope=dict(schema=request['schema'],threads=1,request=request,execution_timeout_ms=1)
    process=subprocess.run([str(model_native.binary_path())],input=json.dumps(envelope).encode(),capture_output=True,timeout=20)
    response=json.loads(process.stdout)
    assert process.returncode==1 and response['ok'] is False and 'result' not in response
    assert 'deadline' in response['error'].lower()

def test_commercial_calibration_transport_cancellation_has_no_fallback():
    with pytest.raises(InterruptedError):model_native.run('commercial-product-graph-1',raw(),cancelled=lambda:True)

def test_commercial_calibration_fresh_process_default_cabi_forbids_python_finance():
    script='''import json,sys
from portfolio_risk.core import runtime
from portfolio_risk.analytics import model_contracts,commercial_stochastic_reference,commercial_calibration_reference
assert runtime._CURRENT.get() is None and runtime.RunConfig().compute_backend == "rust"
def forbidden(*args,**kwargs):raise AssertionError("Python financial callback reached production calibration")
commercial_stochastic_reference.evaluate=forbidden
commercial_stochastic_reference.fit_pricing_law=forbidden
commercial_calibration_reference.verify=forbidden
r=json.load(open(sys.argv[1]))
result=model_contracts.analyze_commercial_graph(r)
assert abs(result["original_oas"][0]["base_oas"]-.017)<2e-10
assert result["generated_oas_calibration"]["physical_outcomes_used_for_calibration"] is False
print("fresh Rust default and C ABI calibration without Python financial callbacks")
'''
    process=subprocess.run([sys.executable,'-c',script,str(ROOT/'examples/models/commercial-generated-calibrated-graph-1.json')],capture_output=True,text=True,timeout=120)
    assert process.returncode==0,process.stdout+process.stderr
