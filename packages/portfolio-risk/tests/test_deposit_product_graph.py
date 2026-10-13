"""Native deposit liability graph versus independent actual-day hand values."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts as models
ROOT=Path(__file__).resolve().parents[3]

def example():
    return json.loads((ROOT/'examples/models/deposit-product-graph-1.json').read_text())

def test_liability_cash_expense_tail_and_original_anchor():
    raw=example();result=models.analyze_deposit_graph(raw);metric=result['metrics'][0]
    assert metric['liability_pv']==pytest.approx(1002.,abs=1e-9)
    assert metric['bank_cash_pv']==pytest.approx(-1002.,abs=1e-9)
    assert metric['supplied_history_liability_dv01']==pytest.approx(1002*math.sinh(.0001*2/365),abs=1e-9)
    assert sum(p['interest_expense'] for p in metric['physical_periods'])+metric['physical_tail']['interest_expense']==pytest.approx(2.)
    assert sum(p['bank_interest_cash'] for p in metric['physical_periods'])+metric['physical_tail']['bank_interest_cash']==pytest.approx(-2.)
    current=deepcopy(raw);current['identity']['input_revision']='revision-2'
    assert models.reprice_deposit_graph(raw,current)['base_anchor']==result['base_anchor']
    current['positions'][0]['oas_target']['base_oas']=0.
    with pytest.raises(ValueError):models.reprice_deposit_graph(raw,current)

def test_deposit_tail_cannot_substitute_franchise_value_for_liability():
    raw=example()
    for role in ('physical','pricing','pricing_down','pricing_up'):
        raw['positions'][0]['histories'][0][role][0]['request']['terminal_policy']={'kind':'franchise_value_only','rationale':'explicit missing cash tail'}
    with pytest.raises(ValueError):models.analyze_deposit_graph(raw)
