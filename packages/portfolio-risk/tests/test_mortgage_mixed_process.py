"""Official default model-ledger ownership/publication; no overrides or skips."""
from copy import deepcopy
from pathlib import Path
import hashlib
import json
import subprocess
import sys
import pytest
from portfolio_risk.analytics import model_ledger as engine
ROOT=Path(__file__).resolve().parents[3]

def raw():return json.loads((ROOT/'examples/models/mortgage-mixed-accounting-1.json').read_text())

def forbidden(*args,**kwargs):raise AssertionError('Python or unrelated financial runtime callback')

@pytest.fixture(scope='module')
def saved(tmp_path_factory):
    request=raw();manifest_path=engine.run_model_ledger(request,tmp_path_factory.mktemp('mortgage-default'),partition_rows=7,timeout=60.)
    manifest=json.loads(manifest_path.read_text());header=json.loads((manifest_path.parent/manifest['modeled_source']['path']).read_text())
    return request,manifest_path,manifest,header

def test_fresh_default_process_forbids_python_and_old_card_execution(tmp_path):
    output=tmp_path/'fresh'
    script="""
import json,sys
from pathlib import Path
from portfolio_risk.analytics import model_ledger,model_contracts,balance_stress,balance_stream
from portfolio_risk.core import model_native,quant_native
from portfolio_risk.analytics import consumer_credit_reference,card_credit_reference,card_credit_v2_reference

def forbidden(*args,**kwargs):raise AssertionError('financial callback outside native model-ledger')
for name in ('project_mortgage_state','account_mortgage_credit','analyze_mortgage_graph','account_card_credit','account_card_credit_v2','simulate_consumer_card'):
    setattr(model_contracts,name,forbidden)
model_native.run=forbidden;quant_native.term_call=forbidden
balance_stress._simulate=forbidden;balance_stream._python=forbidden
consumer_credit_reference.verify=forbidden;card_credit_reference.verify=forbidden;card_credit_v2_reference.verify=forbidden
model_ledger.validate_spec=forbidden
request=json.loads(Path(sys.argv[1]).read_text());binary=model_ledger.binary_path()
assert binary.parent.name=='release' and binary.is_file()
manifest_path=model_ledger.run_model_ledger(request,sys.argv[2],partition_rows=7,timeout=60.)
manifest=json.loads(manifest_path.read_text());print(json.dumps({'backend':manifest['backend'],'model':manifest['model_version'],'validation':manifest['validation'],'binary':str(binary)}))
"""
    proc=subprocess.run([sys.executable,'-c',script,str(ROOT/'examples/models/mortgage-mixed-accounting-1.json'),str(output)],capture_output=True,text=True,timeout=90)
    assert proc.returncode==0,proc.stderr
    result=json.loads(proc.stdout);assert result['backend']=='rust' and result['model']=='mortgage-mixed-accounting-1'
    assert result['validation']['single_daily_gl_verified'] is True
    assert result['validation']['accounting_standard_approval'] is False

def test_full_source_and_persisted_gl_reopen_with_actual_completion(saved):
    from portfolio_risk.analytics import mortgage_mixed_reference as reference
    from portfolio_risk.analytics.mortgage_mixed_transport import SCHEMAS
    import polars as pl
    request,path,manifest,header=saved
    class Stored:
        schemas=SCHEMAS
        @property
        def completion(self):
            descriptor=manifest['producer_completion'];data=(path.parent/descriptor['path']).read_bytes()
            assert hashlib.sha256(data).hexdigest()==descriptor['sha256']
            return json.loads(data)
        def frames(self,name):
            for part in manifest['tables'][name]['parts']:
                data=(path.parent/part['path']).read_bytes();assert hashlib.sha256(data).hexdigest()==part['sha256']
                yield pl.read_parquet(path.parent/part['path'])
    control=reference.verify_preparation(request,header['prepared_request'],header['source_receipt'])
    receipt=reference.verify_stream(request,control,Stored())
    assert receipt==manifest['validation']
    assert receipt['complete_unweighted_tail_verified'] is True
    assert receipt['pending_memo_phase_verified'] is True
    assert receipt['ordered_mixed_journal_recomputed'] is True
    assert receipt['production_adoption'] is False and receipt['capital_ftp_eligible'] is False

