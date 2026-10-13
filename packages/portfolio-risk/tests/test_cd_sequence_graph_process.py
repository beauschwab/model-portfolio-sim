"""Authored real default-process acceptance; pending installation/rebuild."""
from pathlib import Path
from copy import deepcopy
import json,math
import pytest
from portfolio_risk.core import model_native,cd_sequence_native
from portfolio_risk.core.indexed_book_job_native import JobBudget
from portfolio_risk.analytics import cd_sequence_graph_reference as reference
CASES=json.loads((Path(__file__).with_name('fixtures')/'cd-sequence-graph2/cases.json').read_bytes())

def actual(case,monkeypatch):
    path=model_native.binary_path().resolve()
    assert path.parent.name=='release' and path.name in ('portfolio-lifecycle','portfolio-lifecycle.exe')
    budget=JobBudget(30.,lambda:False)
    result,execution=cd_sequence_native.run(case['schema'],case['raw'],budget=budget,expected_binary=model_native.binary_identity(),threads=1)
    return result,execution

@pytest.mark.parametrize('index',[i for i,c in enumerate(CASES)if c['accepted']])
def test_complete_actual_native_original_current_cash_and_preview(index,monkeypatch):
    case=CASES[index];result,execution=actual(case,monkeypatch)
    receipt=reference.verify_request(case['schema'],case['raw'],result)
    assert receipt['complete_preview_tails_verified']is True
    assert execution['runtime_control']=='process-watchdog-1' and execution['caller_cooperative_native_control']is False
    if index==26:
        assert math.isclose(result['metrics'][0]['pv'],-101.,rel_tol=1e-12,abs_tol=1e-9)
        assert math.isclose(result['metrics'][0]['supplied_history_dv01'],-101.*math.sinh(.0001*10/365),rel_tol=1e-12,abs_tol=1e-9)
    if index==29:
        e=result['native_sources'][0]['projection']['result']['schedule']['events'][-1]
        assert abs(e['components']['cash_interest'])==8.881784197001252e-16 and e['components']['principal']==0.

@pytest.mark.parametrize('index',range(5))
def test_actual_native_rejection_has_no_reference(index,monkeypatch):
    with pytest.raises(ValueError,match='native CD rejected'):actual(CASES[index],monkeypatch)

@pytest.mark.parametrize('field',['preview','original_money','current_money','oas','quote','source','date','type','receipt','marker','period_count'])
def test_complete_source_or_price_tamper_rejected(field,monkeypatch):
    case=CASES[12];result,_=actual(case,monkeypatch);bad=deepcopy(result)
    if field=='preview':bad['native_sources'][0]['projection']['result']['action_receipts'][0]['native']['remaining_schedule']['events'][0]['components']['principal']+=1
    elif field=='original_money':bad['original_native_sources'][0]['projection']['result']['schedule']['events'][0]['components']['cash_interest']+=1
    elif field=='current_money':bad['metrics'][0]['pv']+=1
    elif field=='oas':bad['base_anchor']['oas'][0]['base_oas']+=1e-15
    elif field=='quote':bad['base_anchor']['calibration_input'][-1][0]['signed_cash_price']=math.nextafter(bad['base_anchor']['calibration_input'][-1][0]['signed_cash_price'],math.inf)
    elif field=='source':bad['native_sources'][0]['product']['request']['terminal_projection_source']['revision']='other'
    elif field=='date':bad['native_sources'][0]['projection']['result']['schedule']['events'][0]['payment_ordinal']+=1
    elif field=='type':bad['native_sources'][0]['projection']['result']['schedule']['events'][0]['payment_ordinal']=float(bad['native_sources'][0]['projection']['result']['schedule']['events'][0]['payment_ordinal'])
    elif field=='receipt':bad['native_sources'][0]['projection']['result']['action_receipts'].pop()
    elif field=='marker':bad['sequence_policy']['capital_ftp_eligible']=True
    else:bad['metrics'][0]['physical_periods']=[]
    with pytest.raises(ValueError):reference.verify_request(case['schema'],case['raw'],bad)

def test_reference_deadline_cancel_during_complete_replay(monkeypatch):
    case=CASES[12];result,_=actual(case,monkeypatch);calls=[0]
    def check():
        calls[0]+=1
        if calls[0]==100:raise TimeoutError('original job budget')
        return False
    with pytest.raises(TimeoutError,match='original job budget'):reference.verify_request(case['schema'],case['raw'],result,check)

@pytest.mark.parametrize('field',['income_policy','pricing_policy','source_status','extra_source','extra_projection','extra_schedule','extra_metric','missing_cash_lane','signed_zero'])
def test_complete_policy_fields_and_signed_zero_are_exact(field,monkeypatch):
    case=CASES[12];result,_=actual(case,monkeypatch);bad=deepcopy(result)
    if field in ('income_policy','pricing_policy','source_status'):bad[field]='unsupported claim'
    elif field=='extra_source':bad['native_sources'][0]['new_cash_override']=0.
    elif field=='extra_projection':bad['native_sources'][0]['projection']['extra']=0.
    elif field=='extra_schedule':bad['native_sources'][0]['projection']['result']['schedule']['extra']=0.
    elif field=='extra_metric':bad['metrics'][0]['extra']=0.
    elif field=='missing_cash_lane':bad['metrics'][0]['physical_periods'][0].pop('cd_penalty')
    else:
        assert bad['native_sources'][0]['product']['request']['opening']['accrued_interest']==0.
        bad['native_sources'][0]['product']['request']['opening']['accrued_interest']=-0.0
    with pytest.raises(ValueError):reference.verify_request(case['schema'],case['raw'],bad)

@pytest.mark.parametrize('lane',['cash','state'])
def test_exact_tiny_post_principal_zero_right_is_not_erasable(lane,monkeypatch):
    case=CASES[29];result,_=actual(case,monkeypatch);bad=deepcopy(result)
    projection=bad['native_sources'][0]['projection']['result']
    if lane=='cash':
        assert projection['schedule']['events'][-1]['components']['cash_interest']!=0.
        projection['schedule']['events'][-1]['components']['cash_interest']=0.
    else:
        assert projection['live_before_terminal_settlement'][0]['accrued_interest']!=0.
        projection['live_before_terminal_settlement'][0]['accrued_interest']=0.
    with pytest.raises(ValueError,match='erased retained zero-principal'):reference.verify_request(case['schema'],case['raw'],bad)
