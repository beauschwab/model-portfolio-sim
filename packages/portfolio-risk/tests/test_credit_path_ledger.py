"""Independent raw path replay, persisted evidence and publication fences."""
import json
from pathlib import Path
import polars as pl
import pytest
from portfolio_risk.analytics import credit_path_ledger as engine
from portfolio_risk.analytics import credit_path_verification as verification
from portfolio_risk.analytics.balance_stream import PartitionWriter
from portfolio_risk.analytics.ledger_native import validate_spec
ROOT=Path(__file__).resolve().parents[3]
def example():
    return json.loads((ROOT/'examples/models/credit-path-ledger-1.json').read_text())
@pytest.fixture(scope='module')
def saved(tmp_path_factory):
    raw=example();path=engine.run_credit_path_ledger(raw,tmp_path_factory.mktemp('credit-path'),partition_rows=7)
    manifest=json.loads(path.read_text());writer=PartitionWriter(path.parent,7,schemas=engine.SCHEMAS)
    writer.parts={name:table['parts'] for name,table in manifest['tables'].items()}
    return raw,validate_spec(raw),writer,manifest
def test_full_persisted_paths_claims_postings_and_closings_recomputed(saved):
    raw,spec,writer,manifest=saved
    assert verification.verify_sources(raw,spec,writer,lambda:False)==manifest['validation']
    assert manifest['validation']['credit_path_source_reconciled']
    assert manifest['validation']['physical_paths_recomputed']
    assert not manifest['validation']['empirical_validation']
    assert json.loads((writer.root/manifest['input']['path']).read_text())==raw
    assert len(manifest['tables']['journal']['parts'])>1
@pytest.mark.parametrize('table,field',[
    ('credit_path_sources','binding'),('credit_path_sources','economic'),
    ('credit_path_lineage','exact_event'),('credit_path_lineage','accrual'),
    ('credit_path_lineage','transaction_key'),('credit_path_lineage','ordinal'),
    ('credit_path_daily','state'),('credit_path_recoveries','recovery'),
    ('credit_path_claims','claim'),('journal','value'),('journal','transaction_number'),
    ('daily_balance','assets'),('trial_balance','balance'),('summary','final_assets')])
def test_tampered_persisted_evidence_fails(saved,table,field):
    raw,spec,writer,_=saved
    class Tampered:
        def frames(self,name):
            changed=False
            for frame in writer.frames(name):
                if name==table and not changed and frame.height:
                    rows=frame.to_dicts();value=rows[0][field]
                    if field in engine.JSON_FIELDS.get(table,()):
                        value=json.loads(value)
                        if isinstance(value,dict):value['unverified_extra']=1
                        else:value=[value,'unverified_extra']
                        rows[0][field]=json.dumps(value)
                    elif isinstance(value,str):rows[0][field]=value+'-tampered'
                    else:rows[0][field]=value+1
                    frame=pl.DataFrame(rows,schema=frame.schema);changed=True
                yield frame
    with pytest.raises(ArithmeticError):verification.verify_sources(raw,spec,Tampered(),lambda:False)
@pytest.mark.parametrize('table',list(engine.SCHEMAS))
@pytest.mark.parametrize('duplicate',[False,True])
def test_missing_duplicate_table_rows_fail(saved,table,duplicate):
    raw,spec,writer,_=saved
    class Tampered:
        def frames(self,name):
            for frame in writer.frames(name):
                if name==table:
                    if duplicate:yield frame
                    else:continue
                yield frame
    with pytest.raises(ArithmeticError):verification.verify_sources(raw,spec,Tampered(),lambda:False)
def test_large_integer_keys_are_exact():
    with pytest.raises(ArithmeticError):verification.equal(2**63+1,2**63)
@pytest.mark.parametrize('failure',['cancel','deadline','source','verifier'])
def test_failed_replay_never_publishes_manifest(tmp_path,monkeypatch,failure):
    original=verification.verify_sources;cancelled=[False]
    def interrupted(*args):
        result=original(*args)
        if failure=='verifier':raise ArithmeticError('injected independent mismatch')
        if failure=='deadline':raise TimeoutError('injected verification deadline')
        cancelled[0]=True
        return result
    if failure=='source':
        calls=[0]
        def identity():calls[0]+=1;return str(calls[0])
        monkeypatch.setattr(engine,'source_identity',identity)
    else:monkeypatch.setattr(verification,'verify_sources',interrupted)
    with pytest.raises((InterruptedError,TimeoutError,RuntimeError,ArithmeticError)):
        engine.run_credit_path_ledger(example(),tmp_path,cancelled=lambda:cancelled[0])
    assert not list(tmp_path.glob('run-*'))
def test_production_execution_callbacks_forbidden(tmp_path,monkeypatch):
    from portfolio_risk.analytics import balance_stress,balance_stream
    def forbidden(*args,**kwargs):raise AssertionError('Python financial execution callback')
    monkeypatch.setattr(balance_stress,'validate',forbidden)
    monkeypatch.setattr(balance_stress,'_simulate',forbidden)
    monkeypatch.setattr(balance_stream,'_python',forbidden)
    assert engine.run_credit_path_ledger(example(),tmp_path).is_file()
