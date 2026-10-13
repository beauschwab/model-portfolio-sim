"""Standalone native ownership, input rejection and process publication fences."""
import json
from pathlib import Path
import pytest
from portfolio_risk.core import model_native

ROOT=Path(__file__).resolve().parents[3]


def request():
    return json.loads((ROOT/'examples/models/consumer-card-1.json').read_text(encoding='utf-8'))


def test_raw_native_process_preserves_card_rights():
    result,receipt=model_native.run('consumer-card-1',request(),timeout=10.)
    assert result['closing_unbilled_interest']==pytest.approx(1.)
    assert result['closing_principal']==1000.
    assert len(receipt['binary_sha256'])==64


def test_native_rejects_unknown_financial_options():
    with pytest.raises(ValueError,match='native model rejected'):model_native.run('consumer-card-1',dict(request(),python_backend=True))


@pytest.mark.parametrize('failure',['cancel','deadline','changed','missing','threads'])
def test_process_fences_do_not_return_results(monkeypatch,failure):
    kwargs={}
    if failure=='cancel':kwargs['cancelled']=lambda:True
    elif failure=='deadline':kwargs['timeout']=.001
    elif failure=='changed':kwargs['expected_binary']='f'*64
    elif failure=='missing':monkeypatch.setattr(model_native,'binary_path',lambda:ROOT/'absent-model.exe')
    else:kwargs['threads']=0
    with pytest.raises((RuntimeError,TimeoutError,InterruptedError,ValueError)):model_native.run('consumer-card-1',request(),**kwargs)
