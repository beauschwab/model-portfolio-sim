"""Independent exact integer checks for the new numerical reserve."""
from copy import deepcopy
import pytest
from portfolio_risk.analytics.commercial_regression_work_reference import (
    numerical_reserve, shape_bound, verify_numeric_reserve,
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
    assert numerical_reserve(raw,columns=3)==(2*one[0],one[1])


def test_missing_work_or_workspace_is_independently_rejected():
    raw=request(matrix()[0]);raw["policy"]["exercise_dates"]=[{}]
    work,workspace=numerical_reserve(raw)
    actual={"admitted_work_upper":work,"retained_bytes_upper":workspace}
    assert verify_numeric_reserve(raw,actual)==(work,workspace)
    for key in actual:
        bad=deepcopy(actual);bad[key]-=1
        with pytest.raises(ArithmeticError):verify_numeric_reserve(raw,bad)
