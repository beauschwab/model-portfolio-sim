"""Actual public mortgage source processes and independent publication receipts."""
from copy import deepcopy
import json
import os
from pathlib import Path
import pytest
from portfolio_risk.core import model_native
from portfolio_risk.analytics import mortgage_credit_reference as oracle
from test_mortgage_credit_reference import raw

SCHEMA='mortgage-credit-accounting-1'
HERE=Path(__file__).resolve().parent/'fixtures/mortgage_credit'
CASES=json.loads((HERE/'raw_cases.json').read_text())

@pytest.fixture(autouse=True)
def public_binary(monkeypatch):
    override=os.environ.get('MORTGAGE_CREDIT_TEST_BINARY')
    if override:
        path=Path(override).resolve()
        assert path.name in {'portfolio-lifecycle','portfolio-lifecycle.exe'}
        monkeypatch.setattr(model_native,'binary_path',lambda:path)
    assert model_native.binary_path().is_file(),'public mortgage lifecycle build required; no skipped gate or fallback'

@pytest.mark.parametrize('case',CASES,ids=lambda case:case['id'])
def test_public_original_current_raw_source_carry_and_complete_claims(case,monkeypatch):
    request=case['request'];original=deepcopy(request)
    if not case['accepted']:
        with pytest.raises(ArithmeticError):
            if case['stage']=='sequential_admission':oracle.admit(request)
            else:oracle.reconstruct(request)
        with pytest.raises(ValueError,match='native model rejected input'):
            model_native.run(SCHEMA,request,timeout=30.)
        assert request==original
        return
    result,execution=model_native.run(SCHEMA,request,timeout=30.)
    assert execution['backend']=='rust' and execution['schema']==SCHEMA
    assert execution['binary_sha256']==model_native.binary_identity()
    from portfolio_risk.core import quant_native,decision_native
    def forbidden(*args,**kwargs):pytest.fail('independent mortgage verification called native financial code')
    monkeypatch.setattr(model_native,'run',forbidden)
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    frozen=json.dumps([request,result],sort_keys=True)
    receipt=oracle.verify(request,result)
    assert receipt['verified'] and receipt['analytical_mortgage_source_only']
    assert not receipt['mixed_ledger_adoption'] and not receipt['capital_ftp_eligible']
    assert receipt==oracle.verify(json.loads(json.dumps(request)),json.loads(json.dumps(result)))
    assert request==original and json.dumps([request,result],sort_keys=True)==frozen

@pytest.mark.parametrize('field',['cost','yield','note','hazard','principal','date'])
@pytest.mark.parametrize('value',[True,'0.2'])
def test_public_mortgage_numeric_domains_are_typed(field,value):
    request=raw()
    if field=='cost':request['acquisition']['actual_cost']=value
    elif field=='yield':request['acquisition']['original_annual_effective_rate']=value
    elif field=='note':request['contract']['current_note_rate']=value
    elif field=='hazard':request['contract']['path'][0]['credit_hazards_per_year'][0][3]=value
    elif field=='principal':request['contract']['opening_investor_principal'][0]=value
    else:request['acquisition']['purchase_ordinal']=value
    with pytest.raises(ArithmeticError):oracle.reconstruct(request)
    with pytest.raises(ValueError,match='native model rejected input'):
        model_native.run(SCHEMA,request,timeout=30.)

def test_public_mortgage_process_does_not_use_python_reference_to_produce_result(monkeypatch):
    request=raw()
    with monkeypatch.context() as isolated:
        def forbidden(*args,**kwargs):pytest.fail('Python reference used by native producer')
        isolated.setattr(oracle,'reconstruct',forbidden)
        isolated.setattr(oracle.sequential_source,'project',forbidden)
        result,_=model_native.run(SCHEMA,request,timeout=30.)
    assert oracle.verify(request,result)['verified']

@pytest.mark.parametrize('failure',['missing','changed','cancel','deadline'])
def test_public_mortgage_transport_failure_has_no_fallback(monkeypatch,tmp_path,failure):
    request=raw()
    if failure=='missing':
        monkeypatch.setattr(model_native,'binary_path',lambda:tmp_path/'missing-lifecycle.exe')
        with pytest.raises(RuntimeError,match='no fallback'):model_native.run(SCHEMA,request)
    elif failure=='changed':
        with pytest.raises(RuntimeError,match='MODEL_VERSION_CHANGED'):
            model_native.run(SCHEMA,request,expected_binary='f'*64)
    elif failure=='cancel':
        with pytest.raises(InterruptedError):model_native.run(SCHEMA,request,cancelled=lambda:True)
    else:
        with pytest.raises(TimeoutError):model_native.run(SCHEMA,request,timeout=.001)


