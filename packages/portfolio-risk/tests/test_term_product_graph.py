"""Actual-date term/CD graph hand checks at the public native boundary."""
from copy import deepcopy
import json
from math import sinh
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts as models

ROOT=Path(__file__).resolve().parents[3]

def example():
    return json.loads((ROOT/'examples/models/term-product-graph-1.json').read_text())

def test_fixed_term_cash_tail_and_unchanged_original_spread():
    raw=example(); result=models.analyze_term_product_graph(raw)
    metric=next(row for row in result['metrics'] if row['position_id']=='loan:example')
    assert metric['pv']==pytest.approx(102.9,abs=1e-9)
    assert metric['physical_tail']['principal_cash']==100.
    assert metric['physical_tail']['interest_cash']==pytest.approx(2.9,abs=1e-9)
    assert metric['physical_periods'][0]['interest_cash']==0.
    assert metric['supplied_history_dv01']==pytest.approx(102.9*sinh(.0001*29/365),abs=1e-9)
    current=deepcopy(raw);current['identity']['input_revision']='input-2'
    assert models.reprice_term_product_graph(raw,current)['base_anchor']==result['base_anchor']
    current['positions'][0]['oas_target']['base_oas']=0.
    with pytest.raises(ValueError,match='BASE_ANCHOR_CHANGED'):
        models.reprice_term_product_graph(raw,current)

def test_cd_penalty_and_remaining_interest_are_counted_once():
    result=models.analyze_term_product_graph(example())
    metric=next(row for row in result['metrics'] if row['position_id']=='cd:example')
    penalty=40*.05*90/365
    assert metric['pv']==pytest.approx(-(100.6-(penalty-.2)),abs=1e-9)
    assert metric['physical_tail']['principal_cash']==-100.
    assert metric['physical_tail']['interest_cash']==pytest.approx(-.8,abs=1e-9)
    assert metric['physical_tail']['fee_cash']==pytest.approx(penalty,abs=1e-9)
    assert metric['physical_tail']['cd_net_external_cash']==pytest.approx(-(40-(penalty-.2)),abs=1e-9)
    assert metric['physical_tail']['cd_interest_cash_paid']==0.
