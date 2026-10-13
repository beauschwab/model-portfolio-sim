"""New selected native route, official default executable after composition.

No artifact imports, binary override, native DLL or Python financial fallback.
The historical enum parser is intentionally separate from this selected raw wire.
"""
from pathlib import Path
import json,subprocess
import pytest
from portfolio_risk.analytics import saved_derivative_runtime as runtime
from portfolio_risk.analytics.saved_derivative_identity import loads

FIXTURE=Path(__file__).parent/'fixtures/saved_derivative_mixed/base.json'
@pytest.mark.parametrize('run_index',[0,1])
@pytest.mark.parametrize('form',['unit_object','null','bool','unknown'])
def test_default_native_measure_refuses_before_oracle(run_index,form,tmp_path,monkeypatch):
    raw=loads(FIXTURE.read_bytes())
    raw['owners'][0]['runs'][run_index]['saved']['physical']['measure']={
        'unit_object':{'physical':None},'null':None,'bool':True,'unknown':'unknown'}[form]
    def forbidden(*a,**kw):raise AssertionError('native raw refusal must precede oracle')
    monkeypatch.setattr(runtime,'verify_preparation',forbidden)
    with pytest.raises(RuntimeError,match='must be an explicit enum string'):
        runtime.run(raw,tmp_path,timeout=30.)
    assert not list(tmp_path.rglob('manifest.json'))

@pytest.mark.parametrize('domain',['accounting','swap','index','dated'])
def test_default_native_other_selected_enum_aliases_refuse(domain,tmp_path):
    raw=loads(FIXTURE.read_bytes());saved=raw['owners'][0]['runs'][0]['saved']
    if domain=='accounting':saved['physical']['policy']['custody']={'segregated_custodian_memo_only':None}
    elif domain=='swap':saved['book']['trades'][0]['request']['side']={'payer':None}
    elif domain=='index':saved['book']['trades'][0]['request']['floating_periods'][0]['coupon']['calculation']={'term_simple':None}
    else:raw['dated']['recognition_policy']['fees']={'immediate_earnings':None}
    with pytest.raises(RuntimeError,match='must be an explicit enum string'):
        runtime.run(raw,tmp_path,timeout=30.)
    assert not list(tmp_path.rglob('manifest.json'))

def test_default_native_original_byte_duplicate_measure_refused():
    wire=json.dumps({'request':loads(FIXTURE.read_bytes()),'execution_timeout_ms':30000}).encode()
    key=b'"measure": "physical"';assert key in wire
    wire=wire.replace(key,key+b',"measure":{"physical":null}',1)
    p=subprocess.run([str(runtime.binary_path())],input=wire,capture_output=True,timeout=35)
    assert p.returncode!=0 and not p.stdout and b'duplicate object key' in p.stderr
