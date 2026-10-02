"""Legacy identity, generated ABI layout and named transport rejection gates."""
import importlib
import pytest
from portfolio_risk.compat import ALIASES
from portfolio_risk.core import quant_native
from portfolio_risk.core.abi_generated import Op, LAYOUTS, ABI_VERSION


def test_legacy_imports_preserve_module_and_monkeypatch_identity(monkeypatch):
    for alias,canonical in ALIASES.items():
        old=importlib.import_module('portfolio_risk.'+alias)
        new=importlib.import_module('portfolio_risk.'+canonical)
        assert old is new
        marker=object();monkeypatch.setattr(old,'_compatibility_marker',marker,raising=False)
        assert new._compatibility_marker is marker


def test_operation_ids_are_stable_and_named_input_order_is_explicit(monkeypatch):
    assert ABI_VERSION==8 and list(map(int,Op))==list(range(1,32))
    seen=[]
    monkeypatch.setattr(quant_native,'call',lambda op,inputs,shapes:seen.append((op,inputs,shapes)) or ('result',))
    assert quant_native.call_named(Op.MORTGAGE_CACHE,{'clear':1},{'statistics':(5,)})=={'statistics':'result'}
    assert seen==[(Op.MORTGAGE_CACHE,[1],[(5,)])]
    with pytest.raises(ValueError,match='named buffers'):
        quant_native.call_named(Op.MORTGAGE_CACHE,{'wrong':1},{'statistics':(5,)})
    assert len(seen)==1


def test_invalid_buffer_counts_fail_before_loading_or_allocating(monkeypatch):
    def forbidden(*args):raise AssertionError('native loading must not occur')
    monkeypatch.setattr(quant_native,'_load',forbidden)
    with pytest.raises(ValueError,match='buffer count'):
        quant_native.call(Op.CSR_OAS,[],[(1,),(1,)])
    with pytest.raises(ValueError,match='unknown native operation'):
        quant_native.call(99,[],[])
