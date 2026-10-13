"""Standalone ownership, original identity and cancellation of product allocation."""
import json
from pathlib import Path
import pytest
from portfolio_risk.core import product_strategy_native as native

ROOT=Path(__file__).resolve().parents[3]

def example():
    return json.loads((ROOT/'examples/models/strategy-product-1.json').read_text())

def test_standalone_product_allocation_never_calls_python_financial_callbacks(monkeypatch):
    from portfolio_risk.core import quant_native,decision_native
    def forbidden(*a,**k):pytest.fail('Python/in-process financial callback')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    raw=example();result,receipt=native.run(raw,threads=1)
    assert result['feasible'] and result['validated'] and not result['dynamic_validated']
    assert [row['face'] for row in result['allocation']]==pytest.approx([20.,80.],abs=1e-8)
    assert receipt['binary_sha256']==native.binary_identity() and receipt['schema']=='strategy-product-1'
    from portfolio_risk.analytics.product_strategy_reference import verify
    assert verify(raw,result)['verified']

def test_strategy_identity_cancellation_and_deadline_fail_without_a_result(monkeypatch):
    raw=example()
    with pytest.raises(RuntimeError,match='MODEL_VERSION_CHANGED'):
        native.run(raw,expected_binary='f'*64)
    def forbidden(*a,**k):pytest.fail('cancelled request launched a process')
    with monkeypatch.context() as patch:
        patch.setattr(native.subprocess,'Popen',forbidden)
        with pytest.raises(InterruptedError):native.run(raw,cancelled=lambda:True)
    with pytest.raises(TimeoutError):native.run(raw,timeout=.001)


def test_reciprocal_face_and_quote_cannot_understate_native_asset_caps():
    raw=example()
    for unit in raw['product']['units']:
        unit['unit_face']*=.5
        unit['quote_price_per_face_unit']*=2.
    raw['face_limits']=[]
    raw['constraints']['max_total_assets']=100.
    with pytest.raises(ValueError,match='unit_face must equal contractual opening principal'):
        native.run(raw,threads=1)
