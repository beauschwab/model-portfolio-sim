"""Whole-loan native graph versus an independent dated payment recurrence."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts as models
ROOT=Path(__file__).resolve().parents[3]

def example():
    return json.loads((ROOT/'examples/models/mortgage-product-graph-1.json').read_text())

def test_payments_remittance_tail_and_preserved_base_anchor():
    raw=example();result=models.analyze_mortgage_graph(raw);metric=result['metrics'][0]
    balance=100.;interest=0.;cash=[]
    for day in (12,22,32):
        earned=balance*.0365*10/365;interest+=earned
        paid=min(34.,balance+earned);balance-=paid-earned;cash.append((day,paid))
    assert metric['pv']==pytest.approx(sum(amount for _,amount in cash),abs=1e-9)
    assert metric['supplied_history_dv01']==pytest.approx(sum(amount*math.sinh(.0025*day/365) for day,amount in cash)/25,abs=1e-9)
    assert sum(p['gross_recognized_investor_interest'] for p in metric['physical_periods'])+metric['physical_tail']['gross_recognized_investor_interest']==pytest.approx(interest,abs=1e-9)
    assert metric['physical_tail']['investor_principal_cash']>0.
    current=deepcopy(raw);current['identity']['input_revision']='revision-2'
    assert models.reprice_mortgage_graph(raw,current)['base_anchor']==result['base_anchor']
    current['positions'][0]['oas_target']['base_oas']=.01
    with pytest.raises(ValueError):models.reprice_mortgage_graph(raw,current)

def test_mortgage_live_investor_tail_cannot_be_clipped():
    raw=example();raw['positions'][0]['contract']['horizon_days']=20
    with pytest.raises(ValueError):models.analyze_mortgage_graph(raw)
