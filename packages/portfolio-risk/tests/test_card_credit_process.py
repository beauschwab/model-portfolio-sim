"""Fresh public card lifecycle process and independent publication receipts.

Optional test-only DEBUG override selects the actual public lifecycle executable;
missing artifacts never skip, fall back or invoke the artifact review harness.
"""
from copy import deepcopy
import json
import os
from pathlib import Path
import pytest
from portfolio_risk.core import model_native
from portfolio_risk.analytics import card_credit_reference as oracle
from test_card_credit_reference import raw,case,invalid,ACCEPTED,REJECTED

SCHEMA='card-credit-accounting-1'


@pytest.fixture(autouse=True)
def lifecycle_binary(monkeypatch):
    override=os.environ.get('CARD_CREDIT_LIFECYCLE_TEST_BINARY')
    if override:
        path=Path(override).resolve()
        assert path.name in {'portfolio-lifecycle','portfolio-lifecycle.exe'},'only the public lifecycle executable may produce these receipts'
        monkeypatch.setattr(model_native,'binary_path',lambda:path)
    assert model_native.binary_path().is_file(),'fresh public lifecycle required; no artifact fallback or skipped gate'


@pytest.mark.parametrize('name',ACCEPTED)
def test_public_lifecycle_complete_independent_card_source(name,monkeypatch):
    request=case(name);original=deepcopy(request)
    result,execution=model_native.run(SCHEMA,request,timeout=30.)
    assert request==original and execution['schema']==SCHEMA and execution['backend']=='rust'
    assert execution['binary_sha256']==model_native.binary_identity()
    from portfolio_risk.core import quant_native,decision_native
    def forbidden(*a,**k):pytest.fail('native financial callback from independent card verification')
    monkeypatch.setattr(model_native,'run',forbidden);monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    before=json.dumps([request,result],sort_keys=True)
    receipt=oracle.verify(request,result)
    assert receipt['verified'] and receipt['statement_income_not_repeated']
    assert not receipt['mixed_ledger_adoption'] and not receipt['empirical_validation']
    assert receipt==oracle.verify(json.loads(json.dumps(request)),json.loads(json.dumps(result)))
    assert json.dumps([request,result],sort_keys=True)==before


@pytest.mark.parametrize('name',REJECTED)
def test_public_lifecycle_rejects_unsupported_card_without_fallback(name):
    request=invalid(name)
    with pytest.raises(ArithmeticError):oracle.reconstruct(request)
    with pytest.raises(ValueError,match='native model rejected input') as error:
        model_native.run(SCHEMA,request,timeout=30.)
    if name=='negative_tail_basis':assert 'opening live principal plus basis cannot be negative' in str(error.value)


def test_public_card_transport_cancellation_returns_no_receipt():
    with pytest.raises(InterruptedError,match='cancelled'):
        model_native.run(SCHEMA,raw(),cancelled=lambda:True,timeout=30.)


def test_public_card_executable_identity_mismatch_before_execution():
    with pytest.raises(RuntimeError,match='MODEL_VERSION_CHANGED'):
        model_native.run(SCHEMA,raw(),expected_binary='caller-replaced-executable',timeout=30.)


def test_public_card_cancellation_after_process_start_stages_no_receipt(monkeypatch):
    original=model_native.subprocess.Popen;children=[];checks=0
    def launched(*a,**k):
        child=original(*a,**k);children.append(child);return child
    def stopped():
        nonlocal checks
        checks+=1
        return checks>=2
    monkeypatch.setattr(model_native.subprocess,'Popen',launched)
    with pytest.raises(InterruptedError,match='cancelled'):
        model_native.run(SCHEMA,raw(),cancelled=stopped,timeout=30.)
    assert len(children)==1 and children[0].poll() is not None
