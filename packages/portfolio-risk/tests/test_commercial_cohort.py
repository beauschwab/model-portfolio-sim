"""Commercial cohort audit retains actual-date economics and reviewed NII."""
from copy import deepcopy
import pytest
from test_commercial_graph_units import fixture,histories
from portfolio_risk.analytics.model_contracts import audit_graph_cohorts
from portfolio_risk.analytics.commercial_cohort_reference import verify

def cohort():
    def row(name,multiplier):
        g=fixture('commercial-product-graph-1');g['positions'][0]['position_id']=name
        for h in histories(g):
            for role in ('original','current'):h['source']['source'][role]['position_id']=name
        return dict(row_id=name,amount_multiplier=multiplier,graph=dict(kind='commercial',request=g))
    metrics=['date/738886/pv','date/738886/dv01','date/738901/pv','date/738901/dv01','source_full_cash/principal','source_full_cash/interest','source_full_cash/fees','source_full_gross_interest','source_full_effective_interest','source_full_default/principal','source_full_default/principal_loss']
    return dict(schema='graph-cohort-audit-1',source_revision='commercial-explicit-1',monetary_unit='currency_units',cohorts=[dict(cohort_id='commercial',representative=row('representative',2.),members=[row('member-1',1.),row('member-2',1.)],tolerances=[dict(metric=m,absolute_amount=1e-9,relative=1e-12) for m in metrics])])

def test_commercial_representative_and_members_recompute_full_cash_and_eir():
    raw=cohort();result=audit_graph_cohorts(raw);assert verify(raw,result)['full_effective_income_and_cash_recomputed']
    assert result['all_metrics_within_tolerance']
    by_metric={e['metric']:e for e in result['errors']}
    assert by_metric['source_full_gross_interest']['representative']>0.
    assert by_metric['source_full_effective_interest']['representative']==pytest.approx(0.,abs=1e-9)
    changed=deepcopy(raw);changed['cohorts'][0]['members'][0]['amount_multiplier']=1.1
    result=audit_graph_cohorts(changed);assert not result['all_metrics_within_tolerance'];verify(changed,result)

@pytest.mark.parametrize('target',['row','source','error','flag'])
def test_cohort_publication_rejects_raw_financial_and_diagnostic_corruption(target):
    raw=cohort();result=audit_graph_cohorts(raw)
    if target=='row':result['rows'][0]['amounts'][0]['amount']+=1.
    elif target=='source':result['rows'][0]['native_result']['metrics'][0]['dv01']+=1.
    elif target=='error':result['errors'][0]['allowed_error']+=1.
    else:result['cohort_publication_allowed']=True
    with pytest.raises(ArithmeticError):verify(raw,result)
