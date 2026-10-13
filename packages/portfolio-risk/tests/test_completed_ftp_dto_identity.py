"""Exact declared DTO clones, separately from opaque raw Value authority."""
from copy import deepcopy
import json,math
from pathlib import Path
import pytest
from portfolio_risk.analytics._modeled_product_dto_identity import mixed,deposit_owner,dto
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
from portfolio_risk.analytics.modeled_product_ftp_identity import mixed_clones
HERE=Path(__file__).parent/'fixtures/modeled_product_ftp'

def fixture(name):return json.loads((HERE/name).read_text())
def browser(v):
    if type(v)is dict:return{k:browser(x)for k,x in v.items()}
    if type(v)is list:return[browser(x)for x in v]
    return int(v)if type(v)is float and v.is_integer()else v

@pytest.mark.parametrize('value',[0,0.,-0.,True,False,1])
def test_opaque_specification_values_preserve_exact_type_and_bits(value):
    raw=fixture('card-reviewed-gross-funding.json')['source']['sources'][0]['input']['request']
    for spec in [raw['combined_specification'],raw['performing']['combined_specification'],raw['performing']['backbook']['specification']]:spec['transport_note']={'cash':value,'cost':value,'rate':value,'original_annual_effective_rate':value}
    out=mixed(raw)
    assert exact_json_identity(out['combined_specification'],raw['combined_specification'])
    assert exact_json_identity(out['performing']['combined_specification'],raw['performing']['combined_specification'])
    assert exact_json_identity(out['performing']['backbook']['specification'],raw['performing']['backbook']['specification'])

@pytest.mark.parametrize('v',[True,False,float('inf'),float('nan')])
def test_declared_f64_alias_and_nonfinite_refused(v):
    with pytest.raises(ValueError):dto({'note_rate':v},'InstallmentRequest')

def test_declared_dates_indices_and_bools_remain_exact():
    raw={'horizon_days':31,'as_of_ordinal':738916,'initial_purchase_grace':False,'opening_principal':[100,0,0]}
    typed=dto(raw,'CardRequest')
    assert type(typed['horizon_days'])is int and type(typed['as_of_ordinal'])is int and typed['initial_purchase_grace']is False
    assert all(type(v)is float for v in typed['opening_principal'])

def test_browser_integer_spelling_normalizes_all_typed_candidate_fields():
    raw=browser(fixture('card-reviewed-gross-funding.json')['source']['sources'][0]['input']['request']);out=mixed(raw)
    candidate=out['performing']['candidate'];u=candidate['product']['units'][0]
    assert all(type(v)is float for v in u['weights'].values())
    assert type(candidate['owners'][0]['pre_purchase_principal'])is float
    event=out['performing']['backbook']['dated']['sources'][0]['schedule']['events'][0]
    assert type(event['components']['cash_interest'])is float
    assert type(out['combined_specification']['accounts'][0]['cash'])is int

def test_deposit_typed_source_retains_opaque_specification():
    raw=browser(fixture('deposit-continuation.json')['source']['sources'][0]['input']['request']);out=deposit_owner(raw)
    assert exact_json_identity(raw['specification'],out['specification'])
    assert type(out['runs'][0]['severity'])is float
    assert type(out['runs'][0]['source']['continuation']['anchor']['original_principal'])is float
    assert type(out['runs'][0]['source']['continuation']['prefix']['through_ordinal'])is int

def test_actual_native_browser_clones_then_subtolerance_yield_mutation_refused():
    from portfolio_risk.core.product_credit_mixed_native import run
    raw=browser(fixture('card-reviewed-gross-funding.json')['source']['sources'][0]['input']['request'])
    native,_=run(raw,timeout=120.)
    mixed_clones(raw,native)
    changed=deepcopy(native)
    anchor=changed['product_sources'][0]['native']['result']['raw_request']['review']
    anchor['original_annual_effective_rate']=math.nextafter(anchor['original_annual_effective_rate'],math.inf)
    with pytest.raises(ValueError,match='clone'):mixed_clones(raw,changed)


def test_actual_native_raw_value_alias_and_signed_zero_mutations_refused():
    from portfolio_risk.core.product_credit_mixed_native import run
    raw=browser(fixture('card-reviewed-gross-funding.json')['source']['sources'][0]['input']['request'])
    native,_=run(raw,timeout=120.)
    for old,new in [(0,-0.),(1,True),(1,1.)]:
        # Exact clone seam only: computed outputs cannot authorize raw Value changes.
        original=deepcopy(raw);receipt=deepcopy(native)
        original['combined_specification']['identity_note']={'rate':old}
        receipt['preparation']['raw_request']['combined_specification']['identity_note']={'rate':old}
        mixed_clones(original,receipt)
        receipt['preparation']['raw_request']['combined_specification']['identity_note']['rate']=new
        with pytest.raises(ValueError,match='clone'):mixed_clones(original,receipt)
