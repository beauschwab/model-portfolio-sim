"""Exact original JSON and declared Float64 output are separate contracts."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
import polars as pl
from portfolio_risk.analytics import model_ledger, mortgage_mixed_reference as mortgage
from portfolio_risk.analytics import deposit_revised_income_reference as deposit
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity

ROOT=Path(__file__).resolve().parents[3]


def browser_numbers(value):
    if type(value) is float and value.is_integer():return int(value)
    if type(value) is dict:return {k:browser_numbers(v) for k,v in value.items()}
    if type(value) is list:return [browser_numbers(v) for v in value]
    return value


@pytest.fixture(scope='module')
def mortgage_receipt(tmp_path_factory):
    raw=browser_numbers(json.loads((ROOT/'examples/models/mortgage-mixed-accounting-1.json').read_text()))
    manifest=model_ledger.run_model_ledger(raw,tmp_path_factory.mktemp('typed-mortgage'),timeout=60.)
    header=json.loads((manifest.parent/'prepared-source.json').read_text())
    return raw,header['source_receipt']['raw_request']


def test_native_mortgage_typed_receipt_preserves_original_specification_value(mortgage_receipt):
    raw,retained=mortgage_receipt
    mortgage.verify_retained_request_identity(raw,retained)
    assert not exact_json_identity(raw,retained)
    assert exact_json_identity(raw['dated']['specification'],retained['dated']['specification'])


@pytest.mark.parametrize('mutation',['float_type','float_epsilon','negative_zero','integer_date','boolean','raw_value','unknown'])
def test_native_typed_receipt_cannot_change_exact_types_values_or_fields(mortgage_receipt,mutation):
    raw,retained=mortgage_receipt; changed=deepcopy(retained)
    if mutation=='float_type':changed['owners'][0]['unit_face']=int(changed['owners'][0]['unit_face'])
    elif mutation=='float_epsilon':changed['owners'][0]['unit_face']+=1e-12
    elif mutation=='negative_zero':changed['sources'][0]['request']['source']['contract']['opening_investor_principal'][1]=-0.
    elif mutation=='integer_date':changed['reporting_end_ordinal']=float(changed['reporting_end_ordinal'])
    elif mutation=='boolean':changed['owners'][0]['allocated_face']=True
    elif mutation=='raw_value':changed['dated']['specification']['accounts'][0]['cash']=float(changed['dated']['specification']['accounts'][0]['cash'])
    else:changed['caller_verified']=True
    with pytest.raises(ArithmeticError):mortgage.verify_retained_request_identity(raw,changed)


def test_default_deposit_integer_request_has_float64_reports_and_exact_raw(tmp_path):
    raw=browser_numbers(json.loads((ROOT/'examples/models/deposit-revised-income-ledger-1.json').read_text()))
    manifest=model_ledger.run_model_ledger(raw,tmp_path,partition_rows=7,timeout=60.)
    header=json.loads((manifest.parent/'prepared-source.json').read_text())
    control=deposit.verify_preparation(raw,header['prepared_request'],header['source_receipt'])
    assert exact_json_identity(control.raw,raw)
    assert type(control.raw['source']['anchor']['original_principal']) is int
    for table,rows in control.tables.items():
        for row in rows:
            for field,dtype in deposit.SCHEMAS[table].items():
                if dtype==pl.Float64 and field in row and row[field] is not None:
                    assert type(row[field]) is float,(table,field)
    altered=deepcopy(raw);altered['source']['anchor']['original_principal']=float(altered['source']['anchor']['original_principal'])
    with pytest.raises(ArithmeticError):deposit.verify_preparation(altered,header['prepared_request'],header['source_receipt'])


def test_original_financial_tolerance_and_metadata_identity_remain_unchanged():
    deposit.source.equal(0.,0.)
    with pytest.raises(ArithmeticError):deposit.source.equal(0.0001,0.)
    with pytest.raises(ArithmeticError):deposit.source.equal(0.,0)
    with pytest.raises(ArithmeticError):deposit.source.checks.exact(-0.,0.)