# Scoped new-entrypoint admission: shared historical MortgageRate parsing is
# intentionally unchanged. Raw tests retain fields before typed serialization.
FIXED_EXTRAS=[
    ('annual_rate',0.),('annual_rate',.15),('resets',[]),
    ('terms',{'margin':.02}),('assumption_override',{'hazard':9.}),
    ('ignored_source',None),
]


def _forbid_financial_callback(*args,**kwargs):
    pytest.fail('raw mortgage admission invoked a financial backend callback')


@pytest.mark.parametrize('side',['contract','original_contract'])
@pytest.mark.parametrize('key,value',FIXED_EXTRAS)
def test_public_new_mortgage_fixed_rate_extra_fields_reject_exact_raw(side,key,value,monkeypatch):
    request=raw();request[side]['rate'][key]=deepcopy(value);frozen=deepcopy(request)
    from portfolio_risk.core import quant_native,decision_native
    with monkeypatch.context() as reference:
        reference.setattr(model_native,'run',_forbid_financial_callback)
        reference.setattr(quant_native,'term_call',_forbid_financial_callback)
        reference.setattr(decision_native.NativeDecision,'call',_forbid_financial_callback)
        with pytest.raises(ArithmeticError,match='unknown/missing raw fields'):oracle.admit(request)
    with monkeypatch.context() as producer:
        producer.setattr(oracle,'admit',_forbid_financial_callback)
        producer.setattr(oracle,'reconstruct',_forbid_financial_callback)
        producer.setattr(oracle.sequential_source,'project',_forbid_financial_callback)
        with pytest.raises(ValueError) as error:model_native.run(SCHEMA,request,timeout=30.)
    assert str(error.value).startswith('native model rejected input: ')
    assert f'unknown field `{key}` in {side}.rate' in str(error.value)
    assert request==frozen


def _enum_alias_raw(side,field):
    if field.startswith('arm.'):
        request=deepcopy(next(case['request'] for case in CASES if case['accepted'] and case['request']['contract']['rate']['kind']=='arm'))
    else:request=raw()
    c=request[side]
    if field=='identity.measure':c['identity']['measure']={'physical':None}
    elif field=='day_count':c['day_count']={c['day_count']:None}
    elif field=='arm.index':c['rate']['terms']['index']['kind']={c['rate']['terms']['index']['kind']:None}
    elif field=='arm.recast':c['rate']['terms']['payment_recast']={c['rate']['terms']['payment_recast']:None}
    else:
        c['ownership']=dict(kind='agency',issuer='synthetic-issuer',guarantee=dict(id='synthetic-guarantee',revision='synthetic-v1'),remittance_lag_days=0,default_buyout_lag_days=0,servicing_and_guarantee_spread=0.,default_coupon_policy={'frozen_at_default':None})
    return request


ENUM_FIELDS={
 'identity.measure':'identity.measure','day_count':'day_count',
 'arm.index':'rate.terms.index.kind','arm.recast':'rate.terms.payment_recast',
 'agency.coupon':'ownership.default_coupon_policy',
}


@pytest.mark.parametrize('side',['contract','original_contract'])
@pytest.mark.parametrize('field',list(ENUM_FIELDS))
def test_public_new_mortgage_unit_enums_require_documented_string_wire(side,field,monkeypatch):
    request=_enum_alias_raw(side,field);frozen=deepcopy(request)
    with monkeypatch.context() as reference:
        reference.setattr(model_native,'run',_forbid_financial_callback)
        # Existing independent set-membership domains fail closed with TypeError
        # for dict aliases; no coercion or financial validator edit is introduced.
        with pytest.raises((ArithmeticError,TypeError)):oracle.admit(request)
    with monkeypatch.context() as producer:
        producer.setattr(oracle,'admit',_forbid_financial_callback)
        producer.setattr(oracle,'reconstruct',_forbid_financial_callback)
        producer.setattr(oracle.sequential_source,'project',_forbid_financial_callback)
        with pytest.raises(ValueError) as error:model_native.run(SCHEMA,request,timeout=30.)
    assert str(error.value).startswith('native model rejected input: ')
    assert side+'.'+ENUM_FIELDS[field] in str(error.value)
    assert request==frozen


