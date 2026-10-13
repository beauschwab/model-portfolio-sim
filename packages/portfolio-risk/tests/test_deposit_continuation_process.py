"""Default native process gate for supplied purchase-to-revision deposit ownership."""
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_ledger

SCHEMA='deposit-performing-income-ledger-1'
def raw():return json.loads((Path(__file__).resolve().parents[3]/'examples/models'/f'{SCHEMA}.json').read_text())

def test_default_native_continuation_reopens_independently_verified_partitions(tmp_path,monkeypatch):
    def forbidden(*args,**kwargs):raise AssertionError('Python production financial callback forbidden')
    monkeypatch.setattr(model_ledger,'validate_spec',forbidden);monkeypatch.setattr(model_ledger,'_finalize',forbidden)
    assert model_ledger.binary_path().parent.name=='release'
    path=model_ledger.run_model_ledger(raw(),tmp_path,partition_rows=7,timeout=30)
    manifest=json.loads(path.read_text());receipt=manifest['validation']
    for key in ('physical_deposit_source_principal_recomputed','original_yield_continuation_recomputed','gross_origin_rights_reconciled','ordered_same_sim_journal_replayed'):assert receipt[key] is True
    for key in ('accounting_standard_approval','installed_adoption','saved_book_adoption','capital_ftp_eligible'):assert receipt[key] is False
    assert json.loads((path.parent/'producer-completion.json').read_text())=={'complete':True,'protocol':3}
    header=json.loads((path.parent/'prepared-source.json').read_text())
    assert model_ledger.exact_json_identity(header['source_receipt']['raw_request'],raw())
    assert len(manifest['tables']['journal']['parts'])>1

@pytest.mark.parametrize('case',['unknown_prefix','unknown_weight','contract_change'])
def test_default_native_continuation_refuses_unowned_economics(tmp_path,case):
    value=raw()
    if case=='unknown_prefix':value['runs'][0]['source']['continuation']['current']['drivers'][0]['reference_rate']+=.01
    elif case=='unknown_weight':value['runs'][0]['source_weight']=0.
    else:value['runs'][1]['source']['continuation']['current']['segments'][0]['opening_paid_rate']+=.01
    with pytest.raises((ValueError,RuntimeError,ArithmeticError)):model_ledger.run_model_ledger(value,tmp_path,timeout=30)
    assert not list(tmp_path.rglob('manifest.json'))
