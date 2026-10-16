"""Independent exact integer checks for the new numerical reserve."""
from copy import deepcopy
from pathlib import Path
import json
import pytest
from portfolio_risk.analytics.commercial_regression_work_reference import (
    numerical_reserve, shape_bound, verify_numeric_reserve, active_feature_upper,scan_work_upper,
    constant_fee_observations, full_feature_upper,
)
from commercial_credit_gaussian_exercise_benchmark import matrix, request


@pytest.mark.parametrize("m,k", [(8,1),(16,3),(256,3),(256,128),(512,11),(1024,512)])
def test_reserve_is_exact_checked_structural_census(m,k):
    fast=32*m*k**2+8*k**3+65*(2*k**2+8*k)*(m+k)+16384*k+8*m*k
    repair=32*k*(k-1)*(2*m+k+32)+16*m*k+12*k**2+16*m+128*k
    checks=3*m*k**2+2*k**3+12*m*k+8*m+16*k**2+64*k
    assert shape_bound(m,k)==(fast+repair+2*checks,8*(5*m*k+4*k**2+4*m+32*k)+1024)


@pytest.mark.parametrize("m,k", [(0,0),(1,1),(8,5),(1025,1),(8,2**100),(True,1)])
def test_invalid_shape_is_rejected(m,k):
    with pytest.raises(ArithmeticError):shape_bound(m,k)


def test_no_fit_has_no_numerical_admission_delta():
    raw=request(matrix()[0]);raw["policy"]["exercise_dates"]=[]
    assert numerical_reserve(raw)==(0,0)


def test_every_training_fit_is_reserved_and_workspace_is_once():
    raw=request(matrix()[0]);raw["policy"]["exercise_dates"]=[{},{}]
    one=shape_bound(raw["training"]["paths"],3)
    assert numerical_reserve(raw,columns=3)==(2*one[0]+8*scan_work_upper(raw),one[1]+128)


def test_missing_work_or_workspace_is_independently_rejected():
    raw=request(matrix()[0]);raw["policy"]["exercise_dates"]=[{}]
    work,workspace=numerical_reserve(raw)
    actual={"admitted_work_upper":work,"retained_bytes_upper":workspace}
    assert verify_numeric_reserve(raw,actual)==(work,workspace)
    for key in actual:
        bad=deepcopy(actual);bad[key]-=1
        with pytest.raises(ArithmeticError):verify_numeric_reserve(raw,bad)


def test_raw_law_constants_are_proved_without_sample_or_feature_pruning():
    from portfolio_risk.analytics.commercial_generated_physical_reference import feature_bound
    raw=request(matrix()[0])
    assert active_feature_upper(raw)==5
    zero=deepcopy(raw);zero['annual_covariance']=[0.]*len(zero['annual_covariance'])
    assert active_feature_upper(zero)==0
    credit=deepcopy(raw);credit['source']['credit']['intervals'][0]['hazards_per_year'][1][3]=.01
    assert active_feature_upper(credit)==feature_bound(credit)
    payment=deepcopy(raw);payment['source']['source']['current']['principal_plan']={'kind':'retained_payment','original_payment':10.,'terminal':{'kind':'residual_payoff'}}
    assert active_feature_upper(payment)==feature_bound(payment)
    anchored=deepcopy(raw);anchored['hazard_loadings']=[dict(interval_index=0,origin_index=0,destination_index=3,betas=[0.])]
    assert active_feature_upper(anchored)==6
    drawn=deepcopy(raw);drawn['source']['performing_draw_tranches']=[dict(review=raw['identity'],draw_ordinal=1,eligible_origins=[True,False,False],source=raw['source']['source'])]
    assert active_feature_upper(drawn)==feature_bound(drawn)


@pytest.mark.parametrize('driver,active,columns',[('brownian',5,56),('correlated_ou',6,84)])
def test_predeclared_cubic_cases_have_a_finite_raw_law_numerical_bound(driver,active,columns):
    from commercial_convergence_benchmark import Cell,request as general_request
    raw=general_request(Cell('learned','call','cubic',driver,paths=256))
    assert active_feature_upper(raw)==active
    assert numerical_reserve(raw)==(2*shape_bound(256,columns)[0]+8*scan_work_upper(raw),shape_bound(256,columns)[1]+128)
    assert 1_400_901_632+numerical_reserve(raw)[0]<4_000_000_000


def fee_law():
    graph=json.loads((Path(__file__).resolve().parents[3]/'examples/models/commercial-generated-fee-graph-1.json').read_text())
    return graph['positions'][0]['generated_policy']['pricing_training']


def test_independent_fee_proof_counts_every_feature_and_recomputes_each_raw_role():
    raw=fee_law()
    assert any(c>0 for c in raw['annual_covariance'])
    assert constant_fee_observations(raw)
    assert full_feature_upper(raw)>=22
    assert active_feature_upper(raw)<full_feature_upper(raw)
    for hazard in [float.fromhex('0x0.0000000000001p-1022'),.01,-.01,float('nan')]:
        changed=deepcopy(raw)
        changed['source']['credit']['intervals'][-1]['hazards_per_year'][2][3]=hazard
        assert not constant_fee_observations(changed)
        assert active_feature_upper(changed)==full_feature_upper(changed)
        assert constant_fee_observations(raw)
    zero=deepcopy(raw);zero['annual_covariance']=[0.]*len(zero['annual_covariance'])
    assert active_feature_upper(zero)==0
    del zero['nonintegral_fees']
    assert not constant_fee_observations(zero)


@pytest.mark.parametrize('mutation',['payment','dated','review','draw','fee_transfer','loan_transfer'])
def test_unsupported_fee_constancy_headers_retain_the_full_numerical_bound(mutation):
    raw=fee_law();source=raw['source']
    if mutation=='payment':
        source['source']['current']['principal_plan']={'kind':'retained_payment','original_payment':10.,'terminal':{'kind':'residual_payoff'}}
    elif mutation=='dated':
        source['source']['current']['principal_plan']={'kind':'dated_supplied_period_opening_nominal-1','principal_events':[]}
    elif mutation=='review':
        source['reviewed_credit_actions']={}
    elif mutation=='draw':
        source['performing_draw_tranches']=[dict(review=raw['identity'],draw_ordinal=1,eligible_origins=[True,False,False],source=source['source'])]
    elif mutation=='fee_transfer':raw['nonintegral_fees']['policy']['transfer']={}
    else:source['source']['transfer']={}
    # These are header-predicate controls, not accepted native financial requests.
    assert not constant_fee_observations(raw)
    assert active_feature_upper(raw)==full_feature_upper(raw)
