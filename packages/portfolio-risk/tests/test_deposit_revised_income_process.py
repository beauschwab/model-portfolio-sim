"""Installed-default entrypoint gates; no reference backend or artifact imports."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_ledger


@pytest.fixture
def raw():
    root=Path(__file__).resolve().parents[3]
    return json.loads((root/'examples/models/deposit-revised-income-ledger-1.json').read_text())


def forbidden(*args,**kwargs):
    raise AssertionError('Python production financial callback forbidden')


def test_default_native_revised_expectations_preserve_anchors_and_gross_rights(raw,tmp_path,monkeypatch):
    monkeypatch.setattr(model_ledger,'validate_spec',forbidden)
    monkeypatch.setattr(model_ledger,'_finalize',forbidden)
    assert model_ledger.binary_path().is_file(),'rebuild the default model-ledger executable'
    path=model_ledger.run_model_ledger(raw,tmp_path,partition_rows=17,timeout=45.)
    manifest=json.loads(path.read_text());receipt=manifest['validation']
    assert all(receipt[k] is True for k in ['verified','physical_deposit_source_recomputed',
        'original_yield_revision_recomputed','ordered_deposit_journal_recomputed','complete_gross_cash_origin_rights_verified'])
    assert all(receipt[k] is False for k in ['accounting_standard_approval','capital_ftp_eligible','installed_adoption'])
    header=json.loads((path.parent/'prepared-source.json').read_text())
    assert model_ledger.exact_json_identity(header['source_receipt']['raw_request'],raw)
    assert header['source_receipt']['original_anchors']==raw['source']['anchor']
    assert json.loads((path.parent/'producer-completion.json').read_text())=={'complete':True,'protocol':3}


@pytest.mark.parametrize('mutation',['contract_change','unknown_policy','negative_yield','inflow'])
def test_default_revised_unsupported_source_fails_without_publication(raw,tmp_path,mutation):
    r=deepcopy(raw)
    if mutation=='contract_change':r['source']['runs'][0]['revised']['segments'][0]['opening_paid_rate']+=.01
    elif mutation=='unknown_policy':r['source']['policy']['pnl_policy']='automatic_accounting_approval'
    elif mutation=='negative_yield':r['source']['anchor']['original_annual_effective_rate']=-.01
    else:r['source']['runs'][0]['revised']['drivers'][0]['external_inflows'][0]=1.
    with pytest.raises((RuntimeError,ValueError,ArithmeticError)):
        model_ledger.run_model_ledger(r,tmp_path,timeout=45.)
    assert not list(tmp_path.glob('run-*/manifest.json'))
