"""Native mixed finance ownership and bounded transport publication identities."""
import hashlib
import json
from pathlib import Path
import pytest
from portfolio_risk.core import product_mixed_native as native

ROOT=Path(__file__).resolve().parents[3]


def example():
    return json.loads((ROOT/'examples/models/product-mixed-dated-1.json').read_text())


def test_mixed_process_owns_finance_and_records_raw_and_executed_inputs(monkeypatch):
    from portfolio_risk.core import quant_native,decision_native
    def forbidden(*a,**k):pytest.fail('in-process financial callback')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    raw=example();result,receipt=native.run(raw,threads=1,timeout=60.)
    assert result['schema']==native.SCHEMA
    digest=lambda value:hashlib.sha256(json.dumps(value,allow_nan=False,separators=(',',':')).encode()).hexdigest()
    assert receipt['input_sha256']==digest(dict(schema=native.SCHEMA,threads=1,request=raw))
    assert receipt['executed_input_sha256']==digest(native.command(raw,timeout=60.))
    from portfolio_risk.analytics.product_mixed_reference import verify
    assert verify(raw,result)['mixed_performing_only']


def test_mixed_process_identity_and_prelaunch_cancel_publish_no_result(monkeypatch):
    raw=example()
    with pytest.raises(RuntimeError,match='MODEL_VERSION_CHANGED'):
        native.run(raw,expected_binary='f'*64)
    from portfolio_risk.core import product_strategy_native
    def forbidden(*a,**k):pytest.fail('cancelled mixed request launched process')
    monkeypatch.setattr(product_strategy_native.subprocess,'Popen',forbidden)
    with pytest.raises(InterruptedError):native.run(raw,cancelled=lambda:True)
