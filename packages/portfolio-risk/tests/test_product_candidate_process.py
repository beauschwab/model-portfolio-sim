"""Standalone candidate execution and cancellation retain the raw request identity."""
import hashlib
import json
from pathlib import Path
import pytest
from portfolio_risk.core import product_candidate_native as native

ROOT=Path(__file__).resolve().parents[3]


def example():
    return json.loads((ROOT/'examples/models/product-candidate-journal-1.json').read_text())


def test_candidate_process_owns_finance_and_records_both_input_identities(monkeypatch):
    from portfolio_risk.core import quant_native,decision_native
    def forbidden(*a,**k):pytest.fail('in-process financial callback')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    raw=example();result,receipt=native.run(raw,threads=1,timeout=60.)
    assert result['schema']==native.SCHEMA and not result['dynamic_mixed_acceptance']
    assert not result['independent_persisted_verification']
    envelope=dict(schema=native.SCHEMA,threads=1,request=raw)
    digest=lambda value:hashlib.sha256(json.dumps(value,allow_nan=False,separators=(',',':')).encode()).hexdigest()
    assert receipt['input_sha256']==digest(envelope)
    assert receipt['executed_input_sha256']==digest(native.command(raw,timeout=60.))
    from portfolio_risk.analytics.product_candidate_reference import verify
    assert verify(raw,result)['ordered_candidate_journal_recomputed']


def test_candidate_process_identity_and_prelaunch_cancel_fail_without_results(monkeypatch):
    raw=example()
    with pytest.raises(RuntimeError,match='MODEL_VERSION_CHANGED'):
        native.run(raw,expected_binary='f'*64)
    from portfolio_risk.core import product_strategy_native
    def forbidden(*a,**k):pytest.fail('cancelled candidate launched process')
    monkeypatch.setattr(product_strategy_native.subprocess,'Popen',forbidden)
    with pytest.raises(InterruptedError):native.run(raw,cancelled=lambda:True)
