"""Exact immutable cloned source identity versus calculated financial tolerances."""
from copy import deepcopy
import pytest
from portfolio_risk.analytics import market_indexed_reference as ref
from test_market_indexed_process import raw,native
def mutate(result,name):
    if name=='outer':result['cache_parent']['bridge']['original']['locked_oas']+=5e-10
    elif name=='bridge':result['bridge_receipt']['cache_parent']['original']['market']['curves'][0]['discount_factors'][1]+=5e-10
    elif name=='original':result['cases'][1]['original_parent']['market']['curves'][0]['discount_factors'][1]+=5e-10
    elif name=='oas':result['cases'][1]['original_locked_oas']+=5e-10
    elif name=='type':result['cases'][1]['original_locked_oas']=int(result['cases'][1]['original_locked_oas'])
    else:
        import json
        value=json.loads(result['cases'][1]['native_contract_descriptor']);value[6]+=5e-10
        result['cases'][1]['native_contract_descriptor']=json.dumps(value)
@pytest.mark.parametrize('name',['outer','bridge','original','oas','type','descriptor'])
def test_exact_source_fields_cannot_drift_inside_financial_tolerance(name):
    r=raw();result=deepcopy(ref.reconstruct(r));mutate(result,name)
    with pytest.raises(ArithmeticError):ref.verify(r,result)
@pytest.mark.parametrize('name',['bridge','original','oas','type','descriptor'])
def test_fresh_process_clones_are_exactly_bound_to_raw_source(name):
    r=raw();result=native(r);mutate(result,name)
    with pytest.raises(ArithmeticError):ref.verify(r,result)
def test_calculated_output_keeps_original_tolerance():
    r=raw();result=deepcopy(ref.reconstruct(r));result['cases'][1]['swap_result']['clean_value']+=5e-10
    assert ref.verify(r,result)['verified']

@pytest.mark.parametrize('field',['cache_parent','original_locked_oas','native_contract_descriptor'])
@pytest.mark.parametrize('original',[-0.0,0.0])
def test_immutable_float_zero_keeps_its_sign(field,original):
    import json
    r=raw();r['bridge']['original']['locked_oas']=original
    r['bridge']['contract']['fixed_rate']=original
    result=deepcopy(ref.reconstruct(r))
    if field=='cache_parent':result[field]['bridge']['original']['locked_oas']=-original
    elif field=='original_locked_oas':result['cases'][1][field]=-original
    else:
        value=json.loads(result['cases'][1][field]);value[6]=-original
        result['cases'][1][field]=json.dumps(value)
    with pytest.raises(ArithmeticError):ref.verify(r,result)

def test_fresh_native_signed_zero_source_is_preserved_and_bound():
    r=raw();r['bridge']['original']['locked_oas']=-0.0
    result=native(r);assert ref.verify(r,result)['verified']
    result['cases'][1]['original_locked_oas']=0.0
    with pytest.raises(ArithmeticError):ref.verify(r,result)
def test_historical_source_availability_keeps_explicit_unix_seconds():
    r=raw();r['bridge']['original']['market']['available_unix_seconds']=-1
    r['bridge']['current_market']['available_unix_seconds']=-1
    assert ref.verify(r,ref.reconstruct(r))['verified']
    assert ref.verify(r,native(r))['verified']
@pytest.mark.parametrize('side',['original','current_market'])
def test_source_available_before_year_one_fails_both_owners(side):
    r=raw();m=r['bridge']['original']['market'] if side=='original' else r['bridge'][side]
    m['available_unix_seconds']=(1-719163)*86400-1
    with pytest.raises(ArithmeticError):ref.reconstruct(r)
    with pytest.raises(ValueError):native(r)