@pytest.mark.parametrize('target',['ignored_fixed_rate','gross_event','distinct_source','missing_run'])
def test_native_invalid_source_refuses_before_verifier_or_manifest(tmp_path,monkeypatch,target):
    from portfolio_risk.analytics import mortgage_mixed_reference
    request=raw()
    if target=='ignored_fixed_rate':request['sources'][0]['request']['source']['original_contract']['rate']['annual_rate']=0.
    elif target=='gross_event':request['dated']['dated']['sources'][0]['schedule']['events'][0]['components']['cash_interest']=10001.
    elif target=='missing_run':request['owners'][0]['runs'].pop()
    else:
        request['sources'].append(deepcopy(request['sources'][0]));request['sources'][-1]['source_id']='another'
        request['owners'][0]['runs'][-1]['paths'][0]['source_id']='another'
    monkeypatch.setattr(mortgage_mixed_reference,'verify_preparation',forbidden)
    with pytest.raises((ArithmeticError,ValueError,RuntimeError)):engine.run_model_ledger(request,tmp_path,timeout=60.)
    assert not list(tmp_path.rglob('manifest.json'))

@pytest.mark.parametrize('phase',['cancel_before','cancel_after_source','deadline_after_source','source_after_rename'])
def test_original_deadline_cancellation_and_final_source_fence_prevent_publication(tmp_path,monkeypatch,phase):
    from portfolio_risk.analytics import mortgage_mixed_reference as reference
    cancel=[phase=='cancel_before'];verify=reference.verify_preparation
    def checked(*args,**kwargs):
        result=verify(*args,**kwargs)
        if phase=='cancel_after_source':cancel[0]=True
        if phase=='deadline_after_source':raise TimeoutError('original verification deadline expired')
        return result
    monkeypatch.setattr(reference,'verify_preparation',checked)
    if phase=='source_after_rename':
        replace=engine.os.replace
        def changed(source,target):
            replace(source,target)
            if Path(target).name.startswith('run-'):monkeypatch.setattr(engine,'source_identity',lambda:'changed')
        monkeypatch.setattr(engine.os,'replace',changed)
    with pytest.raises((InterruptedError,TimeoutError,RuntimeError)):
        engine.run_model_ledger(raw(),tmp_path,cancelled=lambda:cancel[0],timeout=60.)
    assert not list(tmp_path.rglob('manifest.json'))


def test_missing_default_native_artifact_never_falls_back(tmp_path,monkeypatch):
    from portfolio_risk.core import model_native
    monkeypatch.setattr(engine,'binary_path',lambda:tmp_path/'missing-portfolio-model-ledger')
    monkeypatch.setattr(model_native,'run',forbidden)
    with pytest.raises(RuntimeError,match='no fallback'):engine.run_model_ledger(raw(),tmp_path)
    assert not list(tmp_path.rglob('manifest.json'))


@pytest.mark.parametrize('alias',['bool','integer','signed_zero'])
def test_prepared_header_changed_after_independent_replay_cannot_publish(tmp_path,monkeypatch,alias):
    from portfolio_risk.analytics import mortgage_mixed_reference as reference
    verify=reference.verify_stream
    def changed(request,control,writer,check=lambda:False):
        result=verify(request,control,writer,check)
        path=writer.root/'prepared-source.json'
        header=json.loads(path.read_text(encoding='utf-8'))
        if alias=='bool':header['source_receipt']['accounting_standard_approval']=0
        elif alias=='integer':header['prepared_request']['specification']['horizon_days']=float(header['prepared_request']['specification']['horizon_days'])
        else:header['source_receipt']['raw_request']['sources'][0]['request']['source']['contract']['opening_investor_principal'][1]=-0.
        path.write_text(json.dumps(header,allow_nan=False),encoding='utf-8')
        return result
    monkeypatch.setattr(reference,'verify_stream',changed)
    with pytest.raises(ArithmeticError,match='immutable modeled inputs changed'):
        engine.run_model_ledger(raw(),tmp_path,timeout=60.)
    assert not list(tmp_path.rglob('manifest.json'))
