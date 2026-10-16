"""Independent exact integer checks for the new numerical reserve."""
from copy import deepcopy
import pytest
from portfolio_risk.analytics.commercial_regression_work_reference import (
    numerical_reserve, shape_bound, verify_numeric_reserve, active_feature_upper,scan_work_upper,
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
