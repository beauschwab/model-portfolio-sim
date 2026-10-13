"""Explicit native consumer graph and preserved base calibration contract."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts as models
ROOT=Path(__file__).resolve().parents[3]
def example():return json.loads((ROOT/'examples/models/advanced-product-graph-1.json').read_text())
def test_native_sources_match_hand_cashflow_and_fixed_original_oas():
    raw=example();result=models.analyze_advanced_graph(raw);metric=result['metrics'][0]
    assert metric['pv']==pytest.approx(1044.5,abs=1e-9)
    assert sum(metric['gross_physical_contract_interest'])==pytest.approx(44.5,abs=1e-9)
    assert metric['supplied_history_dv01']==pytest.approx((529*math.sinh(.0025*29/365)+515.5*math.sinh(.0025*60/365))/25,abs=1e-9)
    assert {s['role'] for s in result['native_sources']}=={'physical','pricing','pricing_down','pricing_up'}
    current=deepcopy(raw);current['identity']['input_revision']='new-revision'
    assert models.reprice_advanced_graph(raw,current)['base_anchor']==result['base_anchor']
    changed=deepcopy(current);changed['positions'][0]['oas_target']['base_oas']=.01
    with pytest.raises(ValueError):models.reprice_advanced_graph(raw,changed)
def test_native_capture_complete_owners_and_policy_boundary():
    raw=example();owners=[dict(position_id='personal-1',account_id='bank',source_id='consumer-source',source_revision=raw['identity']['input_revision'],ownership='fully_scenario_generated',direction='receipt')]
    policy=dict(policy_id='reviewed-par-note',policy_revision='1',recognition_model='consumer_simple_interest_par-1',book_basis='par_note_simple_interest',fees='reject')
    result=models.capture_advanced_graph(raw,owners,policy)
    assert result['openings'][0]['principal']==1000.
    assert sum(e['components']['principal'] for e in result['sources'][0]['schedule']['events'])==1000.
    assert result['sources'][0]['schedule']['scenarios']==['base-run']
    assert all(s['origin_id'] for p in result['accrual_plans'] for s in p['segments'])
    with pytest.raises(ValueError):models.capture_advanced_graph(raw,[],policy)
    with pytest.raises(ValueError):models.capture_advanced_graph(raw,owners,dict(policy,book_basis='amortized_cost'))