def _raw_duplicate_contract_bytes(request,side,field):
    compact=lambda value:json.dumps(value,allow_nan=False,separators=(',',':'))
    body=compact(request[side])
    if field=='rate.kind':
        token='"rate":{"kind":"fixed"}'
        replacement='"rate":{"kind":"fixed","kind":"fixed"}'
    else:
        token='"current_note_rate":'+compact(request[side]['current_note_rate'])
        replacement=token+',"current_note_rate":'+compact(request[side]['current_note_rate'])
    assert body.count(token)==1
    duplicate=body.replace(token,replacement,1)
    envelope=compact(dict(schema=SCHEMA,threads=1,request=request))
    needle='"'+side+'":'+body
    assert envelope.count(needle)==1
    return envelope.replace(needle,'"'+side+'":'+duplicate,1).encode('utf-8')


@pytest.mark.parametrize('side',['contract','original_contract'])
@pytest.mark.parametrize('field',['rate.kind','current_note_rate'])
def test_public_new_mortgage_duplicate_keys_reject_actual_cli_input_bytes(side,field,monkeypatch):
    import subprocess
    request=raw();frozen=deepcopy(request);encoded=_raw_duplicate_contract_bytes(request,side,field)
    # A dict cannot express duplicates: send these exact bytes directly to the
    # same real public executable, without JSON-decoding/canonicalizing them.
    def unique_fields(pairs):
        result={}
        for key,value in pairs:
            if key in result:raise ArithmeticError(f'duplicate field `{key}`')
            result[key]=value
        return result
    with pytest.raises(ArithmeticError,match='duplicate field'):
        json.loads(encoded,object_pairs_hook=unique_fields)
    identity=model_native.binary_identity()
    with monkeypatch.context() as producer:
        producer.setattr(oracle,'reconstruct',_forbid_financial_callback)
        producer.setattr(oracle.sequential_source,'project',_forbid_financial_callback)
        process=subprocess.run([str(model_native.binary_path())],input=encoded,capture_output=True,timeout=30.)
    assert model_native.binary_identity()==identity
    assert process.returncode==1
    output=json.loads(process.stdout)
    assert set(output)=={'ok','error'} and output['ok'] is False
    assert 'duplicate field' in output['error'] and field.split('.')[-1] in output['error']
    assert side in output['error']
    assert request==frozen


@pytest.mark.parametrize('field',['accounting_accrued','economic_remainder'])
def test_public_new_mortgage_reviewed_interest_unit_aliases_reject_raw(field,monkeypatch):
    request=raw();interest=request['policy']['interest'];interest[field]={interest[field]:None};frozen=deepcopy(request)
    with monkeypatch.context() as reference:
        reference.setattr(model_native,'run',_forbid_financial_callback)
        with pytest.raises(ArithmeticError,match='reviewed book versus economic disposal policies'):oracle.admit(request)
    with monkeypatch.context() as producer:
        producer.setattr(oracle,'reconstruct',_forbid_financial_callback)
        producer.setattr(oracle.sequential_source,'project',_forbid_financial_callback)
        with pytest.raises(ValueError) as error:model_native.run(SCHEMA,request,timeout=30.)
    assert f'policy.interest.{field} must be an enum string' in str(error.value)
    assert request==frozen


@pytest.mark.parametrize('field',['accounting_accrued','collection_fraction'])
def test_public_new_mortgage_interest_duplicate_keys_reject_actual_bytes(field,monkeypatch):
    import subprocess
    request=raw();frozen=deepcopy(request)
    compact=lambda value:json.dumps(value,allow_nan=False,separators=(',',':'))
    token='"'+field+'":'+compact(request['policy']['interest'][field])
    encoded=compact(dict(schema=SCHEMA,threads=1,request=request))
    assert encoded.count(token)==1
    encoded=encoded.replace(token,token+','+token,1).encode('utf-8')
    def unique_fields(pairs):
        result={}
        for key,value in pairs:
            if key in result:raise ArithmeticError(f'duplicate field `{key}`')
            result[key]=value
        return result
    with pytest.raises(ArithmeticError,match='duplicate field'):
        json.loads(encoded,object_pairs_hook=unique_fields)
    identity=model_native.binary_identity()
    with monkeypatch.context() as producer:
        producer.setattr(oracle,'reconstruct',_forbid_financial_callback)
        producer.setattr(oracle.sequential_source,'project',_forbid_financial_callback)
        process=subprocess.run([str(model_native.binary_path())],input=encoded,capture_output=True,timeout=30.)
    assert model_native.binary_identity()==identity
    assert process.returncode==1
    output=json.loads(process.stdout)
    assert set(output)=={'ok','error'} and output['ok'] is False
    assert 'duplicate field' in output['error'] and field in output['error']
    assert 'policy.interest' in output['error']
    assert request==frozen
