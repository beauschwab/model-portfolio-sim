"""Official default process gates, to run after reviewed native composition/rebuild.
No environment selector, artifact import, reference backend or skip is admitted.
"""
from pathlib import Path
import pytest
from portfolio_risk.analytics import saved_derivative_runtime as runtime
from portfolio_risk.analytics.saved_derivative_identity import loads
FIXTURES=Path(__file__).parent/'fixtures/saved_derivative_mixed'

@pytest.mark.parametrize('name',['base','mpor','signed-im-interest','seasoned-liability'])
def test_default_full_derivative_source_and_ordered_journal(name,tmp_path,monkeypatch):
    from portfolio_risk.core import quant_native,native
    from portfolio_risk.analytics import ledger_native,derivative_verification
    def forbidden(*a,**k):raise AssertionError('financial callback/DLL fallback forbidden')
    for module,attribute in [(quant_native,'_load'),(quant_native,'call'),(quant_native,'term_call'),(native,'discount_backend'),(ledger_native,'validate_spec'),(derivative_verification,'_derive')]:monkeypatch.setattr(module,attribute,forbidden)
    raw=loads((FIXTURES/f'{name}.json').read_bytes())
    manifest=runtime.run(raw,tmp_path,timeout=120.,partition_rows=7)
    receipt=loads(manifest.read_bytes())['validation']
    assert receipt['native_derivative_source_recomputed']is True
    assert receipt['ordered_mixed_journal_recomputed']is True
    assert receipt['complete_gross_rights_verified']is True
    assert receipt['capital_ftp_eligible']is False

def test_default_cancel_and_executable_mismatch_leave_no_manifest(tmp_path):
    raw=loads((FIXTURES/'base.json').read_bytes())
    with pytest.raises(InterruptedError):runtime.run(raw,tmp_path,cancelled=lambda:True,timeout=10.)
    with pytest.raises(RuntimeError):runtime.run(raw,tmp_path,expected_binary='0'*64,timeout=10.)
    assert not list(tmp_path.rglob('manifest.json'))
