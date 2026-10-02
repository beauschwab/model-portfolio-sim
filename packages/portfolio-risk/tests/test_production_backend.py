"""Unconfigured public calls must use native execution, outside the reference harness."""
import subprocess
import sys


def test_fresh_process_defaults_native_and_reference_is_explicit():
    result=subprocess.run([sys.executable,'-c', '''
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.core.quant_native import enabled
from portfolio_risk.analytics.balance_stress import example_specification, run_balance_stress
assert RunConfig().compute_backend == 'rust'
assert enabled()
with run_context(): assert enabled()
with run_context(RunConfig(compute_backend='python')):
    assert not enabled()
assert enabled()
raw=example_specification();raw.update(horizon_days=30,reverse_severities=[])
out=run_balance_stress(raw)
assert out['execution']['financial_events']=='rust'
'''],capture_output=True,text=True)
    assert result.returncode==0,result.stdout+result.stderr


def test_missing_default_native_library_is_an_error_without_fallback():
    result=subprocess.run([sys.executable,'-c', '''
import os
os.environ['PORTFOLIO_RISK_RUST_LIB']='/missing/native-product-library'
from portfolio_risk.core.quant_native import enabled, term_call
assert enabled()
try: term_call('treasury-1',{})
except RuntimeError as exc: assert 'not built' in str(exc)
else: raise AssertionError('missing Rust library did not fail')
'''],capture_output=True,text=True)
    assert result.returncode==0,result.stdout+result.stderr
