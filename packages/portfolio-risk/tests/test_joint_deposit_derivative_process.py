"""Actual default dispatcher gates; pending reviewed native application/build."""
from pathlib import Path
import pytest
from portfolio_risk.analytics import joint_mixed_runtime as runtime
from portfolio_risk.analytics.saved_derivative_identity import loads
FIXTURES=Path(__file__).parent/'fixtures/joint_deposit_derivative'

@pytest.mark.parametrize('name',['base','mpor','physical-option','default-im','seasoned-liability','xva'])
def test_default_one_pooled_native_journal(name,tmp_path,monkeypatch):
    from portfolio_risk.core import quant_native,native
    from portfolio_risk.analytics import ledger_native,derivative_verification
    def forbidden(*a,**kw):raise AssertionError('native DLL/Python finance fallback forbidden')
    for module,key in [(quant_native,'_load'),(quant_native,'call'),(quant_native,'term_call'),(native,'discount_backend'),(ledger_native,'validate_spec'),(derivative_verification,'_derive')]:monkeypatch.setattr(module,key,forbidden)
    manifest=runtime.run(loads((FIXTURES/(name+'.json')).read_bytes()),tmp_path,timeout=120.,partition_rows=7)
    receipt=loads(manifest.read_bytes())['validation']
    assert receipt['ordered_pooled_same_sim_journal_replayed']is True
    assert receipt['single_daily_gl_verified']is True
    assert receipt['full_unweighted_gross_origins_verified']is True
    assert receipt['capital_ftp_eligible']is False

def test_default_distinct_joined_sources_refuse_without_manifest(tmp_path):
    with pytest.raises((ValueError,ArithmeticError,RuntimeError)):
        runtime.run(loads((FIXTURES/'two-deposits.json').read_bytes()),tmp_path,timeout=30.)
    assert not list(tmp_path.rglob('manifest.json'))
