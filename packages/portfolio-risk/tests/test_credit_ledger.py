"""Independent persisted credit replay, evidence tampering and publication fences."""
import json
from pathlib import Path

import polars as pl
import pytest

from portfolio_risk.analytics import credit_ledger as engine
from portfolio_risk.analytics.credit_verification import verify_credit_sources
from portfolio_risk.analytics.balance_stream import PartitionWriter
from portfolio_risk.analytics.ledger_native import validate_spec

ROOT=Path(__file__).resolve().parents[3]


def example():
    return json.loads((ROOT/'examples/models/credit-ledger-1.json').read_text(encoding='utf-8'))


@pytest.fixture(scope='module')
def saved(tmp_path_factory):
    raw=example();path=engine.run_credit_ledger(raw,tmp_path_factory.mktemp('credit'),partition_rows=19)
    manifest=json.loads(path.read_text());writer=PartitionWriter(path.parent,19,schemas=engine.SCHEMAS)
    writer.parts={name:table['parts'] for name,table in manifest['tables'].items()}
    return raw,validate_spec(raw),writer,manifest


def test_complete_credit_sources_and_journal_are_independently_verified(saved):
    raw,spec,writer,manifest=saved
    assert verify_credit_sources(raw,spec,writer,lambda:False)==manifest['validation']
    assert manifest['validation']['credit_source_reconciled']
    assert not manifest['validation']['producer_empirically_validated']
    assert json.loads((writer.root/manifest['input']['path']).read_text())==raw
    assert len(manifest['tables']['journal']['parts'])>1


@pytest.mark.parametrize('table,field',[
    ('credit_states','probability'),('credit_states','live_principal'),
    ('credit_claims','remaining_recovery_cash'),('credit_claims','recovery_tail_after_reporting'),
    ('journal','value'),('credit_lineage','source_revision'),
    ('daily_balance','assets'),('trial_balance','balance'),('summary','final_assets'),
])
def test_tampered_persisted_financial_evidence_fails(saved,table,field):
    raw,spec,writer,_=saved
    class Tampered:
        def frames(self,name):
            altered=False
            for frame in writer.frames(name):
                if name==table and not altered and frame.height:
                    rows=frame.to_dicts();value=rows[0][field]
                    if isinstance(value,list): rows[0][field]=[value[0]+1.,*value[1:]]
                    elif isinstance(value,str): rows[0][field]=value+'-tampered'
                    else: rows[0][field]=value+1.
                    frame=pl.DataFrame(rows,schema=frame.schema);altered=True
                yield frame
    with pytest.raises(ArithmeticError,match='credit persisted source mismatch'):
        verify_credit_sources(raw,spec,Tampered(),lambda:False)


def test_missing_or_duplicate_source_rows_fail(saved):
    raw,spec,writer,_=saved
    for duplicate in (False,True):
        class Tampered:
            def frames(self,name):
                for frame in writer.frames(name):
                    if name=='credit_states':
                        if duplicate: yield frame
                        else: continue
                    yield frame
        with pytest.raises(ArithmeticError): verify_credit_sources(raw,spec,Tampered(),lambda:False)


def test_cancellation_during_independent_replay_publishes_nothing(tmp_path,monkeypatch):
    original=engine.verify_credit_sources;cancelled=[False]
    def interrupted(*args):
        result=original(*args);cancelled[0]=True;return result
    monkeypatch.setattr(engine,'verify_credit_sources',interrupted)
    with pytest.raises(InterruptedError): engine.run_credit_ledger(example(),tmp_path,cancelled=lambda:cancelled[0])
    assert not list(tmp_path.glob('run-*'))


def test_legacy_execution_callbacks_forbidden(tmp_path,monkeypatch):
    from portfolio_risk.analytics import balance_stress,balance_stream
    def forbidden(*args,**kwargs):raise AssertionError('Python financial execution')
    monkeypatch.setattr(balance_stress,'validate',forbidden)
    monkeypatch.setattr(balance_stress,'_simulate',forbidden)
    monkeypatch.setattr(balance_stream,'_python',forbidden)
    assert engine.run_credit_ledger(example(),tmp_path).is_file()


def test_source_changed_during_replay_fails_publication(tmp_path,monkeypatch):
    calls=[0]
    def identity():calls[0]+=1;return str(calls[0])
    monkeypatch.setattr(engine,'source_identity',identity)
    with pytest.raises(RuntimeError,match='identity changed'):engine.run_credit_ledger(example(),tmp_path)
    assert not list(tmp_path.glob('run-*'))
