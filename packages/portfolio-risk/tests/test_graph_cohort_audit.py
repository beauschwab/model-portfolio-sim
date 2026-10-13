"""Native representative/member diagnostics retain full dollars and sources."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts as models
ROOT=Path(__file__).resolve().parents[3]

def example():
    return json.loads((ROOT/'examples/models/graph-cohort-audit-1.json').read_text())

def test_explicit_native_cohort_diagnostics_do_not_authorize_publication():
    raw=example();result=models.audit_graph_cohorts(raw)
    assert result['all_metrics_within_tolerance'] and result['diagnostic_only']
    assert not result['empirical_validation'] and not result['cohort_publication_allowed']
    liability=next(r for r in result['errors'] if r['metric']=='liability_pv')
    assert liability['representative']==pytest.approx(2004.,abs=1e-9)
    assert liability['summed_members']==pytest.approx(2004.,abs=1e-9)
    cash=next(r for r in result['errors'] if r['metric']=='bank_cash_pv')
    assert cash['summed_members']==pytest.approx(-2004.,abs=1e-9)
    assert len(result['rows'])==3 and all(r['native_result']['native_sources'] for r in result['rows'])
    changed=deepcopy(raw);changed['cohorts'][0]['members'][0]['amount_multiplier']=1.1
    mismatch=models.audit_graph_cohorts(changed)
    assert not mismatch['all_metrics_within_tolerance']
    liability=next(r for r in mismatch['errors'] if r['metric']=='liability_pv')
    assert liability['signed_error']==pytest.approx(-100.2,abs=1e-9)

def test_cohort_audit_requires_all_metric_tolerances_and_fixed_member_spreads():
    raw=example();raw['cohorts'][0]['tolerances'].pop()
    with pytest.raises(ValueError):models.audit_graph_cohorts(raw)
    raw=example();raw['cohorts'][0]['members'][0]['graph']['request']['positions'][0]['oas_target']={
        'kind':'calibrate_positive_cash','target_cash_price':1002.,'source_id':'unfrozen-price','source_revision':'1'}
    with pytest.raises(ValueError):models.audit_graph_cohorts(raw)
