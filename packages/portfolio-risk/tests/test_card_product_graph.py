"""Independent hand checks at the native revolving-card graph boundary."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts as models
ROOT=Path(__file__).resolve().parents[3]

def example():
    return json.loads((ROOT/'examples/models/card-product-graph-1.json').read_text())

def test_card_graph_retains_cash_recognition_and_original_oas():
    raw=example();result=models.analyze_card_graph(raw);metric=result['metrics'][0]
    assert metric['pv']==pytest.approx(1001.,abs=1e-9)
    assert metric['supplied_history_dv01']==pytest.approx(1001*math.sinh(.0001*2/365),abs=1e-9)
    assert sum(p['gross_contract_interest'] for p in metric['physical_periods'])==pytest.approx(1.)
    assert sum(p['interest_cash'] for p in metric['physical_periods'])==pytest.approx(1.)
    assert {s['role'] for s in result['native_sources']}=={'physical','pricing','pricing_down','pricing_up'}
    current=deepcopy(raw);current['identity']['input_revision']='revision-2'
    assert models.reprice_card_graph(raw,current)['base_anchor']==result['base_anchor']
    current['positions'][0]['oas_target']['base_oas']=0.
    with pytest.raises(ValueError):models.reprice_card_graph(raw,current)

def test_card_graph_requires_explicit_full_commitment_tail():
    raw=example()
    for role in ('physical','pricing','pricing_down','pricing_up'):
        raw['positions'][0]['histories'][0][role][0]['steps'].pop()
    with pytest.raises(ValueError):models.analyze_card_graph(raw)
