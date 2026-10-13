"""Native whole-book funding with cash/collateral separation."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts as models
ROOT=Path(__file__).resolve().parents[3]

def example():
    return json.loads((ROOT/'examples/models/funding-product-graph-1.json').read_text())

def test_cash_tail_margin_and_original_funding_anchor():
    raw=example();result=models.analyze_funding_graph(raw);metric=result['metrics'][0]
    assert metric['pv']==pytest.approx(102.2,abs=1e-9)
    assert sum(p['amounts']['margin_call_value'] for p in metric['physical_periods'])==pytest.approx(19.56,abs=1e-9)
    assert metric['physical_tail']['amounts']['interest_cash']>0.
    assert metric['physical_tail']['security_movements']
    for source in result['native_sources']:
        assert all(stock['encumbered_quantity_at_tail_end']==0. for stock in source['projection']['inventory'])
    current=deepcopy(raw);current['identity']['input_revision']='revision-2'
    assert models.reprice_funding_graph(raw,current)['base_anchor']==result['base_anchor']
    current['oas_target']['base_oas']=0.
    with pytest.raises(ValueError):models.reprice_funding_graph(raw,current)
