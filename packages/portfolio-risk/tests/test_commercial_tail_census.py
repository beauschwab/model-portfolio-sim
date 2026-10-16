"""Explicit complete finite tail census; genuine graph/journal and cut refusal."""
from copy import deepcopy
import json,os
from pathlib import Path
import pytest
from portfolio_risk.core.commercial_candidate_native import run
from portfolio_risk.analytics.commercial_candidate_reference import verify
ROOT=Path(__file__).resolve().parents[3]
@pytest.fixture(autouse=True)
def artifacts(monkeypatch):
    selected=os.environ.get('TAIL_NATIVE_DEBUG_DIR')
    if selected:
        from portfolio_risk.core import product_strategy_native
        monkeypatch.setattr(product_strategy_native,'binary_path',lambda:Path(selected)/'portfolio-strategy.exe')

def cut_request():return json.loads((ROOT/'packages/portfolio-risk/tests/fixtures/commercial-short-secondary-cut-refusal.json').read_text())
def request():return cut_request()['completed_candidate']
def source(actual,role='physical'):
    return next(s for s in actual['preparation']['native_preparation']['source']['native_graphs'][0]['native_sources']if s['role']==role)['projection']

def test_actual_selected_candidate_keeps_complete_origin_and_separate_accounting_roles():
    raw=request();actual,_=run(raw,timeout=90.);assert verify(raw,actual)['purchased_journal_replayed']
    full=source(actual);q=source(actual,'pricing')
    assert full['selected_accounting_owner']=='reviewed_coupon_book-1'and full['admission']['book_work_upper']>0
    assert q['selected_accounting_owner']=='economic_carry_only-1'and q['admission']['book_work_upper']==q['admission']['book_bytes_upper']==0
    receipt=full['carry']['tail_census'];assert receipt['maximum_recoveries']==1 and receipt['source_cash_per_path']==1 and receipt['gross_coupon_claims_per_path']==4
    assert receipt['observed']['scheduled_source_cash']==1
    assert any(l['earning_origin_index']==3 for g in full['accounting']['groups']for l in g['next']['lots'])

@pytest.mark.parametrize('target',['tail','cash','claims','observed','shape','work','phase'])
def test_independent_raw_replay_rejects_selected_capacity_and_role_tampering(target):
    raw=request();actual,_=run(raw,timeout=90.);projection=source(actual);receipt=projection['carry']['tail_census']
    if target=='tail':receipt['maximum_recoveries']+=1
    elif target=='cash':receipt['source_cash_per_path']+=1
    elif target=='claims':receipt['gross_coupon_claims_per_path']+=1
    elif target=='observed':receipt['observed']['points']+=1
    elif target=='shape':receipt['source_shape']['canonical_coupons']+=1
    elif target=='work':receipt['additional_work_upper']-=1
    else:projection['selected_accounting_owner']='economic_carry_only-1'
    with pytest.raises((ArithmeticError,ValueError)):verify(raw,actual)

def test_zero_allocation_still_carries_full_unweighted_sources_and_census():
    raw=request();raw['allocation'][0]['face']=0.;actual,_=run(raw,timeout=90.)
    assert verify(raw,actual)['verified'];projection=source(actual)
    assert projection['carry']['tail_census']['observed']['scheduled_source_cash']==1
    assert projection['carry']['carry_points'][0]['principal']==100.

def test_actual_source_cancellation_and_original_deadline_are_retained():
    with pytest.raises(InterruptedError):run(request(),cancelled=lambda:True,timeout=90.)
    with pytest.raises((TimeoutError,ValueError)):run(request(),timeout=.001)

def test_short_complete_cut_keeps_root_refusal_before_generation():
    from portfolio_risk.core import commercial_cut_mixed_native as cut
    from portfolio_risk.core import product_strategy_native
    cut.binary_path=product_strategy_native.binary_path
    with pytest.raises(ValueError,match='native cut joined32M/128MiB before dual graph generation'):
        cut.run(cut_request(),timeout=90.)
