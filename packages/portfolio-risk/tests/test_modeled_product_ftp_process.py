"""Default native-only product-source FTP process; run after official rebuild."""
from pathlib import Path
from copy import deepcopy
import json
import pytest
from portfolio_risk.core import modeled_product_ftp_native as native
from portfolio_risk.analytics import modeled_product_ftp_reference as reference

FIXTURES=Path(__file__).parent/'fixtures/modeled_product_ftp'

@pytest.mark.parametrize('name',['deposit-continuation','consumer-initial-allowance','card-reviewed-gross-funding','mortgage','deposit-portfolio-two40','deposit-portfolio-one90'])
def test_default_actual_source_and_independent_full_journal_charge(name,monkeypatch):
    from portfolio_risk.core import model_native,mortgage_capital_native
    def forbidden(*a,**k):raise AssertionError('Python source coordinator or finance fallback')
    monkeypatch.setattr(model_native,'run',forbidden)
    monkeypatch.setattr(mortgage_capital_native,'run',forbidden)
    raw=json.loads((FIXTURES/f'{name}.json').read_text())
    out,execution=native.run(raw,expected_binary=native.binary_identity(),timeout=120.)
    assert execution['backend']=='rust' and reference.verify(raw,out)['verified']
    assert out['result']['capital_ftp_eligible']is False

def test_default_binary_cancel_and_identity_failure_have_no_fallback():
    raw=json.loads((FIXTURES/'deposit-continuation.json').read_text())
    with pytest.raises(InterruptedError):native.run(raw,cancelled=lambda:True,timeout=10.)
    with pytest.raises(RuntimeError,match='executable'):native.run(raw,expected_binary='0'*64,timeout=10.)

def test_default_new_contract_rejects_python_runtime_and_incomplete_source():
    raw=json.loads((FIXTURES/'deposit-continuation.json').read_text())
    invalid=deepcopy(raw);invalid['compute_backend']='python'
    with pytest.raises(ValueError):native.run(invalid,timeout=10.)
    invalid=deepcopy(raw);invalid['source']['reviews'].pop()
    with pytest.raises(ValueError):native.run(invalid,timeout=10.)
