"""Public Card2 standalone transport ownership and independent raw source receipts.

An explicit test-only DEBUG selection must still be portfolio-lifecycle; no skips,
artifact review harness, Python fallback or financial backend substitution.
"""
from copy import deepcopy
import json
import os
from pathlib import Path
import pytest
from portfolio_risk.core import model_native
from portfolio_risk.analytics import card_credit_v2_reference as oracle
from test_card_credit_v2_reference import raw,invalid,multi_class,ACCEPTED,REJECTED,INVALID,CASES

SCHEMA='card-credit-accounting-2'


@pytest.fixture(autouse=True)
def public_binary(monkeypatch):
    override=os.environ.get('CARD_CREDIT_V2_LIFECYCLE_TEST_BINARY')
    if override:
        path=Path(override).resolve()
        assert path.name in {'portfolio-lifecycle','portfolio-lifecycle.exe'}
        monkeypatch.setattr(model_native,'binary_path',lambda:path)
    assert model_native.binary_path().is_file(),'fresh public schema2 lifecycle binary required; no fallback or skipped gate'


@pytest.mark.parametrize('i',ACCEPTED,ids=[CASES[i]['name'] for i in ACCEPTED])
def test_public_card2_native_complete_compact_source_and_independent_receipt(i,monkeypatch):
    request=raw(i);original=deepcopy(request);result,execution=model_native.run(SCHEMA,request,timeout=30.)
    assert request==original and execution['backend']=='rust' and execution['schema']==SCHEMA
    assert execution['binary_sha256']==model_native.binary_identity()
    from portfolio_risk.core import quant_native,decision_native
    def forbidden(*a,**k):pytest.fail('public Card2 source verification called native financial backend')
    monkeypatch.setattr(model_native,'run',forbidden);monkeypatch.setattr(quant_native,'term_call',forbidden);monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    saved=json.dumps([request,result],sort_keys=True)
    receipt=oracle.verify(request,result)
    assert receipt['verified'] and receipt['compact_arena_ranges_recomputed']
    assert not receipt['mixed_ledger_adoption'] and not receipt['empirical_validation']
    assert receipt==oracle.verify(json.loads(json.dumps(request)),json.loads(json.dumps(result)))
    assert json.dumps([request,result],sort_keys=True)==saved


@pytest.mark.parametrize('i',REJECTED,ids=[CASES[i]['name'] for i in REJECTED])
def test_public_card2_original_negative_carry_partial_principal_grace_and_excess_reject(i):
    request=raw(i)
    with pytest.raises(ArithmeticError):oracle.reconstruct(request)
    with pytest.raises(ValueError,match='native model rejected input'):model_native.run(SCHEMA,request,timeout=30.)


@pytest.mark.parametrize('kind',INVALID)
def test_public_card2_domain_admission_rejection(kind):
    request=invalid(kind)
    with pytest.raises(ArithmeticError):oracle.reconstruct(request)
    with pytest.raises(ValueError,match='native model rejected input'):model_native.run(SCHEMA,request,timeout=30.)


def test_public_card2_unpaid_gross_claim_never_complete_by_netting_basis():
    request=raw(8);request['interest_recoveries']=[];request['review']['cost']=100.;request['review']['original_annual_effective_rate']=0.
    result,_=model_native.run(SCHEMA,request,timeout=30.)
    assert oracle.verify(request,result)['full_gross_tail_retained']
    assert result['complete'] is False
    assert result['closing']['core']['claims'][0]['outstanding_accrued_interest']>0.
    assert oracle.carrying(result['closing'])==pytest.approx(0.,abs=1e-12)


def test_public_card2_preprocess_cancellation_and_binary_identity():
    with pytest.raises(InterruptedError,match='cancelled'):model_native.run(SCHEMA,raw(),cancelled=lambda:True,timeout=30.)
    with pytest.raises(RuntimeError,match='MODEL_VERSION_CHANGED'):model_native.run(SCHEMA,raw(),expected_binary='replaced-artifact',timeout=30.)


def test_public_card2_deadline_returns_no_receipt():
    with pytest.raises(TimeoutError,match='deadline'):model_native.run(SCHEMA,raw(),timeout=.001)


def test_public_card2_three_class_original_yield_and_full_actual_tail():
    request=multi_class();result,_=model_native.run(SCHEMA,request,timeout=30.)
    assert oracle.verify(request,result)['original_yield_income_recomputed']
    assert result['complete'] is True
    assert all(p==0. for p in result['economic']['points'][10]['principal'])


@pytest.mark.parametrize('yield_value',[1e-5,1e-6,1.1e-5])
def test_public_card2_f64_capacity_spelling_is_exact(yield_value):
    request=raw();request['review']['original_annual_effective_rate']=yield_value
    result,_=model_native.run(SCHEMA,request,timeout=30.)
    assert result['reserved_bytes']==oracle._admit(request)['reserved_bytes']
    assert oracle.verify(request,result)['verified']


def test_public_card2_integer_financial_json_normalizes_without_retyping_dates():
    request=raw();request['review']['cost']=request['review']['principal']=100
    request['contract']['opening_principal']=[100,0,0]
    result,_=model_native.run(SCHEMA,request,timeout=30.)
    assert oracle.verify(request,result)['verified']
    assert type(result['raw_request']['reporting_ordinal']) is int


def test_public_card2_cancel_after_process_start_has_no_receipt(monkeypatch):
    original=model_native.subprocess.Popen;children=[];checks=0
    def launched(*a,**k):
        child=original(*a,**k);children.append(child);return child
    def cancelled():
        nonlocal checks
        checks+=1
        return checks>=2
    monkeypatch.setattr(model_native.subprocess,'Popen',launched)
    with pytest.raises(InterruptedError,match='cancelled'):model_native.run(SCHEMA,raw(),cancelled=cancelled,timeout=30.)
    assert len(children)==1 and children[0].poll() is not None
