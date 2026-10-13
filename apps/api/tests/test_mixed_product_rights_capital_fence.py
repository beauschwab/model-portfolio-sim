"""The new rights source cannot enter treasury before its explicit mapping gate."""
from copy import deepcopy
import pytest
from app import treasury_store


def execution():
    return dict(schema='product-mixed-product-credit-1', backend='rust',
        summary=dict(protocol=3, complete=True, full_end_ordinal=739900),
        source_verification=dict(verified=True, mixed_product_credit_only=True,
            ordered_mixed_journal_recomputed=True, complete_cash_rights_basis_tail_verified=True))


@pytest.mark.parametrize('value', [False, 0, 1, 'true', None])
def test_explicit_unverified_or_nonboolean_eligibility_fails(monkeypatch, value):
    raw=execution();raw['summary']['capital_ftp_eligible']=value
    monkeypatch.setattr(treasury_store, '_metadata', lambda *_: dict(execution=raw))
    with pytest.raises(ValueError, match='eligibility gates'):
        treasury_store._mixed_protocol({})


@pytest.mark.parametrize('change', ['missing', 'summary_only', 'receipt_only'])
def test_v2_source_requires_both_independent_and_native_eligibility(monkeypatch, change):
    raw=execution();raw['source_verification'].update(card_v2_rights_owned=True, capital_ftp_eligible=False)
    if change=='summary_only':raw['summary']['closing_capital_eligible']=True
    if change=='receipt_only':raw['source_verification']['closing_capital_eligible']=True
    monkeypatch.setattr(treasury_store, '_metadata', lambda *_: dict(execution=raw))
    with pytest.raises(ValueError, match='reviewed closing capital mapping gate'):
        treasury_store._mixed_protocol({})


@pytest.mark.parametrize('side',['summary','source_verification'])
@pytest.mark.parametrize('value',[False,0,1,'true',None])
def test_v2_closing_capital_requires_exact_boolean_on_both_sources(monkeypatch,side,value):
    raw=execution();raw['source_verification'].update(card_v2_rights_owned=True,
        capital_ftp_eligible=False,closing_capital_eligible=True)
    raw['summary'].update(capital_ftp_eligible=False,closing_capital_eligible=True)
    raw[side]['closing_capital_eligible']=value
    monkeypatch.setattr(treasury_store,'_metadata',lambda *_:dict(execution=raw))
    with pytest.raises(ValueError,match='closing capital mapping gate'):treasury_store._mixed_protocol({})


def test_verified_closing_capital_is_distinct_from_false_ftp_eligibility(monkeypatch):
    raw=execution();raw['source_verification'].update(card_v2_rights_owned=True,
        capital_ftp_eligible=False,closing_capital_eligible=True)
    raw['summary'].update(capital_ftp_eligible=False,closing_capital_eligible=True)
    monkeypatch.setattr(treasury_store,'_metadata',lambda *_:dict(execution=raw))
    assert treasury_store._mixed_protocol({})==739900


def test_existing_verified_source_without_new_marker_remains_compatible(monkeypatch):
    raw=execution();before=deepcopy(raw)
    monkeypatch.setattr(treasury_store, '_metadata', lambda *_: dict(execution=raw))
    assert treasury_store._mixed_protocol({})==739900
    assert raw==before
