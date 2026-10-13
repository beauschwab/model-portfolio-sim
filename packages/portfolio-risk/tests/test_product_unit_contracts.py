"""Reviewed original-yield product coefficients, independently hand computed."""
from copy import deepcopy
import json
from math import log1p,expm1,exp,sinh
from pathlib import Path
import pytest
from portfolio_risk.analytics.model_contracts import prepare_product_units

ROOT=Path(__file__).resolve().parents[3]

def example():
    return json.loads((ROOT/'examples/models/product-units-1.json').read_text())

def test_premium_deposit_yield_funding_and_actual_age_risk():
    raw=example();result=prepare_product_units(raw)
    quote=1002*exp(-log1p(.02)*2/365)
    daily=expm1(log1p(.02)/365)
    assert result['shape']==[1,1,10,2]
    assert result['values'][0]==pytest.approx(-quote*daily/1000,abs=1e-12)
    assert result['values'][1]==pytest.approx(-quote*(1+daily)*daily/1000,abs=1e-12)
    assert abs(result['values'][0]+.001)>1e-4
    assert result['units'][0]['income'][0]['opening_states'][-1]['gross_carrying_value']==pytest.approx(0.,abs=1e-9)
    assert result['funding']['shape']==[1,1,3]
    assert result['funding']['demand'][0]==pytest.approx(-quote/1000,abs=1e-12)
    assert result['funding']['demand'][-1]==pytest.approx((1002-quote)/1000,abs=1e-12)
    aged_pv=-1002*exp(-log1p(.02)/365)/1000
    assert result['units'][0]['age_marks'][1]['pv_per_face']==pytest.approx(aged_pv,abs=1e-12)
    assert result['values'][5]==pytest.approx(aged_pv*sinh(.0001/365),abs=1e-12)

def test_retained_yield_and_purchase_spread_are_separate_required_anchors():
    raw=example();bad=deepcopy(raw)
    bad['units'][0]['income_policy']['original_annual_effective_rate']=.03
    with pytest.raises(ValueError):prepare_product_units(bad)
    bad=deepcopy(raw);bad['units'][0]['age_branches'].pop()
    with pytest.raises(ValueError,match='coverage'):prepare_product_units(bad)
    bad=deepcopy(raw)
    bad['units'][0]['age_branches'][1]['graph']['request']['positions'][0]['oas_target']['base_oas']=0.
    with pytest.raises(ValueError):prepare_product_units(bad)
